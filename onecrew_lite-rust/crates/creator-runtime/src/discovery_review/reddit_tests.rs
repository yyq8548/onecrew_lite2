use crate::*;
use serde_json::json;

fn setup() -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_with(
        json!({"min_followers":10000,"language":"西班牙语","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
    )
}

fn setup_with(criteria: serde_json::Value) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "reddit-language".into(),
            brief: "Shampoo creators".into(),
            opencli_version: "1.8.8".into(),
            criteria,
            scopes: vec![DiscoveryPlatformScope {
                platform: "reddit".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["shampoo".into()],
            }],
            target_count: 4,
            max_calls: Some(20),
        })
        .unwrap();
    owner
        .approve_discovery_campaign("reddit-language", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("reddit-language", 1, "reddit", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.reddit.com/comments/abc1/".into(),
                title: "Shampoo".into(),
                author_label: "creator_1".into(),
                author_id: Some("t2_user123".into()),
                author_url: Some("https://www.reddit.com/user/creator_1/".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "reddit-language".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0)
        .id;
    let claim = owner
        .claim_discovery_profile("reddit-language", 1, &hit)
        .unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim,stop_code:None,native_profile:Some(json!({
        "adapter_version":"1.0.0","id":"t2_user123","username":"creator_1","biography":"Hair care",
        "follower_count":15000,"profile_url":"https://www.reddit.com/user/creator_1/","evidence_source":"native_profile_response"
    }))}).unwrap();
    let claim = owner
        .claim_discovery_content("reddit-language", 1, &hit)
        .unwrap();
    owner.finish_discovery_content(FinishDiscoveryContentInput {claim,stop_code:None,native_content:Some(json!({
        "post_id":"abc1","author_id":"t2_user123","source_url":"https://www.reddit.com/comments/abc1/",
        "text":"Original shampoo post","text_truncated":false
    }))}).unwrap();
    (dir, owner, hit)
}

#[test]
fn reddit_manual_contact_requires_current_audited_match_and_survives_reopen() {
    let (dir, owner, hit) = setup_with(
        json!({"min_followers":10000,"requires_ai_tool_demo":false,"requires_public_tool_url":false}),
    );
    let candidates = |owner: &CreatorRuntime| {
        owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "reddit-language".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap()
    };
    assert!(candidates(&owner)[0].evidence["manual_profile_contact"].is_null());
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("reddit-language", 1)
            .unwrap()
            .rows[0]
            .verdict,
        "pending",
        "A native profile contact still requires an audited match score"
    );
    let material = owner
        .prepare_discovery_review_material("reddit-language", 1, &hit)
        .unwrap();
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "reddit-language".into(),
                expected_revision: 1,
                search_hit_id: hit,
                material_hash: material.material_hash,
                request_id: "reddit-manual-contact".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "Original shampoo post matches hair care.".into(),
                    citations: vec![DiscoveryReviewQuote::NativeContent {
                        quote: "Original shampoo post".into(),
                    }],
                    tool: None,
                    country_code: None,
                    match_level: Some(DiscoveryMatchLevel::Good),
                }],
            },
        )
        .unwrap();
    assert_eq!(
        candidates(&owner)[0].evidence["manual_profile_contact"],
        true
    );
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("reddit-language", 1)
            .unwrap()
            .rows[0]
            .verdict,
        "qualified",
        "An audited native social profile is a valid contact"
    );
    let snapshot = owner
        .prepare_discovery_library_snapshot("reddit-language", 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].contacts.len(), 1);
    let contact = &snapshot.rows[0].contacts[0];
    assert_eq!(contact.kind, DiscoveryContactKind::SocialProfile);
    assert_eq!(
        contact.normalized_value,
        "https://www.reddit.com/user/creator_1/"
    );
    assert_eq!(contact.source_url, "https://www.reddit.com/user/creator_1/");
    assert_eq!(contact.native_author_id, "t2_user123");
    assert_eq!(
        contact.attribution,
        DiscoveryContactAttribution::AuthorPublished
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("reddit-language", &[])
            .unwrap()
            .is_none()
    );
    let budget = owner
        .get_discovery_campaign("reddit-language")
        .unwrap()
        .used_calls;
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    let reopened = owner
        .prepare_discovery_library_snapshot("reddit-language", 1)
        .unwrap();
    assert_eq!(reopened.rows[0].verdict, "qualified");
    assert_eq!(reopened.rows[0].contacts, snapshot.rows[0].contacts);
    assert_eq!(
        candidates(&owner)[0].evidence["manual_profile_contact"],
        true
    );
    assert_eq!(
        owner
            .get_discovery_campaign("reddit-language")
            .unwrap()
            .used_calls,
        budget
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_candidates SET verdict='pending', evidence_json=json_set(evidence_json,'$.manual_profile_contact',json('true'))", []).unwrap();
    assert!(
        candidates(&owner)[0].evidence["manual_profile_contact"].is_null(),
        "Saved display markers are not authority"
    );
    db.execute("UPDATE discovery_candidates SET verdict='qualified'", [])
        .unwrap();
    assert_eq!(
        candidates(&owner)[0].evidence["manual_profile_contact"],
        true
    );
    db.execute("UPDATE discovery_campaigns SET current_revision=2", [])
        .unwrap();
    assert!(candidates(&owner)[0].evidence["manual_profile_contact"].is_null());
}

#[tokio::test]
async fn reddit_changed_original_requires_fresh_evaluation_before_comments() {
    let (dir, owner, hit) = setup();
    super::language_tests::require_fresh_language_review(&owner, "reddit-language", &hit, 0).await;
    let before = owner
        .get_discovery_campaign("reddit-language")
        .unwrap()
        .used_calls;
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_content_reads SET native_content_json=json_set(native_content_json,'$.text','Updated original shampoo post') WHERE search_hit_id=?1",
        [&hit],
    ).unwrap();
    assert!(
        owner
            .claim_discovery_language_async("reddit-language".into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("reddit-language")
            .unwrap()
            .used_calls,
        before
    );
    let material = owner
        .prepare_discovery_review_material("reddit-language", 1, &hit)
        .unwrap();
    let finding = DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: DiscoveryReviewOutcome::Insufficient,
        rationale: "The updated original leaves only language unresolved".into(),
        citations: vec![DiscoveryReviewQuote::NativeContent {
            quote: "Updated original shampoo post".into(),
        }],
        tool: None,
        country_code: None,
        match_level: None,
    };
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "reddit-language".into(),
                expected_revision: 1,
                search_hit_id: hit.clone(),
                material_hash: material.material_hash,
                request_id: "updated-original-language-review".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![
                    finding.clone(),
                    DiscoveryReviewFinding {
                        criterion: DiscoveryReviewCriterion::ContentLanguage,
                        ..finding
                    },
                ],
            },
        )
        .unwrap();
    let claim = owner
        .claim_discovery_language_async("reddit-language".into(), 1, hit.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.ordinal, 1);
    assert_eq!(claim.root_url, "https://www.reddit.com/comments/abc1/");
    assert_eq!(
        owner
            .get_discovery_campaign("reddit-language")
            .unwrap()
            .used_calls,
        before + 1
    );
    owner
        .finish_discovery_language_async(claim, None, Some("opencli_invalid_output".into()))
        .await
        .unwrap();
    drop(db);
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert!(
        reopened
            .claim_discovery_language_async("reddit-language".into(), 1, hit)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        reopened
            .get_discovery_campaign("reddit-language")
            .unwrap()
            .used_calls,
        before + 1
    );
}

#[tokio::test]
async fn reddit_language_stops_after_good_or_one_matched_post_sample() {
    for early_good in [true, false] {
        let (dir, owner, hit) = setup();
        crate::discovery_contacts::legacy_approval_fixture(dir.path());
        super::language_tests::require_fresh_language_review(&owner, "reddit-language", &hit, 0)
            .await;
        let last = 1;
        for ordinal in 1..=last {
            let claim = owner
                .claim_discovery_language_async("reddit-language".into(), 1, hit.clone())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(claim.author_handle, "creator_1");
            assert_eq!(claim.ordinal, ordinal);
            let source = format!("https://www.reddit.com/comments/abc{ordinal}/");
            let sample = DiscoveryLanguageSample {
                author_id: "t2_user123".into(),
                source_url: source.clone(),
                text: "Original shampoo post".into(),
                comments: [
                    "Me gusta este champú",
                    "Gracias por explicar el cuidado del cabello",
                    "Quiero probar este producto",
                ]
                .into_iter()
                .enumerate()
                .map(|(index, text)| DiscoveryLanguageComment {
                    source_url: format!("{source}_/reply{}/", index + 1),
                    text: text.into(),
                })
                .collect(),
            };
            assert!(sample.valid_for(&claim));
            let mut wrong = sample.clone();
            wrong.comments[0].source_url = "https://www.reddit.com/comments/other/_/reply1/".into();
            assert!(!wrong.valid_for(&claim));
            wrong = sample.clone();
            wrong.author_id = "t2_other".into();
            assert!(!wrong.valid_for(&claim));
            if ordinal > 1 {
                wrong = sample.clone();
                wrong.source_url = claim.root_url.clone();
                assert!(!wrong.valid_for(&claim));
            }
            owner
                .finish_discovery_language_async(claim, Some(sample), None)
                .await
                .unwrap();
            if early_good {
                let material = owner
                    .prepare_discovery_review_material("reddit-language", 1, &hit)
                    .unwrap();
                owner
                    .submit_discovery_review(
                        "account",
                        SubmitDiscoveryReviewInput {
                            campaign_id: "reddit-language".into(),
                            expected_revision: 1,
                            search_hit_id: hit.clone(),
                            material_hash: material.material_hash,
                            request_id: "spanish-audience-good".into(),
                            reviewer: DiscoveryReviewer {
                                kind: DiscoveryReviewerKind::Agent,
                                label: INITIAL_MATCH_REVIEWER.into(),
                            },
                            findings: vec![DiscoveryReviewFinding {
                                criterion: DiscoveryReviewCriterion::MatchFit,
                                outcome: DiscoveryReviewOutcome::Supported,
                                rationale:
                                    "其他要求符合；粉丝评论语言符合：西班牙语；博主本人语言未核实"
                                        .into(),
                                citations: [
                                    "Me gusta este champú",
                                    "Gracias por explicar el cuidado del cabello",
                                    "Quiero probar este producto",
                                ]
                                .into_iter()
                                .map(|quote| DiscoveryReviewQuote::NativeContent {
                                    quote: quote.into(),
                                })
                                .collect(),
                                tool: None,
                                country_code: None,
                                match_level: Some(DiscoveryMatchLevel::Good),
                            }],
                        },
                    )
                    .unwrap();
            } else {
                super::language_tests::require_fresh_language_review(
                    &owner,
                    "reddit-language",
                    &hit,
                    ordinal,
                )
                .await;
            }
        }
        assert!(
            owner
                .claim_discovery_language_async("reddit-language".into(), 1, hit.clone())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            owner
                .get_discovery_campaign("reddit-language")
                .unwrap()
                .used_calls,
            4
        );
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        let material = reopened
            .prepare_discovery_review_material("reddit-language", 1, &hit)
            .unwrap();
        assert_eq!(material.language_samples.len(), last as usize);
        assert_eq!(
            serde_json::to_value(&material.language_samples[0].comments[0])
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            2
        );
        let snapshot = reopened
            .prepare_discovery_library_snapshot("reddit-language", 1)
            .unwrap();
        assert_eq!(
            snapshot.rows[0].verdict,
            if early_good { "qualified" } else { "pending" }
        );
    }
}

#[test]
fn reddit_review_rejects_saved_author_drift() {
    let (dir, owner, hit) = setup();
    assert!(
        owner
            .prepare_discovery_review_material("reddit-language", 1, &hit)
            .is_ok()
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_content_reads SET native_content_json=json_set(native_content_json,'$.author_id','t2_other')",[]).unwrap();
    assert!(
        owner
            .prepare_discovery_review_material("reddit-language", 1, &hit)
            .is_err()
    );
}
