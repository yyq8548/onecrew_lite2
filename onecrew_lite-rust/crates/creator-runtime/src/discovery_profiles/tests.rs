use crate::*;
use serde_json::json;

#[test]
fn pinterest_native_profile_persists_unknown_followers_and_never_recommends_from_contacts_alone() {
    for (followers, language_required) in [
        (None, true),
        (Some(0_u64), true),
        (Some(12000_u64), true),
        (Some(12000_u64), false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        let mut criteria = json!({"min_followers":1000});
        if language_required {
            criteria["language"] = json!("English");
        }
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "pinterest-profile".into(),
                brief: "Keyboard creators".into(),
                opencli_version: "1.8.8".into(),
                criteria,
                target_count: 2,
                max_calls: None,
                scopes: vec![DiscoveryPlatformScope {
                    platform: "pinterest".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["keyboard".into()],
                }],
            })
            .unwrap();
        owner
            .approve_discovery_campaign("pinterest-profile", 1)
            .unwrap();
        let search = owner
            .claim_discovery_search("pinterest-profile", 1, "pinterest", "dedicated")
            .unwrap();
        let hit = DiscoverySearchHitInput {
            source_url: "https://www.pinterest.com/native_author/".into(),
            title: "Display Name".into(),
            author_label: "Display Name".into(),
            author_id: Some("9876543210123456789".into()),
            author_url: Some("https://www.pinterest.com/native_author/".into()),
        };
        let mut wrong = hit.clone();
        wrong.source_url = "https://www.pinterest.com/other/".into();
        assert!(
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim: search.clone(),
                    status: "completed".into(),
                    stop_code: None,
                    search_hits: vec![wrong]
                })
                .is_err()
        );
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: search,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![hit],
            })
            .unwrap();
        let DiscoveryStep::ReadProfile { search_hit_id } =
            owner.plan_discovery_step("pinterest-profile", 1).unwrap()
        else {
            panic!("native profile must be dispatched");
        };
        let claim = owner
            .claim_discovery_profile("pinterest-profile", 1, &search_hit_id)
            .unwrap();
        let native = json!({"author_id":"9876543210123456789","username":"native_author","author_label":"Display Name","profile_url":"https://www.pinterest.com/native_author/",
            "follower_count":followers,"bio":"Original keyboard reviews. Business: studio@example.org","public_links":["https://studio.example.org/contact"]});
        for (key, bad) in [
            ("author_id", json!("Display Name")),
            ("username", json!("other")),
            ("profile_url", json!("https://www.pinterest.com/other/")),
            ("follower_count", json!("12000")),
            ("public_links", json!(["https://127.0.0.1/"])),
            ("unexpected", json!("unobserved")),
        ] {
            let mut value = native.clone();
            value[key] = bad;
            assert!(
                owner
                    .finish_discovery_profile(FinishDiscoveryProfileInput {
                        claim: claim.clone(),
                        native_profile: Some(value),
                        stop_code: None
                    })
                    .is_err()
            );
        }
        owner
            .finish_discovery_profile(FinishDiscoveryProfileInput {
                claim,
                native_profile: Some(native.clone()),
                stop_code: None,
            })
            .unwrap();
        let candidates = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "pinterest-profile".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].platform_account_id, "9876543210123456789");
        assert_eq!(candidates[0].display_name, "Display Name");
        assert_eq!(
            candidates[0].evidence["assessment"]["subscribers_normalized"],
            json!(followers)
        );
        assert_eq!(
            owner
                .count_discovery_candidates("pinterest-profile")
                .unwrap()
                .qualified,
            0
        );
        let material = owner
            .prepare_discovery_review_material("pinterest-profile", 1, &search_hit_id)
            .unwrap();
        assert_eq!(material.format_version, 2);
        assert_eq!(material.native_author_id, "9876543210123456789");
        assert_eq!(material.native_followers, followers);
        assert_eq!(
            material.native_profile.as_ref().unwrap().bio.as_deref(),
            Some("Original keyboard reviews. Business: studio@example.org")
        );
        assert!(
            material.native_content.is_none(),
            "a profile is not an original Pin"
        );
        assert!(material.language_samples.is_empty());
        assert!(
            material.contains_citation(DiscoveryReviewCitation::NativeProfile {
                profile_url: "https://www.pinterest.com/native_author/",
                quote: "Original keyboard reviews"
            })
        );
        assert!(
            !material.contains_citation(DiscoveryReviewCitation::NativeProfile {
                profile_url: "https://www.pinterest.com/other/",
                quote: "Original keyboard reviews"
            })
        );
        assert!(
            !material.contains_citation(DiscoveryReviewCitation::NativeContent {
                quote: "Original keyboard reviews"
            })
        );
        let review = SubmitDiscoveryReviewInput {
            campaign_id: "pinterest-profile".into(), expected_revision: 1, search_hit_id: search_hit_id.clone(),
            material_hash: material.material_hash.clone(), request_id: "profile-score".into(),
            reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: INITIAL_MATCH_REVIEWER.into() },
            findings: vec![DiscoveryReviewFinding { criterion: DiscoveryReviewCriterion::MatchFit, outcome: DiscoveryReviewOutcome::Supported,
                rationale: "Native biography describes keyboard reviews; this is an initial topic assessment.".into(),
                citations: vec![DiscoveryReviewQuote::NativeProfile { profile_url: "https://www.pinterest.com/native_author/".into(), quote: "Original keyboard reviews".into() }],
                tool: None, country_code: None, match_level: Some(DiscoveryMatchLevel::Good) }],
        };
        let mut forged = review.clone();
        forged.request_id = "forged-profile-score".into();
        forged.findings[0].citations = vec![DiscoveryReviewQuote::NativeProfile {
            profile_url: "https://www.pinterest.com/other/".into(),
            quote: "Original keyboard reviews".into(),
        }];
        assert!(
            owner
                .submit_discovery_review("test-account", forged)
                .is_err()
        );
        if language_required || followers.is_none() || followers == Some(0) {
            let error = owner
                .submit_discovery_review("test-account", review)
                .unwrap_err();
            if followers == Some(12000) {
                assert_eq!(error, "initial_match_content_language_unavailable");
            }
            assert_eq!(
                owner
                    .count_discovery_candidates("pinterest-profile")
                    .unwrap()
                    .qualified,
                0
            );
        } else {
            owner
                .submit_discovery_review("test-account", review)
                .unwrap();
            assert_eq!(
                owner
                    .count_discovery_candidates("pinterest-profile")
                    .unwrap()
                    .qualified,
                1
            );
            let library = owner
                .read_discovery_library_material("pinterest-profile", 1)
                .unwrap();
            let row = &library.sources.rows[0];
            assert_eq!(row.native_author_id.as_deref(), Some("9876543210123456789"));
            assert_eq!(row.verdict, "qualified");
            assert_eq!(row.contacts.len(), 1);
            assert_eq!(row.contacts[0].normalized_value, "studio@example.org");
            assert_eq!(
                row.contacts[0].source_url,
                "https://www.pinterest.com/native_author/"
            );
        }
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened
                .get_discovery_profile("pinterest-profile", &search_hit_id)
                .unwrap()
                .native_profile,
            Some(native)
        );
        assert_eq!(
            reopened
                .get_discovery_campaign("pinterest-profile")
                .unwrap()
                .used_calls,
            2
        );
        if !language_required {
            assert_eq!(
                reopened
                    .count_discovery_candidates("pinterest-profile")
                    .unwrap()
                    .qualified,
                1
            );
            let persisted = reopened
                .read_discovery_library_material("pinterest-profile", 1)
                .unwrap();
            assert_eq!(
                persisted.sources.rows[0].contacts[0].normalized_value,
                "studio@example.org"
            );
            assert_eq!(
                persisted.sources.rows[0]
                    .match_rating
                    .as_ref()
                    .unwrap()
                    .level,
                DiscoveryMatchLevel::Good
            );
        }
    }
}

#[test]
fn hackernews_native_profile_keeps_karma_separate_and_never_recommends_from_bio_alone() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "hn-profile".into(),
            brief: "Keyboard creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":1000,"language":"English"}),
            target_count: 2,
            max_calls: None,
            scopes: vec![DiscoveryPlatformScope {
                platform: "hackernews".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["keyboard".into()],
            }],
        })
        .unwrap();
    owner.approve_discovery_campaign("hn-profile", 1).unwrap();
    let search = owner
        .claim_discovery_search("hn-profile", 1, "hackernews", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://news.ycombinator.com/item?id=100".into(),
                title: "Native story".into(),
                author_label: "Observed submitter".into(),
                author_id: Some("native_author".into()),
                author_url: Some("https://news.ycombinator.com/user?id=native_author".into()),
            }],
        })
        .unwrap();
    let DiscoveryStep::ReadProfile { search_hit_id } =
        owner.plan_discovery_step("hn-profile", 1).unwrap()
    else {
        panic!("profile must be dispatched")
    };
    let claim = owner
        .claim_discovery_profile("hn-profile", 1, &search_hit_id)
        .unwrap();
    let native = json!({"author_id":"native_author","username":"native_author","biography_html":"Business: studio@example.org","biography":"Business: studio@example.org","profile_url":"https://news.ycombinator.com/user?id=native_author","created":1,"karma":50000,"follower_count":null});
    for (field, bad) in [
        ("author_id", json!("other_author")),
        ("username", json!("Display Name")),
        ("follower_count", json!(50000)),
        ("unexpected", json!("unobserved")),
    ] {
        let mut value = native.clone();
        value[field] = bad;
        assert!(
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim: claim.clone(),
                    native_profile: Some(value),
                    stop_code: None
                })
                .is_err()
        );
    }
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim,
            native_profile: Some(native.clone()),
            stop_code: None,
        })
        .unwrap();
    let candidates = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "hn-profile".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].platform_account_id, "native_author");
    assert_eq!(candidates[0].display_name, "native_author");
    assert!(candidates[0].evidence["assessment"]["subscribers_normalized"].is_null());
    assert_eq!(
        owner
            .count_discovery_candidates("hn-profile")
            .unwrap()
            .qualified,
        0
    );
    let DiscoveryStep::ReadContent {
        search_hit_id: content_hit,
    } = owner.plan_discovery_step("hn-profile", 1).unwrap()
    else {
        panic!("native item must follow the profile")
    };
    assert_eq!(content_hit, search_hit_id);
    let content_claim = owner
        .claim_discovery_content("hn-profile", 1, &search_hit_id)
        .unwrap();
    let item = json!({"item_id":100,"author_id":"native_author","source_url":"https://news.ycombinator.com/item?id=100","title":"Native story","text_html":"<p>I built a keyboard.</p>","text":"Native story\n\nI built a keyboard.","external_url":"https://example.org/article","comment_ids":[101,102],"comments":[]});
    for (field, bad) in [
        ("author_id", json!("other_author")),
        ("item_id", json!(999)),
        ("source_url", json!("https://example.org/article")),
        ("comment_ids", json!([101, 101])),
    ] {
        let mut value = item.clone();
        value[field] = bad;
        assert!(
            owner
                .finish_discovery_content(FinishDiscoveryContentInput {
                    claim: content_claim.clone(),
                    native_content: Some(value),
                    stop_code: None
                })
                .is_err()
        );
    }
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim: content_claim,
            native_content: Some(item.clone()),
            stop_code: None,
        })
        .unwrap();
    let material = owner
        .prepare_discovery_review_material("hn-profile", 1, &search_hit_id)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(
        material.native_content.as_ref().unwrap().native_content,
        Some(item)
    );
    assert_eq!(
        material.native_profile.as_ref().unwrap().bio.as_deref(),
        Some("Business: studio@example.org")
    );
    assert!(material.native_followers.is_none());
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_profile("hn-profile", &search_hit_id)
            .unwrap()
            .native_profile,
        Some(native)
    );
    assert_eq!(
        reopened
            .get_discovery_campaign("hn-profile")
            .unwrap()
            .used_calls,
        3
    );
}

#[test]
fn facebook_profile_preserves_unknown_count_and_does_not_approve_content() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "facebook-profile".into(),
            brief: "Creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":8000,"category":"shampoo","language":"中文"}),
            target_count: 4,
            max_calls: Some(8),
            scopes: vec![DiscoveryPlatformScope {
                platform: "facebook".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["shampoo".into()],
            }],
        })
        .unwrap();
    owner
        .approve_discovery_campaign("facebook-profile", 1)
        .unwrap();
    let search = owner
        .claim_discovery_search("facebook-profile", 1, "facebook", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.facebook.com/profile.php?id=61590270912364".into(),
                title: "Observed name".into(),
                author_label: "Observed name".into(),
                author_id: Some("61590270912364".into()),
                author_url: Some("https://www.facebook.com/profile.php?id=61590270912364".into()),
            }],
        })
        .unwrap();
    let DiscoveryStep::ReadProfile { search_hit_id } =
        owner.plan_discovery_step("facebook-profile", 1).unwrap()
    else {
        panic!("Facebook profile must be dispatched");
    };
    let claim = owner
        .claim_discovery_profile("facebook-profile", 1, &search_hit_id)
        .unwrap();
    let native = json!({"adapter_version":"1.0.0","id":"61590270912364","display_name":"Hair Dye Fast",
        "follower_count":null,"follower_text":"1.3K followers","profile_url":claim.author_url,"evidence_source":"visible_numeric_profile_header"});
    for (field, bad) in [
        ("id", json!("456")),
        ("follower_count", json!(1300)),
        ("unexpected", json!("private")),
    ] {
        let mut wrong = native.clone();
        wrong[field] = bad;
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
    let receipt = owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim,
            native_profile: Some(native.clone()),
            stop_code: None,
        })
        .unwrap();
    assert_eq!(receipt.native_profile, Some(native));
    let rows = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "facebook-profile".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].platform_account_id, "61590270912364");
    assert_eq!(rows[0].verdict, "pending");
    assert!(rows[0].evidence["assessment"]["subscribers_normalized"].is_null());
    assert!(rows[0].evidence.get("manual_approval").is_none());
    assert_eq!(
        owner
            .get_discovery_campaign("facebook-profile")
            .unwrap()
            .used_calls,
        2
    );
    assert!(
        owner
            .claim_discovery_profile("facebook-profile", 1, &search_hit_id)
            .is_err()
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_profile("facebook-profile", &search_hit_id)
            .unwrap(),
        receipt
    );
}

#[test]
fn reddit_profile_claim_preserves_unknown_zero_and_native_identity() {
    for followers in [json!(null), json!(0), json!(15000)] {
        let dir = tempfile::tempdir().unwrap();
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "profiles".into(),
                brief: "Creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: json!({"min_followers":10000}),
                target_count: 4,
                max_calls: Some(10),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "reddit".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["shampoo".into()],
                }],
            })
            .unwrap();
        crate::discovery_contacts::legacy_approval_fixture(dir.path());
        owner.approve_discovery_campaign("profiles", 1).unwrap();
        let claim = owner
            .claim_discovery_search("profiles", 1, "reddit", "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![DiscoverySearchHitInput {
                    source_url: "https://www.reddit.com/comments/abc123/".into(),
                    title: "Post".into(),
                    author_label: "Search name".into(),
                    author_id: Some("t2_def456".into()),
                    author_url: Some("https://www.reddit.com/user/creator_1/".into()),
                }],
            })
            .unwrap();
        let DiscoveryStep::ReadProfile { search_hit_id } =
            owner.plan_discovery_step("profiles", 1).unwrap()
        else {
            panic!("Reddit must enter profile review");
        };
        let claim = owner
            .claim_discovery_profile("profiles", 1, &search_hit_id)
            .unwrap();
        assert_eq!(claim.call_number, 2);
        let native = json!({"adapter_version":"1.0.0", "id":"t2_def456", "username":"creator_1",
            "profile_url":claim.author_url, "biography":"Public bio", "follower_count":followers,
            "evidence_source":"native_profile_response"});
        let mut wrong = native.clone();
        wrong["id"] = json!("t2_other");
        assert!(
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim: claim.clone(),
                    native_profile: Some(wrong),
                    stop_code: None,
                })
                .is_err()
        );
        owner
            .finish_discovery_profile(FinishDiscoveryProfileInput {
                claim,
                native_profile: Some(native),
                stop_code: None,
            })
            .unwrap();
        let rows = candidates(&owner);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].platform_account_id, "t2_def456");
        assert_eq!(rows[0].display_name, "creator_1");
        assert_eq!(
            rows[0].evidence["assessment"]["subscribers_normalized"],
            followers
        );
        assert_eq!(
            rows[0].verdict,
            if followers == json!(0) {
                "rejected"
            } else {
                "pending"
            }
        );
        assert_eq!(
            owner.get_discovery_campaign("profiles").unwrap().used_calls,
            2
        );
        assert!(
            owner
                .claim_discovery_profile("profiles", 1, &search_hit_id)
                .is_err()
        );
        assert_eq!(
            owner.plan_discovery_step("profiles", 1).unwrap(),
            DiscoveryStep::ReadContent {
                search_hit_id: search_hit_id.clone()
            }
        );
        let content_claim = owner
            .claim_discovery_content("profiles", 1, &search_hit_id)
            .unwrap();
        let content = json!({"post_id":"abc123", "author_id":"t2_def456",
            "source_url":content_claim.source_url, "text":"Original shampoo post", "text_truncated":false});
        for (field, bad) in [
            ("author_id", json!("t2_other")),
            ("post_id", json!("other")),
            ("comments", json!([{"text":"audience comment"}])),
        ] {
            let mut wrong = content.clone();
            wrong[field] = bad;
            assert!(
                owner
                    .finish_discovery_content(FinishDiscoveryContentInput {
                        claim: content_claim.clone(),
                        native_content: Some(wrong),
                        stop_code: None,
                    })
                    .is_err()
            );
        }
        owner
            .finish_discovery_content(FinishDiscoveryContentInput {
                claim: content_claim,
                native_content: Some(content.clone()),
                stop_code: None,
            })
            .unwrap();
        assert_eq!(
            owner
                .get_discovery_content("profiles", &search_hit_id)
                .unwrap()
                .native_content,
            Some(content)
        );
        assert_eq!(
            owner.get_discovery_campaign("profiles").unwrap().used_calls,
            3
        );
        assert!(
            owner
                .claim_discovery_content("profiles", 1, &search_hit_id)
                .is_err()
        );
        let material = owner
            .prepare_discovery_review_material("profiles", 1, &search_hit_id)
            .unwrap();
        assert_eq!(material.format_version, 2);
        assert_eq!(material.native_followers, followers.as_u64());
        assert!(
            material.contains_citation(DiscoveryReviewCitation::NativeContent {
                quote: "Original shampoo post"
            })
        );
        assert!(
            !material.contains_citation(DiscoveryReviewCitation::NativeContent {
                quote: "audience comment"
            })
        );
        let level = match followers.as_u64() {
            Some(0) => Some(DiscoveryMatchLevel::Low),
            Some(_) => Some(DiscoveryMatchLevel::Good),
            None => None,
        };
        owner
            .submit_discovery_review(
                "account",
                SubmitDiscoveryReviewInput {
                    campaign_id: "profiles".into(),
                    expected_revision: 1,
                    search_hit_id: search_hit_id.clone(),
                    material_hash: material.material_hash,
                    request_id: "reddit-initial".into(),
                    reviewer: DiscoveryReviewer {
                        kind: DiscoveryReviewerKind::Agent,
                        label: INITIAL_MATCH_REVIEWER.into(),
                    },
                    findings: vec![DiscoveryReviewFinding {
                        criterion: DiscoveryReviewCriterion::MatchFit,
                        outcome: if level.is_some() {
                            DiscoveryReviewOutcome::Supported
                        } else {
                            DiscoveryReviewOutcome::Insufficient
                        },
                        rationale:
                            "Fixture judgment over saved original and observed follower requirement"
                                .into(),
                        citations: vec![DiscoveryReviewQuote::NativeContent {
                            quote: "Original shampoo post".into(),
                        }],
                        tool: None,
                        country_code: None,
                        match_level: level,
                    }],
                },
            )
            .unwrap();
        let expected = match followers.as_u64() {
            Some(0) => "rejected",
            Some(_) => "qualified",
            None => "pending",
        };
        assert_eq!(candidates(&owner)[0].verdict, expected);
        // Reload revalidates the saved review rather than trusting unaudited proof fields.
        owner
            .prepare_discovery_review_material("profiles", 1, &search_hit_id)
            .unwrap();
        let snapshot = owner
            .prepare_discovery_library_snapshot("profiles", 1)
            .unwrap();
        assert_eq!(
            snapshot.rows[0].native_author_id.as_deref(),
            Some("t2_def456")
        );
        assert_eq!(snapshot.rows[0].verdict, expected);
    }
}

#[test]
fn library_snapshot_uses_only_exact_native_profile_receipts_not_namesakes() {
    let (dir, owner, hits) = setup(10, false);
    let hit = hits.iter().find(|hit| hit.author_id.is_some()).unwrap();
    let claim = owner
        .claim_discovery_profile("profiles", 1, &hit.id)
        .unwrap();
    owner.finish_discovery_profile(success(claim)).unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot("profiles", 1)
        .unwrap();
    assert_eq!(snapshot.rows.len(), 3);
    let bound = snapshot
        .rows
        .iter()
        .find(|row| row.hit.id == hit.id)
        .unwrap();
    assert_eq!(bound.native_author_id.as_deref(), Some("123"));
    assert_eq!(bound.verdict, "pending");
    assert_eq!(
        snapshot
            .rows
            .iter()
            .filter(|row| row.verdict == "unverified")
            .count(),
        2
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_profile_reads SET native_profile_json=json_set(native_profile_json,'$.mid','456')", []).unwrap();
    assert!(
        owner
            .prepare_discovery_library_snapshot("profiles", 1)
            .unwrap()
            .rows
            .iter()
            .all(|row| row.verdict == "unverified")
    );
}

fn setup(
    max_calls: u32,
    extra_scope: bool,
) -> (tempfile::TempDir, CreatorRuntime, Vec<DiscoverySearchHit>) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    let mut scopes = vec![DiscoveryPlatformScope {
        platform: "bilibili".into(),
        profile_ref: "dedicated".into(),
        keywords: vec!["AI tools".into()],
    }];
    if extra_scope {
        scopes.push(DiscoveryPlatformScope {
            platform: "tiktok".into(),
            profile_ref: "dedicated".into(),
            keywords: vec!["AI tools".into()],
        });
    }
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id:"profiles".into(), brief:"AI tool creators".into(),opencli_version:"1.8.8".into(),
        criteria:json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
        scopes,target_count:30,max_calls: Some(max_calls),
    }).unwrap();
    assert!(
        owner
            .claim_discovery_profile("profiles", 1, "missing")
            .is_err()
    );
    owner.approve_discovery_campaign("profiles", 1).unwrap();
    let claim = owner
        .claim_discovery_search("profiles", 1, "bilibili", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: [
                ("BV1RYqHYnEw8", Some("123")),
                ("BV1RYqHYnEw9", Some("123")),
                ("BV1RYqHYnEw0", None),
            ]
            .into_iter()
            .map(|(video, id)| DiscoverySearchHitInput {
                source_url: format!("https://www.bilibili.com/video/{video}"),
                title: "AI demo".into(),
                author_label: "Same name".into(),
                author_id: id.map(str::to_owned),
                author_url: id.map(|id| format!("https://space.bilibili.com/{id}")),
            })
            .collect(),
        })
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "profiles".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    (dir, owner, hits)
}

fn success(claim: DiscoveryProfileClaim) -> FinishDiscoveryProfileInput {
    FinishDiscoveryProfileInput {
        native_profile: Some(
            json!({"mid":claim.author_id,"profile_url":claim.author_url,"name":"Creator","follower_count":15000}),
        ),
        claim,
        stop_code: None,
    }
}

#[tokio::test]
async fn failed_author_does_not_block_the_next_author_on_the_same_platform() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "authors-continue".into(),
            brief: "Keyboard creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({}),
            target_count: 2,
            max_calls: None,
            scopes: vec![DiscoveryPlatformScope {
                platform: "bilibili".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["keyboard".into()],
            }],
        })
        .unwrap();
    owner
        .approve_discovery_campaign("authors-continue", 1)
        .unwrap();
    let search = owner
        .claim_discovery_search("authors-continue", 1, "bilibili", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: [("BV1RYqHYnEw8", "123"), ("BV1RYqHYnEw9", "456")]
                .into_iter()
                .map(|(video, id)| DiscoverySearchHitInput {
                    source_url: format!("https://www.bilibili.com/video/{video}"),
                    title: "Keyboard review".into(),
                    author_label: format!("Author {id}"),
                    author_id: Some(id.into()),
                    author_url: Some(format!("https://space.bilibili.com/{id}")),
                })
                .collect(),
        })
        .unwrap();
    let DiscoveryStep::ReadProfile {
        search_hit_id: first,
    } = owner.plan_discovery_step("authors-continue", 1).unwrap()
    else {
        panic!("first author");
    };
    let claim = owner
        .claim_discovery_profile("authors-continue", 1, &first)
        .unwrap();
    let mut invalid = success(claim);
    invalid.native_profile.as_mut().unwrap()["mid"] = json!("wrong-author");
    owner.finish_discovery_profile_async(invalid).await.unwrap();
    let DiscoveryStep::ReadProfile {
        search_hit_id: second,
    } = owner.plan_discovery_step("authors-continue", 1).unwrap()
    else {
        panic!("next author must continue");
    };
    assert_ne!(first, second);
    let second_claim = owner
        .claim_discovery_profile("authors-continue", 1, &second)
        .unwrap();
    owner
        .finish_discovery_profile_async(success(second_claim))
        .await
        .unwrap();
    assert_eq!(
        owner
            .get_discovery_campaign("authors-continue")
            .unwrap()
            .used_calls,
        3
    );
    assert_eq!(
        owner
            .count_discovery_candidates("authors-continue")
            .unwrap()
            .qualified,
        0
    );
    assert_eq!(
        owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "authors-continue".into(),
                after_id: None,
                limit: 10
            })
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn invalid_profile_settles_once_and_continues_an_independent_source_after_restart() {
    let (dir, owner, hits) = setup(10, true);
    let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
    let claim = owner
        .claim_discovery_profile("profiles", 1, &hit.id)
        .unwrap();
    let mut invalid = success(claim.clone());
    invalid.native_profile.as_mut().unwrap()["mid"] = json!("wrong-author");
    let read = owner
        .finish_discovery_profile_async(invalid)
        .await
        .expect("invalid provider evidence must settle its reservation and continue");
    assert_eq!(read.status, "failed");
    assert_eq!(read.stop_code.as_deref(), Some("opencli_invalid_output"));
    assert!(candidates(&owner).is_empty());
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.get_discovery_profile("profiles", &hit.id).unwrap(),
        read
    );
    assert_eq!(
        reopened
            .get_discovery_campaign("profiles")
            .unwrap()
            .used_calls,
        2
    );
    assert!(
        reopened
            .claim_discovery_profile("profiles", 1, &hit.id)
            .is_err()
    );
    assert!(
        matches!(reopened.plan_discovery_step("profiles", 1).unwrap(), DiscoveryStep::Search { platform, .. } if platform == "tiktok")
    );
    assert!(
        reopened
            .finish_discovery_profile_async(success(claim))
            .await
            .is_err()
    );
}

#[test]
fn profile_receipts_charge_once_deduplicate_authors_and_never_qualify() {
    let (dir, owner, hits) = setup(10, false);
    let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
    assert!(matches!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::ReadProfile { .. }
    ));
    for (campaign, revision, id) in [
        ("other", 1, hit.id.as_str()),
        ("profiles", 2, hit.id.as_str()),
        ("profiles", 1, "missing"),
    ] {
        assert!(
            owner
                .claim_discovery_profile(campaign, revision, id)
                .is_err()
        );
    }
    assert!(
        owner
            .claim_discovery_profile(
                "profiles",
                1,
                &hits.iter().find(|h| h.author_id.is_none()).unwrap().id
            )
            .is_err()
    );
    assert_eq!(
        owner.get_discovery_campaign("profiles").unwrap().used_calls,
        1
    );
    let claim = owner
        .claim_discovery_profile("profiles", 1, &hit.id)
        .unwrap();
    assert_eq!(claim.call_number, 2);
    assert_eq!(
        owner
            .get_discovery_profile("profiles", &hit.id)
            .unwrap()
            .status,
        "reading"
    );
    assert_eq!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::Busy
    );
    assert!(
        owner
            .claim_discovery_profile("profiles", 1, &hit.id)
            .is_err()
    );
    let receipt = owner
        .finish_discovery_profile(success(claim.clone()))
        .unwrap();
    assert_eq!(receipt.status, "completed");
    assert_eq!(
        owner.finish_discovery_profile(success(claim)).unwrap(),
        receipt
    );
    for same_author in hits.iter().filter(|h| h.author_id.is_some()) {
        assert!(
            owner
                .claim_discovery_profile("profiles", 1, &same_author.id)
                .is_err()
        );
    }
    assert!(matches!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::ReadContent { .. }
    ));
    let candidates = candidates(&owner);
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.platform, "bilibili");
    assert_eq!(candidate.platform_account_id, "123");
    assert_eq!(candidate.display_name, "Creator"); // Not the search card's author label.
    assert_eq!(candidate.profile_url, hit.author_url.as_deref().unwrap());
    assert_eq!(candidate.source_profile_ref, "dedicated");
    assert_eq!(candidate.verdict, "pending");
    assert_eq!(
        candidate.evidence["source_urls"],
        json!([hit.source_url, hit.author_url])
    );
    assert_eq!(
        candidate.evidence["subscribers_source_url"],
        hit.author_url.as_deref().unwrap()
    );
    assert_eq!(candidate.evidence["captured_at"], receipt.captured_at);
    assert_eq!(
        candidate.evidence["assessment"]["subscribers_normalized"],
        15000
    );
    assert_eq!(
        candidate.evidence["assessment"]["reasons"],
        json!(["own_ai_tool_demo_unverified", "public_tool_url_unverified"])
    );
    assert_eq!(
        owner
            .count_discovery_candidates("profiles")
            .unwrap()
            .qualified,
        0
    );
    assert_eq!(
        owner.get_discovery_campaign("profiles").unwrap().used_calls,
        2
    );
    assert!(owner.get_discovery_profile("other", &hit.id).is_err());
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.get_discovery_profile("profiles", &hit.id).unwrap(),
        receipt
    );
    assert_eq!(self::candidates(&reopened), candidates);
}

#[test]
fn profile_recovery_recharges_and_rejects_old_or_forged_receipts() {
    let (dir, owner, hits) = setup(10, false);
    let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
    let old = owner
        .claim_discovery_profile("profiles", 1, &hit.id)
        .unwrap();
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    assert!(
        owner
            .finish_discovery_profile(success(old.clone()))
            .is_err()
    );
    assert_eq!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::ReadProfile {
            search_hit_id: hit.id.clone()
        }
    );
    let new = owner
        .claim_discovery_profile("profiles", 1, &hit.id)
        .unwrap();
    assert_eq!(new.call_number, 3);
    assert!(owner.finish_discovery_profile(success(old)).is_err());
    let mut forged = new.clone();
    forged.author_id = "456".into();
    assert!(owner.finish_discovery_profile(success(forged)).is_err());
    let mut invalid = success(new.clone());
    invalid.native_profile.as_mut().unwrap()["follower_count"] = json!(-1);
    assert!(owner.finish_discovery_profile(invalid).is_err());
    owner.finish_discovery_profile(success(new)).unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
    assert_eq!(
        owner.get_discovery_campaign("profiles").unwrap().used_calls,
        3
    );
}

#[test]
fn profile_reads_require_idle_approved_source_and_budget_before_reservation() {
    let (_dir, owner, hits) = setup(10, true);
    let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
    // Another unsearched platform cannot block this observed author.
    let claim = owner
        .claim_discovery_profile("profiles", 1, &hit.id)
        .unwrap();
    assert_eq!(claim.call_number, 2);
    assert_eq!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::Busy
    );
    owner.finish_discovery_profile(success(claim)).unwrap();
    let search = owner
        .claim_discovery_search("profiles", 1, "tiktok", "dedicated")
        .unwrap();
    assert!(
        owner
            .claim_discovery_content("profiles", 1, &hit.id)
            .is_err()
    );
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "unauthorized".into(),
            stop_code: Some("opencli_auth_required".into()),
            search_hits: vec![],
        })
        .unwrap();
    assert!(matches!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::ReadContent { .. }
    ));
    let (_dir, owner, hits) = setup(1, false);
    let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
    assert!(
        owner
            .claim_discovery_profile("profiles", 1, &hit.id)
            .is_err()
    );
    assert!(owner.get_discovery_profile("profiles", &hit.id).is_err());
    assert_eq!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::BudgetExhausted
    );
}

#[test]
fn profile_failures_are_durable_and_never_implicitly_retried() {
    for (code, status) in [
        ("opencli_auth_required", "unauthorized"),
        ("opencli_risk_control", "risk_control_stop"),
        ("opencli_invalid_output", "completed"),
    ] {
        let (_dir, owner, hits) = setup(10, false);
        let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
        let claim = owner
            .claim_discovery_profile("profiles", 1, &hit.id)
            .unwrap();
        let receipt = owner
            .finish_discovery_profile(FinishDiscoveryProfileInput {
                claim,
                native_profile: None,
                stop_code: Some(code.into()),
            })
            .unwrap();
        assert_eq!(receipt.status, "failed");
        assert_eq!(
            owner
                .get_discovery_campaign("profiles")
                .unwrap()
                .platform_runs[0]
                .status,
            status
        );
        assert_eq!(
            owner.plan_discovery_step("profiles", 1).unwrap(),
            DiscoveryStep::SearchExhausted
        );
        assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
        assert!(
            owner
                .claim_discovery_profile("profiles", 1, &hit.id)
                .is_err()
        );
        assert_eq!(
            owner
                .count_discovery_candidates("profiles")
                .unwrap()
                .qualified,
            0
        );
        assert!(candidates(&owner).is_empty());
    }
}

fn candidates(owner: &CreatorRuntime) -> Vec<DiscoveryCandidate> {
    owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "profiles".into(),
            after_id: None,
            limit: 100,
        })
        .unwrap()
}

#[test]
fn profile_candidate_and_receipt_roll_back_together() {
    let (dir, owner, hits) = setup(10, false);
    let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
    let claim = owner
        .claim_discovery_profile("profiles", 1, &hit.id)
        .unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_profile_candidate BEFORE INSERT ON discovery_candidates
        BEGIN SELECT RAISE(ABORT,'injected candidate failure'); END;",
        )
        .unwrap();
    assert!(
        owner
            .finish_discovery_profile(success(claim.clone()))
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_profile("profiles", &hit.id)
            .unwrap()
            .status,
        "reading"
    );
    assert_eq!(
        owner
            .get_discovery_campaign("profiles")
            .unwrap()
            .platform_runs[0]
            .status,
        "searching"
    );
    assert_eq!(
        owner.get_discovery_campaign("profiles").unwrap().used_calls,
        2
    );
    assert!(candidates(&owner).is_empty());
    connection
        .execute_batch("DROP TRIGGER reject_profile_candidate")
        .unwrap();
    owner.finish_discovery_profile(success(claim)).unwrap();
    assert_eq!(candidates(&owner).len(), 1);
}

#[test]
fn profile_candidates_keep_platform_identity_and_do_not_merge_same_names() {
    let (_dir, owner, _) = setup(10, true);
    let claim = owner
        .claim_discovery_search("profiles", 1, "tiktok", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: [("123", "first"), ("456", "second")]
                .into_iter()
                .map(|(id, username)| DiscoverySearchHitInput {
                    source_url: format!(
                        "https://www.tiktok.com/@{username}/video/7123456789012345678"
                    ),
                    title: "AI tool demonstration".into(),
                    author_label: "Same name".into(),
                    author_id: Some(id.into()),
                    author_url: Some(format!("https://www.tiktok.com/@{username}")),
                })
                .collect(),
        })
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "profiles".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    for (platform, author_id) in [("bilibili", "123"), ("tiktok", "123"), ("tiktok", "456")] {
        let search_hit_id = &hits
            .iter()
            .find(|hit| hit.platform == platform && hit.author_id.as_deref() == Some(author_id))
            .expect("every distinct native identity has a saved source")
            .id;
        let claim = owner
            .claim_discovery_profile("profiles", 1, search_hit_id)
            .unwrap();
        let native = if claim.platform == "tiktok" {
            json!({"id":claim.author_id,"profile_url":claim.author_url,"display_name":"Creator","follower_count":15000})
        } else {
            success(claim.clone()).native_profile.unwrap()
        };
        owner
            .finish_discovery_profile(FinishDiscoveryProfileInput {
                claim,
                native_profile: Some(native),
                stop_code: None,
            })
            .unwrap();
    }
    let rows = candidates(&owner);
    assert_eq!(rows.len(), 3);
    let snapshot = owner
        .prepare_discovery_library_snapshot("profiles", 1)
        .unwrap();
    let bound: Vec<_> = snapshot
        .rows
        .iter()
        .filter(|row| row.candidate_id.is_some())
        .collect();
    assert_eq!(bound.len(), 3);
    for row in bound {
        let candidate = rows
            .iter()
            .find(|candidate| Some(&candidate.id) == row.candidate_id.as_ref())
            .unwrap();
        assert_eq!(row.hit.platform, candidate.platform);
        assert_eq!(
            row.native_author_id.as_deref(),
            Some(candidate.platform_account_id.as_str())
        );
        assert_eq!(row.verdict, "pending");
    }
    assert!(
        rows.iter()
            .all(|row| row.display_name == "Creator" && row.verdict == "pending")
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.platform_account_id == "123")
            .count(),
        2
    );
    assert_eq!(
        owner.count_discovery_candidates("profiles").unwrap(),
        DiscoveryCandidateCounts {
            public_page_errors: 0,
            qualified: 0,
            pending: 3,
            rejected: 0
        }
    );
    assert!(matches!(
        owner.plan_discovery_step("profiles", 1).unwrap(),
        DiscoveryStep::ReadContent { .. }
    ));
    assert_eq!(
        owner.get_discovery_campaign("profiles").unwrap().used_calls,
        5
    );
}

#[test]
fn profile_without_observed_name_keeps_receipt_without_inventing_candidate() {
    for name in [json!(null), json!("  "), json!("bad\u{0000}name")] {
        let (_dir, owner, hits) = setup(10, false);
        let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
        let claim = owner
            .claim_discovery_profile("profiles", 1, &hit.id)
            .unwrap();
        let mut input = success(claim);
        input.native_profile.as_mut().unwrap()["name"] = name;
        assert_eq!(
            owner.finish_discovery_profile(input).unwrap().status,
            "completed"
        );
        assert!(candidates(&owner).is_empty());
    }
}

#[test]
fn profile_candidate_uses_exact_zero_and_rejects_below_frozen_threshold() {
    for followers in [0, 9999, 10000] {
        let (_dir, owner, hits) = setup(10, false);
        let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
        let claim = owner
            .claim_discovery_profile("profiles", 1, &hit.id)
            .unwrap();
        let mut input = success(claim);
        input.native_profile.as_mut().unwrap()["follower_count"] = json!(followers);
        owner.finish_discovery_profile(input).unwrap();
        let rows = candidates(&owner);
        assert_eq!(rows[0].evidence["subscribers_raw"], followers.to_string());
        assert_eq!(
            rows[0].evidence["assessment"]["subscribers_normalized"],
            followers
        );
        assert_eq!(
            rows[0].verdict,
            if followers < 10000 {
                "rejected"
            } else {
                "pending"
            }
        );
    }
}

#[test]
fn profile_receipt_never_overwrites_an_existing_candidate_or_decision() {
    for verdict in ["pending", "qualified", "rejected"] {
        let (dir, owner, hits) = setup(10, false);
        let hit = hits.iter().find(|h| h.author_id.is_some()).unwrap();
        let claim = owner
            .claim_discovery_profile("profiles", 1, &hit.id)
            .unwrap();
        let connection = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        connection.execute("INSERT INTO discovery_candidates VALUES(
            'existing','profiles',1,'bilibili','dedicated','123','Reviewed creator',
            'https://space.bilibili.com/123',?1,'{\"source_urls\":[\"https://space.bilibili.com/123\"]}',1,1,'bilibili')", [verdict]).unwrap();
        let before = candidates(&owner);
        owner.finish_discovery_profile(success(claim)).unwrap();
        assert_eq!(candidates(&owner), before);
        assert_eq!(
            owner
                .get_discovery_profile("profiles", &hit.id)
                .unwrap()
                .status,
            "completed"
        );
        assert_eq!(
            owner.get_discovery_campaign("profiles").unwrap().used_calls,
            2
        );
    }
}

#[test]
fn profile_evidence_rejects_wrong_identity_unbounded_or_ambiguous_outcomes() {
    let claim = DiscoveryProfileClaim {
        campaign_id: "test".into(),
        expected_revision: 1,
        search_hit_id: "hit".into(),
        platform: "tiktok".into(),
        profile_ref: "dedicated".into(),
        author_id: "123".into(),
        author_url: "https://www.tiktok.com/@creator".into(),
        source_url: "https://www.tiktok.com/@creator/video/456".into(),
        call_number: 1,
    };
    let native = json!({"id":"123","profile_url":claim.author_url,"follower_count":0});
    let valid = FinishDiscoveryProfileInput {
        claim,
        native_profile: Some(native),
        stop_code: None,
    };
    assert!(super::validate_profile_result(&valid).is_ok()); // An observed zero is not missing.
    for (field, value) in [
        ("id", json!("456")),
        ("profile_url", json!("https://evil.example")),
        ("follower_count", json!(null)),
        ("signature", json!("x".repeat(16001))),
    ] {
        let mut invalid = valid.clone();
        invalid.native_profile.as_mut().unwrap()[field] = value;
        assert!(super::validate_profile_result(&invalid).is_err());
    }
    let mut invalid = valid.clone();
    invalid.stop_code = Some("opencli_auth_required".into());
    assert!(super::validate_profile_result(&invalid).is_err());
    invalid.native_profile = None;
    invalid.stop_code = Some("raw-provider-error-with-secret".into());
    assert!(super::validate_profile_result(&invalid).is_err());
    invalid.stop_code = None;
    assert!(super::validate_profile_result(&invalid).is_err());
}

#[test]
fn x_native_profile_matches_post_identity_but_still_requires_content_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "x-profile".into(),
            brief: "Verify X author evidence".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "twitter".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 1,
            max_calls: Some(3),
        })
        .unwrap();
    owner.approve_discovery_campaign("x-profile", 1).unwrap();
    let search = owner
        .claim_discovery_search("x-profile", 1, "twitter", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://x.com/i/status/1234567890123456789".into(),
                title: "AI tool demo".into(),
                author_label: "Creator_1".into(),
                author_id: Some("creator_1".into()),
                author_url: Some("https://x.com/creator_1".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "x-profile".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        owner.plan_discovery_step("x-profile", 1).unwrap(),
        DiscoveryStep::ReadIdentity {
            search_hit_id: hit.id.clone()
        }
    );
    assert!(
        owner
            .claim_discovery_profile("x-profile", 1, &hit.id)
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-profile")
            .unwrap()
            .used_calls,
        1
    );
    let identity = owner
        .claim_discovery_identity("x-profile", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_identity(FinishDiscoveryIdentityInput {
            claim: identity,
            account_id: Some("987654321".into()),
            observed_source_url: Some("https://x.com/creator_1/status/1234567890123456789".into()),
            stop_code: None,
        })
        .unwrap();
    let claim = owner
        .claim_discovery_profile("x-profile", 1, &hit.id)
        .unwrap();
    let native = json!({"handle":"creator_1","profile_url":"https://x.com/creator_1",
        "name":"Observed Creator","follower_count":15000,"identity_kind":"native_id","account_id":"987654321",
        "self_declared_location":"California"});
    let mut forged = FinishDiscoveryProfileInput {
        claim: claim.clone(),
        native_profile: Some(native.clone()),
        stop_code: None,
    };
    forged.native_profile.as_mut().unwrap()["handle"] = json!("another");
    assert!(owner.finish_discovery_profile(forged).is_err());
    let mut wrong_id = FinishDiscoveryProfileInput {
        claim: claim.clone(),
        native_profile: Some(native.clone()),
        stop_code: None,
    };
    wrong_id.native_profile.as_mut().unwrap()["account_id"] = json!("111");
    assert!(owner.finish_discovery_profile(wrong_id).is_err());
    assert_eq!(
        owner
            .get_discovery_profile("x-profile", &hit.id)
            .unwrap()
            .status,
        "reading"
    );
    let receipt = owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim,
            native_profile: Some(native),
            stop_code: None,
        })
        .unwrap();
    let rows = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "x-profile".into(),
            after_id: None,
            limit: 100,
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].platform_account_id, "987654321");
    assert_eq!(rows[0].verdict, "pending");
    assert_eq!(rows[0].evidence["author_identity_kind"], "native_id");
    assert!(
        !rows[0].evidence["assessment"]["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("account_id_unverified"))
    );
    assert_eq!(
        owner
            .count_discovery_candidates("x-profile")
            .unwrap()
            .qualified,
        0
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-profile")
            .unwrap()
            .used_calls,
        3
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_profile("x-profile", &hit.id)
            .unwrap(),
        receipt
    );
    assert_eq!(
        reopened
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "x-profile".into(),
                after_id: None,
                limit: 100,
            })
            .unwrap(),
        rows
    );
}

#[test]
fn author_content_continues_before_unread_youtube_and_another_platform() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id: "author-chain".into(), brief: "Mouse creators".into(),
        opencli_version: "1.8.8".into(),
        criteria: json!({"min_followers":8000,"requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes: ["youtube", "bilibili", "tiktok"].into_iter().map(|platform| DiscoveryPlatformScope {
            platform: platform.into(), profile_ref: "dedicated".into(), keywords: vec!["mouse review".into()],
        }).collect(), target_count: 5, max_calls: None,
    }).unwrap();
    owner.approve_discovery_campaign("author-chain", 1).unwrap();
    for platform in ["youtube", "bilibili"] {
        let claim = owner
            .claim_discovery_search("author-chain", 1, platform, "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![DiscoverySearchHitInput {
                    source_url: if platform == "youtube" {
                        "https://www.youtube.com/watch?v=LR04bU_yV5k"
                    } else {
                        "https://www.bilibili.com/video/BV1RYqHYnEw8"
                    }
                    .into(),
                    title: "Mouse review".into(),
                    author_label: "Creator".into(),
                    author_id: (platform == "bilibili").then(|| "123".into()),
                    author_url: (platform == "bilibili")
                        .then(|| "https://space.bilibili.com/123".into()),
                }],
            })
            .unwrap();
    }
    let DiscoveryStep::ReadProfile { search_hit_id } =
        owner.plan_discovery_step("author-chain", 1).unwrap()
    else {
        panic!("native author must not wait for the YouTube pool or unsearched TikTok");
    };
    let claim = owner
        .claim_discovery_profile("author-chain", 1, &search_hit_id)
        .unwrap();
    owner.finish_discovery_profile(success(claim)).unwrap();
    assert_eq!(
        owner.plan_discovery_step("author-chain", 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: search_hit_id.clone()
        }
    );
    let claim = owner
        .claim_discovery_content("author-chain", 1, &search_hit_id)
        .unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: None,
            stop_code: Some("opencli_risk_control".into()),
        })
        .unwrap();
    // A blocked author never prevents work on another approved source.
    assert!(matches!(
        owner.plan_discovery_step("author-chain", 1).unwrap(),
        DiscoveryStep::ReadVideo { .. }
    ));
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert!(matches!(
        reopened.plan_discovery_step("author-chain", 1).unwrap(),
        DiscoveryStep::ReadVideo { .. }
    ));
}

#[test]
fn pinterest_native_website_only_contact_is_bound_and_stops_further_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "pin-contact".into(),
            brief: "Keyboard creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":1000}),
            target_count: 2,
            max_calls: Some(10),
            scopes: vec![DiscoveryPlatformScope {
                platform: "pinterest".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["keyboard".into()],
            }],
        })
        .unwrap();
    owner.approve_discovery_campaign("pin-contact", 1).unwrap();
    let claim = owner
        .claim_discovery_search("pin-contact", 1, "pinterest", "dedicated")
        .unwrap();
    let source = "https://www.pinterest.com/native_author/";
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: source.into(),
                title: "Creator".into(),
                author_label: "Creator".into(),
                author_id: Some("123".into()),
                author_url: Some(source.into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "pin-contact".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let claim = owner
        .claim_discovery_profile("pin-contact", 1, &hit.id)
        .unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim,stop_code:None,native_profile:Some(json!({"author_id":"123","username":"native_author",
        "author_label":"Creator","profile_url":source,"follower_count":15000,"bio":"Original keyboard reviews","public_links":["https://studio.example.org/contact"]}))}).unwrap();
    let material = owner
        .prepare_discovery_review_material("pin-contact", 1, &hit.id)
        .unwrap();
    assert!(material.native_content.is_none());
    assert_eq!(
        material.native_profile.as_ref().unwrap().bio.as_deref(),
        Some("Original keyboard reviews")
    );
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "pin-contact".into(),
                expected_revision: 1,
                search_hit_id: hit.id,
                material_hash: material.material_hash,
                request_id: "pin-contact-score".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale:
                        "Native biography matches keyboard topic; no content language requirement."
                            .into(),
                    citations: vec![DiscoveryReviewQuote::NativeProfile {
                        profile_url: source.into(),
                        quote: "Original keyboard reviews".into(),
                    }],
                    tool: None,
                    country_code: None,
                    match_level: Some(DiscoveryMatchLevel::Good),
                }],
            },
        )
        .unwrap();
    assert_eq!(
        owner
            .count_discovery_candidates("pin-contact")
            .unwrap()
            .qualified,
        0
    );
    let page = owner
        .repository
        .next_discovery_public_page("pin-contact", &[])
        .unwrap()
        .unwrap();
    assert_eq!(page.requested_url, "https://studio.example.org/contact");
    let mut wrong = page.clone();
    wrong.requested_url = "https://unrelated.example/contact".into();
    assert!(owner.claim_discovery_public_page(wrong).is_err());
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some("https://studio.example.org/contact".into()),
            title: "Studio business".into(),
            markdown_excerpt: "Business studio@example.org".into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let snapshot = owner
        .prepare_discovery_library_snapshot("pin-contact", 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(
        snapshot.rows[0].contacts[0].normalized_value,
        "studio@example.org"
    );
    assert_eq!(
        snapshot.rows[0].contacts[0].source_url,
        "https://studio.example.org/contact"
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page("pin-contact", &[])
            .unwrap()
            .is_none()
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("pin-contact", &[])
            .unwrap()
            .is_none()
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot("pin-contact", 1)
            .unwrap()
            .rows,
        snapshot.rows
    );
}
