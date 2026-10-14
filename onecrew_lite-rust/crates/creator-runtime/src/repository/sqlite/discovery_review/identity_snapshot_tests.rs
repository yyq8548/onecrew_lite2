use super::*;

#[test]
fn identity_pair_keeps_one_snapshot_during_concurrent_evidence_commit() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = crate::discovery_identity_merge::tests::setup(dir.path());
    let repository = SqliteRepository::open(dir.path().join("crm.db")).unwrap();
    let mut connection = repository.read_connection().unwrap();
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .unwrap();
    // Establish the snapshot with the first account before a real owner write.
    let left = load_basis(&tx, &input.campaign_id, 1, &input.left_search_hit_id).unwrap();
    let claim = owner
        .claim_discovery_transcript(
            &input.campaign_id,
            1,
            "dedicated",
            &input.right_search_hit_id,
        )
        .unwrap();
    owner
        .finish_discovery_transcript(crate::FinishDiscoveryTranscriptInput {
            claim,
            segments: vec![crate::DiscoveryTranscriptSegment {
                timestamp: "0:01".into(),
                speaker: String::new(),
                text: "New identity context committed while the pair is read.".into(),
            }],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let pair = load_pair_basis(
        &tx,
        &input.campaign_id,
        1,
        &input.left_search_hit_id,
        &input.right_search_hit_id,
    )
    .unwrap();
    assert_eq!(pair[0], left);
    assert!(pair[1].transcript.is_none());
    tx.commit().unwrap();
    let current = read_pair_basis(
        &repository,
        &input.campaign_id,
        1,
        &input.left_search_hit_id,
        &input.right_search_hit_id,
    )
    .unwrap();
    assert!(current[1].transcript.is_some());
}
