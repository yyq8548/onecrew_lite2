//! Native installation paths shared by explicit update, integration staging,
//! and read-only status guidance.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

pub fn current_cli_target() -> Result<&'static str, String> {
    #[cfg(target_os = "windows")]
    {
        // Ask the kernel, not the environment. Codex starts the stdio MCP with
        // a cleared environment and passes only its own core variables, none of
        // which is a PROCESSOR_* one, so the service it spawns inherited no
        // architecture at all and every update check reported an unsupported
        // target.
        use windows_sys::Win32::System::{
            SystemInformation::IMAGE_FILE_MACHINE_AMD64,
            Threading::{GetCurrentProcess, IsWow64Process2},
        };

        let mut process_machine = 0;
        let mut native_machine = 0;
        // SAFETY: both out parameters live for the whole call, and the pseudo
        // handle GetCurrentProcess returns is not a resource to close.
        let queried = unsafe {
            IsWow64Process2(
                GetCurrentProcess(),
                &mut process_machine,
                &mut native_machine,
            )
        };
        if queried == 0 {
            return Err("update_architecture_unavailable".into());
        }
        if native_machine != IMAGE_FILE_MACHINE_AMD64 {
            return Err("update_unsupported_architecture".into());
        }
    }
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        _ => Err("update_unsupported_architecture".into()),
    }
}
#[derive(Clone, Copy)]
enum HostFamily {
    Unix,
    Windows,
}

const HOST_FAMILY: HostFamily = if cfg!(windows) {
    HostFamily::Windows
} else {
    HostFamily::Unix
};

/// Root directory names: `~/.local/share/onecrew_lite` on macOS and Linux, and
/// `%LOCALAPPDATA%\OneCrew` on Windows, which also holds that CLI's `bin`.
const UNIX_ROOT: &str = "onecrew_lite";
const WINDOWS_ROOT: &str = "onecrew_lite";
const CODEX_PLUGIN: &str = "codex-plugin";
const SERVICE_BINARIES: &str = "service-binaries";

#[cfg(windows)]
pub(crate) fn windows_local_data_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(installed_windows_base)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
}

#[cfg(windows)]
fn installed_windows_base(executable: &Path) -> Option<PathBuf> {
    if executable.file_name()? != "onecrew_lite.exe" {
        return None;
    }
    let parent = executable.parent()?;
    let root = if parent.file_name()? == "bin" {
        parent.parent()?
    } else {
        let images = parent.parent()?;
        if images.file_name()? != SERVICE_BINARIES {
            return None;
        }
        images.parent()?
    };
    (root.file_name()? == WINDOWS_ROOT)
        .then(|| root.parent().map(Path::to_path_buf))
        .flatten()
}
/// The OS cache directory name that staged service images used before this
/// layout. Nothing stages there any more; see `is_managed_cli_copy`.
const RETIRED_CACHE_ROOT: &str = "io.onecrewai.onecrew_lite";

/// The fixed per-user OneCrew installation layout.
///
/// The installers place the CLI at a fixed per-user path. Regenerable
/// integration state that other programs keep referring to by absolute path
/// lives in the matching per-user root, never an OS cache directory the host
/// may purge: Codex keeps the marketplace source it registered, and a running
/// service keeps its executable image. The layout follows that install root,
/// not the running executable's location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInstallLayout {
    executable: PathBuf,
    root: PathBuf,
}

impl UserInstallLayout {
    /// Resolve the layout for the current user.
    ///
    /// # Errors
    ///
    /// `install_root_unavailable` when the home directory (macOS and Linux) or
    /// the local application data directory (Windows) is unknown or relative.
    pub fn current() -> Result<Self, String> {
        let base = match HOST_FAMILY {
            HostFamily::Unix => dirs::home_dir(),
            HostFamily::Windows => {
                #[cfg(windows)]
                {
                    windows_local_data_dir()
                }
                #[cfg(not(windows))]
                {
                    dirs::data_local_dir()
                }
            }
        };
        base.filter(|path| path.is_absolute())
            .map(|base| Self::for_host(HOST_FAMILY, &base))
            .ok_or_else(|| "install_root_unavailable".into())
    }

    fn for_host(host: HostFamily, base: &Path) -> Self {
        match host {
            HostFamily::Unix => Self {
                executable: base.join(".local").join("bin").join("onecrew_lite"),
                root: base.join(".local").join("share").join(UNIX_ROOT),
            },
            HostFamily::Windows => {
                let root = base.join(WINDOWS_ROOT);
                Self {
                    executable: root.join("bin").join("onecrew_lite.exe"),
                    root,
                }
            }
        }
    }

    /// The standalone CLI that the installers and `onecrew update` maintain.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// The directory holding one generated Codex marketplace per Codex home.
    pub fn codex_plugin(&self) -> PathBuf {
        self.root.join(CODEX_PLUGIN)
    }

    /// The directory holding content-addressed running service executables.
    pub fn service_binaries(&self) -> PathBuf {
        self.root.join(SERVICE_BINARIES)
    }
}

/// Whether `path` is a copy staged by OneCrew or installed by Codex, rather
/// than the user's CLI installation.
///
/// Staged directories match by path component, ignoring ASCII case as Windows
/// does, so a canonicalized path or another user's root still matches.
pub fn is_managed_cli_copy(path: &Path) -> bool {
    let parts = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>();
    parts.windows(2).any(|pair| {
        pair == ["plugins", "cache"]
            || ((pair[0].eq_ignore_ascii_case(UNIX_ROOT)
                || pair[0].eq_ignore_ascii_case(WINDOWS_ROOT))
                && (pair[1] == CODEX_PLUGIN || pair[1] == SERVICE_BINARIES))
            // `ensure_service` hashes and copies whatever file is at the CLI's
            // executable path when it runs, not the image it loaded. On macOS
            // that path holds the new build as soon as `onecrew update`
            // replaces it. A CLI from before this layout that finds the
            // service stopped for that update can therefore stage and start
            // the new build under the retired cache directory. There this
            // check must still keep the new build from being taken for the
            // user's installation. A retired `cli-plugin` generation needs no
            // pair: OneCrew never launches a marketplace generation's binary,
            // and install readback verifies Codex's copy under `plugins/cache`.
            || pair == [RETIRED_CACHE_ROOT, SERVICE_BINARIES]
    })
}

pub fn default_cli_installation() -> Result<PathBuf, String> {
    Ok(UserInstallLayout::current()?.executable().to_owned())
}

/// Called under the service owner lease before startup, and before opening
/// Library. Interrupted or in-progress whole-package replacements stay closed.
pub(crate) fn require_complete_workspace(package: &Path) -> Result<(), String> {
    let marker = package
        .parent()
        .ok_or("workspace_install_directory_unavailable")?
        .join(".onecrew_lite-install");
    match fs::symlink_metadata(marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err("workspace_installation_pending: complete installation or recovery before starting onecrew_lite".into()),
        Err(_) => Err("workspace_installation_state_unavailable".into()),
    }
}

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, u32),
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let path = path.canonicalize().ok()?;
    if is_managed_cli_copy(&path)
        || path.file_name()?
            != std::ffi::OsStr::new(if cfg!(windows) {
                "onecrew_lite.exe"
            } else {
                "onecrew_lite"
            })
    {
        return None;
    }
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        (
            metadata.dev(),
            metadata.ino(),
            metadata.permissions().mode(),
        )
    };
    Some(FileStamp {
        path,
        size: metadata.len(),
        modified: metadata.modified().ok()?,
        #[cfg(unix)]
        identity,
    })
}

struct CachedPath {
    expected_hash: String,
    stamps: Vec<FileStamp>,
    path: Option<PathBuf>,
    checked: Instant,
}

#[derive(Default)]
pub(crate) struct UpdateCommandPath {
    cached: Mutex<Option<CachedPath>>,
}

impl UpdateCommandPath {
    /// Prove the standalone file matches the running CLI, then reuse that proof
    /// only while its file metadata and the service identity remain unchanged.
    pub(crate) fn resolve(&self, source: &Path, expected_hash: &str) -> Option<PathBuf> {
        self.resolve_candidates(
            source,
            default_cli_installation().ok().as_deref(),
            expected_hash,
        )
    }

    fn resolve_candidates(
        &self,
        source: &Path,
        fallback: Option<&Path>,
        expected_hash: &str,
    ) -> Option<PathBuf> {
        let mut stamps = Vec::new();
        for candidate in std::iter::once(source).chain(fallback) {
            if let Some(stamp) = file_stamp(candidate)
                && !stamps.contains(&stamp)
            {
                stamps.push(stamp);
            }
        }
        let mut cache = self
            .cached
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(cached) = cache.as_ref()
            && cached.expected_hash == expected_hash
            && cached.stamps == stamps
            && cached.checked.elapsed() < Duration::from_secs(15 * 60)
        {
            return cached.path.clone();
        }
        let path = stamps.iter().find_map(|stamp| {
            let hash = crate::paths::executable_identity(&stamp.path).ok()?;
            (hash == expected_hash && file_stamp(&stamp.path).as_ref() == Some(stamp))
                .then(|| stamp.path.clone())
        });
        *cache = Some(CachedPath {
            expected_hash: expected_hash.to_owned(),
            stamps,
            path: path.clone(),
            checked: Instant::now(),
        });
        path
    }
}

#[cfg(test)]
mod tests;
