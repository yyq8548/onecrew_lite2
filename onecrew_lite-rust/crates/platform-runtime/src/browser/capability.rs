//! Peer-facing browser capability projection: whether the user's installed
//! Chrome can carry the login flow at all. Queried standalone by desktop and
//! MCP peers, independent of any login in progress.

use onecrew_chrome::{
    ChromeChannel as DetectedChromeChannel, ChromeInstallScope as DetectedChromeInstallScope,
    DetectedChrome, detect_chrome_install,
};

use crate::LocalPlatform;

/// The host facts `browser_status` is a projection of: whether a Chrome the
/// login flow could drive is installed, and which desktop OS this is.
///
/// A port rather than direct calls, because these two are the only inputs the
/// machine decides. Read straight from the host, they made every test that
/// merely passes near the browser depend on where it runs: on a machine
/// without Chrome the `BrowserNotFound` arm swallows tests that came for an
/// unrelated failure, and on one with it that arm is unreachable — as is every
/// arm belonging to another OS. Neither machine can choose which it exercises,
/// which is the same as saying those arms are untested.
pub(crate) trait BrowserHostProbe: Send + Sync {
    fn detect_chrome(&self) -> Option<DetectedChrome>;
    fn desktop_os(&self) -> DesktopOs;
}

/// The real one: asks the host, fresh, every call.
pub(crate) struct RealBrowserHost;

impl BrowserHostProbe for RealBrowserHost {
    fn detect_chrome(&self) -> Option<DetectedChrome> {
        detect_chrome_install()
    }

    fn desktop_os(&self) -> DesktopOs {
        host_desktop_os()
    }
}

/// A probe answering whatever the test decided, so a test exercises the arm it
/// means to rather than the one its machine happens to be.
#[cfg(test)]
pub(crate) struct FixedBrowserHost {
    chrome: Option<DetectedChrome>,
    os: DesktopOs,
}

#[cfg(test)]
impl FixedBrowserHost {
    /// Chrome present on an OS whose cookies can be read — the configuration a
    /// test wants when the browser is incidental to what it asserts. Nothing
    /// downstream reads the path: a test that truly launches Chrome is not a
    /// unit test.
    pub(crate) fn ready() -> Self {
        Self {
            chrome: Some(DetectedChrome {
                executable: std::path::PathBuf::from("/test/chrome"),
                version: Some("140.0.0.0".into()),
                channel: DetectedChromeChannel::Stable,
                install_scope: DetectedChromeInstallScope::LocalMachine,
            }),
            os: DesktopOs::Macos,
        }
    }

    /// Chrome absent — unreachable on any machine that has it installed.
    pub(crate) fn without_chrome() -> Self {
        Self {
            chrome: None,
            os: DesktopOs::Macos,
        }
    }

    /// Chrome present on an OS with no cookie provider — unreachable on macOS
    /// and Windows.
    pub(crate) fn without_cookie_support() -> Self {
        Self {
            os: DesktopOs::Linux,
            ..Self::ready()
        }
    }
}

#[cfg(test)]
impl BrowserHostProbe for FixedBrowserHost {
    fn detect_chrome(&self) -> Option<DetectedChrome> {
        self.chrome.clone()
    }

    fn desktop_os(&self) -> DesktopOs {
        self.os
    }
}

/// Desktop operating system relevant to the browser-login capability.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DesktopOs {
    Macos,
    Windows,
    Linux,
}

/// End-to-end state of browser login, rather than merely executable discovery.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BrowserCapabilityState {
    Ready,
    BrowserNotFound,
    CookieImportUnavailable,
}

/// Runtime-owned cookie acquisition contract. Adapters must project this value
/// and must not infer provider behavior from the host OS themselves.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BrowserCookieProvider {
    MacosSafeStorage,
    WindowsChromeAssisted,
    WindowsLegacyDpapi,
    Unsupported,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChromeChannel {
    Stable,
    Beta,
    Dev,
    Canary,
    Unknown,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChromeInstallScope {
    User,
    Machine,
    Custom,
    Unknown,
}

/// Stable public error codes for capability gates. Human-readable detection or
/// provider failures remain in `limitations` and are never substituted for the
/// machine-readable code.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BrowserCapabilityCode {
    BrowserNotInstalled,
    BrowserCookieExtractionUnsupported,
}

/// Provider-aware instructions shown immediately before browser completion.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct BrowserCompletionNotice {
    pub provider: BrowserCookieProvider,
    pub message: String,
}

/// Canonical end-to-end browser capability used by desktop and MCP peers.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct BrowserStatus {
    pub os: DesktopOs,
    pub state: BrowserCapabilityState,
    pub installed: bool,
    /// Detected major version (e.g. `"151"`), if readable.
    pub version: Option<String>,
    /// Absolute path to the resolved Chrome binary (for display), if found.
    pub path: Option<String>,
    pub channel: Option<ChromeChannel>,
    pub install_scope: Option<ChromeInstallScope>,
    pub cookie_provider: BrowserCookieProvider,
    pub limitations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_notice: Option<BrowserCompletionNotice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<BrowserCapabilityCode>,
}

impl LocalPlatform {
    /// Is the user's Chrome installed? Resolved fresh each call (the user may
    /// install it and hit "重新检测").
    pub fn browser_status(&self) -> BrowserStatus {
        let os = self.browser_host.desktop_os();
        let Some(install) = self.browser_host.detect_chrome() else {
            return BrowserStatus {
                os,
                state: BrowserCapabilityState::BrowserNotFound,
                installed: false,
                version: None,
                path: None,
                channel: None,
                install_scope: None,
                cookie_provider: cookie_provider(os),
                limitations: vec!["Google Chrome was not detected on this device.".into()],
                completion_notice: None,
                error_code: Some(BrowserCapabilityCode::BrowserNotInstalled),
            };
        };
        let provider = cookie_provider(os);
        let (state, limitations, error_code) = match provider {
            BrowserCookieProvider::MacosSafeStorage
            | BrowserCookieProvider::WindowsChromeAssisted
            | BrowserCookieProvider::WindowsLegacyDpapi => {
                (BrowserCapabilityState::Ready, Vec::new(), None)
            }
            BrowserCookieProvider::Unsupported => (
                BrowserCapabilityState::CookieImportUnavailable,
                vec!["Chrome Cookie extraction is not supported on this operating system.".into()],
                Some(BrowserCapabilityCode::BrowserCookieExtractionUnsupported),
            ),
        };
        BrowserStatus {
            os,
            state,
            installed: true,
            version: install.version,
            path: Some(install.executable.to_string_lossy().into_owned()),
            channel: Some(project_chrome_channel(install.channel)),
            install_scope: Some(project_chrome_install_scope(install.install_scope)),
            cookie_provider: provider,
            limitations,
            completion_notice: completion_notice(provider),
            error_code,
        }
    }

    pub(crate) fn require_browser_start_capability(&self) -> Result<BrowserStatus, String> {
        let capability = self.browser_status();
        require_ready_browser_capability(&capability)?;
        Ok(capability)
    }

    pub(crate) fn require_browser_completion_capability(&self) -> Result<BrowserStatus, String> {
        let capability = self.browser_status();
        require_ready_browser_capability(&capability)?;
        Ok(capability)
    }
}

fn host_desktop_os() -> DesktopOs {
    #[cfg(target_os = "macos")]
    return DesktopOs::Macos;
    #[cfg(target_os = "windows")]
    return DesktopOs::Windows;
    #[cfg(target_os = "linux")]
    return DesktopOs::Linux;
}

fn project_chrome_channel(channel: DetectedChromeChannel) -> ChromeChannel {
    match channel {
        DetectedChromeChannel::Stable => ChromeChannel::Stable,
    }
}

fn project_chrome_install_scope(scope: DetectedChromeInstallScope) -> ChromeInstallScope {
    match scope {
        DetectedChromeInstallScope::ExplicitOverride => ChromeInstallScope::Custom,
        DetectedChromeInstallScope::CurrentUser => ChromeInstallScope::User,
        DetectedChromeInstallScope::LocalMachine => ChromeInstallScope::Machine,
    }
}

fn cookie_provider(os: DesktopOs) -> BrowserCookieProvider {
    match os {
        DesktopOs::Macos => BrowserCookieProvider::MacosSafeStorage,
        // The app-owned profile key is protected for the current Windows user
        // with DPAPI; v20/App-Bound values fail closed in the Chrome crate.
        DesktopOs::Windows => BrowserCookieProvider::WindowsLegacyDpapi,
        DesktopOs::Linux => BrowserCookieProvider::Unsupported,
    }
}

fn completion_notice(provider: BrowserCookieProvider) -> Option<BrowserCompletionNotice> {
    let message = match provider {
        BrowserCookieProvider::MacosSafeStorage => {
            "Before browser completion, macOS may show a native Apple Keychain authorization prompt for Chrome Safe Storage. Enter the current Mac user's login password only in that macOS system dialog; you may choose Always Allow. Never send the system password in chat or supply it to any MCP tool."
        }
        BrowserCookieProvider::WindowsChromeAssisted => {
            "Before browser completion, keep the isolated Chrome login window open until the application closes it. Windows may show a native security prompt; approve it only in that Windows system dialog. Never send Windows credentials in chat or supply them to any MCP tool."
        }
        BrowserCookieProvider::WindowsLegacyDpapi => {
            "Before browser completion, keep the isolated Chrome login window open until the application closes it. Cookie extraction uses the current Windows user's protected credential context; the application and MCP tools will never request the Windows login password. Never send Windows credentials in chat or supply them to any MCP tool."
        }
        BrowserCookieProvider::Unsupported => return None,
    };
    Some(BrowserCompletionNotice {
        provider,
        message: message.into(),
    })
}

fn require_ready_browser_capability(capability: &BrowserStatus) -> Result<(), String> {
    if capability.state == BrowserCapabilityState::Ready {
        return Ok(());
    }
    let code = capability
        .error_code
        .map(|code| match code {
            BrowserCapabilityCode::BrowserNotInstalled => "browser_not_installed",
            BrowserCapabilityCode::BrowserCookieExtractionUnsupported => {
                "browser_cookie_extraction_unsupported"
            }
        })
        .unwrap_or("browser_capability_unavailable");
    let cause = capability
        .limitations
        .first()
        .map(String::as_str)
        .unwrap_or("browser login is unavailable");
    Err(format!("{code}: {cause}"))
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    /// A platform whose browser host facts the test chose. Every arm below was
    /// unreachable before this: a machine either has Chrome or it does not, and
    /// is either macOS or it is not, so whichever arms did not match the machine
    /// were verified by reading the code and nothing else.
    fn platform_with(host: FixedBrowserHost) -> (LocalPlatform, std::path::PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "onecrew-browser-capability-test-{}",
            uuid::Uuid::new_v4()
        ));
        (
            LocalPlatform::open_with_browser_host(&directory, std::sync::Arc::new(host)).unwrap(),
            directory,
        )
    }

    #[test]
    fn a_host_without_chrome_projects_not_found_and_refuses_to_start() {
        let (platform, directory) = platform_with(FixedBrowserHost::without_chrome());
        let status = platform.browser_status();

        assert_eq!(status.state, BrowserCapabilityState::BrowserNotFound);
        assert!(!status.installed);
        assert_eq!(
            status.error_code,
            Some(BrowserCapabilityCode::BrowserNotInstalled)
        );
        // Nothing to report about a browser that is not there.
        assert_eq!(status.version, None);
        assert_eq!(status.path, None);
        assert_eq!(status.channel, None);
        assert_eq!(status.install_scope, None);
        assert!(!status.limitations.is_empty(), "用户要读到原因");

        // The gate every browser login passes through: it must refuse here, and
        // the refusal must carry the machine-readable code rather than only prose.
        let refused = platform.require_browser_start_capability().unwrap_err();
        assert!(refused.starts_with("browser_not_installed:"), "{refused}");

        if let Err(error) = std::fs::remove_dir_all(directory) {
            eprintln!("test directory cleanup failed: {error}");
        }
    }

    #[test]
    fn a_host_without_a_cookie_provider_projects_installed_but_unusable() {
        let (platform, directory) = platform_with(FixedBrowserHost::without_cookie_support());
        let status = platform.browser_status();

        // Chrome is present — the browser is not the problem, reading its
        // cookies is, and the projection has to say which.
        assert!(status.installed);
        assert_eq!(status.path.as_deref(), Some("/test/chrome"));
        assert_eq!(
            status.state,
            BrowserCapabilityState::CookieImportUnavailable
        );
        assert_eq!(status.cookie_provider, BrowserCookieProvider::Unsupported);
        assert_eq!(
            status.error_code,
            Some(BrowserCapabilityCode::BrowserCookieExtractionUnsupported)
        );

        let refused = platform.require_browser_start_capability().unwrap_err();
        assert!(
            refused.starts_with("browser_cookie_extraction_unsupported:"),
            "{refused}"
        );

        if let Err(error) = std::fs::remove_dir_all(directory) {
            eprintln!("test directory cleanup failed: {error}");
        }
    }

    #[test]
    fn a_ready_host_projects_ready_and_opens_the_gate() {
        let (platform, directory) = platform_with(FixedBrowserHost::ready());
        let status = platform.browser_status();

        assert_eq!(status.state, BrowserCapabilityState::Ready);
        assert!(status.installed);
        assert_eq!(status.error_code, None);
        assert!(status.limitations.is_empty());
        assert!(platform.require_browser_start_capability().is_ok());

        if let Err(error) = std::fs::remove_dir_all(directory) {
            eprintln!("test directory cleanup failed: {error}");
        }
    }

    fn capability(
        os: DesktopOs,
        state: BrowserCapabilityState,
        provider: BrowserCookieProvider,
        error_code: Option<BrowserCapabilityCode>,
        limitations: Vec<String>,
    ) -> BrowserStatus {
        BrowserStatus {
            os,
            state,
            installed: state != BrowserCapabilityState::BrowserNotFound,
            version: None,
            path: None,
            channel: None,
            install_scope: None,
            cookie_provider: provider,
            limitations,
            completion_notice: completion_notice(provider),
            error_code,
        }
    }

    #[test]
    fn completion_notice_is_owned_by_the_selected_provider() {
        let notice = completion_notice(BrowserCookieProvider::WindowsChromeAssisted).unwrap();
        assert_eq!(
            notice.provider,
            BrowserCookieProvider::WindowsChromeAssisted
        );
        assert!(notice.message.contains("Windows"));
        assert!(!notice.message.contains("macOS"));
        assert!(completion_notice(BrowserCookieProvider::Unsupported).is_none());
    }

    #[test]
    fn capability_gate_returns_a_stable_code_and_preserves_the_cause() {
        let unavailable = capability(
            DesktopOs::Windows,
            BrowserCapabilityState::CookieImportUnavailable,
            BrowserCookieProvider::Unsupported,
            Some(BrowserCapabilityCode::BrowserCookieExtractionUnsupported),
            vec!["provider rejected the installed Cookie format".into()],
        );
        assert_eq!(
            require_ready_browser_capability(&unavailable).unwrap_err(),
            "browser_cookie_extraction_unsupported: provider rejected the installed Cookie format"
        );
    }

    #[test]
    fn capability_wire_contains_os_provider_state_limitations_and_error_code() {
        let unavailable = capability(
            DesktopOs::Windows,
            BrowserCapabilityState::BrowserNotFound,
            BrowserCookieProvider::WindowsChromeAssisted,
            Some(BrowserCapabilityCode::BrowserNotInstalled),
            vec!["Chrome was not found".into()],
        );
        let wire = serde_json::to_value(unavailable).unwrap();
        assert_eq!(wire["os"], "windows");
        assert_eq!(wire["state"], "browser_not_found");
        assert_eq!(wire["cookie_provider"], "windows_chrome_assisted");
        assert_eq!(wire["error_code"], "browser_not_installed");
        assert_eq!(wire["limitations"][0], "Chrome was not found");
        assert!(wire["completion_notice"].is_object());
    }

    #[test]
    fn detected_chrome_metadata_is_projected_without_losing_scope() {
        assert_eq!(
            project_chrome_channel(DetectedChromeChannel::Stable),
            ChromeChannel::Stable
        );
        assert_eq!(
            project_chrome_install_scope(DetectedChromeInstallScope::ExplicitOverride),
            ChromeInstallScope::Custom
        );
        assert_eq!(
            project_chrome_install_scope(DetectedChromeInstallScope::CurrentUser),
            ChromeInstallScope::User
        );
        assert_eq!(
            project_chrome_install_scope(DetectedChromeInstallScope::LocalMachine),
            ChromeInstallScope::Machine
        );
    }
}
