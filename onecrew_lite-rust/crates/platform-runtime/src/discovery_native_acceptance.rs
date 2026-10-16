//! Explicit bounded live acceptance. Provider evidence and native model scoring
//! compose in a disposable owner; neither customer campaigns nor tables are used.
use crate::{LocalPlatform, discovery_identity_test_support::authenticate};
use onecrew_creator_runtime::*;
use onecrew_opencli::OpenCli;
use serde_json::json;
use std::time::Instant;

fn safe_advance_error(error: &crate::opencli::OpenCliDiscoverySearchError) -> &'static str {
    use crate::opencli::OpenCliDiscoverySearchError;
    match error {
        OpenCliDiscoverySearchError::Dependency(error)
        | OpenCliDiscoverySearchError::Provider(error) => crate::opencli::provider_stop_code(error),
        OpenCliDiscoverySearchError::Campaign(_) => "campaign_error",
        OpenCliDiscoverySearchError::Advance { failure, .. } => safe_advance_error(failure),
    }
}

fn stage(step: &DiscoveryStep) -> &'static str {
    match step {
        DiscoveryStep::ReadVideo { .. } => "video",
        DiscoveryStep::ReadTranscript { .. } => "transcript",
        DiscoveryStep::ReadProfile { .. } => "profile",
        DiscoveryStep::ReadIdentity { .. } => "identity",
        DiscoveryStep::ReadFacebookIdentity { .. } => "facebook_identity",
        DiscoveryStep::ReadWebSource { .. } => "web_source",
        DiscoveryStep::ReadSourceAuthor { .. } => "source_author",
        DiscoveryStep::ReadContent { .. } => "content_or_language",
        DiscoveryStep::ReadContentTargets { .. } => "content_targets",
        DiscoveryStep::ReadPublicPage { .. } => "public_page",
        DiscoveryStep::ReadContactLookup { lookup } => match lookup.method.as_str() {
            "search" => "contact_search",
            "maigret" => "contact_maigret",
            _ => "contact_lookup",
        },
        _ => "terminal",
    }
}

fn native_read(step: &DiscoveryStep) -> bool {
    matches!(
        step,
        DiscoveryStep::ReadVideo { .. }
            | DiscoveryStep::ReadTranscript { .. }
            | DiscoveryStep::ReadProfile { .. }
            | DiscoveryStep::ReadIdentity { .. }
            | DiscoveryStep::ReadFacebookIdentity { .. }
            | DiscoveryStep::ReadWebSource { .. }
            | DiscoveryStep::ReadSourceAuthor { .. }
            | DiscoveryStep::ReadContent { .. }
            | DiscoveryStep::ReadContentTargets { .. }
    )
}

struct NativeAcceptanceRequirements {
    request: String,
    category: String,
    min_subscribers: u64,
}

impl NativeAcceptanceRequirements {
    fn from_environment() -> Self {
        let request = std::env::var("ONECREW_TEST_NATIVE_REQUEST")
            .expect("explicit customer request required for relevance acceptance");
        assert!(
            !request.trim().is_empty(),
            "customer request must not be empty"
        );
        let category = std::env::var("ONECREW_TEST_NATIVE_CATEGORY")
            .expect("explicit content category; use an empty value only for unrestricted QA");
        let min_subscribers = std::env::var("ONECREW_TEST_NATIVE_MIN_SUBSCRIBERS")
            .expect("explicit follower threshold required")
            .parse::<u64>()
            .expect("follower threshold must be an unsigned integer");
        Self {
            request,
            category,
            min_subscribers,
        }
    }

    fn criteria(&self, language: &str) -> serde_json::Value {
        json!({"min_subscribers":self.min_subscribers,"category":self.category,
            "language":language,"region":"","requires_ai_tool_demo":false,
            "requires_public_tool_url":false})
    }
}

#[test]
fn native_acceptance_preserves_customer_requirements() {
    let requirements = NativeAcceptanceRequirements {
        request: "找粉丝数不少于5000的美妆博主".into(),
        category: "美妆".into(),
        min_subscribers: 5000,
    };
    let directory = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let saved = owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "acceptance-requirements".into(),
            brief: requirements.request.clone(),
            opencli_version: "1.8.8".into(),
            criteria: requirements.criteria(""),
            scopes: vec![DiscoveryPlatformScope {
                platform: "instagram".into(),
                profile_ref: "browser".into(),
                keywords: vec!["beauty creator".into()],
            }],
            target_count: 1,
            max_calls: Some(20),
        })
        .unwrap();
    assert_eq!(saved.brief, requirements.request);
    assert_eq!(saved.criteria["category"], "美妆");
    assert_eq!(saved.criteria["min_subscribers"], 5000);
}

#[tokio::test]
#[ignore = "one explicitly selected live entrance, native model scoring, contact and isolated Library/export acceptance"]
async fn actual_native_source_contact_recommendation_and_restart() {
    let requirements = NativeAcceptanceRequirements::from_environment();
    let site = std::env::var("ONECREW_TEST_NATIVE_ENTRANCE").expect("explicit entrance");
    // Pixiv's outstanding policy denial must not be routed around by this harness.
    assert!(
        [
            "youtube",
            "tiktok",
            "instagram",
            "twitter",
            "bilibili",
            "reddit",
            "weibo",
            "zhihu",
            "facebook",
            "pinterest",
            "linkedin",
            "douyin",
            "google",
            "yahoo"
        ]
        .contains(&site.as_str())
    );
    let browser = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit connected identity");
    let query = std::env::var("ONECREW_TEST_NATIVE_QUERY").expect("explicit QA query");
    let language = std::env::var("ONECREW_TEST_NATIVE_LANGUAGE").expect("explicit QA language");
    let stop_before_external_search =
        std::env::var("ONECREW_TEST_NATIVE_STOP_BEFORE_EXTERNAL_SEARCH").as_deref() == Ok("true");
    let expected_outcome =
        std::env::var("ONECREW_TEST_NATIVE_OUTCOME").unwrap_or_else(|_| "contact".into());
    assert!(matches!(
        expected_outcome.as_str(),
        "contact" | "no_contact" | "hidden_email" | "social_account"
    ));
    let expected_profile = std::env::var("ONECREW_TEST_NATIVE_PROFILE").ok();
    let expected_label = std::env::var("ONECREW_TEST_NATIVE_AUTHOR_LABEL").ok();
    let library = std::env::var("ONECREW_TEST_LIBRARY_URL")
        .expect("disposable receiver launcher required before any provider spending");
    let library = url::Url::parse(&library).unwrap();
    assert_eq!(library.scheme(), "http");
    assert_eq!(library.host_str(), Some("127.0.0.1"));
    for key in [
        "ONECREW_TEST_LIBRARY_TOKEN",
        "ONECREW_TEST_LIBRARY_EXPORT",
        "ONECREW_TEST_NATIVE_EXPECTATIONS",
    ] {
        assert!(
            !std::env::var(key)
                .expect("disposable receiver launcher required")
                .is_empty()
        );
    }
    let cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("ONECREW_TEST_NATIVE_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true)
            .await
            .expect("explicit QA addon repair with backup");
    }
    let cli = OpenCli::inspect_system().await.unwrap();
    if site == "bilibili" {
        let addons = cli.addon_status().await;
        for required in [
            "onecrew-bilibili",
            "onecrew-bilibili-profile",
            "onecrew-bilibili-video",
        ] {
            let state = addons
                .iter()
                .find(|addon| addon.id == required)
                .expect("bundled Bilibili dependency");
            println!("Native acceptance local dependency; {}", json!(state));
            assert_eq!(
                state.state,
                onecrew_opencli::OpenCliAddonState::Ready,
                "local reader must match the test build before a paid search"
            );
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    authenticate(&platform, "fixture-operator");
    let owner = platform.creators().unwrap();
    let campaign = format!("native-contact-{}", uuid::Uuid::new_v4());
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: campaign.clone(),
            brief: requirements.request.clone(),
            opencli_version: cli.inventory().version,
            criteria: requirements.criteria(&language),
            scopes: vec![DiscoveryPlatformScope {
                platform: site.clone(),
                profile_ref: browser.clone(),
                keywords: vec![query.clone()],
            }],
            target_count: 1,
            max_calls: Some(20),
        })
        .unwrap();
    owner.approve_discovery_campaign(&campaign, 1).unwrap();
    let claim = owner
        .claim_discovery_search(&campaign, 1, &site, &browser)
        .unwrap();
    assert_eq!(claim.query, query);
    let selected = if let Ok(audit_path) = std::env::var("ONECREW_TEST_NATIVE_SAVED_BATCH_AUDIT") {
        // Replay one previously captured search locator, not a new search or a
        // fabricated identity. All profile/content/model/contact reads remain live.
        assert!(matches!(
            site.as_str(),
            "facebook"
                | "douyin"
                | "bilibili"
                | "yahoo"
                | "twitter"
                | "youtube"
                | "linkedin"
                | "zhihu"
        ));
        let audit: serde_json::Value =
            serde_json::from_slice(&std::fs::read(audit_path).unwrap()).unwrap();
        let row = audit["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["platform"] == site && row["query"] == query)
            .unwrap();
        let source = row["source_url"].as_str().unwrap();
        assert_eq!(
            source,
            if site == "zhihu" {
                "https://www.zhihu.com/question/50536485/answer/935588883"
            } else if site == "linkedin" {
                "https://www.linkedin.com/in/ashleymarsh-conexo/"
            } else if site == "facebook" {
                "https://www.facebook.com/LinusTech"
            } else if site == "douyin" {
                "https://www.douyin.com/video/7690187248574254386"
            } else if site == "bilibili" {
                "https://www.bilibili.com/video/BV1BG4y137mG"
            } else if site == "twitter" {
                "https://x.com/i/status/1833242618442203138"
            } else if site == "youtube" {
                "https://www.youtube.com/watch?v=tAk1hBnurFo&pp=ygUMUmVpZCBXaWxraW5n"
            } else {
                "https://www.youtube.com/watch/uOLqPKuO2Bo"
            }
        );
        if site == "zhihu" {
            assert_eq!(row["author_id"], "d47261bc59975371e180e1eac6e75c12");
            assert_eq!(
                row["author_locator"],
                "https://www.zhihu.com/people/yizhimadingxue"
            );
            assert_eq!(row["author_label"], "陈一丁Dingslook");
        } else if site == "linkedin" {
            assert_eq!(row["author_id"], "ashleymarsh-conexo");
            assert_eq!(row["author_locator"], source);
            assert_eq!(row["author_label"], "Ashley Marsh");
        } else if site == "bilibili" {
            assert_eq!(row["author_id"], "14871346");
            assert_eq!(row["author_locator"], "https://space.bilibili.com/14871346");
            assert_eq!(row["author_label"], "硬件茶谈");
        } else if site == "twitter" {
            // The saved search returned a handle label, not the display name.
            // Native identity/profile reads still establish the numeric owner.
            assert_eq!(row["author_id"], "mkbhd");
            assert_eq!(row["author_locator"], "https://x.com/mkbhd");
            assert_eq!(row["author_label"], "MKBHD");
        } else {
            assert!(row["author_locator"].is_null());
            if site == "youtube" {
                assert!(row["author_id"].is_null());
                assert_eq!(row["author_label"], "Reid Wilking");
            }
        }
        println!(
            "Native acceptance reused saved search locator; platform={site}; new_search_requests=0"
        );
        onecrew_opencli::CreatorSearchHit {
            source_url: source.into(),
            title: source.into(),
            author_label: row["author_label"].as_str().unwrap_or(source).into(),
            author_id: row["author_id"].as_str().map(str::to_owned),
            author_url: row["author_locator"].as_str().map(str::to_owned),
        }
    } else {
        let started = Instant::now();
        let hits = cli
        .creator_search(&browser, &site, &claim.query, 5)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "native acceptance search failed; platform={site}; elapsed_ms={}; error={error}",
                started.elapsed().as_millis()
            )
        });
        println!(
            "Native acceptance search PASS; platform={site}; elapsed_ms={}; actual_rows={}",
            started.elapsed().as_millis(),
            hits.len()
        );
        // Keep only public locator fields before selection. A failed exact-label
        // filter must not require another provider search to diagnose the batch.
        println!(
            "Native acceptance observed batch; {}",
            json!({
                "platform":site,"query":query,
                "rows":hits.iter().map(|hit| json!({
                    "source_url":hit.source_url,"author_label":hit.author_label,
                    "author_id":hit.author_id,"author_url":hit.author_url
                })).collect::<Vec<_>>()
            })
        );
        hits.into_iter().find(|hit| expected_profile.as_ref().is_none_or(|source| hit.author_url.as_ref() == Some(source)) && expected_label.as_ref().is_none_or(|label| &hit.author_label == label))
        .expect("actual matching source in the one returned batch; no additional search or invented author")
    };
    println!(
        "Native acceptance selected source; {}",
        json!({"platform":site,"source_url":selected.source_url,"author_locator":selected.author_url})
    );
    let has_search_author_locator = selected.author_id.is_some() && selected.author_url.is_some();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: selected.source_url,
                title: selected.title,
                author_label: selected.author_label,
                author_id: selected.author_id,
                author_url: selected.author_url,
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: campaign.clone(),
            after_id: None,
            limit: 1,
        })
        .unwrap()
        .remove(0);
    let mut needs_score = true;
    let mut ratings = 0;
    let mut last_model_verdict = None;
    for _ in 0..18 {
        let step = owner.plan_discovery_step(&campaign, 1).unwrap();
        assert!(
            !matches!(&step, DiscoveryStep::Search { .. }),
            "the single observed search batch is exhausted; no additional platform search issued"
        );
        assert!(
            !(stop_before_external_search
                && matches!(&step, DiscoveryStep::ReadContactLookup { lookup } if lookup.method == "search")),
            "external contact search is quarantined for this run; no request issued"
        );
        // Required native originals precede model scoring. Audited language-only
        // supplements use the same production planner, never a harness-made claim.
        if !native_read(&step) && needs_score {
            let started = Instant::now();
            let receipt = platform.evaluate_discovery_match(DiscoveryReviewMaterialInput {
                campaign_id:campaign.clone(),expected_revision:1,search_hit_id:hit.id.clone(),
            }).await.unwrap_or_else(|error| panic!("native acceptance score failed; platform={site}; elapsed_ms={}; error={error}", started.elapsed().as_millis()))
                .unwrap_or_else(|| panic!("native acceptance review material unavailable; platform={site}; saved_author_locator={has_search_author_locator}; stage={}; elapsed_ms={}; no model verdict created", stage(&step), started.elapsed().as_millis()));
            println!(
                "Native acceptance score PASS; platform={site}; elapsed_ms={}; verdict={}",
                started.elapsed().as_millis(),
                receipt.verdict
            );
            ratings += 1;
            last_model_verdict = Some(receipt.verdict);
            needs_score = false;
            continue;
        }
        if native_read(&step)
            || matches!(
                step,
                DiscoveryStep::ReadPublicPage { .. } | DiscoveryStep::ReadContactLookup { .. }
            )
        {
            let started = Instant::now();
            let result = platform.advance_discovery_campaign(crate::opencli::OpenCliDiscoveryAdvanceInput {
                campaign_id:campaign.clone(),expected_revision:1,excluded_platforms:vec![],
            }).await.unwrap_or_else(|error| panic!("native acceptance step failed; platform={site}; stage={}; elapsed_ms={}; error={}",stage(&step),started.elapsed().as_millis(),safe_advance_error(&error)));
            assert_eq!(result.step, step);
            let value = serde_json::to_value(&result).unwrap();
            let stop = [
                "provider_stop_code",
                "profile",
                "content",
                "content_targets",
                "identity",
                "facebook_identity",
                "source_author",
                "web_source",
                "transcript",
                "public_page",
                "contact_lookup",
            ]
            .into_iter()
            .find_map(|key| {
                if key == "provider_stop_code" {
                    value[key].as_str()
                } else {
                    value[key]["stop_code"].as_str()
                }
            });
            println!(
                "Native acceptance read; platform={site}; stage={}; elapsed_ms={}; stop={:?}",
                stage(&step),
                started.elapsed().as_millis(),
                stop
            );
            // A failed independent contact lookup is terminal for that receipt,
            // not for the author. Use the owner's next step without retrying the
            // failed request. Authentication/risk/dependency stops still fail QA.
            let local_contact_failure = matches!(
                step,
                DiscoveryStep::ReadPublicPage { .. } | DiscoveryStep::ReadContactLookup { .. }
            ) && matches!(
                stop,
                Some(
                    "opencli_command_failed"
                        | "opencli_invalid_output"
                        | "opencli_timeout"
                        | "opencli_output_too_large"
                )
            );
            let local_original_failure = site == "facebook"
                && matches!(step, DiscoveryStep::ReadContent { .. })
                && value["content"]["status"] == "failed"
                && matches!(
                    stop,
                    Some(
                        "opencli_command_failed"
                            | "opencli_invalid_output"
                            | "opencli_timeout"
                            | "opencli_output_too_large"
                    )
                );
            assert!(
                stop.is_none() || local_contact_failure || local_original_failure,
                "native/dependency/risk stop ends this bounded acceptance; no retry or provider bypass"
            );
            needs_score = native_read(&step);
            continue;
        }
        break;
    }
    assert!(ratings > 0, "model scoring was not exercised");
    let publication = owner
        .prepare_discovery_library_publication(&campaign, 1)
        .unwrap();
    let row = publication
        .material
        .sources
        .rows
        .iter()
        .find(|row| row.hit.id == hit.id)
        .unwrap();
    println!(
        "Native acceptance terminal evidence; platform={site}; verdict={}; reasons={:?}; contact_kinds={:?}; model_rating={:?}",
        row.verdict,
        row.reasons,
        row.contacts
            .iter()
            .map(|contact| &contact.kind)
            .collect::<Vec<_>>(),
        row.match_rating.as_ref().map(|rating| &rating.level)
    );
    println!("Native acceptance contact provenance; {}", json!(row.contacts.iter().map(|contact| json!({"kind":contact.kind,"source_url":contact.source_url,"author_bound":Some(&contact.native_author_id)==row.native_author_id.as_ref()})).collect::<Vec<_>>()));
    assert_eq!(last_model_verdict.as_deref(), Some("qualified"));
    assert!(
        row.native_author_id
            .as_ref()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(row.match_rating.is_some());
    let displayed_candidates = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: campaign.clone(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    let displayed = displayed_candidates
        .iter()
        .find(|candidate| Some(&candidate.id) == row.candidate_id.as_ref())
        .expect("owner-bound displayed candidate");
    if site == "linkedin" {
        assert_eq!(
            displayed.evidence["manual_profile_contact"], true,
            "qualified LinkedIn author retains the manual entry even with verified contact"
        );
    }
    if expected_outcome == "hidden_email" {
        assert_eq!(
            displayed.evidence["contact_status"]["manual_email_required"],
            true
        );
        assert_eq!(displayed.evidence["contact_status"]["has_contact"], false);
        assert_eq!(displayed.platform, "youtube");
        assert_eq!(
            displayed.profile_url,
            format!(
                "https://www.youtube.com/channel/{}",
                displayed.platform_account_id
            )
        );
    } else if expected_outcome == "social_account" {
        let accounts = displayed.evidence["associated_social_accounts"]
            .as_array()
            .expect("saved associated accounts, not guessed same-name links");
        assert!(!accounts.is_empty());
        assert!(accounts.iter().all(|account| {
            account["kind"] == "social_profile"
                && account["native_author_id"] == displayed.platform_account_id
                && account["source_url"]
                    .as_str()
                    .is_some_and(|url| url.starts_with("https://"))
        }));
    }
    let qualified_count = if expected_outcome == "contact" {
        assert_eq!(row.verdict, "qualified", "real contact gate did not pass");
        assert!(
            row.contacts.iter().any(|contact| matches!(
                contact.kind,
                DiscoveryContactKind::Email | DiscoveryContactKind::Telegram
            ) && Some(&contact.native_author_id)
                == row.native_author_id.as_ref()),
            "links or hidden-email buttons are not actual contact values"
        );
        1
    } else {
        assert_eq!(row.verdict, "pending");
        assert_eq!(row.reasons, vec!["contact_unavailable".to_owned()]);
        assert!(row.contacts.is_empty());
        0
    };
    assert_eq!(
        owner
            .count_discovery_candidates(&campaign)
            .unwrap()
            .qualified,
        qualified_count
    );
    let used = owner.get_discovery_campaign(&campaign).unwrap().used_calls;
    let started = Instant::now();
    platform.assert_test_library_publication(&publication).await;
    let expectation_path = std::env::var("ONECREW_TEST_NATIVE_EXPECTATIONS").unwrap();
    let mut expectations: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&expectation_path).unwrap()).unwrap();
    expectations["displayed_candidates"] = json!(displayed_candidates);
    std::fs::write(
        &expectation_path,
        serde_json::to_vec(&expectations).unwrap(),
    )
    .unwrap();
    println!(
        "Native acceptance Library/export PASS; platform={site}; elapsed_ms={}",
        started.elapsed().as_millis()
    );
    let expected = publication.material.sources.rows.clone();
    let expected_step = owner.plan_discovery_step(&campaign, 1).unwrap();
    assert!(!native_read(&expected_step));
    assert!(!matches!(
        expected_step,
        DiscoveryStep::ReadPublicPage { .. } | DiscoveryStep::ReadContactLookup { .. }
    ));
    if expected_outcome == "contact" {
        assert_eq!(expected_step, DiscoveryStep::TargetReached);
    } else {
        assert_ne!(expected_step, DiscoveryStep::TargetReached);
    }
    drop(owner);
    drop(platform);
    let reopened = LocalPlatform::open(dir.path()).unwrap();
    let owner = reopened.creators().unwrap();
    assert_eq!(
        owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: campaign.clone(),
                after_id: None,
                limit: 10,
            })
            .unwrap(),
        displayed_candidates
    );
    assert_eq!(
        owner
            .prepare_discovery_library_publication(&campaign, 1)
            .unwrap()
            .material
            .sources
            .rows,
        expected
    );
    assert_eq!(
        owner.get_discovery_campaign(&campaign).unwrap().used_calls,
        used
    );
    assert_eq!(
        owner.plan_discovery_step(&campaign, 1).unwrap(),
        expected_step
    );
    println!(
        "Native acceptance source/model/contact/recommendation/Library/restart PASS; platform={site}; expected_outcome={expected_outcome}; qualified={qualified_count}; calls={used}; model_receipts={ratings}"
    );
}
