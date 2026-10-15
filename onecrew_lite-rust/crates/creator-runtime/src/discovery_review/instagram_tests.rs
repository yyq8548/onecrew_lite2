use crate::*;
use serde_json::json;

fn setup_count(followers: Option<u64>) -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_profile(followers, false)
}

fn setup_profile(
    followers: Option<u64>,
    public_contacts: bool,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_records(followers, public_contacts, public_contacts, true)
}

fn setup_records(
    followers: Option<u64>,
    public_email: bool,
    public_link: bool,
    post_contact: bool,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_evidence(
        followers,
        public_email,
        public_link,
        post_contact,
        true,
        false,
    )
}

fn setup_evidence(
    followers: Option<u64>,
    public_email: bool,
    public_link: bool,
    post_contact: bool,
    content: bool,
    web: bool,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    setup_visibility(
        followers,
        public_email,
        public_link,
        post_contact,
        content,
        web,
        false,
    )
}

fn setup_visibility(
    followers: Option<u64>,
    public_email: bool,
    public_link: bool,
    post_contact: bool,
    content: bool,
    web: bool,
    private: bool,
) -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id: "instagram-flow".into(), brief: "Keyboard creators".into(), opencli_version: "1.8.8".into(),
        criteria: if web { json!({"min_followers":10000}) } else { json!({"min_followers":10000,"language":"中文","requires_ai_tool_demo":false,"requires_public_tool_url":false}) },
        scopes: vec![DiscoveryPlatformScope { platform:"instagram".into(), profile_ref:"dedicated".into(), keywords:vec!["keyboard".into()] }],
        target_count:2, max_calls:Some(20),
    }).unwrap();
    owner
        .approve_discovery_campaign("instagram-flow", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("instagram-flow", 1, "instagram", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.instagram.com/keyboard_creator/".into(),
                title: "Keyboards".into(),
                author_label: "Keyboard Creator".into(),
                author_id: Some("12345".into()),
                author_url: Some("https://www.instagram.com/keyboard_creator/".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "instagram-flow".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0)
        .id;
    let claim = owner
        .claim_discovery_profile("instagram-flow", 1, &hit)
        .unwrap();
    let mut profile = json!({
        "adapter_version":"1.0.0","id":"12345","username":"keyboard_creator","display_name":"Keyboard Creator","biography":"Keyboard reviews","follower_count":followers,"private_account":false,"verified":false,"profile_url":"https://www.instagram.com/keyboard_creator/","evidence_source":"native_profile_response"
    });
    if web {
        profile["evidence_source"] = json!("public_profile_dom_search_bound");
        profile["web_text"] = json!("Public profile preview: keyboard review preview");
        profile["private_account"] = json!(null);
        profile["verified"] = json!(null);
    }
    if private {
        profile["private_account"] = json!(true);
    }
    if public_email {
        profile["public_email"] = json!("public@example.com");
    }
    if public_link {
        profile["contact_links"] = json!(["https://creator.example/contact"]);
    }
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim,
            stop_code: None,
            native_profile: Some(profile),
        })
        .unwrap();
    if content {
        let claim = owner
            .claim_discovery_content("instagram-flow", 1, &hit)
            .unwrap();
        owner.finish_discovery_content(FinishDiscoveryContentInput {claim,stop_code:None,native_content:Some(json!({
        "post_id":"98765","author_id":"12345","source_url":"https://www.instagram.com/p/KeyBoard1/","text":if post_contact { "Original keyboard review. Contact: keyboard@example.com" } else { "Original keyboard review." },"text_truncated":false
    }))}).unwrap();
    }
    (dir, owner, hit)
}

#[test]
fn instagram_private_profile_is_rejected_before_review_or_content() {
    for web in [false, true] {
        let (_dir, owner, hit) = setup_visibility(Some(15000), true, true, false, false, web, true);
        let counts = owner.count_discovery_candidates("instagram-flow").unwrap();
        assert_eq!(counts.rejected, 1);
        assert_eq!(counts.qualified, 0);
        assert!(
            owner
                .prepare_discovery_review_material("instagram-flow", 1, &hit)
                .unwrap_err()
                .contains("review_source_unavailable")
        );
        assert!(
            owner
                .claim_discovery_content("instagram-flow", 1, &hit)
                .is_err()
        );
        let candidate = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "instagram-flow".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap()
            .remove(0);
        assert_eq!(
            candidate.profile_url,
            "https://www.instagram.com/keyboard_creator/"
        );
        assert_eq!(
            candidate.evidence["assessment"]["reasons"],
            json!(["private_account"])
        );
        assert!(candidate.evidence["assessment"]["match_rating"].is_null());
    }
}

#[test]
fn instagram_unknown_followers_cannot_be_replaced_by_score_or_contact() {
    let (_dir, owner, hit) = setup_count(None);
    let material = owner
        .prepare_discovery_review_material("instagram-flow", 1, &hit)
        .unwrap();
    assert_eq!(material.native_followers, None);
    let error = owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "instagram-flow".into(),
                expected_revision: 1,
                search_hit_id: hit,
                material_hash: material.material_hash,
                request_id: "unknown-followers".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    rationale: "Keyboard review matches.".into(),
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
            .count_discovery_candidates("instagram-flow")
            .unwrap()
            .qualified,
        0
    );
}

#[tokio::test]
async fn instagram_bound_comments_score_and_contacts_survive_reopen() {
    verify_contacts(false, false).await;
}

#[tokio::test]
async fn instagram_public_email_separate_from_bio_enters_recommendations() {
    verify_contacts(true, false).await;
}

#[tokio::test]
async fn instagram_native_profile_contact_qualifies_without_official_page_lookup() {
    verify_contacts(false, true).await;
}

async fn verify_contacts(public_contacts: bool, link_only: bool) {
    let (dir, owner, hit) = if link_only {
        setup_records(Some(15000), false, true, false)
    } else {
        setup_profile(Some(15000), public_contacts)
    };
    super::language_tests::require_fresh_language_review(&owner, "instagram-flow", &hit, 0).await;
    let claim = owner
        .claim_discovery_language_async("instagram-flow".into(), 1, hit.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.root_url, "https://www.instagram.com/p/KeyBoard1/");
    assert_eq!(claim.ordinal, 1);
    let quotes = [
        "这个键盘的手感不错",
        "谢谢你分享机械键盘测评",
        "我也想试试这款键盘",
    ];
    let sample = DiscoveryLanguageSample {
        author_id: "12345".into(),
        source_url: claim.root_url.clone(),
        text: if link_only {
            "Original keyboard review."
        } else {
            "Original keyboard review. Contact: keyboard@example.com"
        }
        .into(),
        comments: quotes
            .into_iter()
            .enumerate()
            .map(|(i, text)| DiscoveryLanguageComment {
                source_url: format!("https://www.instagram.com/p/KeyBoard1/c/{}/", i + 1),
                text: text.into(),
            })
            .collect(),
    };
    assert!(sample.valid_for(&claim));
    let mut wrong = sample.clone();
    wrong.comments[0].source_url = "https://www.instagram.com/p/OtherPost/c/1/".into();
    assert!(!wrong.valid_for(&claim));
    wrong = sample.clone();
    wrong.author_id = "55555".into();
    assert!(!wrong.valid_for(&claim));
    owner
        .finish_discovery_language_async(claim, Some(sample), None)
        .await
        .unwrap();
    let material = owner
        .prepare_discovery_review_material("instagram-flow", 1, &hit)
        .unwrap();
    assert_eq!(material.format_version, 2);
    assert_eq!(material.native_followers, Some(15000));
    assert_eq!(
        owner
            .count_discovery_candidates("instagram-flow")
            .unwrap()
            .qualified,
        0,
        "A native profile contact still requires an audited match score"
    );
    owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "instagram-flow".into(),
                expected_revision: 1,
                search_hit_id: hit.clone(),
                material_hash: material.material_hash,
                request_id: "instagram-audience-score".into(),
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
            .claim_discovery_language_async("instagram-flow".into(), 1, hit.clone())
            .await
            .unwrap()
            .is_none()
    );
    if link_only {
        assert_eq!(
            owner
                .count_discovery_candidates("instagram-flow")
                .unwrap()
                .qualified,
            1
        );
        assert!(
            owner
                .repository
                .next_discovery_public_page("instagram-flow", &[])
                .unwrap()
                .is_none()
        );
        assert!(
            owner
                .repository
                .next_discovery_contact_lookup("instagram-flow", &[])
                .unwrap()
                .is_none()
        );
    }
    let snapshot = owner
        .prepare_discovery_library_snapshot("instagram-flow", 1)
        .unwrap();
    assert_eq!(snapshot.rows[0].verdict, "qualified");
    assert_eq!(snapshot.rows[0].followers.as_deref(), Some("15000"));
    assert_eq!(
        snapshot.rows[0].contacts.len(),
        if public_contacts {
            3
        } else if link_only {
            1
        } else {
            2
        }
    );
    let profile = snapshot.rows[0]
        .contacts
        .iter()
        .find(|contact| contact.kind == DiscoveryContactKind::SocialProfile)
        .unwrap();
    assert_eq!(
        profile.normalized_value,
        "https://www.instagram.com/keyboard_creator/"
    );
    assert_eq!(
        profile.source_url,
        "https://www.instagram.com/keyboard_creator/"
    );
    assert_eq!(profile.native_author_id, "12345");
    assert_eq!(
        profile.attribution,
        DiscoveryContactAttribution::AuthorPublished
    );
    if link_only {
        assert!(
            owner
                .claim_discovery_public_page(DiscoveryPublicPageInput {
                    campaign_id: "instagram-flow".into(),
                    expected_revision: 1,
                    profile_ref: "dedicated".into(),
                    candidate_id: snapshot.rows[0].candidate_id.clone().unwrap(),
                    requested_url: "https://unrelated.example/contact".into(),
                })
                .is_err()
        );
    }
    if public_contacts {
        assert!(
            snapshot.rows[0]
                .contacts
                .iter()
                .any(|contact| contact.normalized_value == "public@example.com"
                    && contact.source_url == "https://www.instagram.com/keyboard_creator/")
        );
    }
    if !link_only {
        assert_eq!(
            snapshot.rows[0]
                .contacts
                .iter()
                .find(|contact| contact.kind == DiscoveryContactKind::Email
                    && contact.normalized_value == "keyboard@example.com")
                .unwrap()
                .source_url,
            "https://www.instagram.com/p/KeyBoard1/"
        );
    }
    assert_eq!(
        owner
            .count_discovery_candidates("instagram-flow")
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(
        owner
            .get_discovery_campaign("instagram-flow")
            .unwrap()
            .used_calls,
        4
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot("instagram-flow", 1)
            .unwrap()
            .rows[0]
            .contacts,
        snapshot.rows[0].contacts
    );
    assert_eq!(
        reopened
            .count_discovery_candidates("instagram-flow")
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(
        reopened
            .prepare_discovery_review_material("instagram-flow", 1, &hit)
            .unwrap()
            .language_samples
            .len(),
        1
    );
}

#[test]
fn instagram_web_profile_only_material_keeps_insufficient_pending_without_score() {
    for (followers, outcome, level, verdict) in [
        (None, DiscoveryReviewOutcome::Insufficient, None, "pending"),
        (
            Some(15000),
            DiscoveryReviewOutcome::Supported,
            Some(DiscoveryMatchLevel::Good),
            "qualified",
        ),
    ] {
        let (_dir, owner, hit) = setup_evidence(followers, false, false, false, false, true);
        let material = owner
            .prepare_discovery_review_material("instagram-flow", 1, &hit)
            .unwrap();
        let profile = material.native_profile.as_ref().unwrap();
        assert_eq!(
            profile.profile_url,
            "https://www.instagram.com/keyboard_creator/"
        );
        assert!(
            profile
                .web_text
                .as_ref()
                .unwrap()
                .contains("keyboard review preview")
        );
        assert!(material.native_content.is_none());
        let receipt = owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "instagram-flow".into(),
                expected_revision: 1,
                search_hit_id: hit,
                material_hash: material.material_hash,
                request_id: "web-insufficient".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome,
                    rationale: if followers.is_some() { "Visible keyboard review preview and observed followers support initial topic fit." } else { "Missing required follower evidence; retain pending without a fabricated score." }.into(),
                    citations: vec![DiscoveryReviewQuote::NativeProfile {
                        profile_url: profile.profile_url.clone(),
                        quote: "keyboard review preview".into(),
                    }],
                    tool: None,
                    country_code: None,
                    match_level: level,
                }],
            },
        )
        .unwrap();
        assert_eq!(receipt.verdict, verdict);
        if verdict == "pending" {
            assert_eq!(receipt.reasons, vec!["initial_match_unrated"]);
        }
        assert_eq!(
            owner
                .count_discovery_candidates("instagram-flow")
                .unwrap()
                .qualified,
            if verdict == "qualified" { 1 } else { 0 }
        );
    }
}
