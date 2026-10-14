use crate::package::{self, MARKETPLACE, NAME, PluginPackage};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    Unknown,
    Installed,
    NotInstalled,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    NotChecked,
    Cached,
    Fresh,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginStatus {
    pub state: PluginState,
    pub verification: Verification,
    pub installed_version: Option<String>,
    pub enabled: Option<bool>,
    pub last_verified_at: Option<u64>,
    pub diagnostic_code: Option<String>,
}

fn status(state: PluginState, verification: Verification, code: Option<&str>) -> PluginStatus {
    PluginStatus {
        state,
        verification,
        installed_version: None,
        enabled: None,
        last_verified_at: None,
        diagnostic_code: code.map(str::to_owned),
    }
}

const MARKETPLACE_SOURCE_MISSING: &str = "codex_marketplace_source_missing";
const MARKETPLACE_SOURCE_RELOCATED: &str = "codex_marketplace_source_relocated";

impl PluginStatus {
    /// Whether Codex's `onecrew_lite-cli` registration does not name this plugin
    /// root's loadable marketplace. An explicit `onecrew plugin install` is the
    /// repair: it regenerates the marketplace and replaces the registration.
    pub fn requires_marketplace_repair(&self) -> bool {
        matches!(
            self.diagnostic_code.as_deref(),
            Some(MARKETPLACE_SOURCE_MISSING | MARKETPLACE_SOURCE_RELOCATED)
        )
    }
}

// The registered source belongs to the marketplace, not its current plugin name.
// Codex keeps this path in its config, so the caller's plugin root must be a
// durable per-user location rather than a purgeable cache.
fn marketplace_root_for(plugin_root: &Path, home: &Path) -> Result<PathBuf, String> {
    let identity = match fs::canonicalize(home) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::path::absolute(home).map_err(|_| "codex_home_unavailable")?
        }
        Err(_) => return Err("codex_home_unavailable".into()),
    };
    Ok(plugin_root.join(package::sha(identity.as_os_str().as_encoded_bytes())))
}

fn codex_home() -> Result<PathBuf, String> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|path| path.join(".codex")))
        .ok_or_else(|| "codex_home_unavailable".into())
}

/// Every path discovery would try, in the order it tries them.
///
/// Public so the CLI can name them when discovery fails. Independent Windows
/// service hosts do not inherit Codex's injected PATH, so also inspect the
/// desktop app's versioned native CLI directory in the user's Local App Data.
pub fn codex_cli_candidates() -> Vec<PathBuf> {
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .map(|path| path.join(name))
                .collect()
        })
        .unwrap_or_default();
    #[cfg(windows)]
    if let Some(local) = dirs::data_local_dir() {
        candidates.extend(windows_desktop_cli_candidates(&local));
    }
    if cfg!(target_os = "macos") {
        let mut application_directories = vec![PathBuf::from("/Applications")];
        if let Some(home) = dirs::home_dir() {
            application_directories.push(home.join("Applications"));
        }
        candidates.extend(desktop_cli_candidates(&application_directories));
    }
    candidates
}

#[cfg(any(windows, test))]
fn windows_desktop_cli_candidates(local: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(local.join("OpenAI/Codex/bin")) else {
        return Vec::new();
    };
    let mut candidates = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if !(16..=64).contains(&name.len())
                || !name.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return None;
            }
            let cli = entry.path().join("codex.exe");
            let metadata = fs::metadata(&cli).ok()?;
            metadata
                .is_file()
                .then(|| (metadata.modified().unwrap_or(UNIX_EPOCH), cli))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.cmp(left));
    candidates.into_iter().map(|(_, path)| path).collect()
}

fn desktop_cli_candidates(application_directories: &[PathBuf]) -> Vec<PathBuf> {
    application_directories
        .iter()
        .flat_map(|directory| {
            ["Codex.app", "ChatGPT.app"]
                .into_iter()
                .flat_map(move |app| {
                    ["codex-cli/bin/codex", "codex"]
                        .map(|entry| directory.join(app).join("Contents/Resources").join(entry))
                })
        })
        .collect()
}

/// The Codex CLI this invocation will drive.
///
/// An explicit path replaces discovery outright rather than seeding it: the
/// caller has said which CLI to use, so a typo must not silently fall through
/// to another one. The CLI validates the path before it gets here.
pub fn resolve_codex_cli(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return if path.is_file() {
            Ok(path.to_owned())
        } else {
            Err("codex_cli_unavailable".into())
        };
    }
    codex_cli_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "codex_cli_unavailable".into())
}

/// Read the local receipt and Codex's persisted marketplace registration. This
/// never discovers, starts, or queries Codex, and writes nothing.
///
/// `plugin_root` is the per-user directory holding one generated marketplace
/// per Codex home; the install-layout owner supplies it. A registration that
/// needs repair replaces the receipt's state with its diagnostic, keeping the
/// receipt's verification and last check; see
/// [`PluginStatus::requires_marketplace_repair`].
pub fn read_cached_plugin_status(plugin_root: &Path) -> Result<PluginStatus, String> {
    read_cached(plugin_root, &codex_home()?)
}

fn read_cached(plugin_root: &Path, home: &Path) -> Result<PluginStatus, String> {
    let marketplace = marketplace_root_for(plugin_root, home)?;
    let cached = read_cache(&marketplace.join(NAME))?;
    let Some(code) = registration_diagnostic(home, &marketplace) else {
        return Ok(cached);
    };
    Ok(PluginStatus {
        state: PluginState::Unavailable,
        verification: cached.verification,
        installed_version: None,
        enabled: None,
        last_verified_at: cached.last_verified_at,
        diagnostic_code: Some(code.into()),
    })
}

/// Codex cannot load a registered source whose index is gone, such as a cache
/// directory the OS purged; it reports that the marketplace root does not
/// contain a supported manifest. A loadable registration elsewhere is not the
/// marketplace install and refresh maintain. A config that cannot be read is
/// left to explicit install, which reports it before changing anything.
fn registration_diagnostic(home: &Path, marketplace: &Path) -> Option<&'static str> {
    let source = read_codex_config(home).ok()??.marketplaces?.onecrew?;
    if source.source_type == "local"
        && !source
            .source
            .join(".agents/plugins/marketplace.json")
            .is_file()
    {
        Some(MARKETPLACE_SOURCE_MISSING)
    } else if !source.names(marketplace) {
        Some(MARKETPLACE_SOURCE_RELOCATED)
    } else {
        None
    }
}

fn read_cache(root: &Path) -> Result<PluginStatus, String> {
    let invalid = || {
        status(
            PluginState::Unavailable,
            Verification::NotChecked,
            Some("plugin_receipt_invalid"),
        )
    };
    let bytes = match read_receipt(&root.join("status.json")) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => match read_receipt(&root.join("status.previous.json")) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Ok(status(PluginState::Unknown, Verification::NotChecked, None)),
            Err(_) => return Ok(invalid()),
        },
        Err(_) => return Ok(invalid()),
    };
    let mut cached: PluginStatus = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return Ok(invalid()),
    };
    if cached
        .installed_version
        .as_deref()
        .is_some_and(|version| !valid_version(version))
        || (cached.state == PluginState::Installed
            && (cached.installed_version.is_none()
                || cached.last_verified_at.is_none()
                || cached.enabled.is_none()))
        || cached
            .diagnostic_code
            .as_deref()
            .is_some_and(|code| !safe_diagnostic(code))
    {
        return Ok(invalid());
    }
    cached.verification = Verification::Cached;
    Ok(cached)
}

fn read_receipt(path: &Path) -> Result<Option<Vec<u8>>, ()> {
    use std::io::Read;
    const MAX_RECEIPT: u64 = 8192;
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_RECEIPT {
        return Err(());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|_| ())?;
    let opened = file.metadata().map_err(|_| ())?;
    if !opened.file_type().is_file() || opened.len() > MAX_RECEIPT {
        return Err(());
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECEIPT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err(());
    }
    Ok(Some(bytes))
}

fn valid_version(version: &str) -> bool {
    package::version_base(version).is_some()
}

fn safe_diagnostic(code: &str) -> bool {
    matches!(
        code,
        "clock_invalid"
            | "codex_home_unavailable"
            | "codex_plugin_config_read_failed"
            | "codex_plugin_config_invalid"
            | "codex_cli_command_failed"
            | "codex_cli_json_invalid"
            | "codex_cli_output_too_large"
            | "codex_cli_read_failed"
            | "codex_cli_reader_failed"
            | "codex_cli_start_failed"
            | "codex_cli_stdout_missing"
            | "codex_cli_timed_out"
            | "codex_cli_unavailable"
            | "codex_cli_wait_failed"
            | "codex_plugin_readback_ambiguous"
            | "codex_plugin_schema_unsupported"
            | "codex_plugin_version_invalid"
            | "codex_plugin_version_missing"
            | "data_directory_must_be_absolute"
            | "data_directory_not_utf8"
            | "executable_metadata_failed"
            | "executable_read_failed"
            | "executable_target_mismatch"
            | "existing_package_modified"
            | "installed_binary_missing"
            | "installed_content_digest_mismatch"
            | "installed_manifest_invalid"
            | "installed_manifest_mismatch"
            | "installed_manifest_missing"
            | "installed_tree_file_invalid"
            | "installed_tree_path_invalid"
            | "installed_tree_read_failed"
            | "installed_tree_symlink"
            | "invalid_executable"
            | "marketplace_path_not_utf8"
            | "marketplace_write_failed"
            | "plugin_cache_create_failed"
            | "plugin_directory_failed"
            | "plugin_encoding_failed"
            | "plugin_file_write_failed"
            | "plugin_install_readback_mismatch"
            | "plugin_identity_change_required"
            | "plugin_lock_failed"
            | "plugin_lock_open_failed"
            | "plugin_manifest_invalid"
            | "plugin_manifest_missing"
            | "plugin_path_invalid"
            | "plugin_pending_cleanup_failed"
            | "plugin_permissions_failed"
            | "plugin_publish_failed"
            | "plugin_receipt_encoding_failed"
            | "plugin_receipt_invalid"
            | "plugin_receipt_publish_failed"
            | "plugin_receipt_read_failed"
            | "plugin_receipt_write_failed"
            | "unsupported_host"
            | "unsupported_target"
    )
}

fn save(root: &Path, value: &PluginStatus) -> Result<(), String> {
    let pending = root.join("status.pending.json");
    fs::write(
        &pending,
        serde_json::to_vec_pretty(value).map_err(|_| "plugin_receipt_encoding_failed")?,
    )
    .map_err(|_| "plugin_receipt_write_failed")?;
    // Windows rename cannot replace the destination. Keep a recoverable previous receipt.
    let receipt = root.join("status.json");
    let previous = root.join("status.previous.json");
    if previous.exists() {
        fs::remove_file(&previous).map_err(|_| "plugin_receipt_publish_failed")?;
    }
    if receipt.exists() {
        fs::rename(&receipt, &previous).map_err(|_| "plugin_receipt_publish_failed")?;
    }
    if fs::rename(&pending, &receipt).is_err() {
        if previous.exists()
            && let Err(error) = fs::rename(&previous, &receipt)
        {
            log::warn!("plugin receipt restore after a failed publish failed: {error}");
        }
        return Err("plugin_receipt_publish_failed".into());
    }
    Ok(())
}

/// The synchronous worker holds its filesystem lease until all subprocesses have exited.
///
/// Stages and registers the marketplace inside `plugin_root`, as for
/// [`read_cached_plugin_status`].
pub fn install_plugin(
    plugin_root: &Path,
    executable: &Path,
    data_dir: Option<&Path>,
    codex_cli: Option<&Path>,
) -> Result<PluginStatus, String> {
    let home = codex_home()?;
    let marketplace = marketplace_root_for(plugin_root, &home)?;
    let root = marketplace.join(NAME);
    let _marketplace_lease = package::lease(&marketplace)?;
    let _lease = package::lease(&root)?;
    let result = (|| {
        let cli = resolve_codex_cli(codex_cli)?;
        // Validate the replacement before retiring an existing installation.
        package::expected_plugin_version(executable, package::host_target()?, data_dir)?;
        // Codex owns removal of its two former OneCrew plugin identities.
        retire_legacy_plugins(&home, |args| run(&cli, args))?;
        let package = prepare_install(executable, &home, &marketplace, data_dir)?;
        install_with(&package, &home, |args| run(&cli, args))
    })();
    finish(&root, result)
}

fn prepare_install(
    executable: &Path,
    home: &Path,
    marketplace: &Path,
    data_dir: Option<&Path>,
) -> Result<PluginPackage, String> {
    // Check identity before replacing the stable index, which may still serve
    // an enabled old plugin. Recheck in install_with after staging completes.
    plugin_preflight(home, marketplace)?;
    let mut package = package::package_locked_scoped(
        executable,
        package::host_target()?,
        &marketplace.join(NAME),
        data_dir,
    )?;
    package.marketplace_root = package::registration_root(marketplace, &package)?;
    Ok(package)
}

fn install_with(
    package: &PluginPackage,
    home: &Path,
    mut command: impl FnMut(&[&str]) -> Result<serde_json::Value, String>,
) -> Result<PluginStatus, String> {
    let replace_source = plugin_preflight(home, &package.marketplace_root)?;
    let directory = package
        .marketplace_root
        .to_str()
        .ok_or("marketplace_path_not_utf8")?;
    // Register the staged source before listing: Codex cannot list a marketplace
    // whose previous source is missing, even when installed cache entries remain.
    // Repair a registration at a noncanonical source once. Name or version
    // changes within the fixed marketplace root never require removing it.
    if replace_source {
        command(&["plugin", "marketplace", "remove", MARKETPLACE, "--json"])?;
    }
    command(&["plugin", "marketplace", "add", directory, "--json"])?;
    command(&["plugin", "add", &format!("{NAME}@{MARKETPLACE}"), "--json"])?;
    let list = command(&["plugin", "list", "--marketplace", MARKETPLACE, "--json"])?;
    let value = readback(&list, home)?;
    if value.state != PluginState::Installed
        || value.installed_version.as_deref() != Some(&package.version)
        || value.enabled != Some(true)
    {
        return Err("plugin_install_readback_mismatch".into());
    }
    Ok(value)
}

#[derive(Deserialize)]
struct CodexConfig {
    plugins: Option<std::collections::BTreeMap<String, PluginConfig>>,
    marketplaces: Option<Marketplaces>,
}

#[derive(Deserialize)]
struct PluginConfig {
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct Marketplaces {
    #[serde(rename = "onecrew_lite-cli")]
    onecrew: Option<MarketplaceSource>,
}

#[derive(Deserialize)]
struct MarketplaceSource {
    source_type: String,
    source: PathBuf,
}

impl MarketplaceSource {
    /// Whether this registration is the local marketplace at `root`.
    fn names(&self, root: &Path) -> bool {
        self.source_type == "local"
            && (self.source == root
                || fs::canonicalize(&self.source)
                    .ok()
                    .zip(fs::canonicalize(root).ok())
                    .is_some_and(|(registered, staged)| registered == staged))
    }
}

/// Codex's persisted config, read without rewriting it; `None` when absent.
fn read_codex_config(home: &Path) -> Result<Option<CodexConfig>, String> {
    let source = match fs::read_to_string(home.join("config.toml")) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("codex_plugin_config_read_failed".into()),
    };
    toml::from_str(&source)
        .map(Some)
        .map_err(|_| "codex_plugin_config_invalid".into())
}

const RETIRED_PLUGINS: &[&str] = &[
    "onecrew_lite-local@onecrew_lite-cli",
    "onecrew_lite-local@onecrew_lite-local",
];

fn retire_legacy_plugins(
    home: &Path,
    mut command: impl FnMut(&[&str]) -> Result<serde_json::Value, String>,
) -> Result<(), String> {
    for name in RETIRED_PLUGINS {
        let present = read_codex_config(home)?
            .and_then(|config| config.plugins)
            .is_some_and(|plugins| plugins.contains_key(*name));
        if !present {
            continue;
        }
        command(&["plugin", "remove", name, "--json"])
            .map_err(|_| "plugin_identity_change_required")?;
        if read_codex_config(home)?
            .and_then(|config| config.plugins)
            .is_some_and(|plugins| plugins.contains_key(*name))
        {
            return Err("plugin_identity_change_required".into());
        }
    }
    Ok(())
}

fn plugin_preflight(home: &Path, root: &Path) -> Result<bool, String> {
    // Codex's list is marketplace-derived and omits retired names once the source
    // changes. Read its persisted enablement flags without rewriting the config.
    let Some(config) = read_codex_config(home)? else {
        return Ok(false);
    };
    if config.plugins.is_some_and(|plugins| {
        RETIRED_PLUGINS.iter().any(|name| {
            plugins
                .get(*name)
                .is_some_and(|plugin| plugin.enabled != Some(false))
        })
    }) {
        return Err("plugin_identity_change_required".into());
    }
    Ok(config
        .marketplaces
        .and_then(|value| value.onecrew)
        .is_some_and(|source| !source.names(root)))
}

/// Explicit current verification; no install or repair is performed.
///
/// Writes the receipt inside `plugin_root`, as for [`read_cached_plugin_status`].
pub fn refresh_plugin_status(
    plugin_root: &Path,
    codex_cli: Option<&Path>,
) -> Result<PluginStatus, String> {
    refresh_with_generation(plugin_root, codex_cli, &[])
}

/// Explicit verification against the caller's immutable package generation.
/// A valid old plugin is installed, but cannot be claimed compatible with a
/// different current executable or router generation.
pub fn refresh_plugin_status_for_generations(
    plugin_root: &Path,
    codex_cli: Option<&Path>,
    expected_versions: &[String],
) -> Result<PluginStatus, String> {
    if expected_versions.is_empty() {
        return Err("codex_plugin_expected_generation_missing".into());
    }
    refresh_with_generation(plugin_root, codex_cli, expected_versions)
}

fn refresh_with_generation(
    plugin_root: &Path,
    codex_cli: Option<&Path>,
    expected_versions: &[String],
) -> Result<PluginStatus, String> {
    let home = codex_home()?;
    let marketplace = marketplace_root_for(plugin_root, &home)?;
    let root = marketplace.join(NAME);
    let _marketplace_lease = package::lease(&marketplace)?;
    let _lease = package::lease(&root)?;
    let result = (|| {
        let cli = resolve_codex_cli(codex_cli)?;
        let list = run(
            &cli,
            &["plugin", "list", "--marketplace", MARKETPLACE, "--json"],
        )?;
        let value = readback(&list, &home)?;
        Ok(check_generation(value, expected_versions))
    })();
    finish(&root, result)
}

fn check_generation(mut value: PluginStatus, expected: &[String]) -> PluginStatus {
    if value.state == PluginState::Installed
        && !expected.is_empty()
        && !expected
            .iter()
            .any(|version| value.installed_version.as_deref() == Some(version))
    {
        value.state = PluginState::Unavailable;
        value.diagnostic_code = Some("codex_plugin_update_required".into());
    }
    value
}

fn finish(root: &Path, result: Result<PluginStatus, String>) -> Result<PluginStatus, String> {
    match result {
        Ok(mut value) => {
            value.last_verified_at = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| "clock_invalid")?
                    .as_secs(),
            );
            save(root, &value)?;
            Ok(value)
        }
        Err(code) => {
            let mut value = status(
                PluginState::Unavailable,
                Verification::NotChecked,
                Some(&code),
            );
            value.last_verified_at = read_cache(root).ok().and_then(|old| old.last_verified_at);
            save(root, &value)?;
            Ok(value)
        }
    }
}

fn readback(list: &serde_json::Value, home: &Path) -> Result<PluginStatus, String> {
    let entries = list
        .get("installed")
        .and_then(serde_json::Value::as_array)
        .ok_or("codex_plugin_schema_unsupported")?;
    let found: Vec<_> = entries
        .iter()
        .filter(|item| {
            item["name"] == NAME
                && item["marketplaceName"] == MARKETPLACE
                && item["installed"] == true
        })
        .collect();
    let item = match found.as_slice() {
        [] => return Ok(status(PluginState::NotInstalled, Verification::Fresh, None)),
        [item] => *item,
        _ => return Err("codex_plugin_readback_ambiguous".into()),
    };
    let version = item["version"]
        .as_str()
        .ok_or("codex_plugin_version_missing")?;
    if !valid_version(version) {
        return Err("codex_plugin_version_invalid".into());
    }
    let installed = home
        .join("plugins/cache")
        .join(MARKETPLACE)
        .join(NAME)
        .join(version);
    package::verify_installed(&installed, version, package::host_target()?)?;
    Ok(PluginStatus {
        state: PluginState::Installed,
        verification: Verification::Fresh,
        installed_version: Some(version.into()),
        enabled: Some(
            item["enabled"]
                .as_bool()
                .ok_or("codex_plugin_schema_unsupported")?,
        ),
        last_verified_at: None,
        diagnostic_code: None,
    })
}

/// Stop a CLI child that outlived its budget or whose wait failed. The
/// caller's typed reason stays the reported error; a kill or wait failure
/// only means the child is already gone or cannot be reaped from here.
fn stop_cli_child(child: &mut std::process::Child) {
    if let Err(error) = child.kill() {
        log::warn!("codex CLI child kill failed: {error}");
    }
    if let Err(error) = child.wait() {
        log::warn!("codex CLI child wait failed: {error}");
    }
}

fn run(cli: &Path, args: &[&str]) -> Result<serde_json::Value, String> {
    let mut child = Command::new(cli)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "codex_cli_start_failed")?;
    let mut stdout = child.stdout.take().ok_or("codex_cli_stdout_missing")?;
    // Drain concurrently to prevent a full pipe from blocking the child; discard oversized output.
    let reader = thread::spawn(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut oversized = false;
        loop {
            let count = stdout
                .read(&mut buffer)
                .map_err(|_| "codex_cli_read_failed")?;
            if count == 0 {
                break;
            }
            if bytes.len() + count <= 4 * 1024 * 1024 {
                bytes.extend_from_slice(&buffer[..count]);
            } else {
                oversized = true;
            }
        }
        if oversized {
            Err("codex_cli_output_too_large")
        } else {
            Ok(bytes)
        }
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let result = loop {
        match child.try_wait() {
            Ok(Some(exit)) => {
                break if exit.success() {
                    Ok(())
                } else {
                    Err("codex_cli_command_failed")
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(30)),
            Ok(None) => {
                stop_cli_child(&mut child);
                break Err("codex_cli_timed_out");
            }
            Err(_) => {
                stop_cli_child(&mut child);
                break Err("codex_cli_wait_failed");
            }
        }
    };
    let bytes = reader.join().map_err(|_| "codex_cli_reader_failed")??;
    result?;
    serde_json::from_slice(&bytes).map_err(|_| "codex_cli_json_invalid".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_desktop_discovery_works_without_inherited_path_and_prefers_newest() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("OpenAI/Codex/bin");
        let mut expected = Vec::new();
        for (index, generation) in ["aaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbb"].iter().enumerate() {
            let directory = bin.join(generation);
            fs::create_dir_all(&directory).unwrap();
            let path = directory.join("codex.exe");
            let file = fs::File::create(&path).unwrap();
            file.set_times(
                fs::FileTimes::new()
                    .set_modified(UNIX_EPOCH + Duration::from_secs(index as u64 + 1)),
            )
            .unwrap();
            expected.push(path);
        }
        fs::create_dir_all(bin.join("unrelated")).unwrap();
        fs::write(bin.join("unrelated/codex.exe"), b"ignore").unwrap();
        fs::create_dir_all(bin.join("cccccccccccccccc/codex.exe")).unwrap();
        expected.reverse();
        assert_eq!(windows_desktop_cli_candidates(temp.path()), expected);
        assert!(windows_desktop_cli_candidates(&temp.path().join("missing")).is_empty());
    }

    #[test]
    fn an_enabled_valid_old_generation_is_not_current_compatibility_evidence() {
        let mut old = status(PluginState::Installed, Verification::Fresh, None);
        let previous = format!("0.6.28+codex.{}", "a".repeat(64));
        let current = format!("0.6.28+codex.{}", "b".repeat(64));
        old.installed_version = Some(previous.clone());
        old.enabled = Some(true);
        let stale = check_generation(old.clone(), std::slice::from_ref(&current));
        assert_eq!(stale.state, PluginState::Unavailable);
        assert_eq!(
            stale.diagnostic_code.as_deref(),
            Some("codex_plugin_update_required")
        );
        assert_eq!(stale.installed_version, old.installed_version);
        assert_eq!(
            check_generation(old, &[current, previous]).state,
            PluginState::Installed
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn desktop_discovery_includes_current_codex_bundle_layout() {
        let candidates = codex_cli_candidates();
        let current =
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex");
        assert!(
            candidates.contains(&current),
            "current bundled CLI is omitted from discovery"
        );
        let home = dirs::home_dir().unwrap();
        assert!(candidates.contains(
            &home.join("Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex")
        ));
    }

    #[test]
    fn desktop_discovery_uses_the_customers_application_directory() {
        let customer = tempfile::tempdir().unwrap();
        let applications = customer.path().join("Applications");
        let cli = applications.join("Codex.app/Contents/Resources/codex-cli/bin/codex");
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, b"fixture").unwrap();
        let discovered = desktop_cli_candidates(&[applications])
            .into_iter()
            .find(|path| path.is_file());
        assert_eq!(discovered, Some(cli));
        assert!(
            desktop_cli_candidates(&[customer.path().join("empty")])
                .into_iter()
                .all(|path| !path.is_file())
        );
    }

    #[test]
    fn cached_status_flags_a_missing_or_relocated_marketplace_registration() {
        let temp = tempfile::tempdir().unwrap();
        let plugin_root = temp
            .path()
            .join("home/.local/share/onecrew_lite/codex-plugin");
        let home = temp.path().join("home/.codex");
        fs::create_dir_all(&home).unwrap();
        let canonical = marketplace_root_for(&plugin_root, &home).unwrap();
        fs::create_dir_all(canonical.join(NAME)).unwrap();
        let mut receipt = status(PluginState::Installed, Verification::Fresh, None);
        receipt.installed_version = Some(format!("0.6.1+codex.{}", "a".repeat(64)));
        receipt.enabled = Some(true);
        receipt.last_verified_at = Some(123);
        save(&canonical.join(NAME), &receipt).unwrap();
        let receipt_bytes = fs::read(canonical.join(NAME).join("status.json")).unwrap();
        let index = |root: &Path| root.join(".agents/plugins/marketplace.json");
        let register = |source_type: &str, source: &Path| {
            fs::write(
                home.join("config.toml"),
                format!(
                    "[marketplaces.onecrew_lite-cli]\nsource_type = '{source_type}'\nsource = '{}'\n",
                    source.display()
                ),
            )
            .unwrap();
        };
        let unchanged = |value: PluginStatus| {
            assert_eq!(value.state, PluginState::Installed);
            assert_eq!(value.verification, Verification::Cached);
            assert_eq!(value.diagnostic_code, None);
            assert!(!value.requires_marketplace_repair());
        };
        let repair = |value: PluginStatus, code: &str| {
            assert_eq!(value.state, PluginState::Unavailable);
            assert_eq!(value.verification, Verification::Cached);
            assert_eq!(value.last_verified_at, Some(123));
            assert_eq!(value.installed_version, None);
            assert_eq!(value.enabled, None);
            assert_eq!(value.diagnostic_code.as_deref(), Some(code));
            assert!(value.requires_marketplace_repair());
        };

        // Not registered.
        unchanged(read_cached(&plugin_root, &home).unwrap());
        fs::write(home.join("config.toml"), "model = 'example'\n").unwrap();
        unchanged(read_cached(&plugin_root, &home).unwrap());

        // Registered at this plugin root's marketplace, which has its index.
        fs::create_dir_all(index(&canonical).parent().unwrap()).unwrap();
        fs::write(index(&canonical), "{}").unwrap();
        register("local", &canonical);
        unchanged(read_cached(&plugin_root, &home).unwrap());

        // The registered source lost its index, or no longer exists.
        fs::remove_file(index(&canonical)).unwrap();
        repair(
            read_cached(&plugin_root, &home).unwrap(),
            "codex_marketplace_source_missing",
        );
        register("local", &temp.path().join("gone"));
        repair(
            read_cached(&plugin_root, &home).unwrap(),
            "codex_marketplace_source_missing",
        );

        // A loadable registration elsewhere, such as the retired cache location.
        let retired = temp
            .path()
            .join("home/Library/Caches/io.onecrewai.onecrew_lite/cli-plugin")
            .join("c".repeat(64));
        fs::create_dir_all(index(&retired).parent().unwrap()).unwrap();
        fs::write(index(&retired), "{}").unwrap();
        register("local", &retired);
        repair(
            read_cached(&plugin_root, &home).unwrap(),
            "codex_marketplace_source_relocated",
        );
        register("git", &canonical);
        repair(
            read_cached(&plugin_root, &home).unwrap(),
            "codex_marketplace_source_relocated",
        );

        // The OS purged the retired cache while Codex still references it.
        fs::remove_dir_all(temp.path().join("home/Library/Caches")).unwrap();
        register("local", &retired);
        repair(
            read_cached(&plugin_root, &home).unwrap(),
            "codex_marketplace_source_missing",
        );

        // A config status cannot parse is left to explicit install to report.
        fs::write(home.join("config.toml"), "invalid TOML").unwrap();
        unchanged(read_cached(&plugin_root, &home).unwrap());
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).unwrap(),
            "invalid TOML"
        );
        assert_eq!(
            fs::read(canonical.join(NAME).join("status.json")).unwrap(),
            receipt_bytes
        );
    }

    #[test]
    fn marketplaces_live_in_the_given_plugin_root_scoped_to_codex_home_identity() {
        let plugin_root = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let marketplace = marketplace_root_for(plugin_root.path(), first.path()).unwrap();
        assert_eq!(marketplace.parent(), Some(plugin_root.path()));
        assert_eq!(marketplace.file_name().unwrap().len(), 64);
        assert_ne!(
            marketplace,
            marketplace_root_for(plugin_root.path(), second.path()).unwrap()
        );
        assert_eq!(
            marketplace,
            marketplace_root_for(plugin_root.path(), &first.path().join(".")).unwrap()
        );
    }

    #[test]
    fn install_repairs_a_registration_left_at_the_retired_cache_location() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home/.codex");
        fs::create_dir_all(&home).unwrap();
        let identity = "c".repeat(64);
        let retired = temp
            .path()
            .join("home/Library/Caches/io.onecrewai.onecrew_lite/cli-plugin")
            .join(&identity);
        let retired_manifest = retired.join(".agents/plugins/marketplace.json");
        fs::create_dir_all(retired_manifest.parent().unwrap()).unwrap();
        fs::write(&retired_manifest, "previous generation index").unwrap();
        let config = format!(
            "[marketplaces.onecrew_lite-cli]\nsource_type = 'local'\nsource = '{}'\n",
            retired.display()
        );
        fs::write(home.join("config.toml"), &config).unwrap();
        let canonical = temp
            .path()
            .join("home/.local/share/onecrew_lite/codex-plugin")
            .join(&identity);
        fs::create_dir_all(canonical.join(".agents/plugins")).unwrap();
        fs::write(canonical.join(".agents/plugins/marketplace.json"), "{}").unwrap();
        let package = PluginPackage {
            marketplace_root: canonical.clone(),
            plugin_root: canonical.join("onecrew/0.6.1+codex.test/plugins/onecrew"),
            version: "0.6.1+codex.test".into(),
            target: "aarch64-apple-darwin".into(),
            binary_sha256: String::new(),
        };
        let mut calls = Vec::new();
        let result = install_with(&package, &home, |args| {
            calls.push(args.join(" "));
            Ok(serde_json::json!({"installed": []}))
        });
        assert_eq!(result.unwrap_err(), "plugin_install_readback_mismatch");
        assert_eq!(
            calls,
            [
                "plugin marketplace remove onecrew_lite-cli --json".to_owned(),
                format!("plugin marketplace add {} --json", canonical.display()),
                "plugin add onecrew_lite@onecrew_lite-cli --json".to_owned(),
                "plugin list --marketplace onecrew_lite-cli --json".to_owned(),
            ]
        );
        // Repair re-registers through Codex only; the retired tree stays untouched.
        assert_eq!(
            fs::read_to_string(&retired_manifest).unwrap(),
            "previous generation index"
        );
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).unwrap(),
            config
        );
    }

    #[test]
    fn cache_is_only_receipt_io_and_marks_old_evidence_cached() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(read_cache(temp.path()).unwrap().state, PluginState::Unknown);
        let mut value = status(PluginState::Installed, Verification::Fresh, None);
        value.last_verified_at = Some(123);
        value.installed_version = Some(format!("0.5.18+codex.{}", "a".repeat(64)));
        value.enabled = Some(true);
        save(temp.path(), &value).unwrap();
        let cached = read_cache(temp.path()).unwrap();
        assert_eq!(cached.verification, Verification::Cached);
        assert_eq!(cached.last_verified_at, Some(123));
        fs::rename(
            temp.path().join("status.json"),
            temp.path().join("status.previous.json"),
        )
        .unwrap();
        assert_eq!(
            read_cache(temp.path()).unwrap().state,
            PluginState::Installed
        );
    }

    #[test]
    fn malformed_or_oversized_receipts_return_safe_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("status.json");
        let value = status(PluginState::Installed, Verification::Fresh, None);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            read_cache(temp.path()).unwrap().state,
            PluginState::Unavailable
        );
        let value = status(
            PluginState::Unavailable,
            Verification::Fresh,
            Some("private injected text"),
        );
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            read_cache(temp.path()).unwrap().diagnostic_code.as_deref(),
            Some("plugin_receipt_invalid")
        );
        fs::write(&path, vec![b' '; 8193]).unwrap();
        assert_eq!(
            read_cache(temp.path()).unwrap().state,
            PluginState::Unavailable
        );
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert_eq!(
            read_cache(temp.path()).unwrap().state,
            PluginState::Unavailable
        );
    }

    #[cfg(unix)]
    #[test]
    fn receipt_and_recovery_symlinks_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("external");
        fs::write(&target, b"never read this").unwrap();
        for name in ["status.json", "status.previous.json"] {
            let path = temp.path().join(name);
            std::os::unix::fs::symlink(&target, &path).unwrap();
            assert_eq!(
                read_cache(temp.path()).unwrap().state,
                PluginState::Unavailable
            );
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn failed_install_stops_before_add_and_failed_readback_is_not_success() {
        let home = tempfile::tempdir().unwrap();
        let package = PluginPackage {
            marketplace_root: PathBuf::from("/test"),
            plugin_root: PathBuf::from("/test/plugins/onecrew"),
            version: "0.5.18+codex.test".into(),
            target: "aarch64-apple-darwin".into(),
            binary_sha256: String::new(),
        };
        let mut calls = 0;
        assert!(
            install_with(&package, home.path(), |_| {
                calls += 1;
                Err("codex_cli_command_failed".into())
            })
            .is_err()
        );
        assert_eq!(calls, 1);
        let mut calls = 0;
        assert!(
            install_with(&package, home.path(), |_| {
                calls += 1;
                Ok(serde_json::json!({"installed":[]}))
            })
            .is_err()
        );
        assert_eq!(calls, 3);
    }

    #[test]
    fn legacy_plugin_guard_stops_before_any_mutation() {
        let home = tempfile::tempdir().unwrap();
        let package = PluginPackage {
            marketplace_root: PathBuf::from("/test"),
            plugin_root: PathBuf::from("/test/plugins/onecrew"),
            version: "0.5.22+codex.test".into(),
            target: "aarch64-apple-darwin".into(),
            binary_sha256: String::new(),
        };
        for marketplace in ["onecrew_lite-cli", "onecrew_lite-local"] {
            let path = home.path().join("config.toml");
            let config =
                format!("[plugins.\"onecrew_lite-local@{marketplace}\"]\nenabled = true\n");
            fs::write(&path, &config).unwrap();
            let marketplace = home.path().join("marketplace");
            let index = marketplace.join(".agents/plugins/marketplace.json");
            fs::create_dir_all(index.parent().unwrap()).unwrap();
            fs::write(&index, "existing old plugin index").unwrap();
            assert_eq!(
                prepare_install(Path::new("/unused"), home.path(), &marketplace, None).unwrap_err(),
                "plugin_identity_change_required"
            );
            assert_eq!(
                fs::read_to_string(index).unwrap(),
                "existing old plugin index"
            );
            let result = install_with(&package, home.path(), |_| {
                panic!("identity guard must stop before any Codex command")
            });
            assert_eq!(result.unwrap_err(), "plugin_identity_change_required");
            assert_eq!(fs::read_to_string(path).unwrap(), config);
        }
    }

    #[test]
    fn explicit_retirement_removes_only_known_names_and_verifies_each_readback() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config.toml");
        let unrelated = "model = 'kept'\n[plugins.'other@market']\nenabled = true\n";
        let first = "[plugins.'onecrew_lite-local@onecrew_lite-cli']\nenabled = true\n";
        let second = "[plugins.'onecrew_lite-local@onecrew_lite-local']\nenabled = false\n";
        fs::write(&path, format!("{unrelated}{first}{second}")).unwrap();
        let mut calls = 0;
        retire_legacy_plugins(home.path(), |args| {
            assert_eq!(
                args,
                &["plugin", "remove", RETIRED_PLUGINS[calls], "--json"]
            );
            calls += 1;
            fs::write(
                &path,
                if calls == 1 {
                    format!("{unrelated}{second}")
                } else {
                    unrelated.into()
                },
            )
            .unwrap();
            Ok(serde_json::json!({}))
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(fs::read_to_string(&path).unwrap(), unrelated);
        retire_legacy_plugins(home.path(), |_| {
            panic!("repeated install must not remove other plugins")
        })
        .unwrap();
    }

    #[test]
    fn failed_or_unverified_retirement_stops_before_another_command() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config.toml");
        fs::write(&path, "[plugins.'onecrew_lite-local@onecrew_lite-cli']\nenabled = true\n[plugins.'onecrew_lite-local@onecrew_lite-local']\nenabled = true\n").unwrap();
        for result in [Err("command_failed".into()), Ok(serde_json::json!({}))] {
            let mut calls = 0;
            assert_eq!(
                retire_legacy_plugins(home.path(), |_| {
                    calls += 1;
                    result.clone()
                })
                .unwrap_err(),
                "plugin_identity_change_required"
            );
            assert_eq!(calls, 1);
        }
    }

    #[test]
    fn identity_guard_accepts_disabled_or_absent_legacy_plugins_and_rejects_invalid_config() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config.toml");
        assert!(!plugin_preflight(home.path(), Path::new("/test")).unwrap());
        for config in [
            "model = 'example'",
            "[plugins.'onecrew@onecrew-cli']\nenabled = true",
            "[plugins.'onecrew-local@onecrew-cli']\nenabled = true",
            "[plugins.'onecrew_lite@onecrew_lite-cli']\nenabled = true",
            "[plugins.'onecrew_lite-local@unrelated']\nenabled = true",
            "[plugins.'onecrew_lite-local@onecrew_lite-cli']\nenabled = false",
            "[plugins.'onecrew_lite-local@onecrew_lite-local']\nenabled = false",
        ] {
            fs::write(&path, config).unwrap();
            assert!(
                !plugin_preflight(home.path(), Path::new("/test")).unwrap(),
                "{config}"
            );
        }
        fs::write(&path, "[plugins.'onecrew_lite-local@onecrew_lite-cli']").unwrap();
        assert_eq!(
            plugin_preflight(home.path(), Path::new("/test")).unwrap_err(),
            "plugin_identity_change_required"
        );
        for config in [
            "invalid TOML",
            "[plugins.'onecrew_lite-local@onecrew_lite-cli']\nenabled = 'true'",
        ] {
            fs::write(&path, config).unwrap();
            assert_eq!(
                plugin_preflight(home.path(), Path::new("/test")).unwrap_err(),
                "codex_plugin_config_invalid"
            );
        }
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert_eq!(
            plugin_preflight(home.path(), Path::new("/test")).unwrap_err(),
            "codex_plugin_config_read_failed"
        );
    }

    #[test]
    fn missing_registered_source_does_not_block_install_or_hide_failed_readback() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("codex");
        fs::create_dir(&home).unwrap();
        let config =
            "[marketplaces.onecrew_lite-cli]\nsource_type = 'local'\nsource = '/missing'\n";
        fs::write(home.join("config.toml"), config).unwrap();
        let stale_root = temp.path().join("missing");
        let current_root = temp.path().join("current marketplace");
        fs::create_dir_all(current_root.join(".agents/plugins")).unwrap();
        fs::write(
            current_root.join(".agents/plugins/marketplace.json"),
            serde_json::to_vec(&serde_json::json!({"name": MARKETPLACE, "plugins": []})).unwrap(),
        )
        .unwrap();
        let package = PluginPackage {
            marketplace_root: current_root.clone(),
            plugin_root: current_root.join("plugins/onecrew"),
            version: "0.5.22+codex.test".into(),
            target: "aarch64-apple-darwin".into(),
            binary_sha256: String::new(),
        };
        let mut registered = stale_root.clone();
        let mut added = false;
        let result = install_with(&package, &home, |args| match args {
            [
                "plugin",
                "marketplace",
                "remove",
                "onecrew_lite-cli",
                "--json",
            ] => {
                assert_eq!(registered, stale_root);
                registered = PathBuf::new();
                Ok(serde_json::json!({}))
            }
            ["plugin", "marketplace", "add", root, "--json"] => {
                assert!(registered.as_os_str().is_empty());
                registered = PathBuf::from(root);
                assert_eq!(registered, current_root);
                assert!(
                    registered
                        .join(".agents/plugins/marketplace.json")
                        .is_file()
                );
                Ok(serde_json::json!({}))
            }
            ["plugin", "add", "onecrew_lite@onecrew_lite-cli", "--json"] => {
                assert_eq!(registered, current_root);
                added = true;
                Ok(serde_json::json!({}))
            }
            [
                "plugin",
                "list",
                "--marketplace",
                "onecrew_lite-cli",
                "--json",
            ] => {
                assert!(added);
                Ok(serde_json::json!({"installed": []}))
            }
            _ => Err("codex_cli_command_failed".into()),
        });
        assert!(added);
        assert_eq!(result.unwrap_err(), "plugin_install_readback_mismatch");
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).unwrap(),
            config
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires the native Codex CLI; installs only in a temporary CODEX_HOME"]
    fn native_codex_repairs_missing_source_and_reads_back_installed_generation() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("codex");
        fs::create_dir(&home).unwrap();
        let config_path = home.join("config.toml");
        fs::write(
            &config_path,
            "[marketplaces.onecrew_lite-cli]\nsource_type = 'local'\nsource = '/missing'\n",
        )
        .unwrap();
        let root = temp.path().join("marketplace with spaces");
        let _marketplace_lease = package::lease(&root).unwrap();
        let _namespace_lease = package::lease(&root.join(NAME)).unwrap();
        let executable = std::env::current_exe().unwrap();
        let package = prepare_install(&executable, &home, &root, None).unwrap();
        assert_eq!(package.marketplace_root, root);
        assert!(package.plugin_root.starts_with(root.join(NAME)));
        let cli = resolve_codex_cli(None).unwrap();
        let command = |args: &[&str]| {
            let output = Command::new(&cli)
                .args(args)
                .env("CODEX_HOME", &home)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            if !output.status.success() {
                return Err("codex_cli_command_failed".into());
            }
            serde_json::from_slice(&output.stdout).map_err(|_| "codex_cli_json_invalid".into())
        };
        assert!(command(&["plugin", "list", "--json"]).is_err());
        let installed = install_with(&package, &home, command).unwrap();
        assert_eq!(installed.state, PluginState::Installed);
        assert_eq!(installed.verification, Verification::Fresh);
        assert_eq!(installed.enabled, Some(true));
        assert_eq!(
            installed.installed_version.as_deref(),
            Some(package.version.as_str())
        );

        // Simulate the erroneous name-scoped registration, then repair it once.
        let wrong_root = package::registration_root(&root.join(NAME), &package).unwrap();
        command(&["plugin", "marketplace", "remove", MARKETPLACE, "--json"]).unwrap();
        command(&[
            "plugin",
            "marketplace",
            "add",
            wrong_root.to_str().unwrap(),
            "--json",
        ])
        .unwrap();
        let mut removals = 0;
        let repaired = install_with(&package, &home, |args| {
            if args.starts_with(&["plugin", "marketplace", "remove"]) {
                removals += 1;
            }
            command(args)
        })
        .unwrap();
        assert_eq!(repaired.state, PluginState::Installed);
        assert_eq!(removals, 1);

        // A new immutable generation keeps the registered root and never removes it.
        let replacement =
            prepare_install(&executable, &home, &root, Some(&temp.path().join("data"))).unwrap();
        assert_ne!(package.version, replacement.version);
        assert_eq!(replacement.marketplace_root, root);
        let replaced = install_with(&replacement, &home, |args| {
            assert!(!args.starts_with(&["plugin", "marketplace", "remove"]));
            command(args)
        })
        .unwrap();
        assert_eq!(replaced.state, PluginState::Installed);
        assert_eq!(
            replaced.installed_version.as_deref(),
            Some(replacement.version.as_str())
        );
        assert_eq!(replaced.enabled, Some(true));
        assert!(!plugin_preflight(&home, &root).unwrap());

        // A retired identity absent from the new marketplace still blocks reinstall.
        let mut config = fs::read_to_string(&config_path).unwrap();
        config.push_str("\n[plugins.'onecrew_lite-local@onecrew_lite-cli']\nenabled = true\n");
        fs::write(&config_path, &config).unwrap();
        assert_eq!(
            install_with(&package, &home, |_| panic!("must not invoke Codex")).unwrap_err(),
            "plugin_identity_change_required"
        );
        assert_eq!(fs::read_to_string(config_path).unwrap(), config);
    }

    #[test]
    fn canonical_receipt_namespace_never_reads_legacy_receipt() {
        let plugin_root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let marketplace = marketplace_root_for(plugin_root.path(), home.path()).unwrap();
        fs::create_dir_all(&marketplace).unwrap();
        let mut legacy = status(PluginState::Installed, Verification::Fresh, None);
        legacy.installed_version = Some(format!("0.5.22+codex.{}", "a".repeat(64)));
        legacy.enabled = Some(true);
        legacy.last_verified_at = Some(123);
        save(&marketplace, &legacy).unwrap();
        let original = fs::read(marketplace.join("status.json")).unwrap();
        assert_eq!(
            read_cached(plugin_root.path(), home.path()).unwrap().state,
            PluginState::Unknown
        );
        assert_eq!(fs::read(marketplace.join("status.json")).unwrap(), original);
    }

    #[test]
    fn readback_ignores_other_marketplaces_and_rejects_unknown_schema() {
        assert!(readback(&serde_json::json!([]), Path::new("/unused")).is_err());
        let list = serde_json::json!({"installed":[{"name":NAME,"marketplaceName":"onecrew_lite-local","installed":true}]});
        assert_eq!(
            readback(&list, Path::new("/unused")).unwrap().state,
            PluginState::NotInstalled
        );
    }
}
