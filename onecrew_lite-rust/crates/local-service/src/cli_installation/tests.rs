use super::*;

#[test]
fn partial_workspace_cannot_start_until_transaction_is_finished() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("onecrew_lite");
    let marker = temp.path().join(".onecrew_lite-install");
    require_complete_workspace(&package).unwrap();
    fs::create_dir(&marker).unwrap();
    assert!(
        require_complete_workspace(&package)
            .unwrap_err()
            .contains("installation_pending")
    );
    fs::remove_dir(&marker).unwrap();
    // Even a malformed marker cannot silently bypass recovery.
    fs::write(&marker, b"interrupted").unwrap();
    assert!(require_complete_workspace(&package).is_err());
    fs::remove_file(&marker).unwrap();
    require_complete_workspace(&package).unwrap();
}

#[test]
fn unix_layout_keeps_integration_state_beside_the_installed_cli() {
    let layout = UserInstallLayout::for_host(HostFamily::Unix, Path::new("/home/someone"));
    assert_eq!(
        layout.executable(),
        Path::new("/home/someone/.local/bin/onecrew_lite")
    );
    assert_eq!(
        layout.codex_plugin(),
        Path::new("/home/someone/.local/share/onecrew_lite/codex-plugin")
    );
    assert_eq!(
        layout.service_binaries(),
        Path::new("/home/someone/.local/share/onecrew_lite/service-binaries")
    );
}

#[test]
fn windows_layout_keeps_integration_state_inside_the_local_app_data_install() {
    let local = Path::new("/c/Users/someone/AppData/Local");
    let layout = UserInstallLayout::for_host(HostFamily::Windows, local);
    assert_eq!(
        layout.executable(),
        Path::new("/c/Users/someone/AppData/Local/onecrew_lite/bin/onecrew_lite.exe")
    );
    assert_eq!(
        layout.codex_plugin(),
        Path::new("/c/Users/someone/AppData/Local/onecrew_lite/codex-plugin")
    );
    assert_eq!(
        layout.service_binaries(),
        Path::new("/c/Users/someone/AppData/Local/onecrew_lite/service-binaries")
    );
}

#[test]
fn staged_copies_in_either_layout_are_managed_but_the_installed_cli_is_not() {
    for (host, base, name) in [
        (HostFamily::Unix, "/home/someone", "onecrew_lite"),
        (
            HostFamily::Windows,
            "/c/Users/someone/AppData/Local",
            "onecrew_lite.exe",
        ),
    ] {
        let layout = UserInstallLayout::for_host(host, Path::new(base));
        assert!(!is_managed_cli_copy(layout.executable()), "{base}");
        let service = layout.service_binaries().join("a".repeat(64)).join(name);
        assert!(is_managed_cli_copy(&service), "{}", service.display());
        let generation = layout
            .codex_plugin()
            .join("b".repeat(64))
            .join("onecrew_lite/0.6.1+codex.fixture/plugins/onecrew_lite/bin")
            .join(name);
        assert!(is_managed_cli_copy(&generation), "{}", generation.display());
    }
    // Windows compares paths case-insensitively.
    assert!(is_managed_cli_copy(Path::new(
        "/c/Users/someone/AppData/Local/ONECREW_LITE/Service-Binaries/hash/onecrew_lite.exe"
    )));
    assert!(is_managed_cli_copy(Path::new(
        "/home/someone/.codex/plugins/cache/onecrew_lite-cli/onecrew_lite/version/bin/onecrew_lite"
    )));
}

#[test]
fn service_images_staged_in_the_retired_cache_layout_stay_managed() {
    for path in [
        "/Users/someone/Library/Caches/io.onecrewai.onecrew_lite/service-binaries/hash/onecrew_lite",
        "/c/Users/someone/AppData/Local/io.onecrewai.onecrew_lite/service-binaries/hash/onecrew_lite.exe",
    ] {
        assert!(is_managed_cli_copy(Path::new(path)), "{path}");
    }
}

#[test]
fn cache_copies_and_unverified_standalone_files_are_never_update_commands() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory
        .path()
        .join(".local/share/onecrew_lite/service-binaries/hash/onecrew_lite");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, b"running").unwrap();
    let standalone = directory.path().join(if cfg!(windows) {
        "onecrew_lite.exe"
    } else {
        "onecrew_lite"
    });
    fs::write(&standalone, b"old").unwrap();
    let expected = crate::paths::executable_identity(&cache).unwrap();
    let resolver = UpdateCommandPath::default();
    assert!(
        resolver
            .resolve_candidates(&cache, Some(&standalone), &expected)
            .is_none()
    );
    fs::write(&standalone, b"running").unwrap();
    assert_eq!(
        resolver.resolve_candidates(&cache, Some(&standalone), &expected),
        Some(standalone.canonicalize().unwrap())
    );
    fs::remove_file(&standalone).unwrap();
    assert!(
        resolver
            .resolve_candidates(&cache, Some(&standalone), &expected)
            .is_none()
    );
}
#[cfg(windows)]
#[test]
fn installed_paths_preserve_physical_appdata_identity() {
    use super::installed_windows_base;
    let base = std::path::Path::new(r"C:\Users\u\AppData\Local\Packages\Host\LocalCache\Local");
    for relative in [
        r"onecrew_lite\bin\onecrew_lite.exe",
        r"onecrew_lite\service-binaries\digest\onecrew_lite.exe",
    ] {
        assert_eq!(
            installed_windows_base(&base.join(relative)),
            Some(base.to_owned())
        );
    }
    assert!(
        installed_windows_base(std::path::Path::new(r"C:\workspace\onecrew_lite.exe")).is_none()
    );
}
