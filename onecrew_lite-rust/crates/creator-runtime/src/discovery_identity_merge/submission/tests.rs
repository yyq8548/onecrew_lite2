use super::*;
use crate::{CreatorGateways, DiscoveryTranscriptSegment, FinishDiscoveryTranscriptInput};
use serde_json::json;

pub(crate) fn input(
    owner: &CreatorRuntime,
    pair: DiscoveryIdentityMergeMaterialInput,
) -> SubmitDiscoveryIdentityReviewInput {
    let material = owner
        .prepare_discovery_identity_merge_material(pair.clone())
        .unwrap();
    SubmitDiscoveryIdentityReviewInput {
        pair, material_hash: material.material_hash, decision_id: "identity-decision".into(),
        reviewer: crate::DiscoveryReviewer { kind: crate::DiscoveryReviewerKind::Agent, label: "source-bound identity reviewer".into() },
        decision: DiscoveryIdentityDecision::SameAuthor,
        rationale: "Both native authors explicitly identify the other bound account as their own in saved author-owned descriptions. This is an audited reviewer judgment, not proof from names or product links.".into(),
        findings: material.accounts.each_ref().map(|account| DiscoveryIdentityFinding {
            source_search_hit_id: account.hit.id.clone(), relationship: DiscoveryIdentityRelationship::SelfOwnedAccount,
            citation: DiscoveryIdentityQuote::Description { quote: account.video_description.clone().unwrap() },
        }).to_vec(),
    }
}

#[test]
fn audited_native_relationship_is_durable_idempotent_and_does_not_borrow_qualification() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let campaign = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let counts = owner.count_discovery_candidates(&pair.campaign_id).unwrap();
    let submission = input(&owner, pair.clone());
    let receipt = owner
        .submit_discovery_identity_review("operator", submission.clone())
        .unwrap();
    assert_eq!(receipt.kind, DiscoveryIdentityReviewKind::SameAuthor);
    assert!(receipt.revokes_decision_id.is_none());
    assert_ne!(receipt.candidate_ids[0], receipt.candidate_ids[1]);
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", submission.clone())
            .unwrap(),
        receipt
    );
    assert_eq!(
        owner
            .get_discovery_identity_review("operator", &pair.campaign_id, &submission.decision_id)
            .unwrap(),
        Some(receipt.clone())
    );
    assert!(
        owner
            .get_discovery_identity_review(
                "other-account",
                &pair.campaign_id,
                &submission.decision_id
            )
            .is_err()
    );
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        campaign
    );
    assert_eq!(
        owner.count_discovery_candidates(&pair.campaign_id).unwrap(),
        counts
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .submit_discovery_identity_review("operator", submission)
            .unwrap(),
        receipt
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let (n, snapshot): (u64, String) = db
        .query_row(
            "SELECT count(*),material_json FROM discovery_identity_reviews",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(n, 1);
    assert!(snapshot.contains("My other account, also owned by me:"));
}

#[test]
fn changed_payload_or_authenticated_actor_conflicts_with_the_same_decision() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let submission = input(&owner, pair);
    owner
        .submit_discovery_identity_review("operator", submission.clone())
        .unwrap();
    let mut changed = submission.clone();
    changed.rationale.push_str("Changed payload.");
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", changed)
            .unwrap_err(),
        "identity_review_idempotency_conflict"
    );
    assert_eq!(
        owner
            .submit_discovery_identity_review("other-account", submission)
            .unwrap_err(),
        "identity_review_idempotency_conflict"
    );
}

#[test]
fn commit_rechecks_both_sources_and_same_key_replay_wins_over_later_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let submission = input(&owner, pair.clone());
    let hash = request_hash("operator", &submission).unwrap();
    let basis = owner
        .repository
        .read_discovery_identity_submission(&pair, &submission.decision_id)
        .unwrap();
    let DiscoveryIdentitySubmissionBasis::Material(basis) = basis else {
        panic!("not yet committed")
    };
    let prepared = prepare("operator".into(), submission.clone(), hash, *basis).unwrap();
    let claim = owner
        .claim_discovery_transcript(&pair.campaign_id, 1, "dedicated", &pair.right_search_hit_id)
        .unwrap();
    owner
        .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
            claim,
            segments: vec![DiscoveryTranscriptSegment {
                timestamp: "0:01".into(),
                speaker: String::new(),
                text: "New evidence.".into(),
            }],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert_eq!(
        owner
            .repository
            .commit_discovery_identity_review(&prepared)
            .err()
            .unwrap(),
        "identity_review_material_changed"
    );
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", submission.clone())
            .unwrap_err(),
        "identity_review_material_changed"
    );
    assert!(
        owner
            .get_discovery_identity_review("operator", &pair.campaign_id, &submission.decision_id)
            .unwrap()
            .is_none()
    );
    let mut fresh = submission;
    fresh.material_hash = owner
        .prepare_discovery_identity_merge_material(pair.clone())
        .unwrap()
        .material_hash;
    let receipt = owner
        .submit_discovery_identity_review("operator", fresh.clone())
        .unwrap();
    assert_eq!(
        owner
            .repository
            .commit_discovery_identity_review(&prepared)
            .err()
            .unwrap(),
        "identity_review_idempotency_conflict"
    );
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", fresh)
            .unwrap(),
        receipt
    );
}

#[test]
fn fabricated_missing_one_sided_or_partner_relationships_never_create_a_merge() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let submission = input(&owner, pair.clone());
    let mut variants = Vec::new();
    let mut missing = submission.clone();
    missing.findings.pop();
    variants.push(missing);
    let mut fabricated = submission.clone();
    fabricated.findings[0].citation = DiscoveryIdentityQuote::Description {
        quote: "fabricated".into(),
    };
    variants.push(fabricated);
    let mut wrong_source = submission.clone();
    wrong_source.findings[0].source_search_hit_id = "other-hit".into();
    variants.push(wrong_source);
    let mut partner = submission.clone();
    partner.findings[0].relationship = DiscoveryIdentityRelationship::OtherPerson;
    variants.push(partner);
    let mut uncertain = submission.clone();
    uncertain.findings[0].relationship = DiscoveryIdentityRelationship::Unclear;
    variants.push(uncertain);
    let mut duplicate_side = submission.clone();
    duplicate_side.findings[1] = duplicate_side.findings[0].clone();
    variants.push(duplicate_side);
    for invalid in variants {
        assert!(
            owner
                .submit_discovery_identity_review("operator", invalid)
                .is_err()
        );
    }
    assert!(
        owner
            .get_discovery_identity_review("operator", &pair.campaign_id, "identity-decision")
            .unwrap()
            .is_none()
    );
    let expected = Url::parse("https://www.youtube.com/channel/UC123").unwrap();
    assert!(!references_profile(
        "https://www.youtube.com/channel/UC123-extra",
        &expected
    ));
    assert!(!references_profile(
        "https://evil.example/channel/UC123",
        &expected
    ));
    assert!(!references_profile(
        "https://user@www.youtube.com/channel/UC123",
        &expected
    ));
    assert!(references_profile(
        "My account: https://www.youtube.com/channel/UC123?tracking=yes",
        &expected
    ));
}

#[test]
fn matching_names_and_shared_sponsor_urls_are_not_native_account_relationships() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup(dir.path());
    let submission = input(&owner, pair);
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", submission)
            .unwrap_err(),
        "identity_review_quote_target_mismatch"
    );
}

#[tokio::test]
async fn asynchronous_submit_reconciles_concurrent_same_intent_once() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let submission = input(&owner, pair.clone());
    let (left, right) = tokio::join!(
        owner.submit_discovery_identity_review_async("operator".into(), submission.clone()),
        owner.submit_discovery_identity_review_async("operator".into(), submission.clone()),
    );
    let receipt = left.unwrap();
    assert_eq!(receipt, right.unwrap());
    assert_eq!(
        owner
            .get_discovery_identity_review_async(
                "operator".into(),
                pair.campaign_id.clone(),
                submission.decision_id.clone()
            )
            .await
            .unwrap(),
        Some(receipt)
    );
    assert_eq!(
        owner
            .get_discovery_identity_review("operator", &pair.campaign_id, &submission.decision_id)
            .unwrap()
            .unwrap()
            .decision_id,
        submission.decision_id
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_identity_reviews", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn clients_cannot_supply_operator_facts_group_state_or_arbitrary_merge_flags() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let submission = serde_json::to_value(input(&owner, pair)).unwrap();
    for field in [
        "operator_account_id",
        "merged",
        "candidate_ids",
        "accounts",
        "qualified",
        "group_id",
    ] {
        let mut injected = submission.clone();
        injected[field] = json!("forged");
        assert!(serde_json::from_value::<SubmitDiscoveryIdentityReviewInput>(injected).is_err());
    }
}

#[test]
fn insufficient_evidence_is_audited_without_creating_a_relationship() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup(dir.path());
    let counts = owner.count_discovery_candidates(&pair.campaign_id).unwrap();
    let mut submission = input(&owner, pair.clone());
    submission.decision = DiscoveryIdentityDecision::Insufficient;
    submission.rationale =
        "Names and sponsor links do not establish either account ownership or different people."
            .into();
    submission.findings.clear();
    let receipt = owner
        .submit_discovery_identity_review("operator", submission)
        .unwrap();
    assert_eq!(receipt.kind, DiscoveryIdentityReviewKind::Insufficient);
    assert_eq!(
        owner.count_discovery_candidates(&pair.campaign_id).unwrap(),
        counts
    );
}

#[test]
fn reviewed_different_people_are_kept_separate_and_cannot_be_silently_overridden() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_other_links(dir.path());
    let mut submission = input(&owner, pair.clone());
    submission.decision = DiscoveryIdentityDecision::DifferentAuthors;
    submission.rationale = "Native author descriptions explicitly identify the other bound account as a collaborator and different person.".into();
    for finding in &mut submission.findings {
        finding.relationship = DiscoveryIdentityRelationship::OtherPerson;
    }
    let receipt = owner
        .submit_discovery_identity_review("operator", submission)
        .unwrap();
    assert_eq!(receipt.kind, DiscoveryIdentityReviewKind::DifferentAuthors);
    let mut override_request = input(&owner, pair);
    override_request.decision_id = "attempt-override".into();
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", override_request)
            .unwrap_err(),
        "identity_review_conflicting_relationship"
    );
}

#[test]
fn persisted_same_intent_replay_survives_later_source_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let submission = input(&owner, pair.clone());
    let receipt = owner
        .submit_discovery_identity_review("operator", submission.clone())
        .unwrap();
    let claim = owner
        .claim_discovery_transcript(&pair.campaign_id, 1, "dedicated", &pair.left_search_hit_id)
        .unwrap();
    owner
        .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
            claim,
            segments: vec![DiscoveryTranscriptSegment {
                timestamp: "0:01".into(),
                speaker: String::new(),
                text: "Later evidence does not rewrite the historical review.".into(),
            }],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", submission.clone())
            .unwrap(),
        receipt
    );
    let mut new_request = submission;
    new_request.decision_id = "new-decision".into();
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", new_request)
            .unwrap_err(),
        "identity_review_material_changed"
    );
}

#[test]
fn failed_insert_rolls_back_and_new_id_cannot_duplicate_an_active_relation() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let submission = input(&owner, pair.clone());
    let counts = owner.count_discovery_candidates(&pair.campaign_id).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_identity_test BEFORE INSERT ON discovery_identity_reviews BEGIN SELECT RAISE(ABORT,'injected_failure'); END;").unwrap();
    assert!(
        owner
            .submit_discovery_identity_review("operator", submission.clone())
            .is_err()
    );
    assert!(
        owner
            .get_discovery_identity_review("operator", &pair.campaign_id, &submission.decision_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner.count_discovery_candidates(&pair.campaign_id).unwrap(),
        counts
    );
    db.execute_batch("DROP TRIGGER reject_identity_test;")
        .unwrap();
    owner
        .submit_discovery_identity_review("operator", submission.clone())
        .unwrap();
    let mut duplicate = submission;
    duplicate.decision_id = "duplicate-relation".into();
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", duplicate)
            .unwrap_err(),
        "identity_review_relationship_already_reviewed"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_identity_reviews", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
}
