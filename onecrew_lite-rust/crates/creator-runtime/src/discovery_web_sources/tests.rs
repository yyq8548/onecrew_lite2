use crate::*;
use serde_json::{Value, json};
const CAMPAIGN: &str = "web-source-flow";
const CHANNEL: &str = "UCabcdefghijklmnopqrstuv";
const VIDEO: &str = "abcdefghijk";
fn source(video: &str) -> String {
    format!("https://www.youtube.com/watch?v={video}&pp=source-tracking")
}
fn material(video: &str) -> Value {
    json!({"adapterVersion":"1.0.0","videoId":video,"channelId":CHANNEL,
  "sourceUrl":format!("https://www.youtube.com/watch?v={video}"),"title":"Original review","author":"Native creator","subscribers":"Unknown",
  "description":"Original keyboard review. Business studio@example.org","descriptionSource":"ytInitialPlayerResponse.videoDetails.shortDescription",
  "displayDescription":"Translated UI is not language proof","links":[],"playabilityStatus":"OK"})
}
fn twitter_material() -> Value {
    let identity = json!({"adapter_version":"1.0.0","post_id":"1234567890123456789","account_id":"987654321012345678",
        "handle":"creator_1","profile_url":"https://x.com/creator_1","source_url":"https://x.com/creator_1/status/1234567890123456789",
        "identity_kind":"native_post_author_id","evidence_source":"x_tweet_detail"});
    json!({"identity":identity,"profile":{"adapter_version":"1.0.0","account_id":"987654321012345678","handle":"creator_1",
        "profile_url":"https://x.com/creator_1","name":"Native X creator","bio":"Business studio@example.org","self_declared_location":"",
        "follower_count":1500,"identity_kind":"native_id","evidence_source":"x_user_by_screen_name"},
        "content":{"adapter_version":"1.0.0","identity":identity,"text":"Original keyboard review.","text_truncated":false,
        "text_source":"legacy_full_text","created_at":"Fri Oct 08 00:00:00 +0000 2026","has_media":false,"quotes_another_post":false,"expanded_urls":[]}})
}

fn douyin_material() -> Value {
    let author = "MS4wLjABAAAA_native_author_123456789";
    json!({"profile":{"adapter_version":"1.0.0","author_id":author,"author_label":"Native keyboard creator",
        "profile_url":format!("https://www.douyin.com/user/{author}"),"follower_count":4892,"follower_text":"4892",
        "bio":"Business studio@example.org","bio_truncated":false,"public_links":["https://creator.example.com/contact"],"evidence_source":"visible_profile_header"},
        "content":{"post_id":"7688639864627214835","author_id":author,"author_label":"Native keyboard creator",
        "profile_url":format!("https://www.douyin.com/user/{author}"),"source_url":"https://www.douyin.com/video/7688639864627214835",
        "text":"Original keyboard review.","text_truncated":false,"evidence_source":"visible_native_video_caption"}})
}

#[test]
fn engine_douyin_source_preserves_origin_scores_native_text_and_recommends_bound_email() {
    let source = "https://www.douyin.com/video/7688639864627214835";
    for origin in ["google", "yahoo"] {
        let (dir, owner) = setup_with_language(origin, &[source.into()], "English");
        let hit = next(&owner);
        let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
        assert_eq!(claim.target_platform, "douyin");
        assert_eq!(claim.call_number, 3);
        let read = owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim,
                native_material: Some(douyin_material()),
                stop_code: None,
            })
            .unwrap();
        let material = owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap();
        assert_eq!(material.hit.platform, origin);
        assert_eq!(material.native_platform.as_deref(), Some("douyin"));
        assert_eq!(material.native_followers, Some(4892));
        assert_eq!(
            material.video_description.as_deref(),
            Some("Original keyboard review.")
        );
        assert!(
            owner
                .repository
                .claim_discovery_language(CAMPAIGN, 1, &hit)
                .unwrap()
                .is_none()
        );
        score_source(&owner, &hit);
        let publication = owner
            .prepare_discovery_library_publication(CAMPAIGN, 1)
            .unwrap();
        let row = &publication.material.sources.rows[0];
        assert_eq!(row.author_platform(), "douyin");
        assert_eq!(row.verdict, "qualified");
        assert_eq!(row.contacts.len(), 2);
        let email = row
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::Email)
            .unwrap();
        assert_eq!(email.normalized_value, "studio@example.org");
        assert_eq!(Some(&email.native_author_id), row.native_author_id.as_ref());
        assert_eq!(
            Some(email.source_url.as_str()),
            material.hit.author_url.as_deref()
        );
        assert_eq!(
            owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
            DiscoveryStep::SearchExhausted
        );
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened.get_discovery_web_source(CAMPAIGN, &hit).unwrap(),
            read
        );
        assert_eq!(
            reopened
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .used_calls,
            3
        );
        assert!(
            reopened
                .claim_discovery_web_source(CAMPAIGN, 1, &hit)
                .is_err()
        );
    }
}

#[test]
fn engine_douyin_rejects_foreign_author_video_and_truncated_original() {
    let source = "https://www.douyin.com/video/7688639864627214835";
    let (_dir, owner) = setup_with_language("yahoo", &[source.into()], "English");
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    for (path, value) in [
        (
            "/profile/author_id",
            json!("MS4wLjABAAAA_another_author_123"),
        ),
        ("/content/post_id", json!("123")),
        (
            "/content/source_url",
            json!("https://www.douyin.com/video/123"),
        ),
        ("/content/text_truncated", json!(true)),
        ("/profile/follower_count", json!(123)),
    ] {
        let mut native = douyin_material();
        *native.pointer_mut(path).unwrap() = value;
        assert!(
            owner
                .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                    claim: claim.clone(),
                    native_material: Some(native),
                    stop_code: None,
                })
                .is_err(),
            "{path}"
        );
    }
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        3
    );
}

#[test]
fn engine_x_native_receipts_recommend_only_source_bound_profile_email_and_survive_restart() {
    for (origin, source) in [
        (
            "google",
            "https://x.com/creator_1/status/1234567890123456789",
        ),
        (
            "yahoo",
            "https://x.com/creator_1/status/1234567890123456789",
        ),
        (
            "google",
            "https://x.com/Creator_1/status/1234567890123456789",
        ),
        (
            "yahoo",
            "https://x.com/CREATOR_1/status/1234567890123456789",
        ),
    ] {
        let (dir, owner) = setup_with_language(origin, &[source.into()], "English");
        let hit = next(&owner);
        let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
        assert_eq!(claim.target_platform, "twitter");
        assert_eq!(claim.source_url, source);
        assert_eq!(claim.call_number, 4);
        let receipt = owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim,
                native_material: Some(twitter_material()),
                stop_code: None,
            })
            .unwrap();
        let review = owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap();
        assert_eq!(review.native_platform.as_deref(), Some("twitter"));
        assert_eq!(review.native_followers, Some(1500));
        assert_eq!(review.hit.platform, origin);
        assert_eq!(
            review.hit.author_url.as_deref(),
            Some("https://x.com/creator_1")
        );
        assert_eq!(
            review.video_description.as_deref(),
            Some("Original keyboard review.")
        );
        assert!(
            owner
                .repository
                .claim_discovery_language(CAMPAIGN, 1, &hit)
                .unwrap()
                .is_none()
        );
        score_source(&owner, &hit);
        let publication = owner
            .prepare_discovery_library_publication(CAMPAIGN, 1)
            .unwrap();
        let row = &publication.material.sources.rows[0];
        assert_eq!(row.author_platform(), "twitter");
        assert_eq!(row.verdict, "qualified");
        assert_eq!(row.contacts.len(), 2);
        let email = row
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::Email)
            .unwrap();
        assert_eq!(email.source_url, "https://x.com/creator_1");
        assert_eq!(email.normalized_value, "studio@example.org");
        let account = row
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::SocialProfile)
            .unwrap();
        assert_eq!(account.normalized_value, "https://x.com/creator_1");
        assert_eq!(account.native_author_id, email.native_author_id);
        assert_eq!(
            account.attribution,
            DiscoveryContactAttribution::AuthorPublished
        );
        assert!(
            owner
                .repository
                .next_discovery_contact_lookup(CAMPAIGN, &[])
                .unwrap()
                .is_none()
        );
        drop(owner);
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            owner.get_discovery_web_source(CAMPAIGN, &hit).unwrap(),
            receipt
        );
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            4
        );
        assert!(owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).is_err());
    }
}

#[test]
fn engine_x_tampered_native_target_cannot_become_review_material() {
    let source = "https://x.com/creator_1/status/1234567890123456789";
    let (dir, owner) = setup_with_language("yahoo", &[source.into()], "English");
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: Some(twitter_material()),
            stop_code: None,
        })
        .unwrap();
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .is_ok()
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.web_origin.target_platform','youtube') WHERE discovery_campaign_id=?1", [CAMPAIGN]).unwrap();
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .is_err()
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
fn engine_x_wrong_original_profile_private_fields_and_budget_never_promote() {
    let source = "https://x.com/creator_1/status/1234567890123456789";
    let (_dir, owner) = setup_with_language("yahoo", &[source.into()], "English");
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    for (path, value) in [
        ("/profile/account_id", json!("111")),
        ("/content/identity/post_id", json!("222")),
        ("/content/identity/account_id", json!("111")),
        ("/profile/profile_url", json!("https://x.com/other")),
        ("/identity/evidence_source", json!("engine_snippet")),
        ("/content/text_source", json!("translated_dom")),
        (
            "/content/expanded_urls",
            json!(["https://user:private@example.org/"]),
        ),
    ] {
        let mut data = twitter_material();
        *data.pointer_mut(path).unwrap() = value;
        assert!(
            owner
                .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                    claim: claim.clone(),
                    native_material: Some(data),
                    stop_code: None
                })
                .is_err(),
            "{path}"
        );
    }
    let mut data = twitter_material();
    data["profile"]["credential"] = json!("must-not-be-saved");
    assert!(
        owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim: claim.clone(),
                native_material: Some(data),
                stop_code: None
            })
            .is_err()
    );
    owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: None,
            stop_code: Some("opencli_invalid_output".into()),
        })
        .unwrap();
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .is_err()
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        4
    );
    for invalid in [
        "https://x.com/creator_1",
        "https://x.com/creator_1/status/0",
        "https://x.com/Creator-name/status/123",
        "https://x.com/creator_1/status/123?token=private",
        "https://x.com/creator_1/status/123#fragment",
        "https://twitter.com/creator_1/status/123",
    ] {
        assert!(super::source_target(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn engine_douyin_comments_wait_for_audited_unclear_original_and_keep_same_post() {
    let source = "https://www.douyin.com/video/7688639864627214835";
    let (_dir, owner) = setup_with_language("yahoo", &[source.into()], "English");
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: Some(douyin_material()),
            stop_code: None,
        })
        .unwrap();
    assert!(
        owner
            .repository
            .claim_discovery_language(CAMPAIGN, 1, &hit)
            .unwrap()
            .is_none()
    );
    request_source_language(&owner, &hit, "douyin-language-original-only");
    let claim = owner
        .repository
        .claim_discovery_language(CAMPAIGN, 1, &hit)
        .unwrap()
        .expect("one same-post window");
    assert_eq!(claim.root_url, source);
    assert_eq!(claim.author_handle, "MS4wLjABAAAA_native_author_123456789");
    assert_eq!(claim.call_number, 4);
    let sample = DiscoveryLanguageSample {
        author_id: claim.author_id.clone(),
        source_url: source.into(),
        text: "Original keyboard review.".into(),
        comments: vec![DiscoveryLanguageComment {
            source_url:
                "https://www.douyin.com/video/7688639864627214835#comment-2234567890123456789"
                    .into(),
            text: "Useful keyboard review".into(),
        }],
    };
    assert!(sample.valid_for(&claim));
    owner
        .repository
        .finish_discovery_language(claim, Some(sample), None)
        .unwrap();
    assert!(
        owner
            .repository
            .claim_discovery_language(CAMPAIGN, 1, &hit)
            .unwrap()
            .is_none()
    );
}

#[test]
fn engine_x_comments_wait_for_audited_unclear_original_and_keep_same_post() {
    let source = "https://x.com/creator_1/status/1234567890123456789";
    let (_dir, owner) = setup_with_language("yahoo", &[source.into()], "English");
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: Some(twitter_material()),
            stop_code: None,
        })
        .unwrap();
    assert!(
        owner
            .repository
            .claim_discovery_language(CAMPAIGN, 1, &hit)
            .unwrap()
            .is_none()
    );
    request_source_language(&owner, &hit, "x-language-original-only");
    let claim = owner
        .repository
        .claim_discovery_language(CAMPAIGN, 1, &hit)
        .unwrap()
        .expect("one same-post window");
    assert_eq!(claim.root_url, source);
    assert_eq!(claim.author_handle, "creator_1");
    assert_eq!(claim.call_number, 5);
    let sample = DiscoveryLanguageSample {
        author_id: claim.author_id.clone(),
        source_url: source.into(),
        text: "Original keyboard review.".into(),
        comments: vec![DiscoveryLanguageComment {
            source_url: "https://x.com/reader/status/2234567890123456789".into(),
            text: "Useful keyboard review".into(),
        }],
    };
    assert!(sample.valid_for(&claim));
    owner
        .repository
        .finish_discovery_language(claim, Some(sample), None)
        .unwrap();
    assert!(
        owner
            .repository
            .claim_discovery_language(CAMPAIGN, 1, &hit)
            .unwrap()
            .is_none()
    );
}

#[test]
fn engine_x_reserves_three_calls_and_risk_stops_only_native_x_target() {
    let source = "https://x.com/creator_1/status/1234567890123456789";
    let (dir, owner) =
        setup_with_language("yahoo", &[source.into(), self::source(VIDEO)], "English");
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let hit: String = db
        .query_row(
            "SELECT id FROM discovery_search_hits WHERE source_url=?1",
            [source],
            |r| r.get(0),
        )
        .unwrap();
    db.execute(
        "UPDATE discovery_campaigns SET max_calls=3 WHERE id=?1",
        [CAMPAIGN],
    )
    .unwrap();
    assert!(owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        1
    );
    db.execute(
        "UPDATE discovery_campaigns SET max_calls=20 WHERE id=?1",
        [CAMPAIGN],
    )
    .unwrap();
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: None,
            stop_code: Some("opencli_risk_control".into()),
        })
        .unwrap();
    let next_hit = next(&owner);
    assert_ne!(next_hit, hit);
    let claim = owner
        .claim_discovery_web_source(CAMPAIGN, 1, &next_hit)
        .unwrap();
    assert_eq!(claim.target_platform, "youtube");
    assert_eq!(claim.call_number, 5);
    owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: Some(material(VIDEO)),
            stop_code: None,
        })
        .unwrap();
    assert_eq!(
        owner
            .get_discovery_campaign(CAMPAIGN)
            .unwrap()
            .platform_runs[0]
            .status,
        "completed"
    );
}
fn setup(origin: &str, urls: &[String]) -> (tempfile::TempDir, CreatorRuntime) {
    setup_with_language(origin, urls, "Japanese")
}
fn setup_with_language(
    origin: &str,
    urls: &[String],
    language: &str,
) -> (tempfile::TempDir, CreatorRuntime) {
    setup_with_origins(&[origin], urls, language)
}
fn setup_with_origins(
    origins: &[&str],
    urls: &[String],
    language: &str,
) -> (tempfile::TempDir, CreatorRuntime) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: CAMPAIGN.into(),
            brief: "Keyboard reviewers".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"language":language}),
            target_count: 2,
            max_calls: Some(20),
            scopes: origins
                .iter()
                .map(|origin| DiscoveryPlatformScope {
                    platform: (*origin).into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["keyboard review".into()],
                })
                .collect(),
        })
        .unwrap();
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    for origin in origins {
        let claim = owner
            .claim_discovery_search(CAMPAIGN, 1, origin, "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: urls
                    .iter()
                    .map(|url| DiscoverySearchHitInput {
                        source_url: url.clone(),
                        title: "検索要約は著者の原文ではありません".into(),
                        author_label: String::new(),
                        author_id: None,
                        author_url: None,
                    })
                    .collect(),
            })
            .unwrap();
    }
    (dir, owner)
}
fn next(owner: &CreatorRuntime) -> String {
    let DiscoveryStep::ReadWebSource { search_hit_id } =
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
    else {
        panic!("actual source required")
    };
    search_hit_id
}

fn scored_source(
    origin: &str,
    native: Value,
) -> (tempfile::TempDir, CreatorRuntime, DiscoveryWebSourceRead) {
    let (dir, owner) = setup_with_language(origin, &[source(VIDEO)], "English");
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    let read = owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: Some(native),
            stop_code: None,
        })
        .unwrap();
    score_source(&owner, &hit);
    (dir, owner, read)
}

fn score_source(owner: &CreatorRuntime, hit: &str) {
    let review = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, hit)
        .unwrap();
    owner
        .submit_discovery_review(
            "isolated-test-account",
            SubmitDiscoveryReviewInput {
                campaign_id: CAMPAIGN.into(),
                expected_revision: 1,
                search_hit_id: hit.into(),
                material_hash: review.material_hash,
                request_id: "original-score".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "Original English keyboard review matches the request.".into(),
                    citations: vec![DiscoveryReviewQuote::Description {
                        quote: "Original keyboard review.".into(),
                    }],
                    tool: None,
                    country_code: None,
                    match_level: Some(DiscoveryMatchLevel::Good),
                }],
            },
        )
        .unwrap();
}

fn request_source_language(owner: &CreatorRuntime, hit: &str, request: &str) {
    let review = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, hit)
        .unwrap();
    owner
        .submit_discovery_review(
            "isolated-test-account",
            SubmitDiscoveryReviewInput {
                campaign_id: CAMPAIGN.into(),
                expected_revision: 1,
                search_hit_id: hit.into(),
                material_hash: review.material_hash,
                request_id: request.into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: [
                    DiscoveryReviewCriterion::MatchFit,
                    DiscoveryReviewCriterion::ContentLanguage,
                ]
                .into_iter()
                .map(|criterion| DiscoveryReviewFinding {
                    criterion,
                    outcome: DiscoveryReviewOutcome::Insufficient,
                    rationale:
                        "Original text is evaluated; only audience language remains uncertain."
                            .into(),
                    citations: vec![],
                    tool: None,
                    country_code: None,
                    match_level: None,
                })
                .collect(),
            },
        )
        .unwrap();
}

#[tokio::test]
async fn discovery_web_sources_comments_require_audited_original_and_keep_engine_authority() {
    for origin in ["google", "yahoo"] {
        let (dir, owner) = setup_with_language(origin, &[source(VIDEO)], "English");
        let hit = next(&owner);
        let native = material(VIDEO);
        let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
        owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim,
                native_material: Some(native.clone()),
                stop_code: None,
            })
            .unwrap();
        assert!(
            owner
                .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
                .await
                .unwrap()
                .is_none()
        );
        request_source_language(&owner, &hit, "language-only");
        let before = owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls;
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        let mut changed = native.clone();
        changed["description"] = json!("Changed original requires new evaluation");
        db.execute(
            "UPDATE discovery_web_source_reads SET native_material_json=?2 WHERE search_hit_id=?1",
            rusqlite::params![hit, changed.to_string()],
        )
        .unwrap();
        assert!(
            owner
                .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            before
        );
        db.execute(
            "UPDATE discovery_web_source_reads SET native_material_json=?2 WHERE search_hit_id=?1",
            rusqlite::params![hit, native.to_string()],
        )
        .unwrap();
        drop(db);
        assert_eq!(
            owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
            DiscoveryStep::ReadContent {
                search_hit_id: hit.clone()
            }
        );
        let language = owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(language.platform, origin);
        assert_eq!(language.author_id, CHANNEL);
        assert_eq!(
            language.root_url,
            format!("https://www.youtube.com/watch?v={VIDEO}")
        );
        let texts = [
            "This keyboard comparison was useful",
            "I prefer the quieter switches",
            "The key layout works well for me",
        ];
        let sample = DiscoveryLanguageSample {
            author_id: CHANNEL.into(),
            source_url: language.root_url.clone(),
            text: native["description"].as_str().unwrap().into(),
            comments: texts
                .iter()
                .enumerate()
                .map(|(index, text)| DiscoveryLanguageComment {
                    source_url: format!("{}&lc=actual-{index}", language.root_url),
                    text: (*text).into(),
                })
                .collect(),
        };
        let mut wrong = sample.clone();
        wrong.text = "Unbound replacement original".into();
        assert!(
            owner
                .finish_discovery_language_async(language.clone(), Some(wrong), None)
                .await
                .is_err()
        );
        let mut wrong_claim = language.clone();
        wrong_claim.profile_ref = "different-browser".into();
        assert!(
            owner
                .finish_discovery_language_async(
                    wrong_claim,
                    None,
                    Some("opencli_risk_control".into())
                )
                .await
                .is_err()
        );
        let mut sixth = sample.clone();
        sixth.comments.extend(sample.comments.clone());
        assert!(
            owner
                .finish_discovery_language_async(language.clone(), Some(sixth), None)
                .await
                .is_err()
        );
        owner
            .finish_discovery_language_async(language.clone(), Some(sample), None)
            .await
            .unwrap();
        let review = owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap();
        assert_eq!(review.language_samples.len(), 1);
        assert_eq!(review.hit.source_url, source(VIDEO));
        assert_eq!(review.matched_language_source(), language.root_url);
        owner.submit_discovery_review("isolated-test-account", SubmitDiscoveryReviewInput {
            campaign_id: CAMPAIGN.into(), expected_revision: 1, search_hit_id: hit.clone(),
            material_hash: review.material_hash, request_id: "audience-score".into(),
            reviewer: DiscoveryReviewer {kind:DiscoveryReviewerKind::Agent,label:INITIAL_MATCH_REVIEWER.into()},
            findings: vec![DiscoveryReviewFinding {criterion:DiscoveryReviewCriterion::MatchFit,
                outcome:DiscoveryReviewOutcome::Supported, rationale:"Audience comment language matches English; original keyboard topic is relevant.".into(),
                citations:texts.into_iter().map(|text|DiscoveryReviewQuote::NativeContent{quote:text.into()}).collect(),
                tool:None,country_code:None,match_level:Some(DiscoveryMatchLevel::Good)}],
        }).unwrap();
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            before + 1
        );
        assert_eq!(
            owner
                .list_discovery_candidates(ListDiscoveryCandidatesInput {
                    campaign_id: CAMPAIGN.into(),
                    after_id: None,
                    limit: 10,
                })
                .unwrap()[0]
                .verdict,
            "qualified"
        );
        drop(owner);
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
        assert!(
            owner
                .claim_discovery_language_async(CAMPAIGN.into(), 1, hit)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            owner
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .platform_runs
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn discovery_web_sources_comment_failure_isolates_native_target_without_replaying() {
    for origin in ["google", "yahoo"] {
        for stop in [
            "opencli_timeout",
            "opencli_auth_required",
            "opencli_risk_control",
        ] {
            let (dir, owner) =
                setup_with_language(origin, &[source(VIDEO), source("lmnopqrstuv")], "English");
            let hit = next(&owner);
            let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
            let mut native = material(&claim.video_id);
            native["description"] = json!("Keyboard 🔧");
            owner
                .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                    claim,
                    native_material: Some(native),
                    stop_code: None,
                })
                .unwrap();
            request_source_language(&owner, &hit, "language-only");
            let language = owner
                .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
                .await
                .unwrap()
                .unwrap();
            owner
                .finish_discovery_language_async(language, None, Some(stop.into()))
                .await
                .unwrap();
            let calls = owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls;
            drop(owner);
            let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
            assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
            let campaign = owner.get_discovery_campaign(CAMPAIGN).unwrap();
            assert_eq!(campaign.platform_runs.len(), 1);
            assert_eq!(campaign.platform_runs[0].platform, origin);
            assert_eq!(campaign.platform_runs[0].status, "completed");
            assert!(campaign.platform_runs[0].stop_code.is_none());
            assert_eq!(campaign.used_calls, calls);
            assert!(
                owner
                    .claim_discovery_language_async(CAMPAIGN.into(), 1, hit)
                    .await
                    .unwrap()
                    .is_none()
            );
            let next = owner.plan_discovery_step(CAMPAIGN, 1).unwrap();
            if stop == "opencli_timeout" {
                assert!(matches!(next, DiscoveryStep::ReadWebSource { .. }));
            } else {
                assert!(!matches!(
                    next,
                    DiscoveryStep::ReadWebSource { .. } | DiscoveryStep::ReadContent { .. }
                ));
            }
        }
    }
}

#[tokio::test]
async fn discovery_web_sources_comment_target_stop_preserves_real_engine_and_native_runs() {
    for origin in ["google", "yahoo"] {
        let (_dir, owner) = setup_with_origins(&[origin, "youtube"], &[source(VIDEO)], "English");
        let hit = owner
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: CAMPAIGN.into(),
                after_id: None,
                limit: 10,
            })
            .unwrap()
            .into_iter()
            .find(|hit| hit.platform == origin)
            .unwrap()
            .id;
        let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
        owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim,
                native_material: Some(material(VIDEO)),
                stop_code: None,
            })
            .unwrap();
        request_source_language(&owner, &hit, "language-only");
        let language = owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit)
            .await
            .unwrap()
            .unwrap();
        owner
            .finish_discovery_language_async(language, None, Some("opencli_risk_control".into()))
            .await
            .unwrap();
        let campaign = owner.get_discovery_campaign(CAMPAIGN).unwrap();
        assert_eq!(campaign.platform_runs.len(), 2);
        let engine = campaign
            .platform_runs
            .iter()
            .find(|run| run.platform == origin)
            .unwrap();
        assert_eq!(engine.status, "completed");
        assert!(engine.stop_code.is_none());
        let target = campaign
            .platform_runs
            .iter()
            .find(|run| run.platform == "youtube")
            .unwrap();
        assert_eq!(target.status, "risk_control_stop");
        assert_eq!(target.stop_code.as_deref(), Some("opencli_risk_control"));
    }
}

#[test]
fn discovery_web_sources_two_engines_keep_both_sources_and_one_native_recommendation() {
    let (_dir, owner) = setup_with_origins(&["google", "yahoo"], &[source(VIDEO)], "English");
    let first_hit = next(&owner);
    let first = owner
        .claim_discovery_web_source(CAMPAIGN, 1, &first_hit)
        .unwrap();
    let first = owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim: first,
            native_material: Some(material(VIDEO)),
            stop_code: None,
        })
        .unwrap();
    assert!(first.is_primary_source);
    score_source(&owner, &first_hit);
    let second_hit = next(&owner);
    assert_ne!(first_hit, second_hit);
    let second = owner
        .claim_discovery_web_source(CAMPAIGN, 1, &second_hit)
        .unwrap();
    let second = owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim: second,
            native_material: Some(material(VIDEO)),
            stop_code: None,
        })
        .unwrap();
    assert_ne!(first.claim.origin_platform, second.claim.origin_platform);
    assert_eq!(first.candidate_id, second.candidate_id);
    assert!(!second.is_primary_source);
    let publication = owner
        .prepare_discovery_library_publication(CAMPAIGN, 1)
        .unwrap();
    let sources = &publication.material.sources.rows;
    assert_eq!(sources.len(), 2);
    assert!(
        sources
            .iter()
            .all(|row| row.verdict == "qualified" && row.author_platform() == "youtube")
    );
    assert_ne!(sources[0].hit.platform, sources[1].hit.platform);
    let groups = &publication.material.identity.groups;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), 1);
    assert_eq!(groups[0].members[0].sources.len(), 2);
    assert_eq!(
        publication
            .material
            .identity
            .counts
            .qualified_native_accounts,
        1
    );
    let campaign = owner.get_discovery_campaign(CAMPAIGN).unwrap();
    assert_eq!(campaign.used_calls, 4);
    assert_eq!(campaign.platform_runs.len(), 2);
    assert!(
        campaign
            .platform_runs
            .iter()
            .all(|run| matches!(run.platform.as_str(), "google" | "yahoo"))
    );
}

fn finish_next_page(owner: &CreatorRuntime, expected: &str, text: &str, hidden: bool) {
    let input = owner
        .repository
        .next_discovery_public_page(CAMPAIGN, &[])
        .unwrap()
        .unwrap();
    assert_eq!(input.requested_url, expected);
    let claim = owner.claim_discovery_public_page(input).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            claim,
            hidden_email_available: hidden,
            document_url: Some(expected.into()),
            title: "Observed contact information".into(),
            markdown_excerpt: text.into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
}

#[test]
fn discovery_web_sources_native_account_qualifies_without_email_or_extra_reads() {
    for origin in ["google", "yahoo"] {
        let mut native = material(VIDEO);
        native["description"] = json!("Original keyboard review.");
        let (dir, owner, _) = scored_source(origin, native);
        let before = owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls;
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
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        let snapshot = owner
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap();
        let row = &snapshot.rows[0];
        assert_eq!(row.verdict, "qualified");
        assert_eq!(row.contacts.len(), 1);
        let account = &row.contacts[0];
        assert_eq!(account.kind, DiscoveryContactKind::SocialProfile);
        assert_eq!(
            account.normalized_value,
            format!("https://www.youtube.com/channel/{CHANNEL}")
        );
        assert_eq!(account.native_author_id, CHANNEL);
        assert_eq!(
            account.attribution,
            DiscoveryContactAttribution::AuthorPublished
        );
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            before
        );
    }
}

fn legacy_scored_source(
    origin: &str,
    native: Value,
) -> (tempfile::TempDir, CreatorRuntime, DiscoveryWebSourceRead) {
    let (dir, owner, read) = scored_source(origin, native);
    // Older campaigns can retain page/lookup work without the newly persisted
    // native-account contact. Keep real receipts and scores to exercise that work.
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(db.execute("DELETE FROM discovery_contact_observations WHERE candidate_id=?1 AND kind='social_profile' AND source_excerpt='Verified native author account'", [read.candidate_id.as_ref().unwrap()]).unwrap(), 1);
    (dir, owner, read)
}

#[test]
fn discovery_web_sources_legacy_hidden_email_qualifies_and_preserves_manual_view() {
    for origin in ["google", "yahoo"] {
        let mut native = material(VIDEO);
        native["description"] = json!("Original keyboard review.");
        let (_dir, owner, read) = legacy_scored_source(origin, native);
        let profile = format!("https://www.youtube.com/channel/{CHANNEL}");
        finish_next_page(
            &owner,
            &profile,
            "Native creator. Hidden email entry requires manual viewing.",
            true,
        );
        assert!(
            owner
                .repository
                .next_discovery_contact_lookup(CAMPAIGN, &[])
                .unwrap()
                .is_none(),
            "confirmed manual email must stop external enrichment"
        );
        assert!(
            owner
                .repository
                .next_discovery_public_page(CAMPAIGN, &[])
                .unwrap()
                .is_none()
        );
        let publication = owner
            .prepare_discovery_library_publication(CAMPAIGN, 1)
            .unwrap();
        let row = &publication.material.sources.rows[0];
        assert_eq!(row.candidate_id, read.candidate_id);
        assert_eq!(row.verdict, "qualified");
        assert!(
            !row.contacts
                .iter()
                .any(|contact| contact.kind == DiscoveryContactKind::Email)
        );
        let hidden = row
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::HiddenEmail)
            .unwrap();
        assert_eq!(hidden.source_url, profile);
        assert_eq!(hidden.normalized_value, profile);
        assert_eq!(hidden.original_value, "有隐藏邮箱，需手动查看");
        assert!(hidden.source_excerpt.contains("address not disclosed"));
        assert_eq!(hidden.native_author_id, CHANNEL);
        assert_eq!(
            hidden.attribution,
            DiscoveryContactAttribution::AuthorPublished
        );
        assert_eq!(
            publication.material.identity.groups[0].members[0].platform,
            "youtube"
        );
        let candidates = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: CAMPAIGN.into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(Some(candidates[0].id.clone()), read.candidate_id);
        assert_eq!(candidates[0].platform, "youtube");
        assert_eq!(candidates[0].verdict, "qualified");
        assert!(candidates[0].evidence.get("contact_status").is_none());
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            3
        );
    }
}

#[test]
fn discovery_web_sources_legacy_native_links_authorize_official_contact_page_under_engine_run() {
    for origin in ["google", "yahoo"] {
        let mut native = material(VIDEO);
        native["description"] = json!("Original keyboard review.");
        native["links"] = json!([{"url":"https://creator.example/contact","label":"Business","source":"description.runs"}]);
        let (_dir, owner, _) = legacy_scored_source(origin, native);
        finish_next_page(
            &owner,
            &format!("https://www.youtube.com/channel/{CHANNEL}"),
            "Native creator biography.",
            false,
        );
        finish_next_page(
            &owner,
            "https://creator.example/contact",
            "Business studio@example.org",
            false,
        );
        let snapshot = owner
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap();
        assert_eq!(snapshot.rows[0].verdict, "qualified");
        let email = snapshot.rows[0]
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::Email)
            .unwrap();
        assert_eq!(email.source_url, "https://creator.example/contact");
        assert_eq!(email.normalized_value, "studio@example.org");
        assert!(
            owner
                .repository
                .next_discovery_contact_lookup(CAMPAIGN, &[])
                .unwrap()
                .is_none()
        );
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            4
        );
        assert_eq!(
            owner
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .platform_runs[0]
                .platform,
            origin
        );
    }
}

#[test]
fn discovery_web_sources_legacy_external_lookup_uses_native_name_and_requires_observed_author_binding()
 {
    let mut native = material(VIDEO);
    native["description"] = json!("Original keyboard review.");
    let (_dir, owner, _) = legacy_scored_source("yahoo", native);
    let profile = format!("https://www.youtube.com/channel/{CHANNEL}");
    finish_next_page(&owner, &profile, "Native creator biography.", false);
    let lookup = owner
        .repository
        .next_discovery_contact_lookup(CAMPAIGN, &[])
        .unwrap()
        .unwrap();
    assert_eq!(lookup.method, "search");
    assert_eq!(lookup.author_label, "Native creator");
    assert!(lookup.query.contains("Native creator"));
    assert!(!lookup.query.contains("検索要約"));
    let mut claim = owner
        .repository
        .claim_discovery_contact_lookup(lookup)
        .unwrap();
    let campaign = owner.get_discovery_campaign(CAMPAIGN).unwrap();
    assert_eq!(campaign.platform_runs[0].platform, "yahoo");
    assert_eq!(campaign.platform_runs[0].status, "searching");
    claim.status = "completed".into();
    claim.links = vec!["https://creator.example/contact".into()];
    owner
        .repository
        .finish_discovery_contact_lookup(claim)
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert!(
        snapshot.rows[0]
            .contacts
            .iter()
            .all(|contact| contact.kind == DiscoveryContactKind::SocialProfile),
        "readback restores the native account; a search link alone must not add an email"
    );
    finish_next_page(
        &owner,
        "https://creator.example/contact",
        &format!("Business studio@example.org. Official channel {profile}"),
        false,
    );
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(
        snapshot.rows[0]
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::Email)
            .unwrap()
            .normalized_value,
        "studio@example.org"
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        5
    );
}

#[test]
fn discovery_web_sources_preserve_each_engine_authority_native_original_and_paid_receipt() {
    for origin in ["google", "yahoo"] {
        let (dir, owner) = setup(origin, &[source(VIDEO)]);
        let hit = next(&owner);
        let before = owner.get_discovery_campaign(CAMPAIGN).unwrap();
        let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
        assert_eq!(claim.origin_platform, origin);
        assert_eq!(claim.target_platform, "youtube");
        assert_eq!(claim.video_id, VIDEO);
        assert_eq!(claim.call_number, 2);
        let read = owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim: claim.clone(),
                native_material: Some(material(VIDEO)),
                stop_code: None,
            })
            .unwrap();
        assert_eq!(read.native_author_id.as_deref(), Some(CHANNEL));
        assert_eq!(
            read.native_material.as_ref().unwrap()["description"],
            material(VIDEO)["description"]
        );
        let after = owner.get_discovery_campaign(CAMPAIGN).unwrap();
        assert_eq!(after.scopes, before.scopes);
        assert_eq!(after.platform_runs.len(), 1);
        assert_eq!(after.platform_runs[0].platform, origin);
        assert_eq!(after.used_calls, 2);
        assert!(read.candidate_id.is_some());
        assert!(read.is_primary_source);
        let review = owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap();
        assert_eq!(review.format_version, 2);
        assert_eq!(review.hit.platform, origin);
        assert_eq!(review.hit.profile_ref, "dedicated");
        assert_eq!(review.native_platform.as_deref(), Some("youtube"));
        assert_eq!(review.native_author_id, CHANNEL);
        assert_eq!(review.candidate_id, read.candidate_id.clone().unwrap());
        assert_eq!(
            review.video_description.as_deref(),
            material(VIDEO)["description"].as_str()
        );
        assert!(
            !review
                .video_description
                .as_ref()
                .unwrap()
                .contains(&review.hit.title)
        );
        // Original material is available to score, but comments still need an audited language-only judgment.
        assert!(
            owner
                .repository
                .claim_discovery_language(CAMPAIGN, 1, &hit)
                .unwrap()
                .is_none()
        );
        assert!(owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).is_err());
        drop(owner);
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
        assert_eq!(
            owner.get_discovery_web_source(CAMPAIGN, &hit).unwrap(),
            read
        );
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            2
        );
    }
}
#[test]
fn discovery_web_sources_review_saves_only_source_bound_contact_and_retains_score_after_readback() {
    for origin in ["google", "yahoo"] {
        let (dir, owner) = setup_with_language(origin, &[source(VIDEO)], "English");
        let hit = next(&owner);
        let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
        let read = owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim,
                native_material: Some(material(VIDEO)),
                stop_code: None,
            })
            .unwrap();
        let review = owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap();
        let receipt = owner.submit_discovery_review("isolated-test-account", SubmitDiscoveryReviewInput {
            campaign_id: CAMPAIGN.into(), expected_revision: 1, search_hit_id: hit.clone(),
            material_hash: review.material_hash, request_id: "original-source-score".into(),
            reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: INITIAL_MATCH_REVIEWER.into() },
            findings: vec![DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::MatchFit, outcome: DiscoveryReviewOutcome::Supported,
                rationale: "Saved original English keyboard review matches the requested topic and language.".into(),
                citations: vec![DiscoveryReviewQuote::Description { quote: "Original keyboard review.".into() }],
                tool: None, country_code: None, match_level: Some(DiscoveryMatchLevel::Good),
            }],
        }).unwrap();
        assert_eq!(receipt.verdict, "qualified");
        let reread = owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap();
        assert_eq!(reread.candidate_id, read.candidate_id.unwrap());
        assert_eq!(reread.hit.platform, origin);
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        let contact:(String,String,String,String) = db.query_row(
            "SELECT native_author_id,normalized_value,source_url,attribution FROM discovery_contact_observations WHERE candidate_id=?1 AND kind='email'",
            [&reread.candidate_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).unwrap();
        assert_eq!(
            contact,
            (
                CHANNEL.into(),
                "studio@example.org".into(),
                source(VIDEO),
                "author_published".into()
            )
        );
        let verdict: String = db
            .query_row(
                "SELECT verdict FROM discovery_candidates WHERE id=?1",
                [&reread.candidate_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(verdict, "qualified");
        let publication = owner
            .prepare_discovery_library_publication(CAMPAIGN, 1)
            .unwrap();
        let row = &publication.material.sources.rows[0];
        assert_eq!(row.hit.platform, origin);
        assert_eq!(row.native_platform.as_deref(), Some("youtube"));
        assert_eq!(row.author_platform(), "youtube");
        assert_eq!(row.verdict, "qualified");
        assert_eq!(
            row.contacts
                .iter()
                .find(|contact| contact.kind == DiscoveryContactKind::Email)
                .unwrap()
                .normalized_value,
            "studio@example.org"
        );
        let group = &publication.material.identity.groups[0];
        assert_eq!(group.qualified_member_count, 1);
        assert_eq!(group.members[0].platform, "youtube");
        assert_eq!(group.members[0].display_name, "Native creator");
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
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            2
        );
    }
}
#[test]
fn discovery_web_sources_bind_duplicate_videos_to_one_author_without_rescoring() {
    let (dir, owner) = setup("google", &[source(VIDEO), source("mnopqrstuvw")]);
    let first = next(&owner);
    let claim = owner
        .claim_discovery_web_source(CAMPAIGN, 1, &first)
        .unwrap();
    let primary = owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            native_material: Some(material(&claim.video_id)),
            claim,
            stop_code: None,
        })
        .unwrap();
    assert!(primary.is_primary_source);
    let second = next(&owner);
    let claim = owner
        .claim_discovery_web_source(CAMPAIGN, 1, &second)
        .unwrap();
    let duplicate = owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            native_material: Some(material(&claim.video_id)),
            claim,
            stop_code: None,
        })
        .unwrap();
    assert_eq!(duplicate.candidate_id, primary.candidate_id);
    assert!(!duplicate.is_primary_source);
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &second)
            .is_err()
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_candidates", [], |row| row
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_platform_runs WHERE platform='youtube'",
            [],
            |row| row.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        3
    );
}
#[test]
fn discovery_web_sources_failures_and_restart_advance_without_replay() {
    let (dir, owner) = setup("google", &[source(VIDEO), source("mnopqrstuvw")]);
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    let failed = owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: None,
            stop_code: Some("opencli_invalid_output".into()),
        })
        .unwrap();
    assert_eq!(failed.status, "failed");
    let second = next(&owner);
    assert_ne!(hit, second);
    owner
        .claim_discovery_web_source(CAMPAIGN, 1, &second)
        .unwrap();
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    assert_eq!(
        owner
            .get_discovery_web_source(CAMPAIGN, &second)
            .unwrap()
            .stop_code
            .as_deref(),
        Some("interrupted_web_source_read")
    );
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
    assert!(
        owner
            .claim_discovery_web_source(CAMPAIGN, 1, &second)
            .is_err()
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        3
    );
}
#[test]
fn discovery_web_sources_reject_changed_claims_and_foreign_originals_without_charging() {
    let (_dir, owner) = setup("yahoo", &[source(VIDEO)]);
    let hit = next(&owner);
    assert!(owner.claim_discovery_web_source(CAMPAIGN, 2, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        1
    );
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    for key in ["profile_ref", "origin_platform", "source_url"] {
        let mut wrong = serde_json::to_value(&claim).unwrap();
        wrong[key] = json!("wrong");
        assert!(
            owner
                .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                    claim: serde_json::from_value(wrong).unwrap(),
                    native_material: Some(material(VIDEO)),
                    stop_code: None
                })
                .is_err()
        );
    }
    for (key, value) in [
        ("videoId", json!("mnopqrstuvw")),
        ("channelId", json!("Display Name")),
        ("descriptionSource", json!("search_summary")),
        ("sourceUrl", json!(source("mnopqrstuvw"))),
        ("credential", json!("must-never-save")),
    ] {
        let mut native = material(VIDEO);
        native[key] = value;
        assert!(
            owner
                .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                    claim: claim.clone(),
                    native_material: Some(native),
                    stop_code: None
                })
                .is_err()
        );
    }
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        2
    );
    owner
        .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
            claim,
            native_material: None,
            stop_code: Some("opencli_invalid_output".into()),
        })
        .unwrap();
}
#[test]
fn discovery_web_sources_login_and_risk_quarantine_native_target_without_faking_an_engine_failure()
{
    for code in ["opencli_auth_required", "opencli_risk_control"] {
        let (_dir, owner) = setup("google", &[source(VIDEO), source("mnopqrstuvw")]);
        let hit = next(&owner);
        let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
        owner
            .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                claim,
                native_material: None,
                stop_code: Some(code.into()),
            })
            .unwrap();
        let campaign = owner.get_discovery_campaign(CAMPAIGN).unwrap();
        assert_eq!(campaign.platform_runs[0].status, "completed");
        assert_eq!(campaign.platform_runs[0].stop_code, None);
        assert!(!matches!(
            owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
            DiscoveryStep::ReadWebSource { .. }
        ));
        assert_eq!(campaign.used_calls, 2);
    }
}
#[test]
fn discovery_web_sources_yahoo_watch_path_keeps_native_identity_requirement() {
    let url = format!("https://www.youtube.com/watch/{VIDEO}");
    let (_dir, owner) = setup("yahoo", std::slice::from_ref(&url));
    let hit = next(&owner);
    let claim = owner.claim_discovery_web_source(CAMPAIGN, 1, &hit).unwrap();
    assert_eq!(claim.video_id, VIDEO);
    assert_eq!(claim.source_url, url);
    assert_eq!(claim.target_platform, "youtube");
    let mut input = FinishDiscoveryWebSourceInput {
        claim,
        native_material: Some(material("mnopqrstuvw")),
        stop_code: None,
    };
    assert!(super::validate_result(&input).is_err());
    input.native_material = Some(material(VIDEO));
    let read = owner.finish_discovery_web_source(input).unwrap();
    assert_eq!(read.status, "completed");
    assert_eq!(read.native_author_id.as_deref(), Some(CHANNEL));
    assert!(read.candidate_id.is_some());
    assert_eq!(read.claim.source_url, url);
}

#[test]
fn discovery_web_sources_unsupported_urls_are_not_read_and_exclusions_cost_nothing() {
    let (_dir, owner) = setup(
        "google",
        &["https://store.example.org/keyboard".into(), source(VIDEO)],
    );
    let hit = next(&owner);
    for origin in ["google", "youtube"] {
        assert!(!matches!(
            owner
                .plan_discovery_step_excluding(CAMPAIGN, 1, &[origin.into()])
                .unwrap(),
            DiscoveryStep::ReadWebSource { .. }
        ));
    }
    for invalid in [
        "http://www.youtube.com/watch?v=abcdefghijk",
        "https://evil.example.org/watch?v=abcdefghijk",
        "https://www.youtube.com/watch?v=abcdefghijk&v=mnopqrstuvw",
        "https://www.youtube.com/shorts/abcdefghijk/extra",
        "https://www.youtube.com/@Creator",
        "https://www.youtube.com/watch?v=abcdefghijk#other",
        "https://www.youtube.com/watch/abcdefghijk/extra",
        "https://www.youtube.com/watch/abcdefghijk?v=mnopqrstuvw",
        "https://www.youtube.com/watch/abcdefghij",
    ] {
        assert_eq!(super::video_id(invalid), None, "{invalid}");
    }
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        1
    );
    assert!(!hit.is_empty());
}
