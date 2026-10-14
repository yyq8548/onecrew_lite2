use crate::*;
use serde_json::json;
const CAMPAIGN: &str = "weibo-flow";
const SOURCE: &str = "https://weibo.com/12345/Abc123";

fn setup(count: Option<u64>) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {id:CAMPAIGN.into(),brief:"Keyboard creators".into(),opencli_version:"1.8.8".into(),
        criteria:json!({"min_followers":10000,"language":"中文","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes:vec![DiscoveryPlatformScope {platform:"weibo".into(),profile_ref:"dedicated".into(),keywords:vec!["keyboard".into()]}],target_count:2,max_calls:None}).unwrap();
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    let claim = owner
        .claim_discovery_search(CAMPAIGN, 1, "weibo", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: SOURCE.into(),
                title: "Original keyboard review".into(),
                author_label: "Author".into(),
                author_id: Some("12345".into()),
                author_url: Some("https://weibo.com/u/12345".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: CAMPAIGN.into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0)
        .id;
    let claim = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
    let profile = json!({"id":"12345","name":"Author","follower_count":count,"profile_url":"https://weibo.com/u/12345","bio":"商务合作: studio@example.com Telegram: keyboard_studio","self_declared_location":""});
    let mut wrong = profile.clone();
    wrong["id"] = json!("99999");
    assert!(
        owner
            .finish_discovery_profile(FinishDiscoveryProfileInput {
                claim: claim.clone(),
                native_profile: Some(wrong),
                stop_code: None
            })
            .is_err()
    );
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim,
            native_profile: Some(profile),
            stop_code: None,
        })
        .unwrap();
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    let content = json!({"post_id":"98765","author_id":"12345","author":"Author","source_url":SOURCE,"text":"Original keyboard review","text_truncated":false,"text_completeness_known":false,"is_repost":false});
    for (key, value) in [
        ("author_id", json!("99999")),
        ("is_repost", json!(true)),
        ("source_url", json!("https://weibo.com/99999/Other")),
    ] {
        let mut wrong = content.clone();
        wrong[key] = value;
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
    (dir, owner, hit)
}

#[tokio::test]
async fn weibo_native_author_first_five_comments_contacts_library_and_restart() {
    let (dir, owner, hit) = setup(Some(15000));
    super::language_tests::require_fresh_language_review(&owner, CAMPAIGN, &hit, 0).await;
    let claim = owner
        .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.root_url, SOURCE);
    let quotes = [
        "这个键盘的手感不错",
        "谢谢你分享机械键盘测评",
        "我也想试试这款键盘",
    ];
    let sample = DiscoveryLanguageSample {
        author_id: "12345".into(),
        source_url: SOURCE.into(),
        text: "Original keyboard review".into(),
        comments: quotes
            .into_iter()
            .enumerate()
            .map(|(i, text)| DiscoveryLanguageComment {
                source_url: format!("{SOURCE}#comment-{}", i + 100),
                text: text.into(),
            })
            .collect(),
    };
    assert!(sample.valid_for(&claim));
    let mut wrong = sample.clone();
    wrong.comments[0].source_url = "https://weibo.com/12345/Other#comment-100".into();
    assert!(!wrong.valid_for(&claim));
    wrong = sample.clone();
    wrong.author_id = "99999".into();
    assert!(!wrong.valid_for(&claim));
    wrong = sample.clone();
    for _ in 0..3 {
        wrong.comments.push(sample.comments[0].clone());
    }
    assert!(!wrong.valid_for(&claim));
    owner
        .finish_discovery_language_async(claim, Some(sample), None)
        .await
        .unwrap();
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_followers, Some(15000));
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: CAMPAIGN.into(),
                expected_revision: 1,
                search_hit_id: hit.clone(),
                material_hash: material.material_hash,
                request_id: "weibo-audience-good".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "其他要求符合；粉丝评论语言符合：中文；博主本人语言未核实".into(),
                    citations: quotes
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
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(snapshot.rows[0].contacts.len(), 2);
    assert!(
        snapshot.rows[0]
            .contacts
            .iter()
            .all(|c| c.source_url == "https://weibo.com/u/12345")
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
        4
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap()
            .rows[0]
            .contacts
            .len(),
        2
    );
}

#[test]
fn weibo_unknown_followers_stay_unknown_even_with_contacts() {
    let (_dir, owner, hit) = setup(None);
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(material.native_followers, None);
    let error = owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: CAMPAIGN.into(),
                expected_revision: 1,
                search_hit_id: hit,
                material_hash: material.material_hash,
                request_id: "unknown-count".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "Keyboard review".into(),
                    citations: vec![DiscoveryReviewQuote::NativeContent {
                        quote: "Original keyboard review".into(),
                    }],
                    tool: None,
                    country_code: None,
                    match_level: Some(DiscoveryMatchLevel::Good),
                }],
            },
        )
        .unwrap_err();
    assert!(error.contains("initial_match_required_count_unknown"));
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
}
