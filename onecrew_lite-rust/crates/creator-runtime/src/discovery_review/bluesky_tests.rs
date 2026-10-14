use crate::*;
use serde_json::json;
const CAMPAIGN: &str = "bluesky-flow";
const DID: &str = "did:plc:abcdefghijklmnopqrstuvwx";
const SOURCE: &str = "https://bsky.app/profile/did:plc:abcdefghijklmnopqrstuvwx/post/original";

fn setup(count: Option<u64>, contacts: bool) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {id:CAMPAIGN.into(),brief:"Keyboard creators".into(),opencli_version:"1.8.8".into(),
        criteria:json!({"min_followers":10000,"language":"中文","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes:vec![DiscoveryPlatformScope {platform:"bluesky".into(),profile_ref:"dedicated".into(),keywords:vec!["keyboard".into()]}],target_count:2,max_calls:None}).unwrap();
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    let claim = owner
        .claim_discovery_search(CAMPAIGN, 1, "bluesky", "dedicated")
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
                author_id: Some(DID.into()),
                author_url: Some(format!("https://bsky.app/profile/{DID}")),
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
    let bio = if contacts {
        "Original biography. ".repeat(140)
            + "Business: studio@example.com Telegram: keyboard_studio"
    } else {
        "Original biography".into()
    };
    let profile = json!({"did":DID,"handle":"author.bsky.social","display_name":"Author","biography":bio,"follower_count":count,"profile_url":format!("https://bsky.app/profile/{DID}")});
    let mut wrong = profile.clone();
    wrong["did"] = json!("did:plc:zzzzzzzzzzzzzzzzzzzzzzzz");
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
    let content = json!({"author_id":DID,"author_handle":"author.bsky.social","post_uri":format!("at://{DID}/app.bsky.feed.post/original"),"source_url":SOURCE,"text":"Original keyboard review","public_links":[],"comments":[]});
    for (key, value) in [
        ("author_id", json!("did:plc:zzzzzzzzzzzzzzzzzzzzzzzz")),
        (
            "post_uri",
            json!(format!("at://{DID}/app.bsky.feed.post/other")),
        ),
        ("comments", json!([{"text":"unreserved comments"}])),
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

async fn audience_good(owner: &CreatorRuntime, hit: &str) {
    super::language_tests::require_fresh_language_review(owner, CAMPAIGN, hit, 0).await;
    let claim = owner
        .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.into())
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
        author_id: DID.into(),
        source_url: SOURCE.into(),
        text: "Original keyboard review".into(),
        comments: quotes
            .iter()
            .enumerate()
            .map(|(i, text)| DiscoveryLanguageComment {
                source_url: format!("{SOURCE}#comment-at://{DID}/app.bsky.feed.post/reply{i}"),
                text: (*text).into(),
            })
            .collect(),
    };
    assert!(sample.valid_for(&claim));
    let mut wrong = sample.clone();
    wrong.comments[0].source_url = format!(
        "https://bsky.app/profile/{DID}/post/other#comment-at://{DID}/app.bsky.feed.post/reply0"
    );
    assert!(!wrong.valid_for(&claim));
    wrong = sample.clone();
    wrong.comments[1] = wrong.comments[0].clone();
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
        .prepare_discovery_review_material(CAMPAIGN, 1, hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: CAMPAIGN.into(),
                expected_revision: 1,
                search_hit_id: hit.into(),
                material_hash: material.material_hash,
                request_id: "bluesky-audience-good".into(),
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
            .claim_discovery_language_async(CAMPAIGN.into(), 1, hit.into())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn bluesky_native_author_comments_good_late_bio_contacts_library_and_restart() {
    let (dir, owner, hit) = setup(Some(15000), true);
    audience_good(&owner, &hit).await;
    let snapshot = owner
        .prepare_discovery_library_snapshot(CAMPAIGN, 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(snapshot.rows[0].contacts.len(), 2);
    assert!(
        snapshot.rows[0]
            .contacts
            .iter()
            .all(|c| c.source_url == format!("https://bsky.app/profile/{DID}"))
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

#[tokio::test]
async fn bluesky_good_without_contacts_stays_background_and_uses_observed_handle_for_lookup() {
    let (_dir, owner, hit) = setup(Some(15000), false);
    audience_good(&owner, &hit).await;
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
    assert!(
        owner
            .prepare_discovery_library_snapshot(CAMPAIGN, 1)
            .unwrap()
            .rows[0]
            .contacts
            .is_empty()
    );
    let step = owner.plan_discovery_step(CAMPAIGN, 1).unwrap();
    assert!(!matches!(step, DiscoveryStep::ReadContactLookup { .. }));
}

#[test]
fn bluesky_unknown_followers_cannot_be_replaced_by_contact_or_language_votes() {
    let (_dir, owner, hit) = setup(None, true);
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
                request_id: "bluesky-unknown-count".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "中文 keyboard review".into(),
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
