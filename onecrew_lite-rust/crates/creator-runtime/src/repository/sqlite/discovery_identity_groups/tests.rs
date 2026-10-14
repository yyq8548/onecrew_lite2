use super::*;
use crate::{
    discovery_identity_merge::groups::{DiscoveryIdentityGroupReviewState, build},
    discovery_identity_merge::tests::setup_with_owned_links,
};

#[test]
fn oversized_persisted_native_details_stop_instead_of_returning_partial_groups() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = setup_with_owned_links(dir.path());
    // Fault injection is confined to this temporary fixture, never the user store.
    let fixture = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    fixture.execute("UPDATE discovery_candidates SET display_name=?1 WHERE id=(SELECT id FROM discovery_candidates LIMIT 1)",
        ["x".repeat(MAX_BYTES)]).unwrap();
    assert_eq!(
        owner
            .get_discovery_identity_groups(&pair.campaign_id, 1)
            .unwrap_err(),
        "identity_groups_snapshot_too_large"
    );
}

#[test]
fn all_identity_and_native_sources_come_from_the_same_wal_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = setup_with_owned_links(dir.path());
    let input = crate::discovery_identity_merge::tests::identity_review_input(&owner, pair.clone());
    owner
        .submit_discovery_identity_review("operator", input)
        .unwrap();
    let before = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    let mut reader = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let tx = reader
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .unwrap();
    // Pin a real read snapshot before the normal owner completes a new source.
    let _: u64 = tx
        .query_row("SELECT count(*) FROM discovery_identity_reviews", [], |r| {
            r.get(0)
        })
        .unwrap();
    let claim = owner
        .claim_discovery_transcript(&pair.campaign_id, 1, "dedicated", &pair.left_search_hit_id)
        .unwrap();
    owner
        .finish_discovery_transcript(crate::FinishDiscoveryTranscriptInput {
            claim,
            segments: vec![crate::DiscoveryTranscriptSegment {
                timestamp: "0:01".into(),
                speaker: String::new(),
                text: "Concurrent new source.".into(),
            }],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let during = build(read_snapshot(&tx, &pair.campaign_id, 1).unwrap()).unwrap();
    assert_eq!(during, before);
    tx.commit().unwrap();
    let after = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(
        after.reviews[0].state,
        DiscoveryIdentityGroupReviewState::EvidenceChanged
    );
    assert_ne!(after.snapshot_hash, before.snapshot_hash);
}
