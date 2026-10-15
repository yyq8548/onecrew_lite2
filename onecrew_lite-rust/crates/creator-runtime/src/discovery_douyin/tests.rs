use crate::*;
use serde_json::{Value, json};

const CAMPAIGN: &str = "douyin-source-flow";
const AUTHOR: &str = "MS4wLjABAAAA_native_author_123456789";
const POST: &str = "7688639864627214835";
fn source(post: &str) -> String {
    format!("https://www.douyin.com/video/{post}")
}
fn profile(email: bool, known: bool) -> Value {
    json!({"adapter_version":"1.0.0","author_id":AUTHOR,"author_label":"Native keyboard creator",
        "profile_url":format!("https://www.douyin.com/user/{AUTHOR}"),"follower_count":known.then_some(4892),
        "follower_text":if known {"4892"} else {"不可见"},"bio":if email {"Keyboard creator. Business studio@example.org"} else {"Keyboard creator"},
        "bio_truncated":false,"public_links":["https://creator.example.com/contact"],"evidence_source":"visible_profile_header"})
}
fn content(post: &str) -> Value {
    json!({"post_id":post,"author_id":AUTHOR,"author_label":"Native keyboard creator",
        "profile_url":format!("https://www.douyin.com/user/{AUTHOR}"),"source_url":source(post),
        "text":"Original keyboard review and comparison.","text_truncated":false,"evidence_source":"visible_native_video_caption"})
}
fn setup(criteria: Value, posts: &[&str]) -> (tempfile::TempDir, CreatorRuntime) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: CAMPAIGN.into(),
            brief: "Keyboard reviewers".into(),
            opencli_version: "1.8.8".into(),
            criteria,
            target_count: 2,
            max_calls: Some(20),
            scopes: vec![DiscoveryPlatformScope {
                platform: "douyin".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["keyboard review".into()],
            }],
        })
        .unwrap();
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    let claim = owner
        .claim_discovery_search(CAMPAIGN, 1, "douyin", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: posts
                .iter()
                .map(|post| DiscoverySearchHitInput {
                    source_url: source(post),
                    title: "Matched video".into(),
                    author_label: "Native keyboard creator".into(),
                    author_id: None,
                    author_url: None,
                })
                .collect(),
        })
        .unwrap();
    (dir, owner)
}
fn next(owner: &CreatorRuntime) -> String {
    let DiscoveryStep::ReadSourceAuthor { search_hit_id } =
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
    else {
        panic!("native source author required")
    };
    search_hit_id
}
fn save(owner: &CreatorRuntime, hit: &str, email: bool, known: bool) -> DiscoverySourceAuthorRead {
    let claim = owner
        .claim_discovery_source_author(CAMPAIGN, 1, hit)
        .unwrap();
    let body = content(&claim.post_id);
    owner
        .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
            claim,
            native_profile: Some(profile(email, known)),
            native_content: Some(body),
            stop_code: None,
        })
        .unwrap()
}
fn review(owner: &CreatorRuntime, hit: &str) -> SubmitDiscoveryReviewInput {
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_author_id, AUTHOR);
    assert_eq!(material.matched_language_source(), material.hit.source_url);
    SubmitDiscoveryReviewInput {
        campaign_id: CAMPAIGN.into(),
        expected_revision: 1,
        search_hit_id: hit.into(),
        material_hash: material.material_hash,
        request_id: "douyin-original-score".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: INITIAL_MATCH_REVIEWER.into(),
        },
        findings: vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale: "Original keyboard comparison is written in English".into(),
            citations: vec![DiscoveryReviewQuote::NativeContent {
                quote: "Original keyboard review and comparison.".into(),
            }],
            tool: None,
            country_code: None,
            match_level: Some(DiscoveryMatchLevel::Good),
        }],
    }
}

#[test]
fn douyin_legacy_invalid_sources_are_preserved_but_never_dispatched_or_charged() {
    let (dir, owner) = setup(json!({}), &[POST]);
    let valid_hit = next(&owner);
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let invalid_sources = [
        "https://www.douyin.com/search/keyboard",
        "https://www.douyin.com/video/01",
        "https://www.douyin.com/video/0",
        "https://www.douyin.com/video/123?author_id=unbound",
        "https://www.douyin.com/video/123#comments",
        "https://www.douyin.com/video/123456789012345678901",
        "https://www.douyin.com/video/１２３",
        "https://www.douyin.com/video/123/",
        "https://www.douyin.com.evil.example/video/123",
    ];
    for (index, source) in invalid_sources.iter().enumerate() {
        // Simulate a receipt persisted by an older version. Current search
        // admission already refuses these sources.
        db.execute(
            "INSERT INTO discovery_search_hits
             SELECT ?1,discovery_campaign_id,platform,profile_ref,search_keyword,
               ?2,title,author_label,captured_at-1
             FROM discovery_search_hits WHERE id=?3",
            rusqlite::params![format!("legacy-{index}"), source, valid_hit],
        )
        .unwrap();
    }
    assert_eq!(next(&owner), valid_hit);
    for index in 0..invalid_sources.len() {
        assert!(
            owner
                .claim_discovery_source_author(CAMPAIGN, 1, &format!("legacy-{index}"))
                .is_err()
        );
    }
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        1
    );
    let claim = owner
        .claim_discovery_source_author(CAMPAIGN, 1, &valid_hit)
        .unwrap();
    owner
        .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
            claim,
            native_profile: None,
            native_content: None,
            stop_code: Some("opencli_invalid_output".into()),
        })
        .unwrap();
    assert_eq!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::SearchExhausted
    );
    let state = owner.get_discovery_campaign(CAMPAIGN).unwrap();
    assert_eq!(state.phase, "active");
    assert_eq!(state.used_calls, 2);
    let rows: Vec<(String, String)> = db
        .prepare(
            "SELECT id,source_url FROM discovery_search_hits WHERE id LIKE 'legacy-%' ORDER BY id",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), invalid_sources.len());
    for (index, (id, source)) in rows.iter().enumerate() {
        assert_eq!(id, &format!("legacy-{index}"));
        assert_eq!(source, invalid_sources[index]);
    }
    let attempts: u32 = db
        .query_row(
            "SELECT count(*) FROM discovery_douyin_author_reads",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempts, 1);
    drop(db);
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::SearchExhausted
    );
    assert_eq!(
        reopened
            .get_discovery_campaign(CAMPAIGN)
            .unwrap()
            .used_calls,
        2
    );
}

#[tokio::test]
async fn douyin_original_score_profile_email_and_restart_need_no_comment_reservation() {
    let (dir, owner) = setup(json!({"language":"English"}), &[POST]);
    let hit = next(&owner);
    let before = owner.get_discovery_campaign(CAMPAIGN).unwrap();
    assert_eq!(before.used_calls, 1);
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .is_err()
    );
    let saved = save(&owner, &hit, true, true);
    assert_eq!(saved.author_id.as_deref(), Some(AUTHOR));
    let p = owner.get_discovery_profile(CAMPAIGN, &hit).unwrap();
    let body = owner.get_discovery_content(CAMPAIGN, &hit).unwrap();
    assert_eq!(p.claim.call_number, body.claim.call_number);
    assert_eq!(body.claim.source_url, source(POST));
    assert!(
        owner
            .claim_discovery_source_author(CAMPAIGN, 1, &hit)
            .is_err()
    );
    assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
    owner
        .submit_discovery_review("account", review(&owner, &hit))
        .unwrap();
    assert!(
        owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows.len(), 1);
    let row = &snapshot.rows[0];
    assert_eq!(row.verdict, "qualified");
    assert_eq!(row.native_author_id.as_deref(), Some(AUTHOR));
    let email = row
        .contacts
        .iter()
        .find(|contact| contact.kind == DiscoveryContactKind::Email)
        .unwrap();
    assert_eq!(email.normalized_value, "studio@example.org");
    assert_eq!(
        email.source_url,
        format!("https://www.douyin.com/user/{AUTHOR}")
    );
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
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_source_author(CAMPAIGN, &hit)
            .unwrap(),
        saved
    );
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
        2
    );
}

#[test]
fn douyin_wrong_video_native_identity_and_private_fields_never_commit_partial_results() {
    let (_dir, owner) = setup(json!({}), &[POST]);
    let hit = next(&owner);
    let claim = owner
        .claim_discovery_source_author(CAMPAIGN, 1, &hit)
        .unwrap();
    for (key, wrong) in [
        ("author_id", json!("23667677481")),
        ("profile_url", json!("https://www.douyin.com/user/other")),
        ("source_url", json!(source("123"))),
        ("post_id", json!("123")),
        ("recommendation", json!("unbound")),
    ] {
        let mut body = content(POST);
        body[key] = wrong;
        assert!(
            owner
                .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                    claim: claim.clone(),
                    native_profile: Some(profile(true, true)),
                    native_content: Some(body),
                    stop_code: None
                })
                .is_err()
        );
        assert_eq!(
            owner
                .get_discovery_source_author(CAMPAIGN, &hit)
                .unwrap()
                .status,
            "reading"
        );
        assert!(owner.get_discovery_profile(CAMPAIGN, &hit).is_err());
        assert_eq!(
            owner
                .count_discovery_candidates(CAMPAIGN)
                .unwrap()
                .qualified,
            0
        );
    }
    for (key, wrong) in [
        ("follower_count", json!(0)),
        ("session", json!("must not persist")),
        ("public_links", json!(["https://127.0.0.1/"])),
    ] {
        let mut p = profile(true, true);
        p[key] = wrong;
        assert!(
            owner
                .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                    claim: claim.clone(),
                    native_profile: Some(p),
                    native_content: Some(content(POST)),
                    stop_code: None
                })
                .is_err()
        );
    }
    owner
        .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
            claim: claim.clone(),
            native_profile: Some(profile(true, true)),
            native_content: Some(content(POST)),
            stop_code: None,
        })
        .unwrap();
    assert!(
        owner
            .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                claim,
                native_profile: Some(profile(true, true)),
                native_content: Some(content(POST)),
                stop_code: None
            })
            .is_err()
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        2
    );
}

#[test]
fn douyin_duplicate_author_keeps_one_profile_caption_candidate_and_score() {
    let (_dir, owner) = setup(json!({}), &[POST, "7688639864627214836"]);
    let first = next(&owner);
    save(&owner, &first, true, true);
    owner
        .submit_discovery_review("account", review(&owner, &first))
        .unwrap();
    let second = next(&owner);
    assert_ne!(first, second);
    let receipt = save(&owner, &second, true, true);
    assert_eq!(receipt.duplicate_of.as_deref(), Some(first.as_str()));
    assert!(owner.get_discovery_profile(CAMPAIGN, &second).is_err());
    assert!(owner.get_discovery_content(CAMPAIGN, &second).is_err());
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &second)
            .is_err()
    );
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        3
    );
}

#[test]
fn douyin_failed_source_and_restart_interruption_advance_without_replay() {
    for interrupted in [false, true] {
        let (dir, owner) = setup(json!({}), &[POST, "7688639864627214836"]);
        let first = next(&owner);
        let claim = owner
            .claim_discovery_source_author(CAMPAIGN, 1, &first)
            .unwrap();
        if !interrupted {
            owner
                .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                    claim: claim.clone(),
                    native_profile: None,
                    native_content: None,
                    stop_code: Some("opencli_invalid_output".into()),
                })
                .unwrap();
        }
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        if interrupted {
            assert_eq!(reopened.interrupt_discovery_searches().unwrap(), 1);
        }
        assert_eq!(
            reopened
                .get_discovery_source_author(CAMPAIGN, &first)
                .unwrap()
                .status,
            "failed"
        );
        assert!(
            reopened
                .claim_discovery_source_author(CAMPAIGN, 1, &first)
                .is_err()
        );
        assert_ne!(next(&reopened), first);
        assert_eq!(
            reopened.get_discovery_campaign(CAMPAIGN).unwrap().phase,
            "active"
        );
        assert_eq!(
            reopened
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .used_calls,
            2
        );
    }
}

#[test]
fn douyin_abbreviated_followers_preserve_native_display_text() {
    let (_dir, owner) = setup(json!({}), &[POST]);
    let hit = next(&owner);
    save_rounded(&owner, &hit, "4.8万");
    let candidates = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: CAMPAIGN.into(),
            limit: 10,
            after_id: None,
        })
        .unwrap();
    assert_eq!(candidates[0].evidence["subscribers_raw"], "4.8万");
    let receipt = owner.get_discovery_profile(CAMPAIGN, &hit).unwrap();
    assert!(receipt.native_profile.unwrap()["follower_count"].is_null());
}

fn save_rounded(owner: &CreatorRuntime, hit: &str, raw: &str) {
    let claim = owner
        .claim_discovery_source_author(CAMPAIGN, 1, hit)
        .unwrap();
    let mut native = profile(true, false);
    native["follower_text"] = json!(raw);
    let body = content(&claim.post_id);
    owner
        .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
            claim,
            native_profile: Some(native),
            native_content: Some(body),
            stop_code: None,
        })
        .unwrap();
}

#[test]
fn douyin_rounded_followers_use_conservative_bounds_for_review() {
    for (minimum, expected_error) in [
        (8000, None),
        (439000, Some("initial_match_required_count_unknown")),
        (500000, Some("initial_match_required_failure_must_be_low")),
    ] {
        let (_dir, owner) = setup(json!({"min_followers": minimum}), &[POST]);
        let hit = next(&owner);
        save_rounded(&owner, &hit, "43.9万");
        let material = owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap();
        assert_eq!(material.native_followers, None);
        let snapshot = owner.prepare_discovery_library_snapshot(CAMPAIGN, 1).unwrap();
        assert_eq!(snapshot.rows[0].followers.as_deref(), Some("~43.9万"));
        let estimate = material.native_follower_estimate.unwrap();
        assert_eq!(
            (estimate.nominal, estimate.lower, estimate.upper),
            (439000, 438000, 440000)
        );
        let result = owner.submit_discovery_review("account", review(&owner, &hit));
        match expected_error {
            Some(error) => assert!(result.unwrap_err().contains(error)),
            None => {
                result.unwrap();
            }
        }
    }
    for raw in [
        "",
        "约43.9万",
        "43.9万赞",
        "43.9",
        "-1万",
        "1.万",
        "1.0000001万",
        "999999999999999亿",
    ] {
        assert!(super::follower_estimate(raw).is_none(), "{raw}");
    }
}

#[test]
fn douyin_unknown_required_followers_block_but_verified_profile_is_a_contact_route() {
    let (_dir, owner) = setup(json!({"language":"English","min_followers":1000}), &[POST]);
    let hit = next(&owner);
    save(&owner, &hit, true, false);
    assert!(
        owner
            .submit_discovery_review("account", review(&owner, &hit))
            .unwrap_err()
            .contains("initial_match_required_count_unknown")
    );
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
    let (_dir, owner) = setup(json!({"language":"English"}), &[POST]);
    let hit = next(&owner);
    save(&owner, &hit, false, true);
    owner
        .submit_discovery_review("account", review(&owner, &hit))
        .unwrap();
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        1
    );
    let rows = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap()
        .rows;
    assert_eq!(rows[0].verdict, "qualified");
    assert!(rows[0].contacts.iter().any(|contact| contact.kind
        == DiscoveryContactKind::SocialProfile
        && contact.normalized_value == format!("https://www.douyin.com/user/{AUTHOR}")
        && contact.native_author_id == AUTHOR
        && contact.attribution == DiscoveryContactAttribution::AuthorPublished));
    drop(owner);
    let reopened = CreatorRuntime::open(_dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap()
            .rows,
        rows
    );
}

fn language_pending(owner: &CreatorRuntime, hit: &str) {
    let mut input = review(owner, hit);
    input.request_id = "douyin-language-request".into();
    let finding = &mut input.findings[0];
    finding.outcome = DiscoveryReviewOutcome::Insufficient;
    finding.rationale = "Only original content language is unresolved".into();
    finding.match_level = None;
    finding.citations.clear();
    let mut language = finding.clone();
    language.criterion = DiscoveryReviewCriterion::ContentLanguage;
    input.findings.push(language);
    owner.submit_discovery_review("account", input).unwrap();
}
fn comments() -> DiscoveryLanguageSample {
    DiscoveryLanguageSample {
        author_id: AUTHOR.into(),
        source_url: source(POST),
        text: content(POST)["text"].as_str().unwrap().into(),
        comments: [
            "",
            "この説明はとても分かりやすいです",
            "この製品を使ってみたいです",
            "紹介してくれてありがとうございます",
            "Thanks for sharing",
        ]
        .iter()
        .enumerate()
        .map(|(i, text)| DiscoveryLanguageComment {
            source_url: format!("{}#comment-{}", source(POST), 100 + i),
            text: (*text).into(),
        })
        .collect(),
    }
}
#[tokio::test]
async fn douyin_comments_only_after_original_language_pending_three_votes_and_restart() {
    let (dir, owner) = setup(json!({"language":"日本語"}), &[POST]);
    let hit = next(&owner);
    save(&owner, &hit, true, true);
    assert!(
        owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        2
    );
    language_pending(&owner, &hit);
    assert_eq!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.clone()
        }
    );
    let claim = owner
        .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.root_url, source(POST));
    assert_eq!(claim.author_id, AUTHOR);
    let sample = comments();
    for mutate in 0..5 {
        let mut wrong = sample.clone();
        match mutate {
            0 => wrong.text = "Different video caption".into(),
            1 => wrong.author_id = "23667677481".into(),
            2 => wrong.comments[1].source_url = format!("{}#comment-101", source("99")),
            3 => wrong.comments.push(wrong.comments[1].clone()),
            _ => wrong.comments[2].source_url = wrong.comments[1].source_url.clone(),
        }
        assert!(
            owner
                .finish_discovery_language_async(claim.clone(), Some(wrong), None)
                .await
                .is_err()
        );
    }
    owner
        .finish_discovery_language_async(claim.clone(), Some(sample.clone()), None)
        .await
        .unwrap();
    assert!(
        owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    let mut input = review(&owner, &hit);
    input.request_id = "douyin-audience-score".into();
    input.findings[0].rationale =
        "Audience comment language matches 日本語; original caption matches keyboard topic".into();
    input.findings[0].citations = sample
        .comments
        .iter()
        .skip(1)
        .take(2)
        .map(|c| DiscoveryReviewQuote::NativeContent {
            quote: c.text.clone(),
        })
        .collect();
    assert_eq!(
        owner
            .submit_discovery_review("account", input.clone())
            .unwrap_err(),
        "initial_match_audience_sample_insufficient"
    );
    input.findings[0]
        .citations
        .push(DiscoveryReviewQuote::NativeContent {
            quote: sample.comments[3].text.clone(),
        });
    owner.submit_discovery_review("account", input).unwrap();
    let rows = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap()
        .rows;
    assert_eq!(rows[0].verdict, "qualified");
    assert!(
        rows[0]
            .contacts
            .iter()
            .any(|contact| contact.kind == DiscoveryContactKind::Email
                && contact.normalized_value == "studio@example.org")
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap()
            .rows,
        rows
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
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn douyin_comment_failure_continues_next_video_without_replay() {
    let (_dir, owner) = setup(json!({"language":"日本語"}), &[POST, "7688639864627214836"]);
    let first = next(&owner);
    save(&owner, &first, true, true);
    language_pending(&owner, &first);
    let claim = owner
        .claim_discovery_language_async(CAMPAIGN.into(), 1, first.clone())
        .await
        .unwrap()
        .unwrap();
    owner
        .finish_discovery_language_async(claim, None, Some("opencli_invalid_output".into()))
        .await
        .unwrap();
    assert!(
        owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, first.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_ne!(next(&owner), first);
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().phase,
        "active"
    );
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        3
    );
}

#[test]
fn douyin_auth_risk_and_changed_claim_do_not_fabricate_author_or_replay() {
    for (code, status) in [
        ("opencli_auth_required", "unauthorized"),
        ("opencli_risk_control", "risk_control_stop"),
    ] {
        let (_dir, owner) = setup(json!({}), &[POST, "7688639864627214836"]);
        let first = next(&owner);
        let claim = owner
            .claim_discovery_source_author(CAMPAIGN, 1, &first)
            .unwrap();
        let mut wrong = claim.clone();
        wrong.profile_ref = "other".into();
        assert!(
            owner
                .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                    claim: wrong,
                    native_profile: None,
                    native_content: None,
                    stop_code: Some(code.into())
                })
                .is_err()
        );
        let receipt = owner
            .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                claim,
                native_profile: None,
                native_content: None,
                stop_code: Some(code.into()),
            })
            .unwrap();
        assert!(receipt.author_id.is_none());
        assert!(receipt.profile_url.is_none());
        assert_eq!(
            owner
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .platform_runs[0]
                .status,
            status
        );
        assert!(
            owner
                .repository
                .next_discovery_source_author_hit(CAMPAIGN, &[])
                .unwrap()
                .is_none()
        );
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().phase,
            "active"
        );
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            2
        );
    }
    let (dir, owner) = setup(json!({}), &[POST]);
    assert!(
        owner
            .repository
            .next_discovery_source_author_hit(CAMPAIGN, &["douyin".into()])
            .unwrap()
            .is_none()
    );
    let first = next(&owner);
    let claim = owner
        .claim_discovery_source_author(CAMPAIGN, 1, &first)
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_campaigns SET phase='paused' WHERE id=?1",
        [CAMPAIGN],
    )
    .unwrap();
    assert!(
        owner
            .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                claim,
                native_profile: Some(profile(true, true)),
                native_content: Some(content(POST)),
                stop_code: None
            })
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_source_author(CAMPAIGN, &first)
            .unwrap()
            .status,
        "reading"
    );
    assert!(owner.get_discovery_profile(CAMPAIGN, &first).is_err());
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn douyin_official_contact_page_is_bound_to_profile_and_stops_lookup_after_email() {
    let (dir, owner) = setup(json!({"language":"English"}), &[POST]);
    let hit = next(&owner);
    save(&owner, &hit, false, true);
    owner
        .submit_discovery_review("account", review(&owner, &hit))
        .unwrap();
    assert!(
        owner
            .repository
            .next_discovery_public_page(CAMPAIGN, &[])
            .unwrap()
            .is_none()
    );
    // A legacy saved review may have no persisted profile route. Its observed
    // official contact page still uses the same author-bound read guards.
    rusqlite::Connection::open(dir.path().join("crm.db")).unwrap()
        .execute("DELETE FROM discovery_contact_observations WHERE discovery_campaign_id=?1 AND kind='social_profile'", [CAMPAIGN]).unwrap();
    let page = owner
        .repository
        .next_discovery_public_page(CAMPAIGN, &[])
        .unwrap()
        .unwrap();
    assert_eq!(page.requested_url, "https://creator.example.com/contact");
    let claim = owner.claim_discovery_public_page(page).unwrap();
    // The authorization root remains the matched video; the observed official
    // URL and its profile-bound excerpt are retained separately in the claim.
    assert_eq!(claim.source_url, source(POST));
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            claim,
            document_url: Some("https://creator.example.com/contact".into()),
            title: "Business contact".into(),
            markdown_excerpt: "Business website@example.org".into(),
            truncated: false,
            stop_code: None,
            hidden_email_available: false,
        })
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    let email = snapshot.rows[0]
        .contacts
        .iter()
        .find(|contact| contact.kind == DiscoveryContactKind::Email)
        .unwrap();
    assert_eq!(email.normalized_value, "website@example.org");
    assert_eq!(email.source_url, "https://creator.example.com/contact");
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
        3
    );
}
