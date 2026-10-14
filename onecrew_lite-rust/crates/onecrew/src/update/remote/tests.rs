use super::*;

#[test]
fn versions_are_strict_and_asset_urls_remain_same_origin() {
    assert_eq!(version("0.5.22").unwrap(), [0, 5, 22]);
    for invalid in ["v0.5.22", "0.05.22", "0.5", "0.5.22/next", "0.5.22?token=x"] {
        assert!(version(invalid).is_err());
    }
    let remote = Remote::production().unwrap();
    let origin = onecrew_creator_sync::production_access_origin();
    let mut asset = Asset {
        target: "aarch64-apple-darwin".into(),
        size: 64,
        sha256: "a".repeat(64),
        url: format!("{origin}/api/cli/update/0.5.22/files/aarch64-apple-darwin"),
    };
    assert!(remote.asset_url("0.5.22", &asset).is_ok());
    for invalid in [
        "https://outside.example/api/cli/update/0.5.22/files/aarch64-apple-darwin".to_owned(),
        format!("{origin}/api/cli/update/0.5.21/files/aarch64-apple-darwin"),
        format!("{origin}/api/cli/update/0.5.22/files/aarch64-apple-darwin?token=x"),
    ] {
        asset.url = invalid;
        assert!(remote.asset_url("0.5.22", &asset).is_err());
    }
}
