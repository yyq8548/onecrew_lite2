//! Resolve Unix CLI installations even when a desktop host has a minimal PATH.

use std::path::PathBuf;

use crate::OpenCliError;

pub(super) async fn inspect_system() -> Result<crate::OpenCli, OpenCliError> {
    let mut directories = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && home.is_absolute()
    {
        directories.push(home.join(".local/bin"));
    }
    directories.extend([
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
    ]);
    let executable = resolve(&directories)
        .await
        .ok_or(OpenCliError::Unavailable)?;
    // Pin one installed executable. A broken installation must surface its error,
    // rather than silently switching versions for later provider reads.
    crate::OpenCli::inspect(executable).await
}

async fn resolve(directories: &[PathBuf]) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    for directory in directories.iter().filter(|path| path.is_absolute()) {
        let executable = directory.join("opencli");
        if let Ok(metadata) = tokio::fs::metadata(&executable).await
            && metadata.is_file()
            && metadata.permissions().mode() & 0o111 != 0
        {
            return Some(executable);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn cli_at(directory: &Path, mode: u32) -> PathBuf {
        std::fs::create_dir_all(directory).unwrap();
        let executable = directory.join("opencli");
        std::fs::write(&executable, b"#!/bin/sh\nprintf '1.8.8\\n'\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(mode)).unwrap();
        executable
    }

    #[tokio::test]
    async fn desktop_path_can_resolve_user_installation_without_shell_startup() {
        let root = tempfile::tempdir().unwrap();
        let user_bin = root.path().join(".local/bin");
        let installed = cli_at(&user_bin, 0o755);
        let resolved = resolve(&[root.path().join("system-bin"), user_bin])
            .await
            .unwrap();
        assert_eq!(resolved, installed);
        assert_eq!(
            crate::process::run(&resolved, None, &["--version"], crate::PROBE_TIMEOUT)
                .await
                .unwrap()
                .trim(),
            "1.8.8"
        );
    }

    #[tokio::test]
    async fn explicit_path_precedes_user_installation_and_non_executables_are_skipped() {
        let root = tempfile::tempdir().unwrap();
        let denied = root.path().join("denied");
        cli_at(&denied, 0o644);
        let explicit = root.path().join("explicit");
        let expected = cli_at(&explicit, 0o755);
        let user = root.path().join(".local/bin");
        cli_at(&user, 0o755);
        assert_eq!(
            resolve(&[PathBuf::from("."), denied, explicit, user]).await,
            Some(expected)
        );
        assert_eq!(resolve(&[root.path().join("missing")]).await, None);
    }
}
