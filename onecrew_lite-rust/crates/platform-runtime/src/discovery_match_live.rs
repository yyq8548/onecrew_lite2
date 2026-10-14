//! Live provider reads against an explicitly supplied observed source, with
//! scoring/publication confined to a temporary Creator database.
use crate::{LocalPlatform, discovery_identity_test_support::authenticate};
use onecrew_creator_runtime::*;
use serde_json::json;

#[tokio::test]
#[ignore = "live observed Facebook profile/Reels and Codex scoring; isolated campaign and Library"]
async fn facebook_language_screening_stops_and_publishes() {
    scoring_case("facebook", true, false).await;
}

#[tokio::test]
#[ignore = "live TikTok profile and Codex scoring; isolated campaign, no customer writes"]
async fn tiktok_observed_profile_scores_and_publishes_without_unnecessary_comments() {
    scoring_case("tiktok", false, false).await;
}

#[tokio::test]
#[ignore = "live TikTok bounded language reads and Codex scoring; isolated criteria only"]
async fn tiktok_language_screening_stops_and_publishes() {
    scoring_case("tiktok", true, false).await;
}

#[tokio::test]
#[ignore = "live observed YouTube evidence and Codex scoring; isolated campaign and Library"]
async fn youtube_language_screening_stops_and_publishes() {
    scoring_case("youtube", true, false).await;
}

#[tokio::test]
#[ignore = "live user-supplied X post and Codex scoring; isolated campaign and Library"]
async fn twitter_language_screening_stops_and_publishes() {
    scoring_case("twitter", true, false).await;
}

#[tokio::test]
#[ignore = "live user-supplied X post and audience language; isolated bounded language screening and Library"]
async fn twitter_audience_language_screening_stops_and_publishes() {
    scoring_case("twitter", true, true).await;
}

#[tokio::test]
#[ignore = "live observed Reddit profile/content and Codex scoring; isolated campaign and Library"]
async fn reddit_observed_author_scores_and_publishes() {
    scoring_case("reddit", false, false).await;
}

#[tokio::test]
#[ignore = "live English-author YouTube source, Chinese audience criteria; isolated bounded language screening and Library"]
async fn youtube_audience_language_screening_stops_and_publishes() {
    scoring_case("youtube", true, true).await;
}

#[tokio::test]
#[ignore = "one Reddit language-learning search then bounded original/comment scoring; isolated criteria and Library"]
async fn reddit_audience_language_scores_and_publishes() {
    scoring_case("reddit", true, true).await;
}

async fn scoring_case(site: &str, language_case: bool, audience_case: bool) {
    let browser = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
    let contact_case = std::env::var("ONECREW_TEST_LIVE_CONTACT").as_deref() == Ok("true");
    let lookup_only = std::env::var("ONECREW_TEST_LOOKUP_STOP_AFTER").ok();
    let audience_language =
        std::env::var("OPENCLI_TEST_AUDIENCE_LANGUAGE").unwrap_or_else(|_| "中文".into());
    let audience_brief = format!(
        "独立语言验收：至少100粉丝，{audience_language}内容或{audience_language}受众；不限制内容品类"
    );
    let cli = onecrew_opencli::OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
    }
    let (source, username, mut author, author_url) = if site == "reddit" && audience_case {
        let hits = cli
            .creator_search(&browser, "reddit", "subreddit:Spanish English", 1)
            .await
            .unwrap();
        let hit = hits
            .into_iter()
            .next()
            .expect("actual Reddit source required");
        let author = hit.author_id.expect("search native author required");
        let author_url = hit.author_url.expect("search native homepage required");
        let username = author_url
            .strip_prefix("https://www.reddit.com/user/")
            .unwrap()
            .strip_suffix('/')
            .unwrap()
            .to_owned();
        (hit.source_url, username, author, author_url)
    } else if site == "reddit" {
        let rows = cli
            .native_comments(
                &browser,
                onecrew_opencli::NativeCommentSource::RedditPost("1vwpcv8"),
                1,
            )
            .await
            .unwrap();
        let post = &rows[0];
        let username = post["author"].as_str().unwrap().to_owned();
        (
            post["source_url"].as_str().unwrap().to_owned(),
            username.clone(),
            post["native_author_id"].as_str().unwrap().to_owned(),
            format!("https://www.reddit.com/user/{username}/"),
        )
    } else if site == "facebook" {
        let author = "61590270912364".to_owned();
        let profile = format!("https://www.facebook.com/profile.php?id={author}");
        (
            profile.clone(),
            "observed Facebook Reel owner".into(),
            author,
            profile,
        )
    } else if site == "youtube" {
        // Same observed public source as the OpenCLI language acceptance test.
        let author = std::env::var("ONECREW_TEST_YOUTUBE_AUTHOR")
            .unwrap_or_else(|_| "UCwAnu01qlnVg1Ai2AbtTMaA".to_owned());
        (
            std::env::var("ONECREW_TEST_YOUTUBE_SOURCE")
                .unwrap_or_else(|_| "https://www.youtube.com/watch?v=htZRCE2GgIs".to_owned()),
            std::env::var("ONECREW_TEST_YOUTUBE_LABEL").unwrap_or_else(|_| "Jeff Su".to_owned()),
            author.clone(),
            format!("https://www.youtube.com/channel/{author}"),
        )
    } else if site == "twitter" {
        let source = std::env::var("OPENCLI_TEST_X_SOURCE")
            .unwrap_or_else(|_| "https://x.com/1484_god/status/2106744727542591670".into());
        let handle = source
            .strip_prefix("https://x.com/")
            .and_then(|path| path.split_once("/status/"))
            .map(|(handle, _)| handle.to_owned())
            .expect("an explicitly observed native X post URL is required");
        (
            source,
            handle.clone(),
            handle.clone(),
            format!("https://x.com/{handle}"),
        )
    } else {
        let username = std::env::var("OPENCLI_TEST_TIKTOK_USERNAME").unwrap();
        (
            std::env::var("OPENCLI_TEST_TIKTOK_SOURCE").unwrap(),
            username.clone(),
            std::env::var("OPENCLI_TEST_TIKTOK_AUTHOR").unwrap(),
            format!("https://www.tiktok.com/@{username}"),
        )
    };
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    authenticate(&platform, "fixture-operator");
    let owner = platform.creators().unwrap();
    let campaign = "live-material-acceptance";
    let (brief, minimum, category, language, budget) = if site == "facebook" {
        // The native profile can omit an exact follower count. This isolated
        // acceptance does not relax a customer campaign's follower requirement.
        (
            "独立 Facebook 验收：粉丝数不限，英语内容；不限制内容品类",
            0,
            "不限",
            "英语",
            8,
        )
    } else if site == "twitter" && !audience_case {
        (
            "独立语言验收：至少100粉丝，日语内容或日语受众；不限制内容品类",
            100,
            "不限",
            "日语",
            8,
        )
    } else if audience_case && site == "reddit" {
        (
            "独立评论语言验收：粉丝数不限，西班牙语内容或西班牙语受众；不限制内容品类",
            0,
            "不限",
            "西班牙语",
            8,
        )
    } else if audience_case {
        (
            audience_brief.as_str(),
            100,
            "不限",
            audience_language.as_str(),
            if site == "twitter" { 9 } else { 8 },
        )
    } else if language_case {
        (
            "独立语言验收：至少100粉丝，英语内容或英语受众；不限制内容品类",
            100,
            "不限",
            "英语",
            8,
        )
    } else {
        (
            "寻找发布中文洗护内容的博主，至少8000粉丝",
            8000,
            "洗护",
            "中文",
            3,
        )
    };
    let requested_language = std::env::var("ONECREW_TEST_CONTACT_LANGUAGE").ok();
    let language = requested_language.as_deref().unwrap_or(language);
    let contact_brief = format!(
        "独立联系方式验收：至少{minimum}粉丝，{language}内容；不限制内容品类。必要条件与评分通过后才补查联系方式。"
    );
    let brief = if contact_case {
        contact_brief.as_str()
    } else {
        brief
    };
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id:campaign.into(), brief:brief.into(),opencli_version:"1.8.8".into(),
        criteria:json!({"min_followers":minimum,"category":category,"language":language,"region":"","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes:vec![DiscoveryPlatformScope {platform:site.into(),profile_ref:browser.clone(),keywords:vec!["acceptance".into()]}],target_count:1,max_calls:Some(if contact_case { 12 } else { budget }),
    }).unwrap();
    owner.approve_discovery_campaign(campaign, 1).unwrap();
    let search = owner
        .claim_discovery_search(campaign, 1, site, &browser)
        .unwrap();
    // Replay a previously observed locator, not a claim that a fresh search was
    // made. No synthetic caption is supplied to the evaluator.
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                title: source.clone(),
                source_url: source,
                author_label: username.clone(),
                author_id: Some(author.clone()),
                author_url: Some(author_url),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: campaign.into(),
            after_id: None,
            limit: 1,
        })
        .unwrap()
        .remove(0);
    if site == "youtube" {
        platform
            .verify_discovery_youtube_hit_once(campaign, 1, &browser, &hit.id)
            .await
            .unwrap();
    } else {
        if site == "twitter" {
            let identity = platform
                .read_discovery_identity_once(campaign, 1, &hit.id)
                .await
                .unwrap();
            assert_eq!(identity.status, "completed");
            author = identity.account_id.expect("observed native X account ID");
        }
        let read = platform
            .read_discovery_profile_once(campaign, 1, &hit.id)
            .await
            .unwrap();
        assert_eq!(
            read.status, "completed",
            "profile stop: {:?}",
            read.stop_code
        );
        if site == "facebook" {
            assert!(matches!(
                owner.plan_discovery_step(campaign, 1).unwrap(),
                DiscoveryStep::ReadContentTargets { .. }
            ));
            let targets = platform
                .advance_discovery_campaign(crate::opencli::OpenCliDiscoveryAdvanceInput {
                    campaign_id: campaign.into(),
                    expected_revision: 1,
                    excluded_platforms: vec![],
                })
                .await
                .unwrap();
            assert_eq!(targets.content_targets.unwrap().status, "completed");
        }
        if matches!(site, "twitter" | "reddit" | "facebook") {
            let content = platform
                .read_discovery_content_once(campaign, 1, &hit.id)
                .await
                .unwrap();
            assert_eq!(
                content.status, "completed",
                "content stop: {:?}",
                content.stop_code
            );
        }
    }
    let (material, followers, _) = owner
        .prepare_initial_match_material_async(campaign.into(), 1, hit.id.clone())
        .await
        .unwrap();
    assert_eq!(material.native_author_id, author);
    assert!(if language_case {
        followers.is_some_and(|count| count >= minimum) || minimum == 0
    } else {
        followers.is_some_and(|count| count < 8000)
    });
    let mut receipt = platform
        .evaluate_discovery_match(DiscoveryReviewMaterialInput {
            campaign_id: campaign.into(),
            expected_revision: 1,
            search_hit_id: hit.id.clone(),
        })
        .await
        .unwrap()
        .unwrap();
    let mut sources_exhausted = false;
    if language_case {
        for ordinal in 1..=3 {
            let Some(claim) = owner
                .claim_discovery_language_async(campaign.into(), 1, hit.id.clone())
                .await
                .unwrap()
            else {
                break;
            };
            let read = platform.read_discovery_language(claim).await.unwrap();
            if read.stop_code.as_deref() == Some("language_sources_exhausted") {
                assert_eq!(read.status, "failed");
                sources_exhausted = true;
                break;
            }
            assert_eq!(
                read.status, "completed",
                "language stop: {:?}",
                read.stop_code
            );
            receipt = platform
                .evaluate_discovery_match(DiscoveryReviewMaterialInput {
                    campaign_id: campaign.into(),
                    expected_revision: 1,
                    search_hit_id: hit.id.clone(),
                })
                .await
                .unwrap()
                .unwrap();
            println!(
                "{site} language sample={ordinal}; verdict={}",
                receipt.verdict
            );
        }
        if !audience_case {
            assert_eq!(
                receipt.verdict, "qualified",
                "real language evidence did not qualify this test source"
            );
        }
    }
    if contact_case {
        assert_eq!(
            receipt.verdict, "qualified",
            "contact acceptance requires a real passing score"
        );
        // Follow only owner-planned, observed URLs; never invent a contact or URL.
        let mut lookup_methods = vec![];
        for _ in 0..8 {
            let step = owner.plan_discovery_step(campaign, 1).unwrap();
            if !matches!(
                step,
                DiscoveryStep::ReadPublicPage { .. } | DiscoveryStep::ReadContactLookup { .. }
            ) {
                break;
            }
            let result = platform
                .advance_discovery_campaign(crate::opencli::OpenCliDiscoveryAdvanceInput {
                    campaign_id: campaign.into(),
                    expected_revision: 1,
                    excluded_platforms: vec![],
                })
                .await
                .unwrap();
            if let Some(lookup) = result.contact_lookup {
                println!(
                    "contact lookup: method={}, status={}, stop={:?}, links={:?}",
                    lookup.method, lookup.status, lookup.stop_code, lookup.links
                );
                lookup_methods.push(lookup.method);
                assert_eq!(
                    lookup.status, "completed",
                    "stop at provider restriction without retry"
                );
                if lookup_only.as_deref() == lookup_methods.last().map(String::as_str) {
                    break;
                }
                continue;
            }
            let page = result.public_page.expect("planned contact page read");
            println!(
                "contact page: status={}, stop={:?}",
                page.status, page.stop_code
            );
            if page.status == "failed" {
                assert!(
                    matches!(
                        page.stop_code.as_deref(),
                        Some(
                            "opencli_risk_control"
                                | "opencli_auth_required"
                                | "opencli_output_too_large"
                                | "opencli_command_failed"
                                | "opencli_timeout"
                                | "opencli_invalid_output"
                                | "opencli_invalid_input"
                        )
                    ),
                    "shared dependency failure stops acceptance"
                );
            } else {
                assert_eq!(page.status, "completed");
            }
        }
        if let Ok(required) = std::env::var("ONECREW_TEST_REQUIRE_CONTACT_LOOKUP") {
            assert!(
                lookup_methods.contains(&required),
                "requested live lookup was not exercised; existing source contacts are a different acceptance"
            );
        }
    }
    let used = owner.get_discovery_campaign(campaign).unwrap().used_calls;
    let publication = owner
        .prepare_discovery_library_publication(campaign, 1)
        .unwrap();
    let row = publication
        .material
        .sources
        .rows
        .iter()
        .find(|r| r.hit.id == hit.id)
        .unwrap();
    assert_eq!(row.native_author_id.as_deref(), Some(author.as_str()));
    assert_eq!(row.verdict, receipt.verdict);
    if contact_case && lookup_only.is_none() {
        assert!(
            !row.contacts.is_empty(),
            "real observed source did not provide a contact; do not fabricate one"
        );
        assert_eq!(
            owner
                .count_discovery_candidates(campaign)
                .unwrap()
                .qualified,
            1
        );
        println!(
            "real source -> passing score -> {} source-bound contacts -> recommended PASS",
            row.contacts.len()
        );
        println!(
            "observed contact evidence: {}",
            serde_json::to_string(&row.contacts).unwrap()
        );
    }
    if lookup_only.is_some() {
        assert!(
            row.contacts.is_empty(),
            "lookup URLs alone must not create contacts"
        );
        assert_eq!(
            owner
                .count_discovery_candidates(campaign)
                .unwrap()
                .qualified,
            0
        );
        println!(
            "provider-only lookup acceptance; qualified=0; does not prove a positive contact/table chain"
        );
    }
    if !language_case {
        assert_ne!(row.verdict, "qualified");
    }
    let rating = row.match_rating.as_ref();
    if audience_case && receipt.verdict == "pending" {
        assert_eq!(
            receipt.verdict, "pending",
            "unqualified audience test must remain pending"
        );
        assert!(
            receipt
                .judgments
                .iter()
                .any(|j| j.criterion == DiscoveryReviewCriterion::ContentLanguage
                    && j.outcome == DiscoveryReviewOutcome::Insufficient),
            "saved receipt must request language evidence"
        );
        assert!(
            rating.is_none(),
            "unknown language must not receive a supported score"
        );
    } else if audience_case && receipt.verdict == "rejected" {
        assert_eq!(
            rating
                .expect("confirmed language mismatch must have a saved rating")
                .level,
            DiscoveryMatchLevel::Low
        );
    } else if language_case {
        let rating = rating.expect("qualified source requires a persisted rating");
        assert!(matches!(
            rating.level,
            DiscoveryMatchLevel::Good | DiscoveryMatchLevel::Excellent
        ));
    } else {
        let rating = rating.expect("confirmed follower failure is rated, not unknown");
        assert_eq!(rating.level, DiscoveryMatchLevel::Low);
    }
    assert!(
        owner
            .claim_discovery_language_async(campaign.into(), 1, hit.id.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner.get_discovery_campaign(campaign).unwrap().used_calls,
        used
    );
    let samples = owner
        .prepare_discovery_review_material(campaign, 1, &hit.id)
        .unwrap()
        .language_samples;
    println!(
        "{site} observed acceptance: verdict={}, rating={:?}, language_samples={}, requested_language={language}",
        receipt.verdict,
        rating.map(|value| value.level),
        samples.len()
    );
    assert!(samples.len() <= 3);
    if audience_case {
        if std::env::var("OPENCLI_TEST_EXPECT_AUDIENCE_GOOD").as_deref() == Ok("true") {
            assert_eq!(
                receipt.verdict, "qualified",
                "positive audience acceptance requires actual qualification, not a pending sample"
            );
        }
        if site == "reddit" {
            assert_eq!(
                receipt.verdict, "qualified",
                "Reddit audience-only positive acceptance requires actual qualification"
            );
        }
        assert!(
            !samples.is_empty(),
            "audience case must actually exercise native language supplement"
        );
        if receipt.verdict == "qualified" {
            let rating = rating.unwrap();
            assert_eq!(
                rating.level,
                DiscoveryMatchLevel::Good,
                "audience-only evidence cannot justify Excellent"
            );
            assert!(
                rating.reason.contains("粉丝评论语言符合"),
                "audience language rationale must be explicit"
            );
        } else if receipt.verdict == "pending" {
            assert!(
                samples.len() == 3 || sources_exhausted,
                "language-only unknown stops after three samples or validated exhaustion of distinct originals"
            );
        } else {
            assert_eq!(receipt.verdict, "rejected");
            assert!(
                rating.unwrap().reason.contains(language)
                    && rating.unwrap().reason.contains("英文"),
                "Low language rationale must identify observed and requested languages"
            );
        }
    }
    if !language_case {
        assert!(samples.is_empty());
    }
    if std::env::var_os("ONECREW_TEST_LIBRARY_URL").is_some() {
        platform.assert_test_library_publication(&publication).await;
        assert_eq!(
            owner.get_discovery_campaign(campaign).unwrap().used_calls,
            used
        );
    }
    println!(
        "{site} live followers={followers:?}; rating={:?}; language_samples={}; sources_exhausted={sources_exhausted}; Library publication preserves native author; calls={used}",
        rating.map(|rating| rating.level),
        samples.len()
    );
}
