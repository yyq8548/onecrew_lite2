use super::*;
use crate::*;
use serde_json::json;

fn export_language_snapshot(directory: &std::path::Path, suffix: &str) {
    let Some(output) = std::env::var_os("ONECREW_LANGUAGE_ORACLE_DIRECTORY") else { return; };
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).unwrap();
    let db = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
    db.execute("VACUUM INTO ?1", [output.join(format!("tiktok-{suffix}.sqlite")).to_str().unwrap()]).unwrap();
}

#[tokio::test]
async fn tiktok_language_reads_only_matched_post_once_without_commenter_ids() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id:"tiktok-language".into(),brief:"clothing creators".into(),opencli_version:"1.8.8".into(),
        criteria:json!({"min_followers":10000,"category":"服装","language":"中文","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes:vec![DiscoveryPlatformScope {platform:"tiktok".into(),profile_ref:"dedicated".into(),keywords:vec!["clothing".into()]}],
        target_count:4,max_calls: Some(20),
    }).unwrap();
    owner
        .approve_discovery_campaign("tiktok-language", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("tiktok-language", 1, "tiktok", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.tiktok.com/@maker/video/123451".into(),
                title: "Clothing fabrics".into(),
                author_label: "maker".into(),
                author_id: Some("9876543210123456789".into()),
                author_url: Some("https://www.tiktok.com/@maker".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "tiktok-language".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0)
        .id;
    let claim = owner
        .claim_discovery_profile("tiktok-language", 1, &hit)
        .unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim,stop_code:None,native_profile:Some(json!({
        "adapter_version":"1.0.0","id":"9876543210123456789","username":"maker","display_name":"Maker","signature":"Clothing",
        "follower_count":15000,"verified":false,"private_account":false,"profile_url":"https://www.tiktok.com/@maker","evidence_source":"profile_bootstrap"}))}).unwrap();
    require_fresh_language_review(&owner, "tiktok-language", &hit, 0).await;
    // TikTok's bound native search caption is already original text. It does
    // not need another content request, but an older review cannot authorize
    // comments after that caption changes.
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let before_original = owner
        .get_discovery_campaign("tiktok-language")
        .unwrap()
        .used_calls;
    db.execute(
        "UPDATE discovery_search_hits SET title='Changed original caption' WHERE id=?1",
        [&hit],
    )
    .unwrap();
    assert!(
        owner
            .claim_discovery_language_async("tiktok-language".into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none(),
        "the changed original caption must be evaluated before comments"
    );
    assert_eq!(
        owner
            .get_discovery_campaign("tiktok-language")
            .unwrap()
            .used_calls,
        before_original
    );
    db.execute(
        "UPDATE discovery_search_hits SET title='Clothing fabrics' WHERE id=?1",
        [&hit],
    )
    .unwrap();
    drop(db);
    let initial_calls = owner
        .get_discovery_campaign("tiktok-language")
        .unwrap()
        .used_calls;
    export_language_snapshot(dir.path(), "before");
    {
        let ordinal = 1;
        assert_eq!(
            owner.plan_discovery_step("tiktok-language", 1).unwrap(),
            DiscoveryStep::ReadContent {
                search_hit_id: hit.clone()
            }
        );
        let claim = owner
            .claim_discovery_language_async("tiktok-language".into(), 1, hit.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claim.ordinal, ordinal);
        assert_eq!(claim.author_handle, "maker");
        let source = format!("https://www.tiktok.com/@maker/video/12345{ordinal}");
        let sample = DiscoveryLanguageSample {
            author_id: claim.author_id.clone(),
            source_url: source.clone(),
            text: "Clothing".into(),
            comments: vec![DiscoveryLanguageComment {
                source_url: source,
                text: "Unresolved sample".into(),
            }],
        };
        let mut extended = sample.clone();
        export_language_snapshot(dir.path(), "claimed");
        if let Some(output) = std::env::var_os("ONECREW_LANGUAGE_ORACLE_DIRECTORY") {
            let claim_json = json!({"campaign_id":claim.campaign_id,"revision":claim.revision,"search_hit_id":claim.search_hit_id,
                "candidate_id":claim.candidate_id,"platform":claim.platform,"profile_ref":claim.profile_ref,
                "author_id":claim.author_id,"author_handle":claim.author_handle,"root_url":claim.root_url,"ordinal":claim.ordinal,
                "call_number":claim.call_number,"previous_urls":claim.previous_urls,"saved_content_urls":claim.saved_content_urls});
            std::fs::write(std::path::PathBuf::from(output).join("tiktok.json"), serde_json::to_vec_pretty(&json!({"claim":claim_json,"sample":sample})).unwrap()).unwrap();
        }
        extended.comments = (1..=5)
            .map(|i| DiscoveryLanguageComment {
                source_url: claim.root_url.clone(),
                text: format!("Substantive comment {i}"),
            })
            .collect();
        assert!(extended.valid_for(&claim));
        extended.comments.push(DiscoveryLanguageComment {
            source_url: claim.root_url.clone(),
            text: "Sixth comment must not supply a vote".into(),
        });
        assert!(!extended.valid_for(&claim));
        let mut different_post = claim.clone();
        different_post.ordinal = 2;
        assert!(!sample.valid_for(&different_post));
        different_post.ordinal = 1;
        different_post.previous_urls.push(claim.root_url.clone());
        assert!(!sample.valid_for(&different_post));
        let mut wrong = sample.clone();
        wrong.author_id = "999".into();
        assert!(!wrong.valid_for(&claim));
        wrong = sample.clone();
        wrong.comments[0].source_url.push('9');
        assert!(!wrong.valid_for(&claim));
        owner
            .finish_discovery_language_async(claim, Some(sample), None)
            .await
            .unwrap();
        export_language_snapshot(dir.path(), "finished");
        require_fresh_language_review(&owner, "tiktok-language", &hit, ordinal).await;
    }
    assert!(
        owner
            .claim_discovery_language_async("tiktok-language".into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("tiktok-language")
            .unwrap()
            .used_calls,
        initial_calls + 1
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    let material = reopened
        .prepare_discovery_review_material("tiktok-language", 1, &hit)
        .unwrap();
    assert_eq!(material.language_samples.len(), 1);
    assert!(
        serde_json::to_value(material).unwrap()["language_samples"][0]["comments"][0]
            .get("author_id")
            .is_none()
    );
}

pub(super) async fn require_fresh_language_review(
    owner: &CreatorRuntime,
    campaign: &str,
    hit: &str,
    ordinal: u32,
) {
    let used = owner.get_discovery_campaign(campaign).unwrap().used_calls;
    assert!(
        owner
            .claim_discovery_language_async(campaign.into(), 1, hit.into())
            .await
            .unwrap()
            .is_none(),
        "a previous insufficient finding cannot authorize another read of newly available evidence"
    );
    assert_eq!(
        owner.get_discovery_campaign(campaign).unwrap().used_calls,
        used
    );
    let material = owner
        .prepare_discovery_review_material(campaign, 1, hit)
        .unwrap();
    let finding = DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: DiscoveryReviewOutcome::Insufficient,
        rationale: "The new sample still leaves only the author's language unresolved.".into(),
        citations: vec![],
        tool: None,
        country_code: None,
        match_level: None,
    };
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: campaign.into(),
                expected_revision: 1,
                search_hit_id: hit.into(),
                material_hash: material.material_hash,
                request_id: format!("after-language-{ordinal}"),
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
    let saved = owner
        .list_discovery_candidates(crate::ListDiscoveryCandidatesInput {
            campaign_id: campaign.into(),
            after_id: None,
            limit: 100,
        })
        .unwrap();
    assert!(
        saved
            .iter()
            .any(|candidate| candidate.evidence["reviewed_match"]["language_only_pending"] == true)
    );
}

#[tokio::test]
async fn bilibili_language_is_author_bound_persisted_and_limited_to_one_matched_post() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id: "bili-language".into(), brief: "shoe creators".into(), opencli_version: "1.8.8".into(),
        criteria: json!({"min_followers":10000,"language":"中文","category":"鞋子","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes: vec![DiscoveryPlatformScope { platform:"bilibili".into(), profile_ref:"dedicated".into(), keywords:vec!["shoes".into()] }],
        target_count: 4, max_calls: Some(20),
    }).unwrap();
    owner
        .approve_discovery_campaign("bili-language", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("bili-language", 1, "bilibili", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.bilibili.com/video/BV1234567890".into(),
                title: "shoes".into(),
                author_label: "Runner".into(),
                author_id: Some("42".into()),
                author_url: Some("https://space.bilibili.com/42".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "bili-language".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0)
        .id;
    let claim = owner
        .claim_discovery_profile("bili-language", 1, &hit)
        .unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim, native_profile:Some(json!({"mid":"42","profile_url":"https://space.bilibili.com/42","name":"Runner","follower_count":15000})),stop_code:None}).unwrap();
    let claim = owner
        .claim_discovery_content("bili-language", 1, &hit)
        .unwrap();
    owner.finish_discovery_content(FinishDiscoveryContentInput {claim, native_content:Some(json!({"bvid":"BV1234567890","author_id":"42","author":"Runner","source_url":"https://www.bilibili.com/video/BV1234567890","title":"shoes","description":"Shoe test","description_truncated":false,"requires_payment":false,"pay_preview":false})),stop_code:None}).unwrap();
    let material = owner
        .prepare_discovery_review_material("bili-language", 1, &hit)
        .unwrap();
    let finding = DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: DiscoveryReviewOutcome::Insufficient,
        rationale: "Language evidence missing".into(),
        citations: vec![],
        tool: None,
        country_code: None,
        match_level: None,
    };
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "bili-language".into(),
                expected_revision: 1,
                search_hit_id: hit.clone(),
                material_hash: material.material_hash,
                request_id: "bili-language-pending".into(),
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
    {
        let ordinal = 1;
        let claim = owner
            .claim_discovery_language_async("bili-language".into(), 1, hit.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claim.ordinal, ordinal);
        let source = format!("https://www.bilibili.com/video/BV123456789{}", ordinal - 1);
        let sample = DiscoveryLanguageSample {
            author_id: "42".into(),
            source_url: source.clone(),
            text: "original shoe test".into(),
            comments: vec![DiscoveryLanguageComment {
                source_url: if ordinal == 1 {
                    format!("{source}#reply123")
                } else {
                    source.clone()
                },
                text: "观众中文评论".into(),
            }],
        };
        assert!(sample.valid_for(&claim));
        let mut forged = sample.clone();
        forged.author_id = "51".into();
        assert!(!forged.valid_for(&claim));
        forged = sample.clone();
        forged.comments[0].source_url =
            "https://www.bilibili.com/video/BV9999999999#reply123".into();
        assert!(!forged.valid_for(&claim));
        if ordinal == 1 {
            forged = sample.clone();
            let mut duplicate = forged.comments[0].clone();
            duplicate.text = "Conflicting reply body".into();
            forged.comments.push(duplicate);
            assert!(!forged.valid_for(&claim));
        }
        owner
            .finish_discovery_language_async(claim, Some(sample), None)
            .await
            .unwrap();
        let material = owner
            .prepare_discovery_review_material("bili-language", 1, &hit)
            .unwrap();
        assert_eq!(material.language_samples.len(), ordinal as usize);
        require_fresh_language_review(&owner, "bili-language", &hit, ordinal).await;
        assert!(
            material.contains_citation(DiscoveryReviewCitation::NativeContent {
                quote: "观众中文评论"
            })
        );
    }
    assert!(
        owner
            .claim_discovery_language_async("bili-language".into(), 1, hit)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn youtube_language_uses_bound_channel_and_one_matched_post() {
    let dir = tempfile::tempdir().unwrap();
    let channel = "UCaaaaaaaaaaaaaaaaaaaaaa";
    let (owner, hit) = super::tests::setup_with_options(
        &dir.path().join("creator.db"),
        json!({"min_followers":10000,"language":"中文","category":"AI","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        channel,
        20,
    );
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    let finding = DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: DiscoveryReviewOutcome::Insufficient,
        rationale: "Only language requires additional evidence".into(),
        citations: vec![],
        tool: None,
        country_code: None,
        match_level: None,
    };
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "review".into(),
                expected_revision: 1,
                search_hit_id: hit.clone(),
                material_hash: material.material_hash,
                request_id: "yt-language".into(),
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
    {
        let ordinal = 1;
        let claim = owner
            .claim_discovery_language_async("review".into(), 1, hit.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claim.ordinal, ordinal);
        assert_eq!(claim.author_id, channel);
        let source = if ordinal == 1 {
            claim.root_url.clone()
        } else {
            format!("https://www.youtube.com/watch?v=abcdefghij{ordinal}")
        };
        let sample = DiscoveryLanguageSample {
            author_id: channel.into(),
            source_url: source.clone(),
            text: "原创内容".into(),
            comments: vec![DiscoveryLanguageComment {
                source_url: format!("{source}&lc=Ug1"),
                text: "观众的中文评论".into(),
            }],
        };
        assert!(sample.valid_for(&claim));
        let mut wrong = sample.clone();
        wrong.comments[0].source_url = "https://www.youtube.com/watch?v=other123456&lc=Ug1".into();
        assert!(!wrong.valid_for(&claim));
        wrong = sample.clone();
        wrong.author_id = "UCbbbbbbbbbbbbbbbbbbbbbb".into();
        assert!(!wrong.valid_for(&claim));
        owner
            .finish_discovery_language_async(claim, Some(sample), None)
            .await
            .unwrap();
        let material = owner
            .prepare_discovery_review_material("review", 1, &hit)
            .unwrap();
        assert_eq!(material.language_samples.len(), ordinal as usize);
        require_fresh_language_review(&owner, "review", &hit, ordinal).await;
        assert!(
            material.contains_citation(DiscoveryReviewCitation::NativeContent {
                quote: "观众的中文评论"
            })
        );
    }
    assert!(
        owner
            .claim_discovery_language_async("review".into(), 1, hit)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner.get_discovery_campaign("review").unwrap().used_calls,
        3
    );
}

#[tokio::test]
async fn youtube_new_or_changed_transcript_requires_evaluation_before_comments() {
    // A completed, unavailable, or failed transcript is new evidence. None of
    // these outcomes may reuse the earlier description-only language finding.
    for result in ["completed", "unavailable", "failed"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let (owner, hit) = super::tests::setup_with_options(
            &path,
            json!({"min_followers":10000,"language":"日本語","category":"AI",
                "requires_ai_tool_demo":true,"requires_public_tool_url":false}),
            "UCaaaaaaaaaaaaaaaaaaaaaa",
            20,
        );
        let pending = |request: &str| {
            let material = owner
                .prepare_discovery_review_material("review", 1, &hit)
                .unwrap();
            let finding = DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::MatchFit,
                outcome: DiscoveryReviewOutcome::Insufficient,
                rationale: "Other requirements match; only original language remains unclear."
                    .into(),
                citations: vec![],
                tool: None,
                country_code: None,
                match_level: None,
            };
            owner
                .submit_discovery_review(
                    "account",
                    SubmitDiscoveryReviewInput {
                        campaign_id: "review".into(),
                        expected_revision: 1,
                        search_hit_id: hit.clone(),
                        material_hash: material.material_hash,
                        request_id: request.into(),
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
        };
        pending("description-only-language");
        let transcript = owner
            .claim_discovery_transcript("review", 1, "dedicated", &hit)
            .unwrap();
        let segments = if result == "completed" {
            vec![DiscoveryTranscriptSegment {
                timestamp: "0:10".into(),
                speaker: String::new(),
                text: "SSD 32GB RTX".into(),
            }]
        } else {
            vec![]
        };
        owner
            .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
                claim: transcript,
                segments,
                truncated: result == "completed",
                stop_code: (result == "failed").then(|| "opencli_invalid_output".into()),
            })
            .unwrap();
        let used = owner.get_discovery_campaign("review").unwrap().used_calls;
        assert!(
            owner
                .claim_discovery_language_async("review".into(), 1, hit.clone())
                .await
                .unwrap()
                .is_none(),
            "new {result} transcript must be evaluated first"
        );
        assert_eq!(
            owner.get_discovery_campaign("review").unwrap().used_calls,
            used
        );

        pending("transcript-language");
        if result == "completed" {
            // A saved original must still be exactly the one in the review.
            let db = rusqlite::Connection::open(path.join("crm.db")).unwrap();
            let original: String = db
                .query_row(
                    "SELECT segments_json FROM discovery_transcript_reads WHERE search_hit_id=?1",
                    [&hit],
                    |r| r.get(0),
                )
                .unwrap();
            let changed = serde_json::to_string(&vec![DiscoveryTranscriptSegment {
                timestamp: "0:10".into(),
                speaker: String::new(),
                text: "OLED 16GB RTX".into(),
            }])
            .unwrap();
            db.execute(
                "UPDATE discovery_transcript_reads SET segments_json=?2 WHERE search_hit_id=?1",
                rusqlite::params![hit, changed],
            )
            .unwrap();
            assert!(
                owner
                    .claim_discovery_language_async("review".into(), 1, hit.clone())
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                owner.get_discovery_campaign("review").unwrap().used_calls,
                used
            );
            pending("changed-transcript-language");
            let changed_review = owner
                .prepare_discovery_review_material("review", 1, &hit)
                .unwrap();
            assert_eq!(
                changed_review.transcript.unwrap().segments[0].text,
                "OLED 16GB RTX"
            );
            // Keep the changed original: only its fresh review authorizes comments.
            assert_ne!(original, changed);
        }
        let comments = owner
            .claim_discovery_language_async("review".into(), 1, hit.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(comments.ordinal, 1);
        assert_eq!(
            comments.root_url,
            "https://www.youtube.com/watch?v=LR04bU_yV5k"
        );
        assert_eq!(comments.call_number, used + 1);
        owner
            .finish_discovery_language_async(
                comments,
                None,
                Some("language_sample_unavailable".into()),
            )
            .await
            .unwrap();
        drop(owner);
        let reopened = CreatorRuntime::open(&path, CreatorGateways::default()).unwrap();
        assert!(
            reopened
                .claim_discovery_language_async("review".into(), 1, hit)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            reopened
                .get_discovery_campaign("review")
                .unwrap()
                .used_calls,
            used + 1
        );
    }
}

fn pending(owner: &CreatorRuntime, hit: &str, request: &str, level: Option<DiscoveryMatchLevel>) {
    let material = owner
        .prepare_discovery_review_material("x-apparel", 1, hit)
        .unwrap();
    let finding = DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: if level.is_some() {
            DiscoveryReviewOutcome::Supported
        } else {
            DiscoveryReviewOutcome::Insufficient
        },
        rationale: "Topic and followers match; language requires more original evidence.".into(),
        citations: vec![DiscoveryReviewQuote::NativeContent {
            quote: "I run a clothing brand selling internationally.".into(),
        }],
        tool: None,
        country_code: None,
        match_level: level,
    };
    let mut findings = vec![finding.clone()];
    if level.is_none() {
        findings.push(DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::ContentLanguage,
            ..finding
        });
    }
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "x-apparel".into(),
                expected_revision: 1,
                search_hit_id: hit.into(),
                material_hash: material.material_hash,
                request_id: request.into(),
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
async fn language_samples_are_capped_bound_and_allow_audience_citations() {
    let (_dir, owner, hit) = super::native_tests::setup_with_criteria(
        json!({"min_followers":10000,"category":"服装","language":"中文","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
    );
    assert!(
        owner
            .claim_discovery_language_async("x-apparel".into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    pending(&owner, &hit, "pending-language", None);
    {
        let ordinal = 1;
        let claim = owner
            .claim_discovery_language_async("x-apparel".into(), 1, hit.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claim.ordinal, ordinal);
        assert!(
            owner
                .claim_discovery_language_async("x-apparel".into(), 1, hit.clone())
                .await
                .unwrap()
                .is_none()
        );
        let source_url = if ordinal == 1 {
            claim.root_url.clone()
        } else {
            format!("https://x.com/creator_1/status/{}", 123456789 + ordinal)
        };
        let sample = DiscoveryLanguageSample {
            author_id: "987654321".into(),
            source_url,
            text: format!("Original clothing observation {ordinal}"),
            comments: vec![
                DiscoveryLanguageComment {
                    source_url: "https://x.com/viewer/status/222".into(),
                    text: "观众的中文评论".into(),
                },
                DiscoveryLanguageComment {
                    source_url: "https://x.com/creator_1/status/333".into(),
                    text: "作者自己的回复".into(),
                },
            ],
        };
        assert!(sample.valid_for(&claim));
        let mut wrong = sample.clone();
        wrong.author_id = "111".into();
        assert!(!wrong.valid_for(&claim));
        wrong = sample.clone();
        wrong.source_url = "https://x.com/other/status/999".into();
        assert!(!wrong.valid_for(&claim));
        wrong = sample.clone();
        wrong.comments.push(wrong.comments[0].clone());
        assert!(!wrong.valid_for(&claim));
        owner
            .finish_discovery_language_async(claim, Some(sample), None)
            .await
            .unwrap();
        let material = owner
            .prepare_discovery_review_material("x-apparel", 1, &hit)
            .unwrap();
        assert_eq!(material.language_samples.len(), ordinal as usize);
        require_fresh_language_review(&owner, "x-apparel", &hit, ordinal).await;
        assert!(
            material.contains_citation(DiscoveryReviewCitation::NativeContent {
                quote: "观众的中文评论"
            })
        );
        assert!(
            material.contains_citation(DiscoveryReviewCitation::NativeContent {
                quote: "作者自己的回复"
            })
        );
    }
    assert!(
        owner
            .claim_discovery_language_async("x-apparel".into(), 1, hit)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-apparel")
            .unwrap()
            .used_calls,
        5
    );
}

#[tokio::test]
async fn a_resolved_language_stops_sampling_and_failed_reads_are_not_replayed() {
    for fail in [false, true] {
        let (_dir, owner, hit) = super::native_tests::setup_with_criteria(
            json!({"min_followers":10000,"category":"服装","language":"中文","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        );
        pending(&owner, &hit, "pending-language", None);
        let claim = owner
            .claim_discovery_language_async("x-apparel".into(), 1, hit.clone())
            .await
            .unwrap()
            .unwrap();
        let sample = DiscoveryLanguageSample {
            author_id: claim.author_id.clone(),
            source_url: claim.root_url.clone(),
            text: "服装创业的原创中文内容".into(),
            comments: vec![],
        };
        owner
            .finish_discovery_language_async(
                claim,
                (!fail).then_some(sample),
                fail.then(|| "language_sample_unavailable".into()),
            )
            .await
            .unwrap();
        if !fail {
            pending(
                &owner,
                &hit,
                "language-resolved",
                Some(DiscoveryMatchLevel::Good),
            );
        }
        assert!(
            owner
                .claim_discovery_language_async("x-apparel".into(), 1, hit)
                .await
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn an_initial_supported_rating_never_reserves_comment_reads() {
    let (_dir, owner, hit) = super::native_tests::setup_with_criteria(
        json!({"min_followers":10000,"category":"服装","language":"中文","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
    );
    pending(
        &owner,
        &hit,
        "initial-language-supported",
        Some(DiscoveryMatchLevel::Good),
    );
    let used = owner
        .get_discovery_campaign("x-apparel")
        .unwrap()
        .used_calls;
    assert!(
        owner
            .claim_discovery_language_async("x-apparel".into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-apparel")
            .unwrap()
            .used_calls,
        used
    );
    assert!(
        owner
            .prepare_discovery_review_material("x-apparel", 1, &hit)
            .unwrap()
            .language_samples
            .is_empty()
    );
}
