use super::*;

#[test]
fn windows_online_backup_is_private_recoverable_and_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-backup.db");
    let source = Connection::open_in_memory().unwrap();
    source
        .execute_batch("CREATE TABLE proof(value TEXT); INSERT INTO proof VALUES('original');")
        .unwrap();
    create_online_backup(&source, &path).unwrap();
    assert_private_acl(&path);
    let backup = Connection::open(&path).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT value FROM proof", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "original"
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(backup);
    source
        .execute("UPDATE proof SET value='changed'", [])
        .unwrap();
    assert!(create_online_backup(&source, &path).is_err());
    let backup = Connection::open(&path).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT value FROM proof", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "original"
    );
    drop(backup);
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        1,
        "standalone backup must not leave sidecars"
    );
}

#[test]
fn windows_backup_is_private_before_data_and_identity_handle_prevents_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty-backup.db");
    let file = create_private_backup_file(&path).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 0);
    assert_private_acl(&path);
    assert!(std::fs::remove_file(&path).is_err());
    assert!(std::fs::rename(&path, directory.path().join("replaced.db")).is_err());
    assert!(create_private_backup_file(&path).is_err());
    assert_eq!(file.metadata().unwrap().len(), 0);
    drop(file);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn windows_backup_rejects_nul_and_missing_parent_without_artifacts() {
    let directory = tempfile::tempdir().unwrap();
    let nul_path = directory.path().join("backup.db\0ignored");
    assert_eq!(
        create_private_backup_file(&nul_path).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(create_private_backup_file(&directory.path().join("missing/backup.db")).is_err());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

fn assert_private_acl(path: &Path) {
    // Independent OS readback, not a call to the implementation's permission helper.
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", r#"
            $ErrorActionPreference='Stop'
            $acl=[System.IO.File]::GetAccessControl($env:ONECREW_BACKUP_ACL_TEST)
            [pscustomobject]@{
                protected=$acl.AreAccessRulesProtected
                rules=@($acl.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier]) | ForEach-Object { [pscustomobject]@{
                    sid=$_.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value
                    inherited=$_.IsInherited
                    allow=($_.AccessControlType -eq 'Allow')
                    rights=[int]$_.FileSystemRights
                } })
            } | ConvertTo-Json -Compress
        "#])
        .env("ONECREW_BACKUP_ACL_TEST", path)
        .output().unwrap();
    assert!(
        output.status.success(),
        "native ACL inspection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let acl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        acl["protected"], true,
        "backup must not inherit broad directory permissions"
    );
    let rules = acl["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 3);
    let mut principals = Vec::new();
    for rule in rules {
        assert_eq!(rule["inherited"], false);
        assert_eq!(rule["allow"], true);
        assert_eq!(rule["rights"], 2_032_127); // Windows FileSystemRights.FullControl.
        principals.push(rule["sid"].as_str().unwrap());
    }
    principals.sort_unstable();
    assert_eq!(principals, vec!["S-1-3-4", "S-1-5-18", "S-1-5-32-544"]);
}
