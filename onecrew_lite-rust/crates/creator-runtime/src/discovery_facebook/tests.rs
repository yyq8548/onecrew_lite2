use crate::*;
use serde_json::json;

async fn setup(budget: u32) -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_with_contact_tab(budget, false).await
}

async fn setup_with_contact_tab(
    budget: u32,
    contact_tab: bool,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    let profile = "https://www.facebook.com/profile.php?id=123";
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "fb".into(),
            brief: "Hair care".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"category":"不限","language":"西班牙语"}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "facebook".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["hair".into()],
            }],
            target_count: 4,
            max_calls: Some(budget),
        })
        .unwrap();
    owner.approve_discovery_campaign("fb", 1).unwrap();
    let claim = owner
        .claim_discovery_search("fb", 1, "facebook", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: profile.into(),
                title: "Hair care".into(),
                author_label: "Creator".into(),
                author_id: Some("123".into()),
                author_url: Some(profile.into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "fb".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0)
        .id;
    let claim = owner.claim_discovery_profile("fb", 1, &hit).unwrap();
    let mut native = json!({"adapter_version":"1.0.0","id":"123","display_name":"Creator","follower_count":15000,
      "follower_text":"15,000 followers","profile_url":profile,"evidence_source":"visible_numeric_profile_header"});
    if contact_tab {
        native["contact_links"] = json!([format!("{profile}&sk=about_contact_and_basic_info")]);
    }
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim,
            stop_code: None,
            native_profile: Some(native),
        })
        .unwrap();
    (dir, owner, hit)
}

async fn ready(owner: &CreatorRuntime, hit: &str) {
    let claim = owner
        .claim_discovery_facebook_targets_async("fb".into(), 1, hit.into())
        .await
        .unwrap();
    assert!(
        owner
            .claim_discovery_facebook_targets_async("fb".into(), 1, hit.into())
            .await
            .is_err()
    );
    assert!(owner.claim_discovery_content("fb", 1, hit).is_err());
    let urls: Vec<String> = (1..=3)
        .map(|n| format!("https://www.facebook.com/reel/45{n}"))
        .collect();
    for bad in [
        vec![urls[0].clone(), urls[0].clone()],
        vec![claim.source_url.clone()],
        vec![format!("{}?comment_id=6", urls[0])],
    ] {
        assert!(
            owner
                .finish_discovery_facebook_targets_async(claim.clone(), Some(bad), None)
                .await
                .is_err()
        );
    }
    let receipt = owner
        .finish_discovery_facebook_targets_async(claim.clone(), Some(urls.clone()), None)
        .await
        .unwrap();
    assert_eq!(
        owner
            .finish_discovery_facebook_targets_async(claim, Some(urls), None)
            .await
            .unwrap(),
        receipt
    );
    let claim = owner.claim_discovery_content("fb", 1, hit).unwrap();
    assert_eq!(claim.source_url, "https://www.facebook.com/reel/451");
    let mut input = FinishDiscoveryContentInput {
        native_content: Some(
            json!({"post_id":"451","author_id":"123","source_url":claim.source_url,"text":"Original hair care demonstration","text_truncated":false}),
        ),
        claim,
        stop_code: None,
    };
    input.native_content.as_mut().unwrap()["author_id"] = json!("999");
    assert!(owner.finish_discovery_content(input.clone()).is_err());
    input.native_content.as_mut().unwrap()["author_id"] = json!("123");
    owner.finish_discovery_content(input).unwrap();
}

#[tokio::test]
async fn facebook_schedules_original_before_review_and_never_reads_comments_without_ambiguity() {
    let (_dir, owner, hit) = setup(12).await;
    let used = owner.get_discovery_campaign("fb").unwrap().used_calls;
    assert!(matches!(
        owner.plan_discovery_step("fb", 1).unwrap(),
        DiscoveryStep::ReadContentTargets { .. }
    ));
    // The bounded original is reviewed before any comment fallback.
    ready(&owner, &hit).await;
    assert!(!matches!(
        owner.plan_discovery_step("fb", 1).unwrap(),
        DiscoveryStep::ReadContent { .. }
    ));
    review(&owner, &hit, 0, false);
    let after = owner.get_discovery_campaign("fb").unwrap().used_calls;
    assert!(after > used);
    assert!(matches!(
        owner.plan_discovery_step("fb", 1).unwrap(),
        DiscoveryStep::ReadContent { .. }
    ));
    assert_eq!(
        owner.get_discovery_campaign("fb").unwrap().used_calls,
        after
    );
}

fn review(owner: &CreatorRuntime, hit: &str, ordinal: u32, good: bool) {
    let material = owner
        .prepare_discovery_review_material("fb", 1, hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(
        material.hit.source_url,
        "https://www.facebook.com/profile.php?id=123"
    );
    assert_eq!(
        material.native_content.as_ref().unwrap().claim.source_url,
        "https://www.facebook.com/reel/451"
    );
    let finding = DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: if good {
            DiscoveryReviewOutcome::Supported
        } else {
            DiscoveryReviewOutcome::Insufficient
        },
        rationale: if good {
            "粉丝评论语言符合：西班牙语；博主本人语言未核实。"
        } else {
            "All other requirements supported; language alone unresolved."
        }
        .into(),
        citations: if good {
            [
                "Me gusta este producto para el cabello",
                "Gracias por explicar cómo usarlo",
                "Quiero probar este champú",
            ]
            .into_iter()
            .map(|quote| DiscoveryReviewQuote::NativeContent {
                quote: quote.into(),
            })
            .collect()
        } else {
            vec![DiscoveryReviewQuote::NativeContent {
                quote: "Original hair care demonstration".into(),
            }]
        },
        tool: None,
        country_code: None,
        match_level: good.then_some(DiscoveryMatchLevel::Good),
    };
    let mut findings = vec![finding.clone()];
    if !good {
        findings.push(DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::ContentLanguage,
            ..finding
        });
    }
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "fb".into(),
                expected_revision: 1,
                search_hit_id: hit.into(),
                material_hash: material.material_hash,
                request_id: format!("fb-{ordinal}-{good}"),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings,
            },
        )
        .unwrap();
}

#[tokio::test]
async fn facebook_saved_targets_review_one_matched_post_and_library_preserve_provenance() {
    for early_good in [false, true] {
        let (dir, owner, hit) = setup(12).await;
        crate::discovery_contacts::legacy_approval_fixture(dir.path());
        ready(&owner, &hit).await;
        assert_eq!(owner.get_discovery_campaign("fb").unwrap().used_calls, 4);
        review(&owner, &hit, 0, false);
        {
            let ordinal = 1;
            let claim = owner
                .claim_discovery_language_async("fb".into(), 1, hit.clone())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(claim.ordinal, ordinal);
            let source = format!("https://www.facebook.com/reel/45{ordinal}");
            let sample = DiscoveryLanguageSample {
                author_id: "123".into(),
                source_url: source.clone(),
                text: "Original hair care demonstration".into(),
                comments: [
                    "Me gusta este producto para el cabello",
                    "Gracias por explicar cómo usarlo",
                    "Quiero probar este champú",
                ]
                .into_iter()
                .enumerate()
                .map(|(index, text)| DiscoveryLanguageComment {
                    source_url: format!("{source}?comment_id={}", index + 1),
                    text: text.into(),
                })
                .collect(),
            };
            let mut wrong = sample.clone();
            wrong.source_url = "https://www.facebook.com/reel/999".into();
            assert!(!wrong.valid_for(&claim));
            wrong = sample.clone();
            wrong.comments[0].source_url = "https://www.facebook.com/reel/999?comment_id=1".into();
            assert!(!wrong.valid_for(&claim));
            owner
                .finish_discovery_language_async(claim, Some(sample), None)
                .await
                .unwrap();
            assert!(
                owner
                    .claim_discovery_language_async("fb".into(), 1, hit.clone())
                    .await
                    .unwrap()
                    .is_none()
            );
            review(&owner, &hit, ordinal, early_good);
        }
        assert!(
            owner
                .claim_discovery_language_async("fb".into(), 1, hit.clone())
                .await
                .unwrap()
                .is_none()
        );
        let used = owner.get_discovery_campaign("fb").unwrap().used_calls;
        assert_eq!(used, 5);
        let snapshot = owner.prepare_discovery_library_snapshot("fb", 1).unwrap();
        assert_eq!(snapshot.rows[0].followers.as_deref(), Some("15000"));
        assert!(snapshot.rows[0].candidate_id.is_some());
        assert_eq!(
            snapshot.rows[0].verdict,
            if early_good { "qualified" } else { "pending" }
        );
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened
                .prepare_discovery_library_snapshot("fb", 1)
                .unwrap()
                .rows,
            snapshot.rows
        );
        assert_eq!(
            reopened.get_discovery_campaign("fb").unwrap().used_calls,
            used
        );
    }
}

#[tokio::test]
async fn one_matched_post_preserves_pending_without_a_platform_error_or_retry() {
    let (_dir, owner, hit) = setup(8).await;
    ready(&owner, &hit).await;
    review(&owner, &hit, 0, false);
    let first = owner
        .claim_discovery_language_async("fb".into(), 1, hit.clone())
        .await
        .unwrap()
        .unwrap();
    owner
        .finish_discovery_language_async(
            first,
            Some(DiscoveryLanguageSample {
                author_id: "123".into(),
                source_url: "https://www.facebook.com/reel/451".into(),
                text: "Original caption".into(),
                comments: vec![],
            }),
            None,
        )
        .await
        .unwrap();
    review(&owner, &hit, 1, false);
    assert!(
        owner
            .claim_discovery_language_async("fb".into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    let campaign = owner.get_discovery_campaign("fb").unwrap();
    assert_eq!(campaign.used_calls, 5);
    assert_eq!(campaign.platform_runs[0].status, "completed");
    assert!(campaign.platform_runs[0].stop_code.is_none());
    assert!(
        owner
            .claim_discovery_language_async("fb".into(), 1, hit)
            .await
            .unwrap()
            .is_none()
    );
    let rows = owner
        .prepare_discovery_library_snapshot("fb", 1)
        .unwrap()
        .rows;
    assert_eq!(rows[0].verdict, "pending");
    assert!(rows[0].match_rating.is_none());
}

#[tokio::test]
async fn unrelated_search_completion_preserves_inflight_facebook_targets() {
    let (_dir, owner, hit) = setup(8).await;
    let target = owner
        .claim_discovery_facebook_targets_async("fb".into(), 1, hit)
        .await
        .unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "other".into(),
            brief: "Other search".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["hair".into()],
            }],
            target_count: 4,
            max_calls: Some(8),
        })
        .unwrap();
    owner.approve_discovery_campaign("other", 1).unwrap();
    let claim = owner
        .claim_discovery_search("other", 1, "youtube", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![],
        })
        .unwrap();
    let receipt = owner
        .finish_discovery_facebook_targets_async(
            target,
            Some(vec!["https://www.facebook.com/reel/451".into()]),
            None,
        )
        .await
        .unwrap();
    assert_eq!(receipt.status, "completed");
    assert_eq!(owner.get_discovery_campaign("fb").unwrap().used_calls, 3);
}

#[tokio::test]
async fn restart_marks_inflight_facebook_targets_failed_without_retry() {
    let (dir, owner, hit) = setup(8).await;
    owner
        .claim_discovery_facebook_targets_async("fb".into(), 1, hit.clone())
        .await
        .unwrap();
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(reopened.interrupt_discovery_searches().unwrap(), 1);
    assert_eq!(reopened.interrupt_discovery_searches().unwrap(), 0);
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let receipt: (String, String) = db.query_row(
        "SELECT status,stop_code FROM discovery_facebook_content_targets WHERE search_hit_id=?1",
        [&hit], |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!(receipt, ("failed".into(), "interrupted_read".into()));
    assert!(
        reopened
            .claim_discovery_facebook_targets_async("fb".into(), 1, hit.clone())
            .await
            .is_err()
    );
    assert!(reopened.claim_discovery_content("fb", 1, &hit).is_err());
    assert_eq!(reopened.get_discovery_campaign("fb").unwrap().used_calls, 3);
}

#[tokio::test]
async fn facebook_target_failure_or_exhausted_budget_never_claims_caption() {
    let (_dir, owner, hit) = setup(3).await;
    let claim = owner
        .claim_discovery_facebook_targets_async("fb".into(), 1, hit.clone())
        .await
        .unwrap();
    owner
        .finish_discovery_facebook_targets_async(claim, None, Some("opencli_risk_control".into()))
        .await
        .unwrap();
    assert!(owner.claim_discovery_content("fb", 1, &hit).is_err());
    assert_eq!(
        owner.plan_discovery_step("fb", 1).unwrap(),
        DiscoveryStep::BudgetExhausted
    );
}

#[tokio::test]
async fn facebook_native_contact_stops_lookup_and_legacy_contact_tab_survives_reopen() {
    let (dir, owner, hit) = setup_with_contact_tab(12, true).await;
    // Seed historical original-content evidence. Production ordinary Facebook
    // searches must not expand posts or comments to obtain this fixture.
    ready(&owner, &hit).await;
    review(&owner, &hit, 0, false);
    let claim = owner
        .claim_discovery_language_async("fb".into(), 1, hit.clone())
        .await
        .unwrap()
        .unwrap();
    let source = claim.root_url.clone();
    let sample = DiscoveryLanguageSample {
        author_id: "123".into(),
        source_url: source.clone(),
        text: "Original hair care demonstration".into(),
        comments: [
            "Me gusta este producto para el cabello",
            "Gracias por explicar cómo usarlo",
            "Quiero probar este champú",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, text)| DiscoveryLanguageComment {
            source_url: format!("{source}?comment_id={}", i + 1),
            text: text.into(),
        })
        .collect(),
    };
    owner
        .finish_discovery_language_async(claim, Some(sample), None)
        .await
        .unwrap();
    review(&owner, &hit, 1, true);
    assert_eq!(owner.count_discovery_candidates("fb").unwrap().qualified, 1);
    let native_snapshot = owner.prepare_discovery_library_snapshot("fb", 1).unwrap();
    let native = native_snapshot.rows[0]
        .contacts
        .iter()
        .find(|contact| contact.kind == DiscoveryContactKind::SocialProfile)
        .unwrap();
    assert_eq!(native.native_author_id, "123");
    assert_eq!(
        native.normalized_value,
        "https://www.facebook.com/profile.php?id=123"
    );
    assert_eq!(native.source_url, native.normalized_value);
    assert!(
        owner
            .repository
            .next_discovery_public_page("fb", &[])
            .unwrap()
            .is_none()
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("fb", &[])
            .unwrap()
            .is_none()
    );
    // Model a persisted review from before native-account contact observations
    // existed, retaining the audited score, identity, and observed contact tab.
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.execute(
            "DELETE FROM discovery_contact_observations WHERE discovery_campaign_id='fb'
         AND criteria_revision=1 AND native_author_id='123' AND kind='social_profile'
         AND normalized_value=?1 AND source_url=?1
         AND source_excerpt='Verified native author account' AND attribution='author_published'",
            [&native.source_url],
        )
        .unwrap(),
        1
    );
    drop(db);
    // Read-side reprojection keeps the historical author qualified before its
    // native contact observation is persisted.
    assert_eq!(owner.count_discovery_candidates("fb").unwrap().qualified, 1);
    let page = owner
        .repository
        .next_discovery_public_page("fb", &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        page.requested_url,
        "https://www.facebook.com/profile.php?id=123&sk=about_contact_and_basic_info"
    );
    let repeated = page.clone();
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            document_url: Some(claim.input.requested_url.clone()),
            claim,
            title: "Creator contact".into(),
            markdown_excerpt: "Business business@example.com".into(),
            truncated: true,
            stop_code: None,
        })
        .unwrap();
    let snapshot = owner.prepare_discovery_library_snapshot("fb", 1).unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(snapshot.rows[0].contacts.len(), 2);
    let email = snapshot.rows[0]
        .contacts
        .iter()
        .find(|contact| contact.kind == DiscoveryContactKind::Email)
        .unwrap();
    assert_eq!(email.normalized_value, "business@example.com");
    assert_eq!(email.source_url, repeated.requested_url);
    assert!(
        owner
            .repository
            .next_discovery_public_page("fb", &[])
            .unwrap()
            .is_none()
    );
    assert!(owner.claim_discovery_public_page(repeated).is_err());
    let calls = owner.get_discovery_campaign("fb").unwrap().used_calls;
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot("fb", 1)
            .unwrap()
            .rows,
        snapshot.rows
    );
    assert_eq!(
        reopened.get_discovery_campaign("fb").unwrap().used_calls,
        calls
    );
}
