use crate::*;
use serde_json::{Value, json};

const CAMPAIGN: &str = "linkedin-flow";
const HANDLE: &str = "native-author";
const ACCOUNT: &str = "ACoAAA123456789abcdefghijk";
const PROFILE: &str = "https://www.linkedin.com/in/native-author/";
const POST: &str = "https://www.linkedin.com/feed/update/urn:li:activity:7432844064649293824/";
const WEBSITE: &str = "https://creator.example.com/contact";

fn profile(handle: &str, email: bool, known_followers: bool) -> Value {
    json!({"adapter_version":"1.0.0", "author_handle":handle, "account_id":ACCOUNT,
        "author_label":"Native Author", "profile_url":format!("https://www.linkedin.com/in/{handle}/"),
        "follower_count":known_followers.then_some(1606), "follower_text":if known_followers {"1,606 followers"} else {"1.6K followers"},
        "headline":"Keyboard review creator", "bio":if email {"Keyboard review creator. Business studio@example.org"} else {"Keyboard review creator"},
        "public_links":[WEBSITE], "contact_links":[format!("https://www.linkedin.com/in/{handle}/overlay/contact-info/")],
        "evidence_source":"visible_profile_header", "bio_truncated":true})
}
fn content(handle: &str, email: bool) -> Value {
    json!({"account_id":ACCOUNT, "author_handle":handle, "profile_url":format!("https://www.linkedin.com/in/{handle}/"),
        "post_id":"7432844064649293824", "source_url":POST,
        "text":if email {"Original keyboard review and comparison. Business body@example.org"} else {"Original keyboard review and comparison."},
        "text_truncated":true, "evidence_source":"visible_author_activity"})
}
fn new_campaign(criteria: Value) -> (tempfile::TempDir, CreatorRuntime) {
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
                platform: "linkedin".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["keyboard review".into()],
            }],
        })
        .unwrap();
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    (dir, owner)
}
fn search_hit(handle: &str) -> DiscoverySearchHitInput {
    DiscoverySearchHitInput {
        source_url: format!("https://www.linkedin.com/in/{handle}/"),
        title: "Native Author".into(),
        author_label: "Keyboard reviewer".into(),
        author_id: Some(handle.into()),
        author_url: Some(format!("https://www.linkedin.com/in/{handle}/")),
    }
}
fn setup(
    criteria: Value,
    profile_email: bool,
    body_email: bool,
    known_followers: bool,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    let (dir, owner) = new_campaign(criteria);
    let claim = owner
        .claim_discovery_search(CAMPAIGN, 1, "linkedin", "dedicated")
        .unwrap();
    for changes in [
        ("other", PROFILE),
        (ACCOUNT, PROFILE),
        (HANDLE, "https://www.linkedin.com/company/native-author/"),
        (
            HANDLE,
            "https://www.linkedin.com/in/native-author/?next=other",
        ),
    ] {
        let mut wrong = search_hit(HANDLE);
        wrong.author_id = Some(changes.0.into());
        wrong.author_url = Some(changes.1.into());
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
            search_hits: vec![search_hit(HANDLE)],
        })
        .unwrap();
    let DiscoveryStep::ReadProfile { search_hit_id: hit } =
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
    else {
        panic!("expected profile")
    };
    let claim = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
    assert_eq!(
        claim.author_id, HANDLE,
        "search preserves a locator, not the native account ID"
    );
    for (key, bad) in [
        ("author_handle", json!("other")),
        ("account_id", json!(HANDLE)),
        ("profile_url", json!("https://www.linkedin.com/in/other/")),
        ("follower_count", json!(0)),
        ("public_links", json!(["https://127.0.0.1/contact"])),
        ("session", json!("must not persist")),
    ] {
        let mut wrong = profile(HANDLE, profile_email, known_followers);
        wrong[key] = bad;
        assert!(
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim: claim.clone(),
                    native_profile: Some(wrong),
                    stop_code: None
                })
                .is_err(),
            "{key}"
        );
    }
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim,
            native_profile: Some(profile(HANDLE, profile_email, known_followers)),
            stop_code: None,
        })
        .unwrap();
    assert!(owner.claim_discovery_profile(CAMPAIGN, 1, &hit).is_err());
    assert!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .is_err(),
        "bio is not Activity language evidence"
    );
    assert_eq!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.clone()
        }
    );
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    assert_eq!(claim.author_id, HANDLE);
    for (key, bad) in [
        ("account_id", json!("ACoAAA987654321otheraccount")),
        ("author_handle", json!("other")),
        ("source_url", json!(PROFILE)),
        ("post_id", json!("7432844064649293825")),
        ("evidence_source", json!("search_preview")),
        ("comments", json!([])),
    ] {
        let mut wrong = content(HANDLE, body_email);
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
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: Some(content(HANDLE, body_email)),
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
fn review(owner: &CreatorRuntime, hit: &str, bio_only: bool) -> SubmitDiscoveryReviewInput {
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_author_id, ACCOUNT);
    assert_eq!(material.matched_language_source(), POST);
    SubmitDiscoveryReviewInput {
        campaign_id: CAMPAIGN.into(),
        expected_revision: 1,
        search_hit_id: hit.into(),
        material_hash: material.material_hash,
        request_id: format!("linkedin-score-{bio_only}"),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: INITIAL_MATCH_REVIEWER.into(),
        },
        findings: vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale:
                "Original native author Activity body reviews keyboards and is written in English."
                    .into(),
            citations: if bio_only {
                vec![DiscoveryReviewQuote::NativeProfile {
                    profile_url: PROFILE.into(),
                    quote: "Keyboard review creator".into(),
                }]
            } else {
                vec![DiscoveryReviewQuote::NativeContent {
                    quote: "Original keyboard review and comparison.".into(),
                }]
            },
            tool: None,
            country_code: None,
            match_level: Some(DiscoveryMatchLevel::Good),
        }],
    }
}

fn restore_legacy_contactless_candidate(dir: &tempfile::TempDir) {
    // Older persisted reviews predate native-account contact observations. Keep
    // their audited score, identity, and observed links to exercise page guards.
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.execute(
            "DELETE FROM discovery_contact_observations WHERE discovery_campaign_id=?1
             AND criteria_revision=1 AND native_author_id=?2 AND kind='social_profile'
             AND normalized_value=?3 AND source_url=?3
             AND source_excerpt='Verified native author account' AND attribution='author_published'",
            rusqlite::params![CAMPAIGN, ACCOUNT, PROFILE],
        )
        .unwrap(),
        1
    );
}

#[test]
fn linkedin_manual_contact_requires_audited_current_match_with_or_without_email() {
    for email in [false, true] {
        let (dir, owner, hit) = setup(json!({"language":"English"}), email, false, true);
        let candidates = |owner: &CreatorRuntime| {
            owner
                .list_discovery_candidates(ListDiscoveryCandidatesInput {
                    campaign_id: CAMPAIGN.into(),
                    after_id: None,
                    limit: 100,
                })
                .unwrap()
        };
        assert!(
            candidates(&owner)
                .iter()
                .all(|c| c.evidence["manual_profile_contact"].is_null())
        );
        owner
            .submit_discovery_review("account", review(&owner, &hit, false))
            .unwrap();
        assert_eq!(
            candidates(&owner)[0].evidence["manual_profile_contact"].as_bool(),
            Some(true)
        );
        let snapshot = owner
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap();
        assert_eq!(snapshot.rows[0].verdict, "qualified");
        let native = snapshot.rows[0]
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::SocialProfile)
            .unwrap();
        assert_eq!(native.normalized_value, PROFILE);
        assert_eq!(native.source_url, PROFILE);
        assert_eq!(native.native_author_id, ACCOUNT);
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
            candidates(&reopened)[0].evidence["manual_profile_contact"].as_bool(),
            Some(true)
        );
        assert_eq!(
            reopened
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .used_calls,
            3
        );
    }
}

#[test]
fn linkedin_manual_contact_rejects_forged_markers_and_stale_revision() {
    let (dir, owner, hit) = setup(json!({"language":"English"}), false, false, true);
    owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap();
    let projected = || {
        owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: CAMPAIGN.into(),
                after_id: None,
                limit: 100,
            })
            .unwrap()[0]
            .evidence["manual_profile_contact"]
            .clone()
    };
    assert_eq!(projected(), true);
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    // Adversarial persisted display evidence is never authority.
    db.execute("UPDATE discovery_candidates SET verdict='pending', evidence_json=json_set(evidence_json,'$.manual_profile_contact',json('true')) WHERE discovery_campaign_id=?1", [CAMPAIGN]).unwrap();
    assert!(projected().is_null());
    db.execute(
        "UPDATE discovery_candidates SET verdict='qualified' WHERE discovery_campaign_id=?1",
        [CAMPAIGN],
    )
    .unwrap();
    assert_eq!(projected(), true);
    // Simulate a new current revision while retaining the previous audited row.
    db.execute(
        "UPDATE discovery_campaigns SET current_revision=2 WHERE id=?1",
        [CAMPAIGN],
    )
    .unwrap();
    assert!(projected().is_null());
}

#[test]
fn linkedin_native_account_score_profile_email_recommendation_and_restart() {
    let (dir, owner, hit) = setup(json!({"language":"English"}), true, false, true);
    assert_eq!(
        owner
            .submit_discovery_review("account", review(&owner, &hit, true))
            .unwrap_err(),
        "initial_match_content_language_unavailable"
    );
    owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    let row = &snapshot.rows[0];
    assert_eq!(row.native_author_id.as_deref(), Some(ACCOUNT));
    assert_ne!(row.native_author_id.as_deref(), Some(HANDLE));
    assert_eq!(row.verdict, "qualified");
    assert_eq!(row.contacts.len(), 2);
    let email = row
        .contacts
        .iter()
        .find(|contact| contact.kind == DiscoveryContactKind::Email)
        .unwrap();
    assert_eq!(email.normalized_value, "studio@example.org");
    assert_eq!(email.source_url, PROFILE);
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

fn language_pending(owner: &CreatorRuntime, hit: &str) {
    let mut input = review(owner, hit, false);
    input.request_id = "linkedin-request-language".into();
    let finding = &mut input.findings[0];
    finding.outcome = DiscoveryReviewOutcome::Insufficient;
    finding.rationale = "Only language requires additional evidence".into();
    finding.citations.clear();
    finding.match_level = None;
    let mut language = finding.clone();
    language.criterion = DiscoveryReviewCriterion::ContentLanguage;
    input.findings.push(language);
    owner.submit_discovery_review("account", input).unwrap();
}

#[tokio::test]
async fn linkedin_same_post_comments_qualify_with_three_votes_and_survive_restart() {
    let (dir, owner, hit) = setup(json!({"language":"日本語"}), true, false, true);
    assert!(
        owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
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
    assert_eq!(claim.author_id, ACCOUNT);
    assert_eq!(claim.author_handle, HANDLE);
    assert_eq!(claim.root_url, POST);
    let sample = DiscoveryLanguageSample {
        author_id: ACCOUNT.into(),
        source_url: POST.into(),
        text: content(HANDLE, false)["text"].as_str().unwrap().into(),
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
            source_url: format!("{POST}#comment-{}", 100 + i),
            text: (*text).into(),
        })
        .collect(),
    };
    assert!(sample.valid_for(&claim));
    let mut wrong = sample.clone();
    wrong.text = "Different Activity body".into();
    assert!(
        owner
            .finish_discovery_language_async(claim.clone(), Some(wrong), None)
            .await
            .is_err()
    );
    let mut wrong = sample.clone();
    wrong.comments[1].source_url =
        "https://www.linkedin.com/feed/update/urn:li:activity:7481027555072962560/#comment-101"
            .into();
    assert!(!wrong.valid_for(&claim));
    let mut wrong = sample.clone();
    wrong.author_id = HANDLE.into();
    assert!(!wrong.valid_for(&claim));
    let mut wrong = sample.clone();
    wrong.comments.push(wrong.comments[1].clone());
    assert!(!wrong.valid_for(&claim));
    let mut wrong = sample.clone();
    wrong.comments[2].source_url = wrong.comments[1].source_url.clone();
    assert!(!wrong.valid_for(&claim));
    owner
        .finish_discovery_language_async(claim.clone(), Some(sample.clone()), None)
        .await
        .unwrap();
    assert!(
        owner
            .finish_discovery_language_async(claim, Some(sample.clone()), None)
            .await
            .is_err()
    );
    assert!(
        owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    let mut input = review(&owner, &hit, false);
    input.request_id = "linkedin-comment-score".into();
    input.findings[0].rationale =
        "Audience comment language matches 日本語; keyboard topic matches original Activity".into();
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
        4
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
async fn linkedin_failed_comment_read_is_terminal_for_source_and_does_not_pause_campaign() {
    let (_dir, owner, hit) = setup(json!({"language":"English"}), true, false, true);
    language_pending(&owner, &hit);
    let claim = owner
        .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
        .await
        .unwrap()
        .unwrap();
    owner
        .finish_discovery_language_async(claim, None, Some("invalid_output".into()))
        .await
        .unwrap();
    assert!(
        owner
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit)
            .await
            .unwrap()
            .is_none()
    );
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
        4
    );
}
#[test]
fn linkedin_body_only_email_preserves_actual_activity_source() {
    let (_dir, owner, hit) = setup(json!({"language":"English"}), false, true, true);
    owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap();
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
        "body@example.org"
    );
    assert_eq!(
        snapshot.rows[0]
            .contacts
            .iter()
            .find(|contact| contact.kind == DiscoveryContactKind::Email)
            .unwrap()
            .source_url,
        POST
    );
}
#[test]
fn linkedin_legacy_official_website_email_is_source_bound_and_stops_further_lookup() {
    let (dir, owner, hit) = setup(json!({"language":"English"}), false, false, true);
    owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap();
    restore_legacy_contactless_candidate(&dir);
    // Library reprojects the native route even before the legacy observation
    // table has been populated; legacy page planning still exercises its guards.
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        1
    );
    let page = owner
        .repository
        .next_discovery_public_page(CAMPAIGN, &[])
        .unwrap()
        .unwrap();
    assert_eq!(page.requested_url, WEBSITE);
    for source in [
        "https://unrelated.example.com/contact",
        "https://www.linkedin.com/in/other/overlay/contact-info/",
    ] {
        let mut wrong = page.clone();
        wrong.requested_url = source.into();
        assert!(owner.claim_discovery_public_page(wrong).is_err());
    }
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            claim,
            document_url: Some(WEBSITE.into()),
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
    assert_eq!(email.source_url, WEBSITE);
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
}
#[test]
fn linkedin_legacy_contact_dialog_email_enters_recommendation_and_survives_restart() {
    let (dir, owner, hit) = setup(json!({"language":"English"}), false, false, true);
    owner
        .submit_discovery_review("account", review(&owner, &hit, false))
        .unwrap();
    restore_legacy_contactless_candidate(&dir);
    let mut page = owner
        .repository
        .next_discovery_public_page(CAMPAIGN, &[])
        .unwrap()
        .unwrap();
    page.requested_url = format!("{PROFILE}overlay/contact-info/");
    let claim = owner.claim_discovery_public_page(page.clone()).unwrap();
    assert_eq!(claim.source_url, PROFILE);
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            claim,
            document_url: Some(page.requested_url.clone()),
            title: "Native Author".into(),
            markdown_excerpt: "dialog@example.org".into(),
            truncated: true,
            stop_code: None,
            hidden_email_available: false,
        })
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(snapshot.rows[0].native_author_id.as_deref(), Some(ACCOUNT));
    let email = snapshot.rows[0]
        .contacts
        .iter()
        .find(|contact| contact.kind == DiscoveryContactKind::Email)
        .unwrap();
    assert_eq!(email.normalized_value, "dialog@example.org");
    assert_eq!(email.source_url, page.requested_url);
    assert!(owner.claim_discovery_public_page(page).is_err());
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
        4
    );
}

#[test]
fn linkedin_unknown_required_followers_cannot_be_overridden_by_email_or_score() {
    let (_dir, owner, hit) = setup(
        json!({"language":"English","min_followers":1000}),
        true,
        false,
        false,
    );
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
fn linkedin_failed_source_continues_to_next_author_without_replay_or_double_charge() {
    for fail_content in [false, true] {
        let (dir, owner) = new_campaign(json!({}));
        let claim = owner
            .claim_discovery_search(CAMPAIGN, 1, "linkedin", "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![search_hit(HANDLE), search_hit("second-author")],
            })
            .unwrap();
        let DiscoveryStep::ReadProfile { search_hit_id: hit } =
            owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
        else {
            panic!("expected profile")
        };
        let saved = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
        if fail_content {
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim: saved.clone(),
                    native_profile: Some(profile(&saved.author_id, false, true)),
                    stop_code: None,
                })
                .unwrap();
            let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
            owner
                .finish_discovery_content(FinishDiscoveryContentInput {
                    claim,
                    native_content: None,
                    stop_code: Some("opencli_invalid_output".into()),
                })
                .unwrap();
            assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
        } else {
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim: saved,
                    native_profile: None,
                    stop_code: Some("opencli_invalid_output".into()),
                })
                .unwrap();
        }
        let expected_calls = if fail_content { 3 } else { 2 };
        let DiscoveryStep::ReadProfile {
            search_hit_id: next,
        } = owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
        else {
            panic!("expected next author")
        };
        assert_ne!(next, hit);
        assert_eq!(
            owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
            expected_calls
        );
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
            expected_calls
        );
    }
}
