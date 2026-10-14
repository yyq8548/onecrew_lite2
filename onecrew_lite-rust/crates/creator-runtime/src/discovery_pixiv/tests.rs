use crate::*;
use serde_json::json;
const CAMPAIGN: &str = "pixiv-flow";
const SOURCE: &str = "https://www.pixiv.net/artworks/456";
const PROFILE: &str = "https://www.pixiv.net/users/123";
fn setup(
    criteria: serde_json::Value,
    contacts: bool,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: CAMPAIGN.into(),
            brief: "Illustration creators".into(),
            opencli_version: "1.8.8".into(),
            criteria,
            target_count: 2,
            max_calls: Some(20),
            scopes: vec![DiscoveryPlatformScope {
                platform: "pixiv".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["illustration".into()],
            }],
        })
        .unwrap();
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    let claim = owner
        .claim_discovery_search(CAMPAIGN, 1, "pixiv", "dedicated")
        .unwrap();
    let hit = DiscoverySearchHitInput {
        source_url: SOURCE.into(),
        title: "Original illustration".into(),
        author_label: "Artist".into(),
        author_id: Some("123".into()),
        author_url: Some(PROFILE.into()),
    };
    for (id, url) in [
        ("Artist", PROFILE),
        ("123", "https://www.pixiv.net/users/999"),
    ] {
        let mut wrong = hit.clone();
        wrong.author_id = Some(id.into());
        wrong.author_url = Some(url.into());
        assert!(
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim: claim.clone(),
                    status: "completed".into(),
                    stop_code: None,
                    search_hits: vec![wrong]
                })
                .is_err()
        );
    }
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![hit],
        })
        .unwrap();
    let DiscoveryStep::ReadProfile { search_hit_id: hit } =
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
    else {
        panic!("expected native profile")
    };
    let claim = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
    let bio = if contacts {
        "Original illustration artist. Business art@example.com"
    } else {
        "Original illustration artist"
    };
    let profile = json!({"adapter_version":"1.0.0","author_id":"123","author_label":"Artist","profile_url":PROFILE,"follower_count":null,"bio":bio,"public_links":["https://artist.example.com/contact"],"evidence_source":"native_ajax_response"});
    for (key, bad) in [
        ("author_id", json!("999")),
        ("profile_url", json!("https://www.pixiv.net/users/999")),
        ("follower_count", json!(12000)),
        ("public_links", json!(["https://127.0.0.1/contact"])),
        ("session", json!("must not persist")),
    ] {
        let mut wrong = profile.clone();
        wrong[key] = bad;
        assert!(
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim: claim.clone(),
                    native_profile: Some(wrong),
                    stop_code: None
                })
                .is_err()
        );
    }
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim: claim.clone(),
            native_profile: Some(profile),
            stop_code: None,
        })
        .unwrap();
    assert!(owner.claim_discovery_profile(CAMPAIGN, 1, &hit).is_err());
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .is_err(),
        "profile alone cannot establish artwork ownership"
    );
    assert_eq!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.clone()
        }
    );
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    let content = json!({"post_id":"456","author_id":"123","source_url":SOURCE,"text":"Original illustration review of clothing and fashion design.","text_truncated":false});
    for (key, bad) in [
        ("post_id", json!("999")),
        ("author_id", json!("999")),
        ("source_url", json!("https://www.pixiv.net/artworks/999")),
        ("text_truncated", json!("false")),
        ("comments", json!([])),
    ] {
        let mut wrong = content.clone();
        wrong[key] = bad;
        assert!(
            owner
                .finish_discovery_content(FinishDiscoveryContentInput {
                    claim: claim.clone(),
                    native_content: Some(wrong),
                    stop_code: None
                })
                .is_err()
        );
    }
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: Some(content),
            stop_code: None,
        })
        .unwrap();
    assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        3
    );
    (dir, owner, hit)
}
fn review(owner: &CreatorRuntime, hit: &str, profile_only: bool) -> SubmitDiscoveryReviewInput {
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_author_id, "123");
    assert_eq!(material.native_followers, None);
    SubmitDiscoveryReviewInput {campaign_id:CAMPAIGN.into(),expected_revision:1,search_hit_id:hit.into(),material_hash:material.material_hash,request_id:format!("pixiv-score-{profile_only}"),reviewer:DiscoveryReviewer {kind:DiscoveryReviewerKind::Agent,label:INITIAL_MATCH_REVIEWER.into()},findings:vec![DiscoveryReviewFinding {criterion:DiscoveryReviewCriterion::MatchFit,outcome:DiscoveryReviewOutcome::Supported,rationale:"Original author-bound artwork description matches illustration and is written in English.".into(),citations:if profile_only {vec![DiscoveryReviewQuote::NativeProfile {profile_url:PROFILE.into(),quote:"Original illustration artist".into()}]} else {vec![DiscoveryReviewQuote::NativeContent {quote:"Original illustration review of clothing and fashion design.".into()}]},tool:None,country_code:None,match_level:Some(DiscoveryMatchLevel::Good)}]}
}
#[test]
fn pixiv_native_artwork_score_direct_email_recommendation_and_restart() {
    let (dir, owner, hit) = setup(json!({"language":"English"}), true);
    let error = owner
        .submit_discovery_review("account", review(&owner, &hit, true))
        .unwrap_err();
    assert_eq!(error, "initial_match_content_language_unavailable");
    owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(snapshot.rows[0].contacts.len(), 1);
    assert_eq!(
        snapshot.rows[0].contacts[0].normalized_value,
        "art@example.com"
    );
    assert_eq!(snapshot.rows[0].contacts[0].source_url, PROFILE);
    assert!(
        owner
            .repository
            .next_discovery_public_page(CAMPAIGN, &[])
            .unwrap()
            .is_none()
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup(CAMPAIGN, &[])
            .unwrap()
            .is_none()
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap()
            .rows,
        snapshot.rows
    );
    assert_eq!(
        reopened
            .get_discovery_campaign(CAMPAIGN)
            .unwrap()
            .used_calls,
        3
    );
}
#[test]
fn pixiv_unknown_required_followers_cannot_be_overridden_by_email_or_score() {
    let (_dir, owner, hit) = setup(json!({"min_followers":1000,"language":"English"}), true);
    let error = owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap_err();
    assert!(
        error.contains("initial_match_required_count_unknown"),
        "{error}"
    );
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
}
#[test]
fn pixiv_official_contact_link_is_source_bound_and_stops_after_actual_email() {
    let (dir, owner, hit) = setup(json!({"language":"English"}), false);
    owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap();
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
    let page = owner
        .repository
        .next_discovery_public_page(CAMPAIGN, &[])
        .unwrap()
        .unwrap();
    assert_eq!(page.requested_url, "https://artist.example.com/contact");
    let mut wrong = page.clone();
    wrong.requested_url = "https://unrelated.example.com/contact".into();
    assert!(owner.claim_discovery_public_page(wrong).is_err());
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            claim,
            document_url: Some("https://artist.example.com/contact".into()),
            title: "Artist business".into(),
            markdown_excerpt: "Business art@example.com".into(),
            truncated: false,
            stop_code: None,
            hidden_email_available: false,
        })
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(
        snapshot.rows[0].contacts[0].source_url,
        "https://artist.example.com/contact"
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup(CAMPAIGN, &[])
            .unwrap()
            .is_none()
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page(CAMPAIGN, &[])
            .unwrap()
            .is_none()
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap()
            .rows,
        snapshot.rows
    );
}

#[test]
fn pixiv_failed_author_is_saved_and_next_author_runs_without_replaying_or_extra_charges() {
    for fail_content in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: CAMPAIGN.into(),
                brief: "Illustration creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: json!({}),
                target_count: 2,
                max_calls: Some(20),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "pixiv".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["illustration".into()],
                }],
            })
            .unwrap();
        owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
        let claim = owner
            .claim_discovery_search(CAMPAIGN, 1, "pixiv", "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![
                    DiscoverySearchHitInput {
                        source_url: SOURCE.into(),
                        title: "Artwork one".into(),
                        author_label: "First".into(),
                        author_id: Some("123".into()),
                        author_url: Some(PROFILE.into()),
                    },
                    DiscoverySearchHitInput {
                        source_url: "https://www.pixiv.net/artworks/789".into(),
                        title: "Artwork two".into(),
                        author_label: "Second".into(),
                        author_id: Some("987".into()),
                        author_url: Some("https://www.pixiv.net/users/987".into()),
                    },
                ],
            })
            .unwrap();
        let DiscoveryStep::ReadProfile { search_hit_id: hit } =
            owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
        else {
            panic!("expected first profile")
        };
        let profile = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
        if fail_content {
            owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim:profile.clone(),native_profile:Some(json!({"adapter_version":"1.0.0","author_id":profile.author_id,"author_label":"Artist","profile_url":profile.author_url,"follower_count":null,"bio":"Original illustration artist","public_links":[],"evidence_source":"native_ajax_response"})),stop_code:None}).unwrap();
            let content = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
            owner
                .finish_discovery_content(FinishDiscoveryContentInput {
                    claim: content.clone(),
                    native_content: None,
                    stop_code: Some("opencli_invalid_output".into()),
                })
                .unwrap();
            assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
            assert_eq!(
                owner
                    .get_discovery_content(CAMPAIGN, &hit)
                    .unwrap()
                    .stop_code
                    .as_deref(),
                Some("opencli_invalid_output")
            );
        } else {
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim: profile,
                    native_profile: None,
                    stop_code: Some("opencli_invalid_output".into()),
                })
                .unwrap();
            assert_eq!(
                owner.get_discovery_profile(CAMPAIGN, &hit).unwrap().status,
                "failed"
            );
        }
        let expected = if fail_content { 3 } else { 2 };
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            expected
        );
        let DiscoveryStep::ReadProfile {
            search_hit_id: next,
        } = owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
        else {
            panic!("candidate failure must continue next author")
        };
        assert_ne!(next, hit);
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened.plan_discovery_step(CAMPAIGN, 1).unwrap(),
            DiscoveryStep::ReadProfile {
                search_hit_id: next
            }
        );
        assert_eq!(
            reopened
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .used_calls,
            expected
        );
    }
}
