use super::*;
use crate::{
    CreatorGateways, DiscoveryIdentityDecision, DiscoveryReviewer, DiscoveryReviewerKind,
    DiscoveryTranscriptSegment, FinishDiscoveryTranscriptInput, RevokeDiscoveryIdentityReviewInput,
};

fn submit(owner: &CreatorRuntime, pair: crate::DiscoveryIdentityMergeMaterialInput) {
    let input = super::super::submission::tests::input(owner, pair);
    owner
        .submit_discovery_identity_review("operator", input)
        .unwrap();
}

#[tokio::test]
async fn library_publication_versions_survive_restart_and_review_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_mixed_followers(dir.path());
    let campaign = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let first = owner
        .prepare_discovery_library_publication(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(first.material.sources.source_revision, 1);
    assert_eq!(first.identity_revision, 1);
    assert_eq!(
        owner
            .prepare_discovery_library_publication_async(pair.campaign_id.clone(), 1)
            .await
            .unwrap(),
        first
    );
    let stale = owner
        .repository
        .read_discovery_identity_groups_basis(&pair.campaign_id, 1)
        .unwrap();
    submit(&owner, pair.clone());
    assert_eq!(
        owner
            .repository
            .reserve_discovery_library_publication(
                &stale,
                &first.material.sources.snapshot_hash,
                &first.material.identity.snapshot_hash,
            )
            .unwrap_err(),
        "library_publication_changed"
    );
    let merged = owner
        .prepare_discovery_library_publication(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(merged.material.sources, first.material.sources);
    assert_eq!(merged.identity_revision, 2);
    assert_eq!(merged.material.identity.groups.len(), 1);
    revoke(&owner, &pair.campaign_id);
    let revoked = owner
        .prepare_discovery_library_publication(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(revoked.material.sources, first.material.sources);
    assert_eq!(revoked.identity_revision, 3);
    assert_eq!(revoked.material.identity.groups.len(), 2);
    review_shampoo(&owner, &pair.campaign_id, &pair.left_search_hit_id);
    let evidence_updated = owner
        .prepare_discovery_library_publication(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(evidence_updated.material.sources.source_revision, 2);
    assert_eq!(evidence_updated.identity_revision, 4);
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        campaign
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_publication(&pair.campaign_id, 1)
            .unwrap(),
        evidence_updated
    );
}

#[test]
fn library_publication_rechecks_native_details_and_rolls_back_both_versions() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_mixed_followers(dir.path());
    let first = owner
        .prepare_discovery_library_publication(&pair.campaign_id, 1)
        .unwrap();
    let stale = owner
        .repository
        .read_discovery_identity_groups_basis(&pair.campaign_id, 1)
        .unwrap();
    // Fault injection is restricted to this disposable fixture.
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_candidates SET display_name='Updated native name'",
        [],
    )
    .unwrap();
    assert_eq!(
        owner
            .repository
            .reserve_discovery_library_publication(
                &stale,
                &first.material.sources.snapshot_hash,
                &first.material.identity.snapshot_hash,
            )
            .unwrap_err(),
        "library_publication_changed"
    );
    db.execute(
        "UPDATE discovery_library_identity_versions SET identity_revision=9007199254740991",
        [],
    )
    .unwrap();
    db.execute(
        "UPDATE discovery_search_hits SET title='Updated source title'",
        [],
    )
    .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_publication(&pair.campaign_id, 1)
            .unwrap_err(),
        "library_identity_version_exhausted"
    );
    let source_version: (u64, String) = db
        .query_row(
            "SELECT source_revision,snapshot_hash FROM discovery_library_source_versions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(source_version, (1, first.material.sources.snapshot_hash));
    assert_eq!(
        db.query_row(
            "SELECT identity_revision FROM discovery_library_identity_versions",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        9_007_199_254_740_991
    );
}

#[tokio::test]
async fn library_material_binds_source_rows_and_reviews_without_reserving_versions() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_mixed_followers(dir.path());
    let before = owner
        .read_discovery_library_material(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(before.sources.source_revision, 0);
    assert_eq!(before.sources.campaign_id, before.identity.campaign_id);
    assert_eq!(
        before.sources.criteria_revision,
        before.identity.criteria_revision
    );
    for group in &before.identity.groups {
        for member in &group.members {
            for source in &member.sources {
                assert!(before.sources.rows.contains(source));
            }
        }
    }
    submit(&owner, pair.clone());
    let after = owner
        .read_discovery_library_material_async(pair.campaign_id.clone(), 1)
        .await
        .unwrap();
    assert_eq!(
        before.sources, after.sources,
        "An identity decision does not rewrite qualification or source facts"
    );
    assert_ne!(before.identity.snapshot_hash, after.identity.snapshot_hash);
    assert_eq!(before.identity.groups.len(), 2);
    assert_eq!(after.identity.groups.len(), 1);
    revoke(&owner, &pair.campaign_id);
    let revoked = owner
        .read_discovery_library_material(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(revoked.sources, before.sources);
    assert_eq!(revoked.identity.groups.len(), 2);
    assert_ne!(revoked.identity.snapshot_hash, after.identity.snapshot_hash);
    assert!(
        owner
            .read_discovery_library_material(&pair.campaign_id, 2)
            .is_err()
    );
}

fn revoke(owner: &CreatorRuntime, campaign: &str) {
    owner
        .revoke_discovery_identity_review(
            "operator",
            RevokeDiscoveryIdentityReviewInput {
                campaign_id: campaign.into(),
                expected_revision: 1,
                decision_id: "revoke-groups".into(),
                revokes_decision_id: "identity-decision".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: "identity correction".into(),
                },
                rationale: "Withdraw earlier relationship while retaining both sources.".into(),
            },
        )
        .unwrap();
}

fn review_shampoo(owner: &CreatorRuntime, campaign: &str, hit: &str) {
    let material = owner
        .prepare_discovery_review_material(campaign, 1, hit)
        .unwrap();
    owner.submit_discovery_review("operator", crate::SubmitDiscoveryReviewInput {
        campaign_id:campaign.into(),expected_revision:1,search_hit_id:hit.into(),
        material_hash:material.material_hash,request_id:format!("shampoo-{hit}"),
        reviewer:DiscoveryReviewer {kind:DiscoveryReviewerKind::Agent,label:"source-bound shampoo reviewer".into()},
        findings:vec![crate::DiscoveryReviewFinding {
            criterion:crate::DiscoveryReviewCriterion::CategoryContent,
            outcome:crate::DiscoveryReviewOutcome::Supported,
            rationale:"The saved native author description explicitly identifies this as a shampoo review.".into(),
            citations:vec![crate::DiscoveryReviewQuote::Description {quote:"My shampoo review.".into()}],
            tool:None,match_level: None, country_code:None,
        }],
    }).unwrap();
}

#[test]
fn reviewed_group_counts_qualified_members_once_without_borrowing_followers() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_mixed_followers(dir.path());
    crate::discovery_contacts::legacy_approval_fixture(dir.path());
    review_shampoo(&owner, &pair.campaign_id, &pair.left_search_hit_id);
    review_shampoo(&owner, &pair.campaign_id, &pair.right_search_hit_id);
    let separate = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(separate.counts.qualified_native_accounts, 1);
    assert_eq!(separate.counts.qualified_identity_groups, 1);
    assert_eq!(separate.counts.qualified_native_only_groups, 1);
    submit(&owner, pair.clone());
    let joined = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(joined.counts.native_accounts, 2);
    assert_eq!(joined.counts.qualified_native_accounts, 1);
    assert_eq!(joined.counts.qualified_identity_groups, 1);
    assert_eq!(joined.counts.qualified_native_only_groups, 0);
    assert_eq!(joined.groups[0].qualified_member_count, 1);
    assert_eq!(
        joined.groups[0]
            .members
            .iter()
            .filter(|m| m.verdict == "rejected")
            .count(),
        1
    );
    assert!(
        joined.groups[0]
            .members
            .iter()
            .any(|m| m.reasons.contains(&"below_min_subscribers".into()))
    );
}

#[test]
fn two_qualified_native_accounts_count_as_one_reviewed_group_then_two_after_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_shampoo_owned_links(dir.path());
    crate::discovery_contacts::legacy_approval_fixture(dir.path());
    review_shampoo(&owner, &pair.campaign_id, &pair.left_search_hit_id);
    review_shampoo(&owner, &pair.campaign_id, &pair.right_search_hit_id);
    let separate = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(separate.counts.qualified_native_accounts, 2);
    assert_eq!(separate.counts.qualified_identity_groups, 2);
    assert_eq!(separate.counts.qualified_native_only_groups, 2);
    submit(&owner, pair.clone());
    let joined = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(joined.counts.qualified_native_accounts, 2);
    assert_eq!(joined.counts.qualified_identity_groups, 1);
    assert_eq!(joined.groups[0].qualified_member_count, 2);
    revoke(&owner, &pair.campaign_id);
    let revoked = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(revoked.counts.qualified_native_accounts, 2);
    assert_eq!(revoked.counts.qualified_identity_groups, 2);
}

#[test]
fn same_names_and_shared_sponsor_links_remain_two_native_only_groups() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup(dir.path());
    let campaign = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let counts = owner.count_discovery_candidates(&pair.campaign_id).unwrap();
    let snapshot = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(snapshot.counts.native_accounts, 2);
    assert_eq!(snapshot.counts.identity_groups, 2);
    assert_eq!(snapshot.counts.qualified_native_accounts, 0);
    assert_eq!(snapshot.counts.qualified_identity_groups, 0);
    assert_eq!(snapshot.counts.unbound_source_hits, 0);
    assert!(snapshot.reviews.is_empty());
    assert!(
        snapshot
            .groups
            .iter()
            .all(|g| !g.has_same_author_review && g.members.len() == 1)
    );
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        campaign
    );
    assert_eq!(
        owner.count_discovery_candidates(&pair.campaign_id).unwrap(),
        counts
    );
}

#[tokio::test]
async fn reviewed_relationship_groups_once_preserves_every_source_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    submit(&owner, pair.clone());
    let campaign = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let before = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(before.groups.len(), 1);
    assert_eq!(before.groups[0].members.len(), 2);
    assert!(before.groups[0].has_same_author_review);
    assert_eq!(before.groups[0].decision_ids, vec!["identity-decision"]);
    assert_eq!(
        before.reviews[0].state,
        DiscoveryIdentityGroupReviewState::Current
    );
    assert_eq!(before.groups[0].qualified_member_count, 0);
    assert!(before.groups[0].members.iter().all(|m| {
        m.verdict == "pending"
            && m.sources.len() == 1
            && m.profile_url
                .starts_with("https://www.youtube.com/channel/")
            && !m.display_name.is_empty()
    }));
    assert_eq!(
        owner
            .get_discovery_identity_groups_async(pair.campaign_id.clone(), 1)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        campaign
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_identity_groups(&pair.campaign_id, 1)
            .unwrap(),
        before
    );
}

#[test]
fn revocation_restores_singletons_and_keeps_both_audit_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let initial = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    submit(&owner, pair.clone());
    let joined = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    revoke(&owner, &pair.campaign_id);
    let separate = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(separate.groups, initial.groups);
    assert_ne!(separate.snapshot_hash, initial.snapshot_hash);
    assert_ne!(separate.snapshot_hash, joined.snapshot_hash);
    assert_eq!(separate.reviews.len(), 2);
    assert_eq!(
        separate
            .reviews
            .iter()
            .find(|r| r.receipt.kind == DiscoveryIdentityReviewKind::SameAuthor)
            .unwrap()
            .state,
        DiscoveryIdentityGroupReviewState::Revoked
    );
    assert!(separate.groups.iter().all(|g| g.decision_ids.is_empty()));
}

#[test]
fn changed_native_source_marks_old_relation_stale_without_reusing_it_for_counts() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    submit(&owner, pair.clone());
    let before = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
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
                text: "New identity context.".into(),
            }],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let campaign = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let after = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(after.groups.len(), 2);
    assert_eq!(after.counts.stale_identity_reviews, 1);
    assert_eq!(
        after.reviews[0].state,
        DiscoveryIdentityGroupReviewState::EvidenceChanged
    );
    assert_eq!(before.reviews[0].receipt, after.reviews[0].receipt);
    assert_ne!(before.snapshot_hash, after.snapshot_hash);
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        campaign
    );
    revoke(&owner, &pair.campaign_id);
    assert_eq!(
        owner
            .get_discovery_identity_groups(&pair.campaign_id, 1)
            .unwrap()
            .counts
            .stale_identity_reviews,
        0
    );
}

#[test]
fn insufficient_and_different_person_reviews_do_not_create_membership_edges() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_other_links(dir.path());
    let mut input = super::super::submission::tests::input(&owner, pair.clone());
    input.decision = DiscoveryIdentityDecision::Insufficient;
    input.findings.clear();
    owner
        .submit_discovery_identity_review("operator", input)
        .unwrap();
    let mut input = super::super::submission::tests::input(&owner, pair.clone());
    input.decision_id = "different".into();
    input.decision = DiscoveryIdentityDecision::DifferentAuthors;
    for finding in &mut input.findings {
        finding.relationship = crate::DiscoveryIdentityRelationship::OtherPerson;
    }
    owner
        .submit_discovery_identity_review("operator", input)
        .unwrap();
    let snapshot = owner
        .get_discovery_identity_groups(&pair.campaign_id, 1)
        .unwrap();
    assert_eq!(snapshot.groups.len(), 2);
    assert_eq!(snapshot.reviews.len(), 2);
    assert!(
        snapshot
            .reviews
            .iter()
            .all(|r| r.state == DiscoveryIdentityGroupReviewState::Current)
    );
    assert!(snapshot.groups.iter().all(|g| !g.has_same_author_review));
}

#[test]
fn wrong_campaign_or_revision_cannot_project_current_identity_or_qualification() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    submit(&owner, pair.clone());
    assert!(owner.get_discovery_identity_groups("missing", 1).is_err());
    assert!(
        owner
            .get_discovery_identity_groups(&pair.campaign_id, 2)
            .is_err()
    );
    assert!(
        owner
            .get_discovery_identity_groups(&pair.campaign_id, 0)
            .is_err()
    );
}

#[test]
fn ledger_receipt_or_request_corruption_cannot_create_a_group() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    submit(&owner, pair.clone());
    for field in ["operator_account_id", "material_hash", "native_author_ids"] {
        let mut basis = owner
            .repository
            .read_discovery_identity_groups_basis(&pair.campaign_id, 1)
            .unwrap();
        let mut receipt: serde_json::Value =
            serde_json::from_str(&basis.reviews[0].receipt_json).unwrap();
        receipt[field] = if field == "native_author_ids" {
            serde_json::json!(["forged", "forged-other"])
        } else {
            serde_json::json!("forged")
        };
        basis.reviews[0].receipt_json = receipt.to_string();
        assert_eq!(build(basis).unwrap_err(), "identity_groups_ledger_invalid");
    }
    let mut basis = owner
        .repository
        .read_discovery_identity_groups_basis(&pair.campaign_id, 1)
        .unwrap();
    basis.reviews[0].request_hash = "0".repeat(64);
    assert_eq!(build(basis).unwrap_err(), "identity_groups_ledger_invalid");
}

#[test]
fn export_discovery_identity_node_oracle() {
    let Some(directory) = std::env::var_os("ONECREW_IDENTITY_ORACLE_DIRECTORY") else { return; };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let (owner, pair) = super::super::tests::setup_shampoo_owned_links(&directory);
    crate::discovery_contacts::legacy_approval_fixture(&directory);
    review_shampoo(&owner, &pair.campaign_id, &pair.left_search_hit_id);
    review_shampoo(&owner, &pair.campaign_id, &pair.right_search_hit_id);
    let snapshot = |name: &str| {
        let database = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
        database.execute("VACUUM INTO ?1", [directory.join(name).to_str().unwrap()]).unwrap();
    };
    snapshot("baseline.sqlite");
    let material = owner.prepare_discovery_identity_merge_material(pair.clone()).unwrap();
    let input = super::super::submission::tests::input(&owner, pair.clone());
    let hash = super::super::submission::request_hash("operator", &input).unwrap();
    let before = owner.get_discovery_identity_groups(&pair.campaign_id, 1).unwrap();
    let receipt = owner.submit_discovery_identity_review("operator", input.clone()).unwrap();
    let merged = owner.get_discovery_identity_groups(&pair.campaign_id, 1).unwrap();
    snapshot("merged.sqlite");
    let fresh_material = owner.prepare_discovery_review_material(&pair.campaign_id, 1, &pair.left_search_hit_id).unwrap();
    owner.submit_discovery_review("operator", crate::SubmitDiscoveryReviewInput {
        campaign_id: pair.campaign_id.clone(), expected_revision: 1, search_hit_id: pair.left_search_hit_id.clone(),
        material_hash: fresh_material.material_hash, request_id: "shampoo-updated".into(),
        reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: "source-bound shampoo reviewer".into() },
        findings: vec![crate::DiscoveryReviewFinding {
            criterion: crate::DiscoveryReviewCriterion::CategoryContent, outcome: crate::DiscoveryReviewOutcome::Supported,
            rationale: "The saved native author description explicitly identifies this as a shampoo review.".into(),
            citations: vec![crate::DiscoveryReviewQuote::Description { quote: "My shampoo review.".into() }],
            tool: None, match_level: None, country_code: None,
        }],
    }).unwrap();
    let stale = owner.get_discovery_identity_groups(&pair.campaign_id, 1).unwrap();
    snapshot("stale.sqlite");
    let revocation_input = RevokeDiscoveryIdentityReviewInput {
        campaign_id: pair.campaign_id.clone(), expected_revision: 1, decision_id: "revoke-groups".into(), revokes_decision_id: input.decision_id.clone(),
        reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: "identity correction".into() }, rationale: "Withdraw earlier relationship while retaining both sources.".into(),
    };
    let revoked_receipt = owner.revoke_discovery_identity_review("operator", revocation_input.clone()).unwrap();
    let revoked = owner.get_discovery_identity_groups(&pair.campaign_id, 1).unwrap();
    snapshot("revoked.sqlite");
    let cases = serde_json::json!({ "pair": pair, "material": material, "input": input, "request_hash": hash, "before": before, "receipt": receipt, "merged": merged, "stale": stale, "revocation_input": revocation_input, "revoked_receipt": revoked_receipt, "revoked": revoked });
    std::fs::write(directory.join("oracle.json"), serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
}
