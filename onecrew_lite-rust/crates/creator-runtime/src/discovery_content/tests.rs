use crate::*;
use serde_json::json;

mod facebook;
mod native;

fn setup(max_calls: u32) -> (tempfile::TempDir, CreatorRuntime, DiscoverySearchHit) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "content".into(),
            brief: "AI tool creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "bilibili".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 20,
            max_calls: Some(max_calls),
        })
        .unwrap();
    owner.approve_discovery_campaign("content", 1).unwrap();
    let search = owner
        .claim_discovery_search("content", 1, "bilibili", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.bilibili.com/video/BV1RYqHYnEw8".into(),
                title: "Demo".into(),
                author_label: "Creator".into(),
                author_id: Some("123".into()),
                author_url: Some("https://space.bilibili.com/123".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "content".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let profile = owner
        .claim_discovery_profile("content", 1, &hit.id)
        .unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim:profile.clone(),
        native_profile:Some(json!({"mid":"123","profile_url":profile.author_url,"name":"Creator","follower_count":15000})),
        stop_code:None}).unwrap();
    (dir, owner, hit)
}

fn success(claim: DiscoveryContentClaim) -> FinishDiscoveryContentInput {
    FinishDiscoveryContentInput {
        native_content: Some(json!({
            "bvid":"BV1RYqHYnEw8","author_id":claim.author_id,"author":"Creator",
            "source_url":claim.source_url,"title":"Demo","description":"Public description",
            "description_truncated":false,"requires_payment":false,"pay_preview":false,
            "payment_status_known":false,
        })),
        claim,
        stop_code: None,
    }
}

fn setup_x(max_calls: u32) -> (tempfile::TempDir, CreatorRuntime, DiscoverySearchHit) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "x-content".into(),
            brief: "Review recent original X content".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "twitter".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 5,
            max_calls: Some(max_calls),
        })
        .unwrap();
    owner.approve_discovery_campaign("x-content", 1).unwrap();
    let search = owner
        .claim_discovery_search("x-content", 1, "twitter", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://x.com/creator_1/status/123456789".into(),
                title: "Demo".into(),
                author_label: "Creator".into(),
                author_id: Some("creator_1".into()),
                author_url: Some("https://x.com/creator_1".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "x-content".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let identity = owner
        .claim_discovery_identity("x-content", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_identity(FinishDiscoveryIdentityInput {
            claim: identity,
            account_id: Some("987654321".into()),
            observed_source_url: Some(hit.source_url.clone()),
            stop_code: None,
        })
        .unwrap();
    let profile = owner
        .claim_discovery_profile("x-content", 1, &hit.id)
        .unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim:profile,
        native_profile:Some(json!({"handle":"creator_1","profile_url":"https://x.com/creator_1","name":"Creator",
            "bio":"","self_declared_location":"","follower_count":15000,"identity_kind":"native_id","account_id":"987654321"})),stop_code:None}).unwrap();
    (dir, owner, hit)
}

fn x_success(claim: DiscoveryContentClaim) -> FinishDiscoveryContentInput {
    FinishDiscoveryContentInput {
        native_content: Some(json!({
            "adapter_version":"1.0.0", "identity": {
                "adapter_version":"1.0.0", "post_id":"123456789", "account_id":"987654321",
                "handle":"creator_1", "profile_url":"https://x.com/creator_1",
                "source_url":"https://x.com/creator_1/status/123456789",
                "identity_kind":"native_post_author_id", "evidence_source":"x_tweet_detail" },
            "text":"AI demo", "text_truncated":false, "text_source":"legacy_full_text",
            "created_at":"Fri Sep 25 23:00:10 +0000 2026", "has_media":true,
            "quotes_another_post":false, "expanded_urls":["https://example.com/tool"]
        })),
        claim,
        stop_code: None,
    }
}

#[test]
fn x_native_content_is_author_bound_durable_and_never_proves_demo() {
    let (dir, owner, hit) = setup_x(4);
    assert_eq!(
        owner.plan_discovery_step("x-content", 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.id.clone()
        }
    );
    let claim = owner
        .claim_discovery_content("x-content", 1, &hit.id)
        .unwrap();
    assert_eq!(claim.call_number, 4);
    for (path, value) in [
        ("account_id", json!("111")),
        ("profile_url", json!("https://x.com/other")),
        ("post_id", json!("333")),
        ("identity_kind", json!("native_id")),
    ] {
        let mut wrong = x_success(claim.clone());
        wrong.native_content.as_mut().unwrap()["identity"][path] = value;
        assert!(owner.finish_discovery_content(wrong).is_err());
    }
    let mut wrong = x_success(claim.clone());
    wrong.native_content.as_mut().unwrap()["identity"]["source_url"] =
        json!("https://x.com/other/status/123456789");
    assert!(owner.finish_discovery_content(wrong).is_err());
    let receipt = owner.finish_discovery_content(x_success(claim)).unwrap();
    assert_eq!(receipt.status, "completed");
    assert_eq!(
        owner
            .count_discovery_candidates("x-content")
            .unwrap()
            .qualified,
        0
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_content("x-content", &hit.id)
            .unwrap(),
        receipt
    );
}

#[test]
fn content_is_budgeted_author_bound_durable_and_never_qualifies() {
    let (dir, owner, hit) = setup(5);
    assert_eq!(
        owner.plan_discovery_step("content", 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.id.clone()
        }
    );
    assert!(
        owner
            .claim_discovery_content("content", 2, &hit.id)
            .is_err()
    );
    assert!(owner.claim_discovery_content("other", 1, &hit.id).is_err());
    let claim = owner
        .claim_discovery_content("content", 1, &hit.id)
        .unwrap();
    assert_eq!(claim.call_number, 3);
    assert_eq!(
        owner.plan_discovery_step("content", 1).unwrap(),
        DiscoveryStep::Busy
    );
    assert!(
        owner
            .claim_discovery_content("content", 1, &hit.id)
            .is_err()
    );
    let mut wrong = success(claim.clone());
    wrong.native_content.as_mut().unwrap()["author_id"] = "999".into();
    assert!(owner.finish_discovery_content(wrong).is_err());
    let mut wrong = success(claim.clone());
    wrong.native_content.as_mut().unwrap()["bvid"] = "BV1RYqHYnEw9".into();
    assert!(owner.finish_discovery_content(wrong).is_err());
    let mut wrong = success(claim.clone());
    wrong
        .native_content
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("description");
    assert!(owner.finish_discovery_content(wrong).is_err());
    let mut wrong = success(claim.clone());
    wrong.native_content.as_mut().unwrap()["secret"] = "should never persist".into();
    assert!(owner.finish_discovery_content(wrong).is_err());
    let receipt = owner
        .finish_discovery_content(success(claim.clone()))
        .unwrap();
    assert_eq!(receipt.status, "completed");
    assert!(owner.finish_discovery_content(success(claim)).is_err());
    assert_eq!(
        owner
            .count_discovery_candidates("content")
            .unwrap()
            .qualified,
        0
    );
    assert_eq!(
        owner.get_discovery_campaign("content").unwrap().used_calls,
        3
    );
    assert!(owner.get_discovery_content("other", &hit.id).is_err());
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.get_discovery_content("content", &hit.id).unwrap(),
        receipt
    );
}

#[test]
fn interrupted_claim_reserves_another_call_and_old_claim_cannot_finish() {
    let (_dir, owner, hit) = setup(4);
    let old = owner
        .claim_discovery_content("content", 1, &hit.id)
        .unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    assert_eq!(
        owner.plan_discovery_step("content", 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.id.clone()
        }
    );
    let new = owner
        .claim_discovery_content("content", 1, &hit.id)
        .unwrap();
    assert_eq!(new.call_number, old.call_number + 1);
    assert!(owner.finish_discovery_content(success(old)).is_err());
    let receipt = owner.finish_discovery_content(success(new)).unwrap();
    assert_eq!(receipt.status, "completed");
    assert_eq!(
        owner.get_discovery_campaign("content").unwrap().used_calls,
        4
    );
}

#[test]
fn provider_stops_and_budget_limit_do_not_retry_or_qualify() {
    let (_dir, exhausted, hit) = setup(2);
    assert_eq!(
        exhausted.plan_discovery_step("content", 1).unwrap(),
        DiscoveryStep::BudgetExhausted
    );
    assert!(
        exhausted
            .claim_discovery_content("content", 1, &hit.id)
            .is_err()
    );
    for (stop, expected) in [
        ("opencli_auth_required", "unauthorized"),
        ("opencli_risk_control", "risk_control_stop"),
    ] {
        let (_dir, owner, hit) = setup(3);
        let claim = owner
            .claim_discovery_content("content", 1, &hit.id)
            .unwrap();
        let receipt = owner
            .finish_discovery_content(FinishDiscoveryContentInput {
                claim,
                native_content: None,
                stop_code: Some(stop.into()),
            })
            .unwrap();
        assert_eq!(receipt.status, "failed");
        assert_eq!(
            owner
                .get_discovery_campaign("content")
                .unwrap()
                .platform_runs[0]
                .status,
            expected
        );
        assert_eq!(
            owner
                .count_discovery_candidates("content")
                .unwrap()
                .qualified,
            0
        );
        assert_eq!(
            owner.plan_discovery_step("content", 1).unwrap(),
            DiscoveryStep::BudgetExhausted
        );
    }
}

#[test]
fn receipt_and_platform_checkpoint_roll_back_together() {
    let (dir, owner, hit) = setup(4);
    let claim = owner
        .claim_discovery_content("content", 1, &hit.id)
        .unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_content_receipt BEFORE UPDATE ON discovery_content_reads
        BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END;",
        )
        .unwrap();
    assert!(
        owner
            .finish_discovery_content(success(claim.clone()))
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_content("content", &hit.id)
            .unwrap()
            .status,
        "reading"
    );
    assert_eq!(
        owner
            .get_discovery_campaign("content")
            .unwrap()
            .platform_runs[0]
            .status,
        "searching"
    );
    assert_eq!(
        owner.get_discovery_campaign("content").unwrap().used_calls,
        3
    );
    connection
        .execute_batch("DROP TRIGGER reject_content_receipt")
        .unwrap();
    owner.finish_discovery_content(success(claim)).unwrap();
}
