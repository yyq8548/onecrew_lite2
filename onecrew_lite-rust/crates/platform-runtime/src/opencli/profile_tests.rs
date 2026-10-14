use super::*;

#[test]
fn auth_observations_share_only_the_requested_profile_without_refreshing_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    platform.opencli_auth_observations.lock().unwrap().push(OpenCliAuthObservation {
        platform: "instagram".into(), profile_ref: "profile-a".into(),
        status: onecrew_opencli::OpenCliAuthState::LoggedIn, checked_at: 42,
        method: "full", issue: None,
    });
    assert!(platform.opencli_auth_observations("profile-b").unwrap().is_empty());
    assert_eq!(platform.opencli_auth_observations("profile-a").unwrap()[0].checked_at, 42);
    assert_eq!(platform.opencli_auth_observations("profile-a").unwrap().len(), 1);
}
use onecrew_creator_runtime::{
    CreateDiscoveryCampaignInput, DiscoveryPlatformScope, ListDiscoveryCandidatesInput,
    ListDiscoverySearchHitsInput,
};

#[tokio::test]
async fn profile_read_requires_saved_author_and_approval_before_dependency_access() {
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    assert!(matches!(
        platform
            .read_discovery_profile_once("missing", 1, "hit")
            .await,
        Err(OpenCliDiscoverySearchError::Campaign(_))
    ));
}

/// Two bounded live reads into a disposable DB; no customer Campaign or messages.
#[tokio::test]
#[ignore = "requires OPENCLI_TEST_PROFILE and explicit connected Chrome; one Bilibili search and one author read"]
async fn campaign_profile_receipt_persists_real_author_evidence_and_budget() {
    let profile =
        std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated browser identity");
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    let owner = platform.creators().unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id:"profile-live".into(),brief:"Verify sourced AI-tool creators; no qualification from search alone".into(),
        opencli_version:VERIFIED_OPENCLI_VERSION.into(),
        criteria:json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
        scopes:vec![DiscoveryPlatformScope {platform:"bilibili".into(),profile_ref:profile.clone(),keywords:vec!["AI工具".into()]}],
        target_count:30,max_calls: Some(2),
    }).unwrap();
    owner.approve_discovery_campaign("profile-live", 1).unwrap();
    platform
        .search_discovery_once("profile-live", 1, "bilibili", &profile, 5)
        .await
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "profile-live".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert!(!hits.is_empty());
    let result = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "profile-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert!(matches!(result.step, DiscoveryStep::ReadProfile { .. }));
    let receipt = result.profile.expect("durable profile receipt");
    assert_eq!(receipt.status, "completed", "stop={:?}", receipt.stop_code);
    assert_eq!(receipt.claim.call_number, 2);
    assert_eq!(receipt.claim.profile_ref, profile);
    let hit = hits
        .iter()
        .find(|h| h.id == receipt.claim.search_hit_id)
        .unwrap();
    assert_eq!(
        hit.author_id.as_deref(),
        Some(receipt.claim.author_id.as_str())
    );
    assert_eq!(
        hit.author_url.as_deref(),
        Some(receipt.claim.author_url.as_str())
    );
    assert_eq!(result.campaign.used_calls, 2);
    assert_eq!(result.counts.qualified, 0);
    let candidates = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "profile-live".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.platform, "bilibili");
    assert_eq!(candidate.platform_account_id, receipt.claim.author_id);
    assert_eq!(candidate.profile_url, receipt.claim.author_url);
    assert_eq!(
        candidate.evidence["assessment"]["subscribers_normalized"],
        receipt.native_profile.as_ref().unwrap()["follower_count"]
    );
    assert_eq!(
        candidate.evidence["source_urls"],
        json!([receipt.claim.source_url, receipt.claim.author_url])
    );
    assert_ne!(candidate.verdict, "qualified");
    println!(
        "source={} author={} followers={} used_calls=2 qualified=0",
        receipt.claim.source_url,
        receipt.claim.author_url,
        receipt.native_profile.as_ref().unwrap()["follower_count"]
    );
    drop(owner);
    drop(platform);
    let reopened = LocalPlatform::open(dir.path()).unwrap();
    let saved = reopened
        .creators()
        .unwrap()
        .get_discovery_profile("profile-live", &receipt.claim.search_hit_id)
        .unwrap();
    assert_eq!(saved, receipt);
    assert_eq!(
        reopened
            .creators()
            .unwrap()
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "profile-live".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap(),
        candidates
    );
    let stopped = reopened
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "profile-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert_eq!(stopped.step, DiscoveryStep::BudgetExhausted);
    assert_eq!(stopped.campaign.used_calls, 2);
}

/// Three public X reads into an isolated Campaign; original-post and current
/// profile IDs must agree, while missing content proof still blocks qualification.
#[tokio::test]
#[ignore = "requires OPENCLI_TEST_PROFILE and a connected X login"]
async fn x_search_profile_receipt_matches_original_native_identity_live() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("connected browser identity");
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    let owner = platform.creators().unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id: "x-live".into(),
        brief: "Verify observed X authors".into(),
        opencli_version: VERIFIED_OPENCLI_VERSION.into(),
        criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
        scopes: vec![DiscoveryPlatformScope {
            platform: "twitter".into(), profile_ref: profile.clone(), keywords: vec!["AI tools".into()],
        }],
        target_count: 5, max_calls: Some(3),
    }).unwrap();
    owner.approve_discovery_campaign("x-live", 1).unwrap();
    platform
        .search_discovery_once("x-live", 1, "twitter", &profile, 5)
        .await
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "x-live".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert!(!hits.is_empty());
    assert!(
        hits.iter()
            .all(|hit| hit.author_id.is_some() && hit.author_url.is_some())
    );
    let identity = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "x-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap()
        .identity
        .expect("original-post identity");
    assert_eq!(
        identity.status, "completed",
        "stop={:?}",
        identity.stop_code
    );
    let result = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "x-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert!(matches!(result.step, DiscoveryStep::ReadProfile { .. }));
    let receipt = result.profile.unwrap();
    assert_eq!(receipt.status, "completed", "stop={:?}", receipt.stop_code);
    assert_eq!(receipt.claim.call_number, 3);
    assert_eq!(result.counts.qualified, 0);
    let candidates = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "x-live".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(candidates.len(), 1);
    let followers = receipt.native_profile.as_ref().unwrap()["follower_count"].as_u64();
    assert_eq!(
        candidates[0].verdict,
        if followers.is_some_and(|count| count < 10_000) {
            "rejected"
        } else {
            "pending"
        }
    );
    assert_eq!(candidates[0].evidence["author_identity_kind"], "native_id");
    assert_eq!(
        Some(candidates[0].platform_account_id.as_str()),
        identity.account_id.as_deref()
    );
    assert_eq!(
        receipt.native_profile.as_ref().unwrap()["account_id"].as_str(),
        identity.account_id.as_deref()
    );
    if candidates[0].verdict == "pending" {
        assert!(
            !candidates[0].evidence["assessment"]["reasons"]
                .as_array()
                .unwrap()
                .contains(&json!("account_id_unverified"))
        );
    }
    drop(owner);
    drop(platform);
    let reopened = LocalPlatform::open(dir.path()).unwrap();
    assert_eq!(
        reopened
            .creators()
            .unwrap()
            .get_discovery_profile("x-live", &receipt.claim.search_hit_id)
            .unwrap(),
        receipt
    );
}

/// Four bounded public reads in a disposable Campaign. Search, identity, profile and
/// focal-post content shares the native author; no demo is certified.
#[tokio::test]
#[ignore = "requires OPENCLI_TEST_PROFILE and a connected X login"]
async fn x_native_content_follows_sourced_author_without_qualification_live() {
    let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("connected browser identity");
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    let owner = platform.creators().unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id:"x-content-live".into(),brief:"Review author-bound recent X posts".into(),
        opencli_version:VERIFIED_OPENCLI_VERSION.into(),
        criteria:json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
        scopes:vec![DiscoveryPlatformScope {platform:"twitter".into(),profile_ref:profile.clone(),keywords:vec!["AI tools".into()]}],
        target_count:5,max_calls: Some(4),
    }).unwrap();
    owner
        .approve_discovery_campaign("x-content-live", 1)
        .unwrap();
    platform
        .search_discovery_once("x-content-live", 1, "twitter", &profile, 5)
        .await
        .unwrap();
    let identity = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "x-content-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap()
        .identity
        .expect("original-post identity");
    assert_eq!(
        identity.status, "completed",
        "stop={:?}",
        identity.stop_code
    );
    let profile_step = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "x-content-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert!(matches!(
        profile_step.step,
        DiscoveryStep::ReadProfile { .. }
    ));
    let profile_receipt = profile_step.profile.unwrap();
    assert_eq!(
        profile_receipt.status, "completed",
        "stop={:?}",
        profile_receipt.stop_code
    );
    let content_step = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "x-content-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert!(matches!(
        content_step.step,
        DiscoveryStep::ReadContent { .. }
    ));
    let content = content_step.content.unwrap();
    assert_eq!(content.status, "completed", "stop={:?}", content.stop_code);
    assert_eq!(content.claim.author_id, profile_receipt.claim.author_id);
    assert_eq!(content.claim.call_number, 4);
    assert_eq!(
        content.native_content.as_ref().unwrap()["identity"]["account_id"].as_str(),
        identity.account_id.as_deref()
    );
    assert_eq!(content_step.counts.qualified, 0);
    drop(owner);
    drop(platform);
    let reopened = LocalPlatform::open(dir.path()).unwrap();
    assert_eq!(
        reopened
            .creators()
            .unwrap()
            .get_discovery_content("x-content-live", &content.claim.search_hit_id)
            .unwrap(),
        content
    );
}

/// Three public reads in a disposable DB. Explicitly select a new-machine
/// Chrome identity; never reuse the handoff machine's profile reference.
#[tokio::test]
#[ignore = "requires OPENCLI_TEST_PROFILE, OpenCLI 1.8.8 and an explicitly connected Chrome identity"]
async fn campaign_content_receipt_binds_video_to_observed_author() {
    let profile =
        std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated browser identity");
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    let owner = platform.creators().unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "content-live".into(),
            brief: "Read one public Bilibili video".into(),
            opencli_version: VERIFIED_OPENCLI_VERSION.into(),
            criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "bilibili".into(),
                profile_ref: profile.clone(),
                keywords: vec!["AI工具".into()],
            }],
            target_count: 20,
            max_calls: Some(3),
        })
        .unwrap();
    owner.approve_discovery_campaign("content-live", 1).unwrap();
    platform
        .search_discovery_once("content-live", 1, "bilibili", &profile, 1)
        .await
        .unwrap();
    let profile_step = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "content-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert!(matches!(
        profile_step.step,
        DiscoveryStep::ReadProfile { .. }
    ));
    assert_eq!(
        profile_step
            .profile
            .as_ref()
            .expect("profile receipt")
            .status,
        "completed"
    );
    let content_step = platform
        .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
            excluded_platforms: vec![],
            campaign_id: "content-live".into(),
            expected_revision: 1,
        })
        .await
        .unwrap();
    assert!(matches!(
        content_step.step,
        DiscoveryStep::ReadContent { .. }
    ));
    let receipt = content_step.content.expect("content receipt");
    assert_eq!(receipt.status, "completed", "stop={:?}", receipt.stop_code);
    assert_eq!(receipt.claim.call_number, 3);
    assert_eq!(
        receipt.native_content.as_ref().unwrap()["author_id"],
        receipt.claim.author_id
    );
    assert_eq!(
        receipt.native_content.as_ref().unwrap()["source_url"],
        receipt.claim.source_url
    );
    assert_eq!(content_step.counts.qualified, 0);
    drop(owner);
    drop(platform);
    let reopened = LocalPlatform::open(dir.path()).unwrap();
    assert_eq!(
        reopened
            .creators()
            .unwrap()
            .get_discovery_content("content-live", &receipt.claim.search_hit_id)
            .unwrap(),
        receipt
    );
}
