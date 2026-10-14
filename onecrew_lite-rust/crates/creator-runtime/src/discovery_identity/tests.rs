use crate::*;
use serde_json::json;

#[test]
fn library_snapshot_requires_exact_x_permanent_profile_and_post_binding() {
    let (dir, owner, hit) = setup(3);
    let identity = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    owner.finish_discovery_identity(success(identity)).unwrap();
    let claim = owner.claim_discovery_profile("identity", 1, &hit).unwrap();
    owner
        .finish_discovery_profile(native_profile(claim, json!(15000)))
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot("identity", 1)
        .unwrap();
    assert_eq!(
        snapshot.rows[0].native_author_id.as_deref(),
        Some("987654321")
    );
    assert_eq!(snapshot.rows[0].verdict, "pending");
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_identity_reads SET account_id='123456789'",
        [],
    )
    .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("identity", 1)
            .unwrap()
            .rows[0]
            .verdict,
        "unverified"
    );
    db.execute(
        "UPDATE discovery_identity_reads SET account_id='987654321'",
        [],
    )
    .unwrap();
    db.execute("DELETE FROM discovery_profile_identity_matches", [])
        .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("identity", 1)
            .unwrap()
            .rows[0]
            .verdict,
        "unverified"
    );
}

fn setup(budget: u32) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "identity".into(),
            brief: "AI tool creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "twitter".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 30,
            max_calls: Some(budget),
        })
        .unwrap();
    assert!(
        owner
            .claim_discovery_identity("identity", 1, "missing")
            .is_err()
    );
    owner.approve_discovery_campaign("identity", 1).unwrap();
    let claim = owner
        .claim_discovery_search("identity", 1, "twitter", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://x.com/i/status/123456789".into(),
                title: "Demo".into(),
                author_label: "Creator".into(),
                author_id: Some("creator_1".into()),
                author_url: Some("https://x.com/creator_1".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "identity".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    (dir, owner, hit.id)
}

fn success(claim: DiscoveryIdentityClaim) -> FinishDiscoveryIdentityInput {
    FinishDiscoveryIdentityInput {
        claim,
        account_id: Some("987654321".into()),
        observed_source_url: Some("https://x.com/creator_1/status/123456789".into()),
        stop_code: None,
    }
}

#[test]
fn identity_claim_is_budgeted_bound_and_durable_without_qualifying_a_creator() {
    let (dir, owner, hit) = setup(2);
    assert!(owner.claim_discovery_identity("identity", 2, &hit).is_err());
    assert!(owner.claim_discovery_identity("other", 1, &hit).is_err());
    let claim = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    assert_eq!(claim.call_number, 2);
    assert_eq!(claim.profile_ref, "dedicated");
    assert_eq!(claim.post_id, "123456789");
    assert!(owner.claim_discovery_identity("identity", 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign("identity").unwrap().used_calls,
        2
    );
    let receipt = owner
        .finish_discovery_identity(success(claim.clone()))
        .unwrap();
    assert_eq!(receipt.status, "completed");
    assert!(owner.finish_discovery_identity(success(claim)).is_err());
    assert!(owner.get_discovery_identity("other", &hit).is_err());
    assert_eq!(
        owner
            .count_discovery_candidates("identity")
            .unwrap()
            .qualified,
        0
    );
    assert_eq!(
        owner
            .count_discovery_candidates("identity")
            .unwrap()
            .pending,
        0
    );
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner.get_discovery_identity("identity", &hit).unwrap(),
        receipt
    );
    assert_eq!(
        owner.get_discovery_campaign("identity").unwrap().used_calls,
        2
    );
}

#[test]
fn planner_reads_identity_once_before_profile_and_preserves_budget_for_the_next_step() {
    let (_dir, owner, hit) = setup(4);
    for _ in 0..2 {
        assert_eq!(
            owner.plan_discovery_step("identity", 1).unwrap(),
            DiscoveryStep::ReadIdentity {
                search_hit_id: hit.clone()
            }
        );
    }
    assert_eq!(
        owner.get_discovery_campaign("identity").unwrap().used_calls,
        1
    );
    let claim = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    assert_eq!(
        owner.plan_discovery_step("identity", 1).unwrap(),
        DiscoveryStep::Busy
    );
    owner.finish_discovery_identity(success(claim)).unwrap();
    assert_eq!(
        owner.plan_discovery_step("identity", 1).unwrap(),
        DiscoveryStep::ReadProfile { search_hit_id: hit }
    );
    assert_eq!(
        owner.get_discovery_campaign("identity").unwrap().used_calls,
        2
    );
    assert_eq!(
        owner
            .count_discovery_candidates("identity")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn identity_mismatches_cannot_finish_or_rewrite_the_reserved_claim() {
    let (_dir, owner, hit) = setup(3);
    let claim = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    for bad in ["0", "01", "1.2", "123x", "111111111111111111111"] {
        let mut input = success(claim.clone());
        input.account_id = Some(bad.into());
        assert!(owner.finish_discovery_identity(input).is_err());
    }
    let mut input = success(claim.clone());
    input.observed_source_url = Some("https://x.com/other/status/123456789".into());
    assert!(owner.finish_discovery_identity(input).is_err());
    let mut input = success(claim.clone());
    input.claim.profile_ref = "other-profile".into();
    assert!(owner.finish_discovery_identity(input).is_err());
    let mut input = success(claim.clone());
    input.claim.call_number += 1;
    assert!(owner.finish_discovery_identity(input).is_err());
    assert_eq!(
        owner
            .get_discovery_identity("identity", &hit)
            .unwrap()
            .status,
        "reading"
    );
    owner.finish_discovery_identity(success(claim)).unwrap();
}

#[test]
fn identity_failure_and_exhausted_budget_never_retry() {
    let (_dir, owner, hit) = setup(1);
    assert!(owner.claim_discovery_identity("identity", 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign("identity").unwrap().used_calls,
        1
    );
    for (code, state) in [
        ("opencli_auth_required", "unauthorized"),
        ("opencli_risk_control", "risk_control_stop"),
        ("opencli_invalid_output", "completed"),
    ] {
        let (_dir, owner, hit) = setup(3);
        let claim = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
        let receipt = owner
            .finish_discovery_identity(FinishDiscoveryIdentityInput {
                claim,
                account_id: None,
                observed_source_url: None,
                stop_code: Some(code.into()),
            })
            .unwrap();
        assert_eq!(receipt.status, "failed");
        assert_eq!(
            owner
                .get_discovery_campaign("identity")
                .unwrap()
                .platform_runs[0]
                .status,
            state
        );
        assert!(owner.claim_discovery_identity("identity", 1, &hit).is_err());
        assert_eq!(
            owner.plan_discovery_step("identity", 1).unwrap(),
            DiscoveryStep::SearchExhausted
        );
        assert_eq!(
            owner.get_discovery_campaign("identity").unwrap().used_calls,
            2
        );
    }
}

#[test]
fn identity_restart_marks_unknown_read_interrupted_without_replay_or_extra_budget() {
    let (dir, owner, hit) = setup(3);
    let claim = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
    let receipt = owner.get_discovery_identity("identity", &hit).unwrap();
    assert_eq!(receipt.status, "failed");
    assert_eq!(
        receipt.stop_code.as_deref(),
        Some("interrupted_identity_read")
    );
    assert!(owner.finish_discovery_identity(success(claim)).is_err());
    assert!(owner.claim_discovery_identity("identity", 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign("identity").unwrap().used_calls,
        2
    );
    assert_eq!(
        owner.plan_discovery_step("identity", 1).unwrap(),
        DiscoveryStep::SearchExhausted
    );
}

#[test]
fn identity_receipt_failure_rolls_back_platform_finish() {
    let (dir, owner, hit) = setup(3);
    let claim = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER refuse_identity BEFORE UPDATE ON discovery_identity_reads
        BEGIN SELECT RAISE(ABORT,'test receipt failure'); END;",
    )
    .unwrap();
    assert!(
        owner
            .finish_discovery_identity(success(claim.clone()))
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("identity")
            .unwrap()
            .platform_runs[0]
            .status,
        "searching"
    );
    assert_eq!(
        owner
            .get_discovery_identity("identity", &hit)
            .unwrap()
            .status,
        "reading"
    );
    db.execute_batch("DROP TRIGGER refuse_identity;").unwrap();
    owner.finish_discovery_identity(success(claim)).unwrap();
}

#[test]
fn identity_targets_require_exact_saved_post_and_observed_handle() {
    use super::post_id_for_source;
    assert_eq!(
        post_id_for_source("creator_1", "https://x.com/i/status/123").unwrap(),
        "123"
    );
    assert_eq!(
        post_id_for_source("creator_1", "https://x.com/creator_1/status/123").unwrap(),
        "123"
    );
    for url in [
        "https://x.com/other/status/123",
        "https://x.com/i/status/01",
        "https://x.com/i/status/123?token=secret",
        "https://evil.test/i/status/123",
        "https://user:pass@x.com/i/status/123",
        "https://x.com:443/i/status/123",
    ] {
        assert!(post_id_for_source("creator_1", url).is_err());
    }
}

#[test]
fn identity_reservation_failure_rolls_back_budget_and_platform_state() {
    let (dir, owner, hit) = setup(3);
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER refuse_identity_claim BEFORE INSERT ON discovery_identity_reads
        BEGIN SELECT RAISE(ABORT,'test reservation failure'); END;",
    )
    .unwrap();
    assert!(owner.claim_discovery_identity("identity", 1, &hit).is_err());
    let campaign = owner.get_discovery_campaign("identity").unwrap();
    assert_eq!(campaign.used_calls, 1);
    assert_eq!(campaign.platform_runs[0].status, "completed");
    assert!(owner.get_discovery_identity("identity", &hit).is_err());
    db.execute_batch("DROP TRIGGER refuse_identity_claim;")
        .unwrap();
    assert_eq!(
        owner
            .claim_discovery_identity("identity", 1, &hit)
            .unwrap()
            .call_number,
        2
    );
}

#[test]
fn identity_finish_rechecks_approval_after_provider_execution() {
    let (dir, owner, hit) = setup(3);
    let claim = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_campaign_revisions SET approved_at=NULL WHERE discovery_campaign_id='identity'", []).unwrap();
    assert!(owner.finish_discovery_identity(success(claim)).is_err());
    assert_eq!(
        owner
            .get_discovery_identity("identity", &hit)
            .unwrap()
            .status,
        "reading"
    );
    assert_eq!(
        owner.get_discovery_campaign("identity").unwrap().used_calls,
        2
    );
}

fn native_profile(
    claim: DiscoveryProfileClaim,
    followers: serde_json::Value,
) -> FinishDiscoveryProfileInput {
    FinishDiscoveryProfileInput {
        claim,
        native_profile: Some(json!({
            "account_id":"987654321","handle":"creator_1","profile_url":"https://x.com/creator_1",
            "name":"Creator","follower_count":followers,"identity_kind":"native_id"
        })),
        stop_code: None,
    }
}

#[test]
fn native_profile_distinguishes_zero_from_unknown_and_preserves_matched_identity() {
    for (followers, verdict) in [
        (json!(0), "rejected"),
        (json!(null), "pending"),
        (json!(15000), "pending"),
    ] {
        let (dir, owner, hit) = setup(3);
        let identity = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
        owner.finish_discovery_identity(success(identity)).unwrap();
        let claim = owner.claim_discovery_profile("identity", 1, &hit).unwrap();
        let receipt = owner
            .finish_discovery_profile(native_profile(claim, followers.clone()))
            .unwrap();
        assert_eq!(
            receipt.native_profile.as_ref().unwrap()["follower_count"],
            followers
        );
        let rows = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "identity".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].platform_account_id, "987654321");
        assert_eq!(rows[0].verdict, verdict);
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT account_id FROM discovery_profile_identity_matches",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "987654321"
        );
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened.get_discovery_profile("identity", &hit).unwrap(),
            receipt
        );
        assert_eq!(
            reopened
                .get_discovery_campaign("identity")
                .unwrap()
                .used_calls,
            3
        );
    }
}

#[test]
fn native_profile_candidate_failure_rolls_back_receipt_and_identity_match_together() {
    let (dir, owner, hit) = setup(3);
    let identity = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    owner.finish_discovery_identity(success(identity)).unwrap();
    let claim = owner.claim_discovery_profile("identity", 1, &hit).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch("CREATE TRIGGER refuse_native_candidate BEFORE INSERT ON discovery_candidates BEGIN SELECT RAISE(ABORT,'test candidate failure'); END;").unwrap();
    assert!(
        owner
            .finish_discovery_profile(native_profile(claim.clone(), json!(15000)))
            .is_err()
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_profile_identity_matches",
            [],
            |row| row.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        owner
            .get_discovery_profile("identity", &hit)
            .unwrap()
            .status,
        "reading"
    );
    assert_eq!(
        owner
            .get_discovery_campaign("identity")
            .unwrap()
            .platform_runs[0]
            .status,
        "searching"
    );
    db.execute_batch("DROP TRIGGER refuse_native_candidate;")
        .unwrap();
    owner
        .finish_discovery_profile(native_profile(claim, json!(15000)))
        .unwrap();
}

#[test]
fn native_profile_rechecks_original_identity_before_claim_and_finish() {
    for mutation in [
        "expected_revision=2",
        "profile_ref='other'",
        "author_handle='other'",
        "source_url='https://x.com/i/status/555'",
    ] {
        let (dir, owner, hit) = setup(3);
        let identity = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
        owner.finish_discovery_identity(success(identity)).unwrap();
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        // Isolated corruption fixture, never production data repair.
        db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        db.execute(
            &format!("UPDATE discovery_identity_reads SET {mutation}"),
            [],
        )
        .unwrap();
        assert!(owner.claim_discovery_profile("identity", 1, &hit).is_err());
        assert_eq!(
            owner.get_discovery_campaign("identity").unwrap().used_calls,
            2
        );
    }
    let (dir, owner, hit) = setup(3);
    let identity = owner.claim_discovery_identity("identity", 1, &hit).unwrap();
    owner.finish_discovery_identity(success(identity)).unwrap();
    let claim = owner.claim_discovery_profile("identity", 1, &hit).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_identity_reads SET account_id='111'", [])
        .unwrap();
    assert!(
        owner
            .finish_discovery_profile(native_profile(claim, json!(15000)))
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_profile("identity", &hit)
            .unwrap()
            .status,
        "reading"
    );
}

#[test]
fn native_profile_deduplicates_account_across_handles_without_rewriting_first_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "aliases".into(),
            brief: "Observe distinct X locators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "twitter".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 30,
            max_calls: Some(5),
        })
        .unwrap();
    owner.approve_discovery_campaign("aliases", 1).unwrap();
    let search = owner
        .claim_discovery_search("aliases", 1, "twitter", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: [("creator_1", "123"), ("creator_2", "456")]
                .into_iter()
                .map(|(handle, post)| DiscoverySearchHitInput {
                    source_url: format!("https://x.com/{handle}/status/{post}"),
                    title: "Demo".into(),
                    author_label: handle.into(),
                    author_id: Some(handle.into()),
                    author_url: Some(format!("https://x.com/{handle}")),
                })
                .collect(),
        })
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "aliases".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    let mut first = None;
    for hit in hits {
        let claim = owner
            .claim_discovery_identity("aliases", 1, &hit.id)
            .unwrap();
        owner
            .finish_discovery_identity(FinishDiscoveryIdentityInput {
                claim,
                account_id: Some("987654321".into()),
                observed_source_url: Some(hit.source_url),
                stop_code: None,
            })
            .unwrap();
        let claim = owner
            .claim_discovery_profile("aliases", 1, &hit.id)
            .unwrap();
        let native = json!({"handle":claim.author_id,"profile_url":claim.author_url,"name":"Creator",
            "follower_count":15000,"identity_kind":"native_id","account_id":"987654321"});
        owner
            .finish_discovery_profile(FinishDiscoveryProfileInput {
                claim,
                native_profile: Some(native),
                stop_code: None,
            })
            .unwrap();
        let candidates = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "aliases".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].platform_account_id, "987654321");
        if let Some(original) = &first {
            assert_eq!(&candidates, original)
        } else {
            first = Some(candidates)
        };
    }
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_profile_identity_matches",
            [],
            |row| row.get::<_, u32>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        owner.get_discovery_campaign("aliases").unwrap().used_calls,
        5
    );
}
