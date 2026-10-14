use crate::*;
use serde_json::json;
const CAMPAIGN: &str = "pinterest-content-flow";
const PROFILE: &str = "https://www.pinterest.com/native_author/";
const PIN: &str = "https://www.pinterest.com/pin/456/";
const TEXT: &str =
    "Original keyboard review for English speaking readers. Business studio@example.org";

fn setup() -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_source(PROFILE)
}

fn setup_source(search_source: &str) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: CAMPAIGN.into(),
            brief: "Keyboard creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"language":"English"}),
            target_count: 2,
            max_calls: Some(20),
            scopes: vec![DiscoveryPlatformScope {
                platform: "pinterest".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["keyboard".into(), "mouse".into()],
            }],
        })
        .unwrap();
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    let claim = owner
        .claim_discovery_search(CAMPAIGN, 1, "pinterest", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: search_source.into(),
                title: "Keyboard creator".into(),
                author_label: "Keyboard creator".into(),
                author_id: Some("123".into()),
                author_url: Some(PROFILE.into()),
            }],
        })
        .unwrap();
    let DiscoveryStep::ReadProfile { search_hit_id: hit } =
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
    else {
        panic!("profile first");
    };
    let claim = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim,stop_code:None,
        native_profile:Some(json!({"author_id":"123","username":"native_author","author_label":"Keyboard creator","profile_url":PROFILE,
            "follower_count":null,"bio":"Keyboard creator","public_links":[]}))}).unwrap();
    assert_eq!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.clone()
        }
    );
    (dir, owner, hit)
}
fn caption() -> serde_json::Value {
    json!({"pin_id":"456","author_id":"123","author_username":"native_author","profile_url":PROFILE,"source_url":PIN,"text":TEXT,"text_truncated":true})
}

#[test]
fn pin_search_keeps_its_exact_source_through_profile_content_and_review() {
    let (dir, owner, hit) = setup_source(PIN);
    let before = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(before.hit.source_url, PIN);
    assert_eq!(before.hit.author_url.as_deref(), Some(PROFILE));
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    assert_eq!(claim.source_url, PIN);
    let mut other_pin = caption();
    other_pin["pin_id"] = json!("789");
    other_pin["source_url"] = json!("https://www.pinterest.com/pin/789/");
    let mut other_profile = caption();
    other_profile["author_username"] = json!("different");
    other_profile["profile_url"] = json!("https://www.pinterest.com/different/");
    for wrong in [other_pin, other_profile] {
        assert!(
            owner
                .finish_discovery_content(FinishDiscoveryContentInput {
                    claim: claim.clone(),
                    native_content: Some(wrong),
                    stop_code: None,
                })
                .is_err()
        );
    }
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: Some(caption()),
            stop_code: None,
        })
        .unwrap();
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(material.hit.source_url, PIN);
    assert_eq!(
        material
            .native_content
            .as_ref()
            .unwrap()
            .observed_source_url(),
        Some(PIN)
    );
    owner.submit_discovery_review("account", SubmitDiscoveryReviewInput {
        campaign_id: CAMPAIGN.into(), expected_revision: 1, search_hit_id: hit.clone(),
        material_hash: material.material_hash, request_id: "pin-search-score".into(),
        reviewer: DiscoveryReviewer {kind: DiscoveryReviewerKind::Agent, label: INITIAL_MATCH_REVIEWER.into()},
        findings: vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit, outcome: DiscoveryReviewOutcome::Supported,
            rationale: "The exact searched Pin caption describes keyboard reviews; image ownership is not inferred.".into(),
            citations: vec![DiscoveryReviewQuote::NativeContent {quote: TEXT.into()}],
            tool: None, country_code: None, match_level: Some(DiscoveryMatchLevel::Good),
        }],
    }).unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(snapshot.rows[0].contacts[0].source_url, PIN);
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    let material = reopened
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(material.hit.source_url, PIN);
    assert_eq!(material.hit.author_url.as_deref(), Some(PROFILE));
}

#[test]
fn pinterest_pin_caption_language_score_and_email_recommendation_survive_restart() {
    let (dir, owner, hit) = setup();
    let before = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert!(before.native_content.is_none());
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    assert_eq!(
        claim.source_url, PROFILE,
        "claim stays bound to authorized native feed"
    );
    for (key, bad) in [
        ("author_id", json!("999")),
        ("profile_url", json!("https://www.pinterest.com/other/")),
        ("author_username", json!("other")),
        ("pin_id", json!("789")),
        ("source_url", json!("https://www.pinterest.com/pin/789/")),
        ("text_truncated", json!(false)),
        ("session", json!("private")),
    ] {
        let mut wrong = caption();
        wrong[key] = bad;
        assert!(
            owner
                .finish_discovery_content(FinishDiscoveryContentInput {
                    claim: claim.clone(),
                    native_content: Some(wrong),
                    stop_code: None
                })
                .is_err(),
            "{key}"
        );
    }
    let receipt = owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: Some(caption()),
            stop_code: None,
        })
        .unwrap();
    assert_eq!(receipt.observed_source_url(), Some(PIN));
    assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_ne!(material.material_hash, before.material_hash);
    assert_eq!(material.native_author_id, "123");
    assert_eq!(
        material
            .native_content
            .as_ref()
            .unwrap()
            .observed_source_url(),
        Some(PIN)
    );
    owner.submit_discovery_review("account",SubmitDiscoveryReviewInput {
        campaign_id:CAMPAIGN.into(),expected_revision:1,search_hit_id:hit.clone(),material_hash:material.material_hash,request_id:"pin-caption-score".into(),
        reviewer:DiscoveryReviewer {kind:DiscoveryReviewerKind::Agent,label:INITIAL_MATCH_REVIEWER.into()},
        findings:vec![DiscoveryReviewFinding {criterion:DiscoveryReviewCriterion::MatchFit,outcome:DiscoveryReviewOutcome::Supported,
            rationale:"The saved native pinner-bound caption matches keyboard reviews and is in English. Image originality is not claimed.".into(),
            citations:vec![DiscoveryReviewQuote::NativeContent {quote:TEXT.into()}],tool:None,country_code:None,match_level:Some(DiscoveryMatchLevel::Good)}]
    }).unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(
        snapshot.rows[0].contacts[0].normalized_value,
        "studio@example.org"
    );
    assert_eq!(snapshot.rows[0].contacts[0].source_url, PIN);
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
    assert_eq!(
        reopened
            .get_discovery_content(CAMPAIGN, &hit)
            .unwrap()
            .observed_source_url(),
        Some(PIN)
    );
}

#[test]
fn pinterest_failed_content_is_not_replayed_and_next_author_continues() {
    let (dir, owner, hit) = setup();
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: None,
            stop_code: Some("opencli_invalid_output".into()),
        })
        .unwrap();
    assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_content(CAMPAIGN, &hit).unwrap().status,
        "failed"
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().phase,
        "active"
    );
    assert!(!matches!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadContent { .. } | DiscoveryStep::Busy
    ));
    let search = owner
        .claim_discovery_search(CAMPAIGN, 1, "pinterest", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.pinterest.com/next_author/".into(),
                title: "Next creator".into(),
                author_label: "Next creator".into(),
                author_id: Some("999".into()),
                author_url: Some("https://www.pinterest.com/next_author/".into()),
            }],
        })
        .unwrap();
    let DiscoveryStep::ReadProfile {
        search_hit_id: next,
    } = owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
    else {
        panic!("next author must run");
    };
    assert_ne!(next, hit);
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_campaign(CAMPAIGN)
            .unwrap()
            .used_calls,
        4
    );
    assert_eq!(
        reopened.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadProfile {
            search_hit_id: next
        }
    );
}
