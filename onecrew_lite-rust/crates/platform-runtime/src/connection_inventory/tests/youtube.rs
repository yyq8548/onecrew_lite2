use super::{cleanup, platform};
use crate::LocalConnectionAvailability;

#[test]
fn youtube_mapper_sorts_and_keeps_unavailable_credential_id() {
    let (platform, directory) = platform();
    let zulu = platform
        .put_youtube_api_key("Zulu YouTube", None, "youtube-secret-z")
        .unwrap();
    let alpha = platform
        .put_youtube_api_key("Alpha YouTube", None, "youtube-secret-a")
        .unwrap();
    platform
        .vault
        .mark_service_credential_failure(zulu.id, "sanitized owner failure", false, None)
        .unwrap();

    let summaries = platform.platform_connection_list().unwrap().youtube;
    assert_eq!(summaries[0].credential_id, alpha.id.to_string());
    assert_eq!(summaries[1].credential_id, zulu.id.to_string());
    assert_eq!(
        summaries[1].availability,
        LocalConnectionAvailability::Unavailable
    );

    cleanup(platform, directory);
}
