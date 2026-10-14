use super::*;
use crate::*;
use serde_json::json;

#[test]
fn bilibili_shoe_review_and_contact_lookup_require_bound_native_sources() {
    bilibili_contact_case(false);
}

#[test]
fn bilibili_header_biography_email_qualifies_without_external_lookup() {
    bilibili_contact_case(true);
}

fn bilibili_contact_case(biography_email: bool) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "bili-shoes".into(),
            brief: "Shoe creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"category":"鞋子","region":"","language":"",
            "requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            scopes: vec![
                DiscoveryPlatformScope {
                    platform: "bilibili".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["鞋子测评".into()],
                },
                DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["shoe review".into()],
                },
            ],
            target_count: 1,
            max_calls: Some(5),
        })
        .unwrap();
    owner.approve_discovery_campaign("bili-shoes", 1).unwrap();
    let claim = owner
        .claim_discovery_search("bili-shoes", 1, "bilibili", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.bilibili.com/video/BV1shoe00001".into(),
                title: "跑鞋测评".into(),
                author_label: "Runner".into(),
                author_id: Some("168310298".into()),
                author_url: Some("https://space.bilibili.com/168310298".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "bili-shoes".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    assert_eq!(
        owner.plan_discovery_step("bili-shoes", 1).unwrap(),
        DiscoveryStep::ReadProfile {
            search_hit_id: hit.id.clone()
        }
    );
    let profile = owner
        .claim_discovery_profile("bili-shoes", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim: profile,
            native_profile: Some(
                json!({"mid":"168310298","profile_url":"https://space.bilibili.com/168310298",
            "name":"Runner","follower_count":15000,
            "sign": if biography_email { "商务合作：studio@example.org" } else { "跑鞋测评" }}),
            ),
            stop_code: None,
        })
        .unwrap();
    assert_eq!(
        owner.plan_discovery_step("bili-shoes", 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.id.clone()
        }
    );
    let content = owner
        .claim_discovery_content("bili-shoes", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim: content,
            native_content: Some(
                json!({"bvid":"BV1shoe00001","author_id":"168310298","author":"Runner",
            "source_url":"https://www.bilibili.com/video/BV1shoe00001","title":"跑鞋测评",
            "description":"这期实测新款跑鞋的缓震和抓地力。 https://creator.example/contact","description_truncated":false,
            "requires_payment":false,"pay_preview":false}),
            ),
            stop_code: None,
        })
        .unwrap();
    let material = owner
        .prepare_discovery_review_material("bili-shoes", 1, &hit.id)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_followers, Some(15000));
    assert_eq!(
        material.native_profile.as_ref().unwrap().bio.as_deref(),
        Some(if biography_email {
            "商务合作：studio@example.org"
        } else {
            "跑鞋测评"
        })
    );
    assert!(
        material.contains_citation(DiscoveryReviewCitation::NativeContent {
            quote: "实测新款跑鞋"
        })
    );
    assert!(
        !material.contains_citation(DiscoveryReviewCitation::NativeContent {
            quote: "虚构内容"
        })
    );
    let input = SubmitDiscoveryReviewInput {
        campaign_id: "bili-shoes".into(),
        expected_revision: 1,
        search_hit_id: hit.id.clone(),
        material_hash: material.material_hash,
        request_id: "bili-shoes-review".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: "test reviewer".into(),
        },
        findings: vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::CategoryContent,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale: "Native video describes a shoe test.".into(),
            citations: vec![DiscoveryReviewQuote::NativeContent {
                quote: "实测新款跑鞋的缓震和抓地力".into(),
            }],
            tool: None,
            match_level: None,
            country_code: None,
        }],
    };
    let mut forged = input.clone();
    forged.request_id = "bili-shoes-forged".into();
    forged.findings[0].citations = vec![DiscoveryReviewQuote::NativeContent {
        quote: "虚构内容".into(),
    }];
    assert!(owner.submit_discovery_review("account", forged).is_err());
    let receipt = owner.submit_discovery_review("account", input).unwrap();
    assert_eq!(receipt.verdict, "qualified");
    assert_eq!(receipt.subscribers, Some(15000));
    assert!(receipt.reasons.is_empty());
    // A checklist outcome is not enough: only the saved passing match score
    // authorizes the separately source-bound contact lookup.
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        0
    );
    let material = owner
        .prepare_discovery_review_material("bili-shoes", 1, &hit.id)
        .unwrap();
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "bili-shoes".into(),
                expected_revision: 1,
                search_hit_id: hit.id.clone(),
                material_hash: material.material_hash,
                request_id: "initial-bili-contact".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: initial_match::INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "Saved native shoe test matches the requested category.".into(),
                    citations: vec![DiscoveryReviewQuote::NativeContent {
                        quote: "实测新款跑鞋的缓震和抓地力".into(),
                    }],
                    tool: None,
                    country_code: None,
                    match_level: Some(DiscoveryMatchLevel::Good),
                }],
            },
        )
        .unwrap();
    if biography_email {
        assert_eq!(
            owner.plan_discovery_step("bili-shoes", 1).unwrap(),
            DiscoveryStep::TargetReached
        );
        let publication = owner
            .prepare_discovery_library_publication("bili-shoes", 1)
            .unwrap();
        let row = &publication.material.sources.rows[0];
        assert_eq!(row.verdict, "qualified");
        assert_eq!(row.contacts.len(), 1);
        assert_eq!(row.contacts[0].normalized_value, "studio@example.org");
        assert_eq!(
            row.contacts[0].source_url,
            "https://space.bilibili.com/168310298"
        );
        assert_eq!(row.contacts[0].native_author_id, "168310298");
        assert_eq!(
            owner
                .get_discovery_campaign("bili-shoes")
                .unwrap()
                .used_calls,
            3
        );
        return;
    }
    assert!(matches!(
        owner.plan_discovery_step("bili-shoes", 1).unwrap(),
        DiscoveryStep::ReadPublicPage { .. }
    ));
    let claim = owner
        .claim_discovery_public_page(DiscoveryPublicPageInput {
            campaign_id: "bili-shoes".into(),
            expected_revision: 1,
            candidate_id: receipt.candidate_id.clone(),
            profile_ref: "dedicated".into(),
            requested_url: "https://creator.example/contact".into(),
        })
        .unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some("https://creator.example/contact".into()),
            title: "Contact".into(),
            markdown_excerpt:
                "Visit [Business contact](/business/contact-us). https://unrelated.example/contact"
                    .into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        0
    );
    let child = DiscoveryPublicPageInput {
        campaign_id: "bili-shoes".into(),
        expected_revision: 1,
        candidate_id: receipt.candidate_id.clone(),
        profile_ref: "dedicated".into(),
        requested_url: "https://creator.example/business/contact-us".into(),
    };
    for url in [
        "https://unrelated.example/contact",
        "https://creator.example/contact-guessed",
    ] {
        let mut forged = child.clone();
        forged.requested_url = url.into();
        assert!(owner.claim_discovery_public_page(forged).is_err());
    }
    assert!(matches!(
        owner.plan_discovery_step("bili-shoes", 1).unwrap(),
        DiscoveryStep::ReadPublicPage { .. }
    ));
    let claim = owner.claim_discovery_public_page(child.clone()).unwrap();
    assert!(owner.claim_discovery_public_page(child.clone()).is_err());
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some(child.requested_url.clone()),
            title: "Business contact".into(),
            markdown_excerpt: "Business: shoes@example.org".into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert!(owner.claim_discovery_public_page(child).is_err());
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(
        owner.plan_discovery_step("bili-shoes", 1).unwrap(),
        DiscoveryStep::TargetReached
    );
    assert_eq!(
        owner
            .get_discovery_campaign("bili-shoes")
            .unwrap()
            .platform_runs[1]
            .status,
        "unsearched"
    );
    let publication = owner
        .prepare_discovery_library_publication("bili-shoes", 1)
        .unwrap();
    assert_eq!(
        publication.material.sources.rows[0].contacts[0].normalized_value,
        "shoes@example.org"
    );
}

#[test]
fn later_youtube_video_preserves_audited_shoe_proof() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "youtube-shoes".into(),
            brief: "Shoe creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_subscribers":10000,"category":"鞋子",
                "requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["shoe reviews".into()],
            }],
            target_count: 30,
            max_calls: Some(4),
        })
        .unwrap();
    owner
        .approve_discovery_campaign("youtube-shoes", 1)
        .unwrap();
    let search = owner
        .claim_discovery_search("youtube-shoes", 1, "youtube", "dedicated")
        .unwrap();
    let first = "https://www.youtube.com/watch?v=LR04bU_yV5k";
    let second = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: [first, second]
                .into_iter()
                .map(|source_url| DiscoverySearchHitInput {
                    source_url: source_url.into(),
                    title: "Shoe review".into(),
                    author_label: "Reviewer".into(),
                    author_id: None,
                    author_url: None,
                })
                .collect(),
        })
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "youtube-shoes".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    let make_candidate = |source: &str, description: &str| UpsertDiscoveryCandidateInput {
        campaign_id: "youtube-shoes".into(),
        expected_revision: 1,
        platform: "youtube".into(),
        source_profile_ref: "dedicated".into(),
        platform_account_id: "UC1234567890".into(),
        display_name: "Reviewer".into(),
        profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
        evidence: json!({"source_urls":[source],"subscribers_raw":"15K subscribers",
            "subscribers_source_url":source,"captured_at":1,
            "video_title":"Shoe review","video_description":description}),
    };
    let first_hit = hits.iter().find(|hit| hit.source_url == first).unwrap();
    let claim = owner
        .claim_discovery_video("youtube-shoes", 1, "dedicated", &first_hit.id)
        .unwrap();
    owner
        .finish_discovery_video(FinishDiscoveryVideoInput {
            claim,
            candidate: Some(make_candidate(
                first,
                "I reviewed the running shoes and their fit.",
            )),
            stop_code: None,
        })
        .unwrap();
    let material = owner
        .prepare_discovery_review_material("youtube-shoes", 1, &first_hit.id)
        .unwrap();
    let receipt = owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "youtube-shoes".into(),
                expected_revision: 1,
                search_hit_id: first_hit.id.clone(),
                material_hash: material.material_hash,
                request_id: "shoe-review-1".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: "test reviewer".into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::CategoryContent,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "Description reviews running shoes.".into(),
                    citations: vec![DiscoveryReviewQuote::Description {
                        quote: "I reviewed the running shoes and their fit.".into(),
                    }],
                    tool: None,
                    match_level: None,
                    country_code: None,
                }],
            },
        )
        .unwrap();
    assert_eq!(receipt.verdict, "qualified");
    let second_hit = hits.iter().find(|hit| hit.source_url == second).unwrap();
    let claim = owner
        .claim_discovery_video("youtube-shoes", 1, "dedicated", &second_hit.id)
        .unwrap();
    owner
        .finish_discovery_video(FinishDiscoveryVideoInput {
            claim,
            candidate: Some(make_candidate(
                second,
                "Another review from the same channel.",
            )),
            stop_code: None,
        })
        .unwrap();
    let candidates = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "youtube-shoes".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].verdict, "qualified");
    assert_eq!(
        candidates[0].evidence["reviewed_category_content"]["review_id"],
        receipt.review_id
    );
}

fn setup() -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_with_criteria(
        json!({"min_followers":10000,"category":"服装","region":"中国大陆以外",
        "requires_ai_tool_demo":false,"requires_public_tool_url":false}),
    )
}

pub(super) fn setup_with_criteria(
    criteria: serde_json::Value,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "x-apparel".into(),
            brief: "Overseas apparel business".into(),
            opencli_version: "1.8.8".into(),
            criteria,
            scopes: vec![DiscoveryPlatformScope {
                platform: "twitter".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["apparel export".into()],
            }],
            target_count: 30,
            max_calls: Some(10),
        })
        .unwrap();
    owner.approve_discovery_campaign("x-apparel", 1).unwrap();
    let search = owner
        .claim_discovery_search("x-apparel", 1, "twitter", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://x.com/i/status/123456789".into(),
                title: "Apparel export".into(),
                author_label: "Creator".into(),
                author_id: Some("creator_1".into()),
                author_url: Some("https://x.com/creator_1".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "x-apparel".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    assert!(
        owner
            .prepare_discovery_review_material("x-apparel", 1, &hit.id)
            .is_err()
    );
    let identity = owner
        .claim_discovery_identity("x-apparel", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_identity(FinishDiscoveryIdentityInput {
            claim: identity,
            account_id: Some("987654321".into()),
            observed_source_url: Some("https://x.com/creator_1/status/123456789".into()),
            stop_code: None,
        })
        .unwrap();
    let profile = owner
        .claim_discovery_profile("x-apparel", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim: profile,
            native_profile: Some(
                json!({"handle":"creator_1","profile_url":"https://x.com/creator_1",
            "name":"Creator","bio":"New York apparel founder","self_declared_location":"New York, NY","follower_count":15000,
            "identity_kind":"native_id","account_id":"987654321"}),
            ),
            stop_code: None,
        })
        .unwrap();
    let claim = owner
        .claim_discovery_content("x-apparel", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: Some(json!({"adapter_version":"1.0.0","identity":{
            "adapter_version":"1.0.0","post_id":"123456789","account_id":"987654321",
            "handle":"creator_1","profile_url":"https://x.com/creator_1",
            "source_url":"https://x.com/creator_1/status/123456789",
            "identity_kind":"native_post_author_id","evidence_source":"x_tweet_detail"},
            "text":"I run a clothing brand selling internationally.","text_truncated":false,
            "text_source":"legacy_full_text","created_at":"2026-09-28","has_media":false,
            "quotes_another_post":false,"expanded_urls":[]})),
            stop_code: None,
        })
        .unwrap();
    (dir, owner, hit.id)
}

#[test]
fn x_review_requires_bound_native_post_and_keeps_residence_pending() {
    let (dir, owner, hit) = setup();
    let material = owner
        .prepare_discovery_review_material("x-apparel", 1, &hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_followers, Some(15000));
    let profile = material.native_profile.as_ref().unwrap();
    assert_eq!(profile.profile_url, "https://x.com/creator_1");
    assert_eq!(profile.bio.as_deref(), Some("New York apparel founder"));
    assert_eq!(
        profile.self_declared_location.as_deref(),
        Some("New York, NY")
    );
    assert_eq!(material.native_author_id, "987654321");
    assert!(
        material.contains_citation(DiscoveryReviewCitation::NativeContent {
            quote: "clothing brand selling internationally"
        })
    );
    assert!(
        !material.contains_citation(DiscoveryReviewCitation::NativeContent {
            quote: "fabricated residence"
        })
    );
    let input = SubmitDiscoveryReviewInput {
        campaign_id: "x-apparel".into(),
        expected_revision: 1,
        search_hit_id: hit.clone(),
        material_hash: material.material_hash.clone(),
        request_id: "review-x".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: "test reviewer".into(),
        },
        findings: vec![
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::ApparelBusinessContent,
                outcome: DiscoveryReviewOutcome::Supported,
                rationale: "Author discusses clothing business.".into(),
                citations: vec![DiscoveryReviewQuote::NativeContent {
                    quote: "I run a clothing brand selling internationally.".into(),
                }],
                tool: None,
                match_level: None,
                country_code: None,
            },
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::CreatorResidenceOutsideMainland,
                outcome: DiscoveryReviewOutcome::Insufficient,
                rationale: "No independent residence evidence.".into(),
                citations: vec![],
                tool: None,
                match_level: None,
                country_code: None,
            },
        ],
    };
    let mut forged = input.clone();
    forged.request_id = "forged-x".into();
    forged.findings[0].citations = vec![DiscoveryReviewQuote::NativeContent {
        quote: "I live in Toronto".into(),
    }];
    assert!(owner.submit_discovery_review("account", forged).is_err());
    let receipt = owner
        .submit_discovery_review("account", input.clone())
        .unwrap();
    assert_eq!(receipt.policy_version, "apparel-native-review-v2");
    assert_eq!(receipt.verdict, "pending");
    assert!(receipt.reasons.contains(&"region_unverified".to_string()));
    assert!(!receipt.reasons.contains(&"category_unverified".to_string()));
    assert!(
        !receipt
            .reasons
            .contains(&"subscriber_count_unverified".to_string())
    );
    assert_eq!(
        owner.submit_discovery_review("account", input).unwrap(),
        receipt
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT json_extract(material_json,'$.format_version') FROM discovery_evidence_reviews",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn x_review_rejects_profile_url_that_disagrees_with_the_native_post_author() {
    let (dir, owner, hit) = setup();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_profile_reads SET native_profile_json=json_set(native_profile_json,'$.profile_url','https://x.com/imposter') WHERE search_hit_id=?1",
        [&hit],
    )
    .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_review_material("x-apparel", 1, &hit)
            .unwrap_err(),
        "review_native_profile_binding_unavailable"
    );
}

#[test]
fn tiktok_caption_review_requires_native_author_profile_and_keeps_residence_pending() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "tiktok-apparel".into(),
            brief: "Overseas apparel".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"category":"服装","region":"中国大陆以外",
            "requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "tiktok".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["apparel export".into()],
            }],
            target_count: 30,
            max_calls: Some(4),
        })
        .unwrap();
    owner
        .approve_discovery_campaign("tiktok-apparel", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("tiktok-apparel", 1, "tiktok", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.tiktok.com/@maker/video/1234567890123456789".into(),
                title: "We make dresses for international wholesale buyers.".into(),
                author_label: "maker".into(),
                author_id: Some("9876543210123456789".into()),
                author_url: Some("https://www.tiktok.com/@maker".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "tiktok-apparel".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    assert!(
        owner
            .prepare_discovery_review_material("tiktok-apparel", 1, &hit.id)
            .is_err()
    );
    let profile = owner
        .claim_discovery_profile("tiktok-apparel", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim: profile,
            native_profile: Some(json!({"adapter_version":"1.0.0","id":"9876543210123456789",
            "username":"maker","display_name":"Maker","signature":"Wholesale apparel",
            "follower_count":15000,"verified":false,"private_account":false,
            "profile_url":"https://www.tiktok.com/@maker","evidence_source":"profile_bootstrap"})),
            stop_code: None,
        })
        .unwrap();
    let material = owner
        .prepare_discovery_review_material("tiktok-apparel", 1, &hit.id)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_followers, Some(15000));
    assert_eq!(
        material.native_profile.as_ref().unwrap().bio.as_deref(),
        Some("Wholesale apparel")
    );
    assert_eq!(
        material.hit.author_id.as_deref(),
        Some("9876543210123456789")
    );
    assert!(
        material.contains_citation(DiscoveryReviewCitation::SearchCaption {
            quote: "dresses for international wholesale"
        })
    );
    assert!(
        !material.contains_citation(DiscoveryReviewCitation::SearchCaption {
            quote: "based in Canada"
        })
    );
    let input = SubmitDiscoveryReviewInput {
        campaign_id: "tiktok-apparel".into(),
        expected_revision: 1,
        search_hit_id: hit.id.clone(),
        material_hash: material.material_hash.clone(),
        request_id: "tiktok-review".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: "test reviewer".into(),
        },
        findings: vec![
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::ApparelBusinessContent,
                outcome: DiscoveryReviewOutcome::Supported,
                rationale: "Caption describes original apparel wholesale offer.".into(),
                citations: vec![DiscoveryReviewQuote::SearchCaption {
                    quote: "dresses for international wholesale buyers".into(),
                }],
                tool: None,
                match_level: None,
                country_code: None,
            },
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::CreatorResidenceOutsideMainland,
                outcome: DiscoveryReviewOutcome::Insufficient,
                rationale: "No independently verified residence.".into(),
                citations: vec![],
                tool: None,
                match_level: None,
                country_code: None,
            },
        ],
    };
    let mut forged = input.clone();
    forged.request_id = "forged-tiktok".into();
    forged.findings[0].citations = vec![DiscoveryReviewQuote::SearchCaption {
        quote: "based in Canada".into(),
    }];
    assert!(owner.submit_discovery_review("account", forged).is_err());
    let receipt = owner
        .submit_discovery_review("account", input.clone())
        .unwrap();
    assert_eq!(receipt.policy_version, "apparel-native-review-v2");
    assert_eq!(receipt.verdict, "pending");
    assert_eq!(receipt.subscribers, Some(15000));
    assert!(receipt.reasons.contains(&"region_unverified".to_string()));
    assert!(!receipt.reasons.contains(&"category_unverified".to_string()));
    assert_eq!(
        owner.submit_discovery_review("account", input).unwrap(),
        receipt
    );
}

#[test]
fn tiktok_shoe_caption_qualifies_only_with_a_bound_native_author_and_exact_quote() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "tiktok-shoes".into(),
            brief: "Shoe creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"category":"鞋子","region":"",
                "language":"","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "tiktok".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["running shoes".into()],
            }],
            target_count: 30,
            max_calls: Some(4),
        })
        .unwrap();
    owner.approve_discovery_campaign("tiktok-shoes", 1).unwrap();
    let claim = owner
        .claim_discovery_search("tiktok-shoes", 1, "tiktok", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.tiktok.com/@shoemaker/video/1234567890123456789".into(),
                title: "I compare three running shoes on foot.".into(),
                author_label: "shoemaker".into(),
                author_id: Some("9876543210123456789".into()),
                author_url: Some("https://www.tiktok.com/@shoemaker".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "tiktok-shoes".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    assert!(
        owner
            .prepare_discovery_review_material("tiktok-shoes", 1, &hit.id)
            .is_err()
    );
    let profile = owner
        .claim_discovery_profile("tiktok-shoes", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim: profile,
            native_profile: Some(json!({"adapter_version":"1.0.0","id":"9876543210123456789",
                "username":"shoemaker","display_name":"Shoemaker","signature":"Shoe reviews",
                "follower_count":15000,"verified":false,"private_account":false,
                "profile_url":"https://www.tiktok.com/@shoemaker","evidence_source":"profile_bootstrap"})),
            stop_code: None,
        })
        .unwrap();
    let material = owner
        .prepare_discovery_review_material("tiktok-shoes", 1, &hit.id)
        .unwrap();
    assert_eq!(material.native_author_id, "9876543210123456789");
    let input = SubmitDiscoveryReviewInput {
        campaign_id: "tiktok-shoes".into(),
        expected_revision: 1,
        search_hit_id: hit.id,
        material_hash: material.material_hash,
        request_id: "shoes-review".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: "test reviewer".into(),
        },
        findings: vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::CategoryContent,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale: "Native author compares shoes in the saved caption.".into(),
            citations: vec![DiscoveryReviewQuote::SearchCaption {
                quote: "compare three running shoes".into(),
            }],
            tool: None,
            match_level: None,
            country_code: None,
        }],
    };
    let mut forged = input.clone();
    forged.request_id = "forged-shoes-review".into();
    forged.findings[0].citations = vec![DiscoveryReviewQuote::SearchCaption {
        quote: "fabricated shoe review".into(),
    }];
    assert!(owner.submit_discovery_review("account", forged).is_err());
    let receipt = owner.submit_discovery_review("account", input).unwrap();
    assert_eq!(receipt.verdict, "qualified");
    assert_eq!(receipt.subscribers, Some(15000));
    assert!(receipt.reasons.is_empty());
}
