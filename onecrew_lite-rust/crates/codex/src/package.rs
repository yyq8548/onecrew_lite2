use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) const NAME: &str = "onecrew_lite";
pub(crate) const MARKETPLACE: &str = "onecrew_lite-cli";

#[derive(Debug, Clone, Serialize)]
pub struct PluginPackage {
    pub marketplace_root: PathBuf,
    pub plugin_root: PathBuf,
    pub version: String,
    pub target: String,
    pub binary_sha256: String,
}

macro_rules! asset {
    ($path:literal) => {
        (
            $path,
            include_bytes!(concat!(
                "../../../integrations/codex-marketplace/plugins/onecrew/",
                $path
            ))
            .as_slice(),
        )
    };
}

fn assets() -> BTreeMap<String, Vec<u8>> {
    [
        asset!(".codex-plugin/plugin.json"),
        asset!(".mcp.json"),
        asset!("assets/onecrew-favicon.svg"),
        asset!("skills/onecrew/SKILL.md"),
        asset!("skills/onecrew/references/campaign-discovery.md"),
        asset!("skills/onecrew/references/match-evaluation.md"),
        asset!("skills/onecrew/references/onecrew-writes.md"),
        asset!("skills/onecrew/references/toolkit-connections.md"),
    ]
    .into_iter()
    .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
    .collect()
}

pub(crate) fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn host_target() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        _ => Err("unsupported_host".into()),
    }
}

pub(crate) fn binary_name(target: &str) -> &'static str {
    if target.ends_with("windows-msvc") {
        "onecrew_lite.exe"
    } else {
        "onecrew_lite"
    }
}

fn validate_binary(bytes: &[u8], target: &str) -> Result<(), String> {
    let valid = match target {
        "aarch64-apple-darwin" | "x86_64-apple-darwin" => {
            let cpu = if target.starts_with("aarch64") {
                0x0100000cu32
            } else {
                0x01000007
            };
            bytes.len() >= 32
                && bytes[..4] == [0xcf, 0xfa, 0xed, 0xfe]
                && bytes[4..8] == cpu.to_le_bytes()
                && bytes[12..16] == 2u32.to_le_bytes()
        }
        "x86_64-pc-windows-msvc" => {
            if bytes.len() < 64 || &bytes[..2] != b"MZ" {
                false
            } else {
                let offset =
                    u32::from_le_bytes(bytes[60..64].try_into().map_err(|_| "invalid_executable")?)
                        as usize;
                bytes.get(offset..offset.saturating_add(6)) == Some(b"PE\0\0\x64\x86".as_slice())
                    && bytes.get(offset.saturating_add(24)..offset.saturating_add(26))
                        == Some([0x0b, 0x02].as_slice())
            }
        }
        _ => return Err("unsupported_target".into()),
    };
    if valid {
        Ok(())
    } else {
        Err("executable_target_mismatch".into())
    }
}

pub(crate) fn lease(root: &Path) -> Result<fs::File, String> {
    fs::create_dir_all(root).map_err(|_| "plugin_cache_create_failed")?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("generation.lock"))
        .map_err(|_| "plugin_lock_open_failed")?;
    fs2::FileExt::lock_exclusive(&file).map_err(|_| "plugin_lock_failed")?;
    Ok(file)
}

/// Publish an immutable, complete marketplace generation. Old generations remain intact.
pub fn package_plugin(
    executable: &Path,
    target: &str,
    destination: &Path,
) -> Result<PluginPackage, String> {
    let _lease = lease(destination)?;
    package_locked(executable, target, destination)
}

pub(crate) fn package_locked(
    executable: &Path,
    target: &str,
    destination: &Path,
) -> Result<PluginPackage, String> {
    package_locked_scoped(executable, target, destination, None)
}

pub(crate) fn mcp_args(data_dir: Option<&Path>) -> Result<Vec<String>, String> {
    let mut args = vec!["mcp".to_owned(), "--stdio".to_owned()];
    if let Some(path) = data_dir {
        if !path.is_absolute() {
            return Err("data_directory_must_be_absolute".into());
        }
        args.push("--data-dir".to_owned());
        args.push(path.to_str().ok_or("data_directory_not_utf8")?.to_owned());
    }
    Ok(args)
}

/// Calculate the exact immutable package identity without writing a cache or invoking Codex.
pub fn expected_plugin_version(
    executable: &Path,
    target: &str,
    data_dir: Option<&Path>,
) -> Result<String, String> {
    let binary = fs::read(executable).map_err(|_| "executable_read_failed")?;
    validate_binary(&binary, target)?;
    generation(binary, target, data_dir).map(|(_, version)| version)
}

pub(crate) fn package_locked_scoped(
    executable: &Path,
    target: &str,
    destination: &Path,
    data_dir: Option<&Path>,
) -> Result<PluginPackage, String> {
    let binary = fs::read(executable).map_err(|_| "executable_read_failed")?;
    validate_binary(&binary, target)?;
    let binary_sha256 = sha(&binary);
    let (files, version) = generation(binary, target, data_dir)?;
    let root = destination.join(&version);
    let pending = destination.join("pending");
    // A crashed writer can leave only an unpublished temporary tree; published trees are immutable.
    if pending.exists() {
        fs::remove_dir_all(&pending).map_err(|_| "plugin_pending_cleanup_failed")?;
    }
    if root.exists() {
        verify_files(&root.join("plugins").join(NAME), &files)?;
    } else {
        let plugin = pending.join("plugins").join(NAME);
        for (path, bytes) in &files {
            let output = plugin.join(path);
            fs::create_dir_all(output.parent().ok_or("plugin_path_invalid")?)
                .map_err(|_| "plugin_directory_failed")?;
            fs::write(&output, bytes).map_err(|_| "plugin_file_write_failed")?;
        }
        fs::set_permissions(
            plugin.join("bin").join(binary_name(target)),
            fs::metadata(executable)
                .map_err(|_| "executable_metadata_failed")?
                .permissions(),
        )
        .map_err(|_| "plugin_permissions_failed")?;
        let marketplace = serde_json::json!({"name":MARKETPLACE,"interface":{"displayName":"OneCrew CLI"},"plugins":[{"name":NAME,"source":{"source":"local","path":format!("./plugins/{NAME}")},"policy":{"installation":"AVAILABLE","authentication":"ON_INSTALL"},"category":"Productivity"}]});
        fs::create_dir_all(pending.join(".agents/plugins"))
            .map_err(|_| "plugin_directory_failed")?;
        fs::write(
            pending.join(".agents/plugins/marketplace.json"),
            serde_json::to_vec_pretty(&marketplace).map_err(|_| "plugin_encoding_failed")?,
        )
        .map_err(|_| "marketplace_write_failed")?;
        fs::rename(&pending, &root).map_err(|_| "plugin_publish_failed")?;
    }
    Ok(PluginPackage {
        plugin_root: root.join("plugins").join(NAME),
        marketplace_root: root,
        version,
        target: target.into(),
        binary_sha256,
    })
}

fn generation(
    binary: Vec<u8>,
    target: &str,
    data_dir: Option<&Path>,
) -> Result<(BTreeMap<String, Vec<u8>>, String), String> {
    let mut files = assets();
    let manifest = files
        .get_mut(".codex-plugin/plugin.json")
        .ok_or("plugin_manifest_missing")?;
    let value: serde_json::Value =
        serde_json::from_slice(manifest).map_err(|_| "plugin_manifest_invalid")?;
    *manifest = serde_json::to_vec_pretty(&value).map_err(|_| "plugin_encoding_failed")?;
    let mut connection = serde_json::json!({"command": format!("./bin/{}", binary_name(target)), "cwd": ".", "args": mcp_args(data_dir)?});
    if target.ends_with("windows-msvc") {
        // Windows Rust Command resolves a relative executable before current_dir.
        // Codex resolves cwd to the installed plugin root; cmd resolves this fixed
        // launcher after entering it. No customer path is evaluated as shell code.
        connection = serde_json::json!({"command": "cmd.exe", "cwd": ".", "args": ["/d", "/v:off", "/c", ".\\bin\\onecrew_lite-mcp.cmd"]});
        let scoped = if let Some(data) = data_dir {
            let data = data.to_str().ok_or("data_directory_not_utf8")?;
            if data.contains(['"', '\r', '\n']) {
                return Err("data_directory_invalid_windows_path".into());
            }
            connection["env"] = serde_json::json!({"ONECREW_LITE_MCP_DATA_DIR": data});
            " --data-dir \"%ONECREW_LITE_MCP_DATA_DIR%\""
        } else {
            ""
        };
        files.insert("bin/onecrew_lite-mcp.cmd".into(), format!("@echo off\r\nsetlocal DisableDelayedExpansion\r\n\"%~dp0onecrew_lite.exe\" mcp --stdio{scoped}\r\nexit /b %errorlevel%\r\n").into_bytes());
    }
    files.insert(
        ".mcp.json".into(),
        serde_json::to_vec_pretty(&serde_json::json!({"mcpServers": {"onecrew_lite": connection}}))
            .map_err(|_| "plugin_encoding_failed")?,
    );
    files.insert(format!("bin/{}", binary_name(target)), binary);
    let base = value["version"].as_str().ok_or("plugin_manifest_invalid")?;
    let version = digest_version(&files, base);
    let manifest = files
        .get_mut(".codex-plugin/plugin.json")
        .ok_or("plugin_manifest_missing")?;
    let mut value: serde_json::Value =
        serde_json::from_slice(manifest).map_err(|_| "plugin_manifest_invalid")?;
    value["version"] = version.clone().into();
    *manifest = serde_json::to_vec_pretty(&value).map_err(|_| "plugin_encoding_failed")?;
    Ok((files, version))
}

/// Codex registers a marketplace by its source path. Keep that path stable
/// while selecting an immutable, content-addressed plugin generation inside it.
pub(crate) fn registration_root(root: &Path, package: &PluginPackage) -> Result<PathBuf, String> {
    use std::io::Write;
    let relative = package
        .plugin_root
        .strip_prefix(root)
        .map_err(|_| "plugin_path_invalid")?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("plugin_path_invalid".into());
    }
    let source = format!(
        "./{}",
        relative
            .to_str()
            .ok_or("plugin_path_invalid")?
            .replace('\\', "/")
    );
    let directory = root.join(".agents/plugins");
    fs::create_dir_all(&directory).map_err(|_| "plugin_directory_failed")?;
    let manifest = serde_json::json!({
        "name": MARKETPLACE,
        "interface": {"displayName": "OneCrew CLI"},
        "plugins": [{
            "name": NAME,
            "source": {"source": "local", "path": source},
            "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
            "category": "Productivity"
        }]
    });
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| "plugin_encoding_failed")?;
    let mut pending =
        tempfile::NamedTempFile::new_in(&directory).map_err(|_| "marketplace_write_failed")?;
    pending
        .write_all(&bytes)
        .map_err(|_| "marketplace_write_failed")?;
    pending
        .as_file()
        .sync_all()
        .map_err(|_| "marketplace_write_failed")?;
    pending
        .persist(directory.join("marketplace.json"))
        .map_err(|_| "marketplace_write_failed")?;
    Ok(root.to_owned())
}

pub(crate) fn verify_installed(root: &Path, version: &str, target: &str) -> Result<(), String> {
    let binary = fs::read(root.join("bin").join(binary_name(target)))
        .map_err(|_| "installed_binary_missing")?;
    validate_binary(&binary, target)?;
    let mut files = BTreeMap::new();
    read_tree(root, root, &mut files)?;
    let manifest = files
        .get_mut(".codex-plugin/plugin.json")
        .ok_or("installed_manifest_missing")?;
    let mut value: serde_json::Value =
        serde_json::from_slice(manifest).map_err(|_| "installed_manifest_invalid")?;
    if value["name"] != NAME || value["version"] != version {
        return Err("installed_manifest_mismatch".into());
    }
    let base = version_base(version).ok_or("codex_plugin_version_invalid")?;
    value["version"] = base.into();
    *manifest = serde_json::to_vec_pretty(&value).map_err(|_| "plugin_encoding_failed")?;
    if digest_version(&files, base) != version {
        return Err("installed_content_digest_mismatch".into());
    }
    Ok(())
}

pub(crate) fn version_base(version: &str) -> Option<&str> {
    let (base, digest) = version.split_once("+codex.")?;
    let parts: Vec<_> = base.split('.').collect();
    (parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && (part.len() == 1 || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
        })
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then_some(base)
}

fn digest_version(files: &BTreeMap<String, Vec<u8>>, base: &str) -> String {
    let mut digest = Sha256::new();
    for (path, bytes) in files {
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    format!("{base}+codex.{:x}", digest.finalize())
}

fn read_tree(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|_| "installed_tree_read_failed")? {
        let entry = entry.map_err(|_| "installed_tree_read_failed")?;
        let kind = entry
            .file_type()
            .map_err(|_| "installed_tree_read_failed")?;
        if kind.is_symlink() {
            return Err("installed_tree_symlink".into());
        }
        if kind.is_dir() {
            read_tree(root, &entry.path(), files)?;
        } else if kind.is_file() {
            let path = entry.path();
            let name = path
                .strip_prefix(root)
                .map_err(|_| "installed_tree_path_invalid")?
                .to_str()
                .ok_or("installed_tree_path_invalid")?
                .replace('\\', "/");
            files.insert(
                name,
                fs::read(path).map_err(|_| "installed_tree_read_failed")?,
            );
        } else {
            return Err("installed_tree_file_invalid".into());
        }
    }
    Ok(())
}

fn verify_files(root: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let mut actual = BTreeMap::new();
    read_tree(root, root, &mut actual)?;
    if actual != *files {
        return Err("existing_package_modified".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_path_stays_stable_across_namespaces_and_generations() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("onecrew_lite");
        let root = temp.path().join("marketplace");
        fs::create_dir_all(&root).unwrap();
        fs::write(&binary, macho(0x0100000c)).unwrap();
        let first = package_locked_scoped(
            &binary,
            "aarch64-apple-darwin",
            &root.join("old-name"),
            None,
        )
        .unwrap();
        let registered = registration_root(&root, &first).unwrap();
        let mut changed = macho(0x0100000c);
        changed.push(1);
        fs::write(&binary, changed).unwrap();
        let second =
            package_locked_scoped(&binary, "aarch64-apple-darwin", &root.join(NAME), None).unwrap();
        assert_ne!(first.version, second.version);
        assert_eq!(registration_root(&root, &second).unwrap(), registered);
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(registered.join(".agents/plugins/marketplace.json")).unwrap(),
        )
        .unwrap();
        let source = manifest["plugins"][0]["source"]["path"].as_str().unwrap();
        assert_eq!(
            source,
            format!("./{NAME}/{}/plugins/{NAME}", second.version)
        );
        assert_eq!(
            registered.join(source.trim_start_matches("./")),
            second.plugin_root
        );
        verify_installed(&first.plugin_root, &first.version, "aarch64-apple-darwin").unwrap();
        verify_installed(&second.plugin_root, &second.version, "aarch64-apple-darwin").unwrap();
    }

    #[test]
    fn registration_rejects_a_package_outside_the_fixed_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("marketplace");
        let package = PluginPackage {
            marketplace_root: temp.path().join("elsewhere"),
            plugin_root: temp.path().join("elsewhere/plugins/onecrew_lite"),
            version: String::new(),
            target: String::new(),
            binary_sha256: String::new(),
        };
        assert_eq!(
            registration_root(&root, &package).unwrap_err(),
            "plugin_path_invalid"
        );
        assert!(!root.exists());
    }

    #[test]
    fn generated_version_tracks_source_and_accepts_future_bases() {
        let source: serde_json::Value =
            serde_json::from_slice(assets().get(".codex-plugin/plugin.json").unwrap()).unwrap();
        let (_, version) = generation(macho(0x0100000c), "aarch64-apple-darwin", None).unwrap();
        assert_eq!(version_base(&version), source["version"].as_str());
        for base in ["0.5.19", "10.20.300"] {
            assert_eq!(
                version_base(&format!("{base}+codex.{}", "a".repeat(64))),
                Some(base)
            );
        }
        for invalid in ["0.5.19+codex.a", "0.05.19+codex.", "0.5.19+codex.é"] {
            assert_eq!(version_base(invalid), None);
        }
    }

    pub(crate) fn macho(cpu: u32) -> Vec<u8> {
        let mut bytes = vec![0; 64];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        bytes[4..8].copy_from_slice(&cpu.to_le_bytes());
        bytes[12..16].copy_from_slice(&2u32.to_le_bytes());
        bytes
    }

    #[test]
    fn rejects_wrong_architecture_and_unsupported_targets() {
        let bytes = macho(0x0100000c);
        assert!(validate_binary(&bytes, "aarch64-apple-darwin").is_ok());
        assert!(validate_binary(&bytes, "x86_64-apple-darwin").is_err());
        assert!(validate_binary(&bytes, "x86_64-pc-windows-msvc").is_err());
        assert!(validate_binary(&bytes, "aarch64-unknown-linux-gnu").is_err());
    }

    #[test]
    fn complete_immutable_generations_include_binary_and_recover_pending_tree() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("入口 one's binary");
        let bytes = macho(0x0100000c);
        fs::write(&executable, &bytes).unwrap();
        let destination = temp.path().join("packages");
        let first = package_plugin(&executable, "aarch64-apple-darwin", &destination).unwrap();
        assert!(
            first
                .marketplace_root
                .join(".agents/plugins/marketplace.json")
                .is_file()
        );
        assert!(!first.plugin_root.join("hooks.json").exists());
        assert_eq!(
            fs::read_dir(first.plugin_root.join("bin")).unwrap().count(),
            1
        );
        assert_eq!(
            fs::read(
                first
                    .plugin_root
                    .join("skills/onecrew/references/campaign-discovery.md")
            )
            .unwrap(),
            assets()["skills/onecrew/references/campaign-discovery.md"]
        );
        assert_eq!(
            fs::read(
                first
                    .plugin_root
                    .join("skills/onecrew/references/match-evaluation.md")
            )
            .unwrap(),
            assets()["skills/onecrew/references/match-evaluation.md"]
        );
        verify_installed(&first.plugin_root, &first.version, &first.target).unwrap();
        let same = package_plugin(&executable, &first.target, &destination).unwrap();
        assert_eq!(same.version, first.version);
        fs::create_dir(destination.join("pending")).unwrap();
        fs::write(destination.join("pending/incomplete"), b"partial").unwrap();
        let mut changed = bytes.clone();
        changed.push(1);
        fs::write(&executable, changed).unwrap();
        let second = package_plugin(&executable, &first.target, &destination).unwrap();
        assert_ne!(first.version, second.version);
        assert_eq!(
            fs::read(first.plugin_root.join("bin/onecrew_lite")).unwrap(),
            bytes
        );
        assert!(!destination.join("pending").exists());
        fs::write(
            second.plugin_root.join("skills/onecrew/SKILL.md"),
            "tampered",
        )
        .unwrap();
        assert!(verify_installed(&second.plugin_root, &second.version, &second.target).is_err());
        assert!(package_plugin(&executable, &first.target, &destination).is_err());
    }

    #[test]
    fn scoped_data_directory_changes_digest_and_preserves_exact_argument() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("测试 one's data");
        let (plain, plain_version) =
            generation(macho(0x0100000c), "aarch64-apple-darwin", None).unwrap();
        let (scoped, scoped_version) =
            generation(macho(0x0100000c), "aarch64-apple-darwin", Some(&data)).unwrap();
        assert_ne!(plain_version, scoped_version);
        let plain: serde_json::Value = serde_json::from_slice(&plain[".mcp.json"]).unwrap();
        let scoped: serde_json::Value = serde_json::from_slice(&scoped[".mcp.json"]).unwrap();
        assert_eq!(
            plain["mcpServers"]["onecrew_lite"]["args"],
            serde_json::json!(["mcp", "--stdio"])
        );
        assert_eq!(
            scoped["mcpServers"]["onecrew_lite"]["args"],
            serde_json::json!(["mcp", "--stdio", "--data-dir", data.to_str().unwrap()])
        );
        assert!(mcp_args(Some(Path::new("relative"))).is_err());
    }

    #[test]
    fn windows_package_has_native_exe_and_command() {
        let temp = tempfile::tempdir().unwrap();
        let mut bytes = vec![0; 128];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..70].copy_from_slice(b"PE\0\0\x64\x86");
        bytes[88..90].copy_from_slice(&[0x0b, 0x02]);
        let executable = temp.path().join("onecrew_lite.exe");
        fs::write(&executable, bytes).unwrap();
        let package = package_plugin(
            &executable,
            "x86_64-pc-windows-msvc",
            &temp.path().join("output"),
        )
        .unwrap();
        let mcp: serde_json::Value =
            serde_json::from_slice(&fs::read(package.plugin_root.join(".mcp.json")).unwrap())
                .unwrap();
        assert_eq!(mcp["mcpServers"]["onecrew_lite"]["command"], "cmd.exe");
        assert_eq!(
            mcp["mcpServers"]["onecrew_lite"]["args"],
            serde_json::json!(["/d", "/v:off", "/c", ".\\bin\\onecrew_lite-mcp.cmd"])
        );
        assert!(
            package
                .plugin_root
                .join("bin/onecrew_lite-mcp.cmd")
                .is_file()
        );
        assert!(package.plugin_root.join("bin/onecrew_lite.exe").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn windows_packaged_launcher_preserves_stdio_and_scoped_path() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("fixture.rs");
        fs::write(
            &source,
            r#"fn main() {
            use std::io::{self, BufRead};
            for arg in std::env::args().skip(1) { println!("{arg}"); }
            let line = io::stdin().lock().lines().next().unwrap().unwrap();
            println!("{line}");
        }"#,
        )
        .unwrap();
        let binary = temp.path().join("fixture.exe");
        let built = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let root = temp.path().join("插件 one's & ! %PATH% directory");
        let data = temp.path().join("数据 & ! %PATH% one's data");
        for scope in [None, Some(data.as_path())] {
            let package =
                package_locked_scoped(&binary, "x86_64-pc-windows-msvc", &root, scope).unwrap();
            let manifest: serde_json::Value =
                serde_json::from_slice(&fs::read(package.plugin_root.join(".mcp.json")).unwrap())
                    .unwrap();
            let config = &manifest["mcpServers"][NAME];
            let mut command = Command::new(config["command"].as_str().unwrap());
            command
                .args(
                    config["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap()),
                )
                .current_dir(package.plugin_root.join(config["cwd"].as_str().unwrap()))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            if let Some(env) = config["env"].as_object() {
                command.envs(
                    env.iter()
                        .map(|(key, value)| (key, value.as_str().unwrap())),
                );
            }
            let mut child = command.spawn().unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"stdio-roundtrip\n")
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let actual = String::from_utf8(output.stdout).unwrap();
            let mut expected = mcp_args(scope).unwrap();
            expected.push("stdio-roundtrip".into());
            assert_eq!(actual.lines().collect::<Vec<_>>(), expected);
        }
    }
}
