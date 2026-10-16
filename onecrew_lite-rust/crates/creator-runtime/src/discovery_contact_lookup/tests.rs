use super::*;
use crate::discovery_review::initial_match;
use crate::*;
use serde_json::json;
#[test]
fn export_discovery_contact_lookup_node_oracle() {
    let Ok(output) = std::env::var("ONECREW_CONTACT_LOOKUP_ORACLE_DIRECTORY") else { return; };
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).unwrap();
    fn snapshot(path: &std::path::Path, output: &std::path::Path, name: &str) {
        rusqlite::Connection::open(path.join("crm.db")).unwrap().execute("VACUUM INTO ?1", [output.join(name).to_str().unwrap()]).unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let (owner, _) = fixture(directory.path(), Some(DiscoveryMatchLevel::Good));
    let planned = owner.repository.next_discovery_contact_lookup("bili-shoes", &[]).unwrap().unwrap();
    snapshot(directory.path(), &output, "before.sqlite");
    let claim = owner.repository.claim_discovery_contact_lookup(planned.clone()).unwrap();
    snapshot(directory.path(), &output, "claimed.sqlite");
    let mut completion = claim.clone();
    completion.status = "completed".into();
    completion.links = vec!["https://runner.example/contact".into()];
    let completed = owner.repository.finish_discovery_contact_lookup(completion).unwrap();
    snapshot(directory.path(), &output, "completed.sqlite");
    let page_claim = owner.claim_discovery_public_page(DiscoveryPublicPageInput {campaign_id:completed.campaign_id.clone(),expected_revision:completed.expected_revision,candidate_id:completed.candidate_id.clone(),profile_ref:completed.profile_ref.clone(),requested_url:completed.links[0].clone()}).unwrap();
    let page = owner.finish_discovery_public_page(FinishDiscoveryPublicPageInput {hidden_email_available:false,claim:page_claim.clone(),document_url:Some(completed.links[0].clone()),title:"Runner contact".into(),markdown_excerpt:"Official: https://space.bilibili.com/168310298 Email runner@example.com".into(),truncated:false,stop_code:None}).unwrap();
    snapshot(directory.path(), &output, "contacted.sqlite");
    let invalid_directory = tempfile::tempdir().unwrap();
    let (_invalid, _) = fixture(invalid_directory.path(), None);
    snapshot(invalid_directory.path(), &output, "unscored.sqlite");
    std::fs::write(output.join("oracle.json"), serde_json::to_vec_pretty(&json!({"planned":planned,"claim":claim,"completed":completed,"page_claim":page_claim,"page":page})).unwrap()).unwrap();
}
fn fixture(path: &std::path::Path, level: Option<DiscoveryMatchLevel>) -> (CreatorRuntime, String) {
    fixture_with_description(path, level, "这期实测新款跑鞋的缓震和抓地力。")
}
fn fixture_with_description(
    path: &std::path::Path,
    level: Option<DiscoveryMatchLevel>,
    description: &str,
) -> (CreatorRuntime, String) {
    let owner = CreatorRuntime::open(path, CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "bili-shoes".into(),
            brief: "Shoe creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000,"category":"鞋子","region":"","language":"",
            "requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "bilibili".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["鞋子测评".into()],
            }],
            target_count: 30,
            max_calls: Some(100),
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
    let profile = owner
        .claim_discovery_profile("bili-shoes", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_profile(FinishDiscoveryProfileInput {
            claim: profile,
            native_profile: Some(
                json!({"mid":"168310298","profile_url":"https://space.bilibili.com/168310298",
            "name":"Runner","follower_count":15000}),
            ),
            stop_code: None,
        })
        .unwrap();
    let content = owner
        .claim_discovery_content("bili-shoes", 1, &hit.id)
        .unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim: content,
            native_content: Some(
                json!({"bvid":"BV1shoe00001","author_id":"168310298","author":"Runner",
            "source_url":"https://www.bilibili.com/video/BV1shoe00001","title":"跑鞋测评",
            "description":description,"description_truncated":false,
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
    if let Some(level) = level {
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
                        match_level: Some(level),
                    }],
                },
            )
            .unwrap();
    }
    (owner, receipt.candidate_id)
}

#[test]
fn accumulated_contact_pages_do_not_leave_the_twenty_first_claim_busy() {
    let directory = tempfile::tempdir().unwrap();
    let links: Vec<_> = (0..21)
        .map(|index| format!("https://creator.example/page-{index}"))
        .collect();
    let description = format!("这期实测新款跑鞋的缓震和抓地力。 {}", links.join(" "));
    let (owner, hit) = fixture_with_description(
        directory.path(),
        Some(DiscoveryMatchLevel::Good),
        &description,
    );
    let candidate = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "bili-shoes".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    for link in links {
        let claim = owner
            .claim_discovery_public_page(DiscoveryPublicPageInput {
                campaign_id: "bili-shoes".into(),
                expected_revision: 1,
                profile_ref: "dedicated".into(),
                candidate_id: candidate.id.clone(),
                requested_url: link.clone(),
            })
            .unwrap();
        owner
            .settle_discovery_public_page(FinishDiscoveryPublicPageInput {
                hidden_email_available: false,
                claim,
                document_url: Some(link),
                title: "Creator".into(),
                markdown_excerpt: "Creator public page without a contact address".into(),
                truncated: false,
                stop_code: None,
            })
            .unwrap();
    }
    assert_eq!(
        owner
            .get_discovery_campaign("bili-shoes")
            .unwrap()
            .platform_runs[0]
            .status,
        "completed"
    );
    assert!(
        owner
            .prepare_discovery_review_material("bili-shoes", 1, &hit)
            .is_err()
    );
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn observed_contact_link_output_limit_does_not_stop_search_supplement() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, id) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。 https://runner.example/profile",
    );
    let input = DiscoveryPublicPageInput {
        campaign_id: "bili-shoes".into(),
        expected_revision: 1,
        candidate_id: id,
        profile_ref: "dedicated".into(),
        requested_url: "https://runner.example/profile".into(),
    };
    let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
    let receipt = owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: None,
            title: String::new(),
            markdown_excerpt: String::new(),
            truncated: false,
            stop_code: Some("opencli_output_too_large".into()),
        })
        .unwrap();
    assert_eq!(receipt.status, "failed");
    assert_eq!(
        receipt.stop_code.as_deref(),
        Some("opencli_output_too_large")
    );
    assert!(owner.claim_discovery_public_page(input).is_err());
    assert_eq!(
        owner
            .get_discovery_campaign("bili-shoes")
            .unwrap()
            .platform_runs[0]
            .status,
        "completed"
    );
    assert!(!matches!(
        owner.plan_discovery_step("bili-shoes", 1).unwrap(),
        DiscoveryStep::ReadContactLookup { .. }
    ));
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn only_audited_passing_score_starts_lookup() {
    for level in [
        None,
        Some(DiscoveryMatchLevel::Low),
        Some(DiscoveryMatchLevel::Good),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (owner, _) = fixture(dir.path(), level);
        let next = owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap();
        assert_eq!(next.is_some(), level == Some(DiscoveryMatchLevel::Good));
        if next.is_none() {
            let fabricated = DiscoveryContactLookup {
                campaign_id: "bili-shoes".into(),
                expected_revision: 1,
                candidate_id: "fake".into(),
                profile_ref: "dedicated".into(),
                method: "search".into(),
                native_author_id: "168310298".into(),
                author_label: "Runner".into(),
                username: None,
                query: "Runner email".into(),
                call_number: 0,
                status: "planned".into(),
                links: vec![],
                stop_code: None,
            };
            let used = owner
                .get_discovery_campaign("bili-shoes")
                .unwrap()
                .used_calls;
            assert!(
                owner
                    .repository
                    .claim_discovery_contact_lookup(fabricated)
                    .is_err()
            );
            assert_eq!(
                owner
                    .get_discovery_campaign("bili-shoes")
                    .unwrap()
                    .used_calls,
                used
            );
        }
    }
}
#[test]
fn search_receipt_survives_reopen_and_only_actual_related_page_provides_contact() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, id) = fixture(dir.path(), Some(DiscoveryMatchLevel::Good));
    let planned = owner
        .repository
        .next_discovery_contact_lookup("bili-shoes", &[])
        .unwrap()
        .unwrap();
    assert_eq!(planned.method, "search"); // Numeric Bili IDs are never guessed usernames.
    let mut forged = planned.clone();
    forged.query = "other email".into();
    assert!(
        owner
            .repository
            .claim_discovery_contact_lookup(forged)
            .is_err()
    );
    let mut claim = owner
        .repository
        .claim_discovery_contact_lookup(planned.clone())
        .unwrap();
    assert!(
        owner
            .repository
            .claim_discovery_contact_lookup(planned)
            .is_err()
    );
    claim.status = "completed".into();
    claim.links = vec![
        "https://unrelated.example/contact".into(),
        "https://runner.example/contact".into(),
    ];
    owner
        .repository
        .finish_discovery_contact_lookup(claim)
        .unwrap();
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    for (url, text, expected) in [
        (
            "https://unrelated.example/contact",
            "Business email: other@example.org",
            0,
        ),
        (
            "https://runner.example/contact",
            "Runner business email: runner@example.org https://space.bilibili.com/168310298",
            1,
        ),
    ] {
        let input = DiscoveryPublicPageInput {
            campaign_id: "bili-shoes".into(),
            expected_revision: 1,
            candidate_id: id.clone(),
            profile_ref: "dedicated".into(),
            requested_url: url.into(),
        };
        let claim = owner.claim_discovery_public_page(input).unwrap();
        owner
            .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
                hidden_email_available: false,
                claim,
                document_url: Some(url.into()),
                title: "Contact".into(),
                markdown_excerpt: text.into(),
                truncated: false,
                stop_code: None,
            })
            .unwrap();
        assert_eq!(
            owner
                .count_discovery_candidates("bili-shoes")
                .unwrap()
                .qualified,
            expected
        );
    }
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner
            .prepare_discovery_library_publication("bili-shoes", 1)
            .unwrap()
            .material
            .sources
            .rows[0]
            .contacts[0]
            .normalized_value,
        "runner@example.org"
    );
}
#[test]
fn paused_and_finished_lookups_never_repeat() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture(dir.path(), Some(DiscoveryMatchLevel::Good));
    let planned = owner
        .repository
        .next_discovery_contact_lookup("bili-shoes", &[])
        .unwrap()
        .unwrap();
    rusqlite::Connection::open(dir.path().join("crm.db"))
        .unwrap()
        .execute_batch("UPDATE discovery_campaigns SET phase='paused'")
        .unwrap();
    assert!(
        owner
            .repository
            .claim_discovery_contact_lookup(planned)
            .is_err()
    );
}

#[test]
fn restart_preserves_charged_attempt_without_replaying_contact_search() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture(dir.path(), Some(DiscoveryMatchLevel::Good));
    let planned = owner
        .repository
        .next_discovery_contact_lookup("bili-shoes", &[])
        .unwrap()
        .unwrap();
    let claim = owner
        .repository
        .claim_discovery_contact_lookup(planned)
        .unwrap();
    let used = owner
        .get_discovery_campaign("bili-shoes")
        .unwrap()
        .used_calls;
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    assert_eq!(
        owner
            .get_discovery_campaign("bili-shoes")
            .unwrap()
            .used_calls,
        used
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    let mut late = claim;
    late.status = "completed".into();
    late.links = vec!["https://runner.example/contact".into()];
    assert!(
        owner
            .repository
            .finish_discovery_contact_lookup(late)
            .is_err()
    );
}

#[test]
fn contact_page_risk_stops_that_host_without_stopping_native_platform() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, id) = fixture(dir.path(), Some(DiscoveryMatchLevel::Good));
    let planned = owner
        .repository
        .next_discovery_contact_lookup("bili-shoes", &[])
        .unwrap()
        .unwrap();
    let mut lookup = owner
        .repository
        .claim_discovery_contact_lookup(planned)
        .unwrap();
    lookup.status = "completed".into();
    lookup.links = vec![
        "https://blocked.example/contact".into(),
        "https://blocked.example/another".into(),
        "https://runner.example/contact".into(),
    ];
    owner
        .repository
        .finish_discovery_contact_lookup(lookup)
        .unwrap();
    let input = DiscoveryPublicPageInput {
        campaign_id: "bili-shoes".into(),
        expected_revision: 1,
        candidate_id: id,
        profile_ref: "dedicated".into(),
        requested_url: "https://blocked.example/contact".into(),
    };
    let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
    let receipt = owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: None,
            title: String::new(),
            markdown_excerpt: String::new(),
            truncated: false,
            stop_code: Some("opencli_risk_control".into()),
        })
        .unwrap();
    assert_eq!(receipt.status, "failed");
    let mut repeated = input;
    repeated.requested_url = "https://blocked.example/another".into();
    assert!(owner.claim_discovery_public_page(repeated).is_err());
    assert!(
        matches!(owner.plan_discovery_step("bili-shoes",1).unwrap(),DiscoveryStep::ReadPublicPage{requested_url,..} if requested_url=="https://runner.example/contact")
    );
    assert_eq!(
        owner
            .get_discovery_campaign("bili-shoes")
            .unwrap()
            .platform_runs[0]
            .status,
        "completed"
    );
}

// Two independently bound authors are necessary here: a single-author fixture
// cannot reveal a failed lookup incorrectly disabling the next author.
fn two_reddit_authors(path: &std::path::Path) -> CreatorRuntime {
    let owner = two_reddit_authors_with_scopes(path, vec![]);
    // Persisted campaigns predating native-account contact observations can still
    // have queued enrichment. Keep their failure-isolation coverage source-bound.
    let db = rusqlite::Connection::open(path.join("crm.db")).unwrap();
    assert_eq!(db.execute("DELETE FROM discovery_contact_observations WHERE discovery_campaign_id='contact-isolation' AND kind='social_profile' AND source_excerpt='Verified native author account'", []).unwrap(), 2);
    owner
}
fn two_reddit_authors_with_scopes(
    path: &std::path::Path,
    additional_scopes: Vec<DiscoveryPlatformScope>,
) -> CreatorRuntime {
    let owner = CreatorRuntime::open(path, CreatorGateways::default()).unwrap();
    let mut scopes = vec![DiscoveryPlatformScope {
        platform: "reddit".into(),
        profile_ref: "dedicated".into(),
        keywords: vec!["keyboard".into()],
    }];
    scopes.extend(additional_scopes);
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id: "contact-isolation".into(), brief: "Keyboard creators".into(), opencli_version: "1.8.8".into(),
        criteria: json!({"min_followers":100,"language":"","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes,
        target_count: 3, max_calls: None,
    }).unwrap();
    owner
        .approve_discovery_campaign("contact-isolation", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("contact-isolation", 1, "reddit", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: (1..=2)
                .map(|n| DiscoverySearchHitInput {
                    source_url: format!("https://www.reddit.com/comments/post{n}/"),
                    title: "Keyboard test".into(),
                    author_label: format!("creator_{n}"),
                    author_id: Some(format!("t2_user{n}")),
                    author_url: Some(format!("https://www.reddit.com/user/creator_{n}/")),
                })
                .collect(),
        })
        .unwrap();
    for hit in owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "contact-isolation".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
    {
        let n = hit.author_label.strip_prefix("creator_").unwrap();
        let claim = owner
            .claim_discovery_profile("contact-isolation", 1, &hit.id)
            .unwrap();
        owner.finish_discovery_profile(FinishDiscoveryProfileInput {
            claim, stop_code:None, native_profile:Some(json!({
                "adapter_version":"1.0.0", "id":format!("t2_user{n}"), "username":hit.author_label,
                "biography":"Keyboard reviewer", "follower_count":15000,
                "profile_url":format!("https://www.reddit.com/user/creator_{n}/"), "evidence_source":"native_profile_response"
            })),
        }).unwrap();
        let claim = owner
            .claim_discovery_content("contact-isolation", 1, &hit.id)
            .unwrap();
        owner.finish_discovery_content(FinishDiscoveryContentInput {
            claim, stop_code:None, native_content:Some(json!({
                "post_id":format!("post{n}"), "author_id":format!("t2_user{n}"),
                "source_url":hit.source_url, "text":"I tested this mechanical keyboard.", "text_truncated":false
            })),
        }).unwrap();
        let material = owner
            .prepare_discovery_review_material("contact-isolation", 1, &hit.id)
            .unwrap();
        owner
            .submit_discovery_review(
                "account",
                SubmitDiscoveryReviewInput {
                    campaign_id: "contact-isolation".into(),
                    expected_revision: 1,
                    search_hit_id: hit.id.clone(),
                    material_hash: material.material_hash,
                    request_id: format!("score-{n}"),
                    reviewer: DiscoveryReviewer {
                        kind: DiscoveryReviewerKind::Agent,
                        label: initial_match::INITIAL_MATCH_REVIEWER.into(),
                    },
                    findings: vec![DiscoveryReviewFinding {
                        criterion: DiscoveryReviewCriterion::MatchFit,
                        outcome: DiscoveryReviewOutcome::Supported,
                        rationale: "The native author tested a mechanical keyboard.".into(),
                        citations: vec![DiscoveryReviewQuote::NativeContent {
                            quote: "I tested this mechanical keyboard.".into(),
                        }],
                        tool: None,
                        country_code: None,
                        match_level: Some(DiscoveryMatchLevel::Good),
                    }],
                },
            )
            .unwrap();
    }
    owner
}

fn finish_empty_lookup(
    owner: &CreatorRuntime,
    mut planned: DiscoveryContactLookup,
    stop: Option<&str>,
) {
    planned = owner
        .repository
        .claim_discovery_contact_lookup(planned)
        .unwrap();
    planned.status = if stop.is_some() {
        "failed"
    } else {
        "completed"
    }
    .into();
    planned.stop_code = stop.map(str::to_string);
    owner
        .repository
        .finish_discovery_contact_lookup(planned)
        .unwrap();
}

#[test]
fn one_maigret_failure_does_not_disable_the_next_author_but_risk_does() {
    for (stop, next_method) in [
        ("opencli_command_failed", "maigret"),
        ("opencli_timeout", "maigret"),
        ("opencli_invalid_output", "maigret"),
        ("opencli_output_too_large", "maigret"),
        ("opencli_risk_control", "search"),
        ("opencli_auth_required", "search"),
        ("opencli_unavailable", "search"),
        ("opencli_version_changed", "search"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let owner = two_reddit_authors(dir.path());
        let first = owner
            .repository
            .next_discovery_contact_lookup("contact-isolation", &[])
            .unwrap()
            .unwrap();
        assert_eq!(first.method, "maigret");
        let first_id = first.candidate_id.clone();
        finish_empty_lookup(&owner, first, Some(stop));
        let search = owner
            .repository
            .next_discovery_contact_lookup("contact-isolation", &[])
            .unwrap()
            .unwrap();
        assert_eq!(search.candidate_id, first_id);
        assert_eq!(search.method, "search");
        finish_empty_lookup(&owner, search, None);
        drop(owner);
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        let next = owner
            .repository
            .next_discovery_contact_lookup("contact-isolation", &[])
            .unwrap()
            .unwrap();
        assert_ne!(next.candidate_id, first_id);
        assert_eq!(next.method, next_method, "{stop}");
        assert_eq!(
            owner
                .count_discovery_candidates("contact-isolation")
                .unwrap()
                .qualified,
            2
        );
    }
}

#[test]
fn risk_on_a_maigret_link_does_not_disable_other_authors_username_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let owner = two_reddit_authors(dir.path());
    let first = owner
        .repository
        .next_discovery_contact_lookup("contact-isolation", &[])
        .unwrap()
        .unwrap();
    let mut lookup = owner
        .repository
        .claim_discovery_contact_lookup(first)
        .unwrap();
    let first_id = lookup.candidate_id.clone();
    lookup.status = "completed".into();
    lookup.links = vec!["https://blocked.example/contact".into()];
    owner
        .repository
        .finish_discovery_contact_lookup(lookup)
        .unwrap();
    let claim = owner
        .claim_discovery_public_page(DiscoveryPublicPageInput {
            campaign_id: "contact-isolation".into(),
            expected_revision: 1,
            candidate_id: first_id.clone(),
            profile_ref: "dedicated".into(),
            requested_url: "https://blocked.example/contact".into(),
        })
        .unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: None,
            title: String::new(),
            markdown_excerpt: String::new(),
            truncated: false,
            stop_code: Some("opencli_risk_control".into()),
        })
        .unwrap();
    let search = owner
        .repository
        .next_discovery_contact_lookup("contact-isolation", &[])
        .unwrap()
        .unwrap();
    assert_eq!(search.candidate_id, first_id);
    finish_empty_lookup(&owner, search, None);
    let next = owner
        .repository
        .next_discovery_contact_lookup("contact-isolation", &[])
        .unwrap()
        .unwrap();
    assert_ne!(next.candidate_id, first_id);
    assert_eq!(next.method, "maigret");
}

#[test]
fn excluded_platform_skips_native_contact_pages_without_charging_or_erasing_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。Contact https://runner.example/contact",
    );
    let before = owner.get_discovery_campaign("bili-shoes").unwrap();
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &[])
            .unwrap()
            .is_some()
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &["bilibili".into()])
            .unwrap()
            .is_none()
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_some()
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &["bilibili".into()])
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        owner
            .plan_discovery_step_excluding("bili-shoes", 1, &["bilibili".into()])
            .unwrap(),
        DiscoveryStep::SearchExhausted
    ));
    assert_eq!(owner.get_discovery_campaign("bili-shoes").unwrap(), before);
}

#[test]
fn excluded_authors_do_not_block_the_next_source_or_its_selected_contact_claim() {
    let dir = tempfile::tempdir().unwrap();
    let owner = two_reddit_authors_with_scopes(
        dir.path(),
        vec![DiscoveryPlatformScope {
            platform: "pinterest".into(),
            profile_ref: "dedicated".into(),
            keywords: vec!["keyboard".into()],
        }],
    );
    let excluded = vec!["reddit".into()];
    let historical = owner
        .count_discovery_candidates("contact-isolation")
        .unwrap();
    assert_eq!(historical.pending, 0);
    assert_eq!(historical.qualified, 2);
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("contact-isolation", &excluded)
            .unwrap()
            .is_none()
    );
    assert!(
        matches!(owner.plan_discovery_step_excluding("contact-isolation", 1, &excluded).unwrap(), DiscoveryStep::Search { platform, .. } if platform == "pinterest")
    );
    let claim = owner
        .claim_discovery_search("contact-isolation", 1, "pinterest", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.pinterest.com/native_author/".into(),
                title: "Native author".into(),
                author_label: "Native author".into(),
                author_id: Some("9876543210123456789".into()),
                author_url: Some("https://www.pinterest.com/native_author/".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "contact-isolation".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .into_iter()
        .find(|hit| hit.platform == "pinterest")
        .unwrap();
    let claim = owner
        .claim_discovery_profile("contact-isolation", 1, &hit.id)
        .unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {claim,stop_code:None,native_profile:Some(json!({"author_id":"9876543210123456789","username":"native_author","author_label":"Native author","profile_url":"https://www.pinterest.com/native_author/","follower_count":15000,"bio":"Original keyboard reviews","public_links":[]}))}).unwrap();
    let material = owner
        .prepare_discovery_review_material("contact-isolation", 1, &hit.id)
        .unwrap();
    owner.submit_discovery_review("account", SubmitDiscoveryReviewInput {campaign_id:"contact-isolation".into(),expected_revision:1,search_hit_id:hit.id,material_hash:material.material_hash,request_id:"pinterest-score".into(),
        reviewer:DiscoveryReviewer {kind:DiscoveryReviewerKind::Agent,label:initial_match::INITIAL_MATCH_REVIEWER.into()}, findings:vec![DiscoveryReviewFinding {criterion:DiscoveryReviewCriterion::MatchFit,outcome:DiscoveryReviewOutcome::Supported,rationale:"Native profile describes original keyboard reviews, as initial topic context.".into(),
            citations:vec![DiscoveryReviewQuote::NativeProfile {profile_url:"https://www.pinterest.com/native_author/".into(),quote:"Original keyboard reviews".into()}],tool:None,country_code:None,match_level:Some(DiscoveryMatchLevel::Good)}]}).unwrap();
    let planned = owner
        .repository
        .next_discovery_contact_lookup("contact-isolation", &excluded)
        .unwrap()
        .unwrap();
    assert_eq!(planned.native_author_id, "9876543210123456789");
    let before = owner
        .get_discovery_campaign("contact-isolation")
        .unwrap()
        .used_calls;
    let claimed = owner
        .repository
        .claim_discovery_contact_lookup(planned.clone())
        .unwrap();
    assert_eq!(claimed.candidate_id, planned.candidate_id);
    assert_eq!(claimed.status, "reading");
    assert_eq!(
        owner
            .get_discovery_campaign("contact-isolation")
            .unwrap()
            .used_calls,
        before + 1
    );
    assert_eq!(
        owner
            .count_discovery_candidates("contact-isolation")
            .unwrap()
            .pending,
        historical.pending + 1
    );
}

#[test]
fn contact_navigation_prioritizes_contact_pages_over_shorteners() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。\nBuy shoes https://bit.ly/product-offer\nOther https://bit.ly/unknown\nBusiness https://runner.example/contact",
    );
    let page = owner
        .repository
        .next_discovery_public_page("bili-shoes", &[])
        .unwrap()
        .unwrap();
    assert_eq!(page.requested_url, "https://runner.example/contact");
    let before = owner
        .get_discovery_campaign("bili-shoes")
        .unwrap()
        .used_calls;
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some("https://runner.example/contact".into()),
            title: "Runner business".into(),
            markdown_excerpt: "Runner business email: runner@example.org".into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert_eq!(
        owner
            .get_discovery_campaign("bili-shoes")
            .unwrap()
            .used_calls
            - before,
        1
    );
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        1
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
}

#[test]
fn contact_navigation_keeps_unknown_shortener_as_fallback_and_skips_explicit_purchase() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。\nBuy shoes https://bit.ly/product-offer\nOther https://bit.ly/unknown\nBusiness https://runner.example/contact",
    );
    let page = owner
        .repository
        .next_discovery_public_page("bili-shoes", &[])
        .unwrap()
        .unwrap();
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some("https://runner.example/contact".into()),
            title: "Runner".into(),
            markdown_excerpt: "Runner has no published address here.".into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let next = owner
        .repository
        .next_discovery_public_page("bili-shoes", &[])
        .unwrap()
        .unwrap();
    assert_eq!(next.requested_url, "https://bit.ly/unknown");
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn contact_navigation_qualifies_known_social_page_without_redundant_reads() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。Twitter: https://twitter.com/runner_native",
    );
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_none(),
        "published social account qualifies contact and stops external expansion"
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    let counts = owner.count_discovery_candidates("bili-shoes").unwrap();
    assert_eq!(counts.qualified, 1);
    assert_eq!(counts.public_page_errors, 0);
}

#[test]
fn effective_recommendation_counts_require_contacts_even_when_topic_matches() {
    for (description, verdict) in [
        ("这期实测新款跑鞋的缓震和抓地力。", "pending"),
        (
            "这期实测新款跑鞋的缓震和抓地力。Twitter: https://twitter.com/runner_native",
            "qualified",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (owner, _) =
            fixture_with_description(dir.path(), Some(DiscoveryMatchLevel::Good), description);
        let rows = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "bili-shoes".into(),
                after_id: None,
                limit: 100,
            })
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].verdict, "qualified",
            "matching remains independent of recommendation eligibility"
        );
        let groups = owner
            .get_discovery_identity_groups("bili-shoes", 1)
            .unwrap();
        assert_eq!(groups.groups.len(), 1);
        assert_eq!(groups.groups[0].members[0].verdict, verdict);
        let counts = owner.count_discovery_candidates("bili-shoes").unwrap();
        assert_eq!(counts.qualified, u32::from(verdict == "qualified"));
    }
}

#[test]
fn contact_navigation_owner_skips_products_and_confirmed_social_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。\nhttps://youtu.be/6gYVGUBOsvA\nhttps://collabs.shop/uhjmep\nhttps://rover.ebay.com/rover/1/123\nFollow https://twitter.com/runner_native/\nFollow https://x.com/runner_native",
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        1
    );
}

#[test]
fn contact_navigation_owner_rejects_shop_collection_as_creator_contact() {
    let dir = tempfile::tempdir().unwrap();
    let product = "https://shop.wuquestudio.com/collections/nama-1";
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        &format!(
            "这期实测新款跑鞋的缓震和抓地力。\nThe keyboard - Wuque Studio NAMA: {product}\nBusiness contact: https://runner.example/contact"
        ),
    );
    let page = owner
        .repository
        .next_discovery_public_page("bili-shoes", &[])
        .unwrap()
        .unwrap();
    assert_eq!(page.requested_url, "https://runner.example/contact");
    let mut product_page = page.clone();
    product_page.requested_url = product.into();
    assert!(owner.claim_discovery_public_page(product_page).is_err());
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some("https://runner.example/contact".into()),
            title: "Runner".into(),
            markdown_excerpt: "No contact details".into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn contact_navigation_owner_excludes_staff_but_preserves_business_contact() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。\nFollow the staff:\nAdam https://instagram.com/adam\nEric https://eric.example\n\nBusiness contact:\nhttps://runner.example/contact",
    );
    let page = owner
        .repository
        .next_discovery_public_page("bili-shoes", &[])
        .unwrap()
        .unwrap();
    assert_eq!(page.requested_url, "https://runner.example/contact");
    let mut staff = page.clone();
    staff.requested_url = "https://eric.example".into();
    assert!(owner.claim_discovery_public_page(staff).is_err());
    let claim = owner.claim_discovery_public_page(page).unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some("https://runner.example/contact".into()),
            title: "Runner".into(),
            markdown_excerpt: "No contact details".into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
}

#[test]
fn observed_social_profile_qualifies_contact_and_preserves_author_attribution() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。 https://www.instagram.com/runner_creator https://space.bilibili.com/168310298",
    );
    assert_eq!(
        owner
            .count_discovery_candidates("bili-shoes")
            .unwrap()
            .qualified,
        1
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let rows: Vec<(String, String)> = db
        .prepare("SELECT kind,normalized_value FROM discovery_contact_observations")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![(
            "social_profile".into(),
            "https://www.instagram.com/runner_creator".into()
        )]
    );
    let candidates = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "bili-shoes".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    let accounts = candidates[0].evidence["associated_social_accounts"]
        .as_array()
        .unwrap();
    assert!(
        owner
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_none(),
        "published alternative account must stop external enrichment"
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page("bili-shoes", &[])
            .unwrap()
            .is_none(),
        "confirmed contact stops redundant official-page reads"
    );
    assert_eq!(accounts.len(), 1);
    assert_eq!(
        accounts[0]["normalized_value"],
        "https://www.instagram.com/runner_creator"
    );
    assert_eq!(accounts[0]["attribution"], "author_published");
    // Unconfirmed links must not suppress discovery. If a queued lookup becomes
    // unnecessary before claim, reject it without charging a provider call.
    db.execute("UPDATE discovery_contact_observations SET attribution='unconfirmed' WHERE kind='social_profile'", []).unwrap();
    let stale = owner
        .repository
        .next_discovery_contact_lookup("bili-shoes", &[])
        .unwrap()
        .unwrap();
    db.execute("UPDATE discovery_contact_observations SET attribution='author_published' WHERE kind='social_profile'", []).unwrap();
    let used = owner
        .get_discovery_campaign("bili-shoes")
        .unwrap()
        .used_calls;
    assert!(
        owner
            .repository
            .claim_discovery_contact_lookup(stale)
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("bili-shoes")
            .unwrap()
            .used_calls,
        used
    );
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert!(
        reopened
            .repository
            .next_discovery_contact_lookup("bili-shoes", &[])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        accounts[0]["native_author_id"],
        candidates[0].platform_account_id
    );
    assert!(
        accounts[0]["source_url"]
            .as_str()
            .unwrap()
            .starts_with("https://")
    );
}

#[test]
fn associated_website_link_to_primary_platform_is_not_an_alternative_contact() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, id) = fixture_with_description(
        dir.path(),
        Some(DiscoveryMatchLevel::Good),
        "这期实测新款跑鞋的缓震和抓地力。 https://runner.example/contact",
    );
    let claim = owner
        .claim_discovery_public_page(DiscoveryPublicPageInput {
            campaign_id: "bili-shoes".into(),
            expected_revision: 1,
            candidate_id: id,
            profile_ref: "dedicated".into(),
            requested_url: "https://runner.example/contact".into(),
        })
        .unwrap();
    owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some("https://runner.example/contact".into()),
            title: "Runner".into(),
            markdown_excerpt: "Follow my channel https://space.bilibili.com/168310298".into(),
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
}
