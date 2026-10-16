use super::*;
use crate::opencli::{OpenCliDiscoveryProfile, OpenCliDiscoverySource};

#[tokio::test]
#[ignore = "使用真实来源注册表和已登录 Codex 验证美妆需求，不执行平台搜索"]
async fn live_beauty_plan_preserves_platform_followers_and_total() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let inventory: OpenCliDiscoverySources = platform.opencli_setup_report().await.into();
    assert!(inventory.issue.is_none(), "{:?}", inventory.issue);
    let mut request = input();
    request.brief = "我要找美妆博主，帮我在tiktok, x 和 youtube上找粉丝数不少于5000的100个\nDefault search entries when no platform is requested: youtube".into();
    request.target_count = None;
    request.max_calls = None;
    request.profile_ref = inventory
        .profiles
        .first()
        .expect("已连接的浏览器身份")
        .context_id
        .clone();
    let proposal = codex::plan(&request.brief, &inventory).await.unwrap();
    let draft = build_draft(&request, &inventory, proposal).unwrap();
    assert_eq!(draft.target_count, 100);
    assert_eq!(draft.criteria["min_subscribers"], 5000);
    let platforms: BTreeSet<_> = draft.scopes.iter().map(|s| s.platform.as_str()).collect();
    assert_eq!(platforms, BTreeSet::from(["tiktok", "twitter", "youtube"]));
    for scope in draft.scopes {
        assert!((6..=8).contains(&scope.keywords.len()));
        println!("{}：{}", scope.platform, scope.keywords.join("；"));
    }
}

#[tokio::test]
#[ignore = "authorized native intent then query planning, no provider reads or customer writes"]
async fn native_codex_headphone_intent_and_queries_acceptance() {
    let mut request = input();
    request.brief = "我要推销耳机 帮我在tiktok上找粉丝数不少于5000的10个博主\nDefault search entries when no platform is requested: youtube".into();
    request.target_count = None;
    request.max_calls = None;
    let mut inventory = sources();
    let mut source = inventory.sources[0].clone();
    source.platform = "tiktok".into();
    inventory.sources.push(source);
    let plan = codex::plan(&request.brief, &inventory).await.unwrap();
    let draft = build_draft(&request, &inventory, plan).unwrap();
    assert_eq!(draft.target_count, 10);
    assert_eq!(draft.criteria["min_subscribers"], 5000);
    assert_eq!(draft.criteria["automatic_query_expansion"], true);
    assert_eq!(draft.criteria["table_subject"], "耳机");
    assert_eq!(draft.scopes.len(), 1);
    assert_eq!(draft.scopes[0].platform, "tiktok");
    assert!(draft.scopes[0].keywords.len() >= 6);
    println!("Headphone planning acceptance: {}", serde_json::to_string(&draft.scopes).unwrap());
}

fn input() -> PrepareDiscoveryDraftInput {
    PrepareDiscoveryDraftInput {
        id: "draft-test".into(),
        brief: "我要卖鞋子，便宜时尚".into(),
        profile_ref: "browser".into(),
        opencli_version: "1.8.8".into(),
        target_count: Some(30),
        max_calls: Some(200),
        min_followers_override: None,
        criteria_overrides: None,
        scope_overrides: vec![],
        refinement: None,
    }
}

#[test]
fn edited_plan_can_clear_optional_criteria() {
    let mut request = input();
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: Some(String::new()),
        language: Some("中文".into()),
        category: Some("稳定币".into()),
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    assert_eq!(validate_input(&request), Ok(()));
    let draft = build_draft(&request, &sources(), plan()).unwrap();
    assert_eq!(draft.criteria["region"], "");
    assert_eq!(draft.criteria["language"], "中文");
    assert_eq!(draft.criteria["category"], "稳定币");
}

#[test]
fn new_drafts_retire_residence_even_from_old_form_settings() {
    let mut request = input();
    request.brief.push_str("\nCreator location: 美国");
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: Some("美国".into()),
        language: None,
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    let mut proposal = plan();
    proposal.intent.original_request = request.brief.clone();
    proposal.intent.region = ProposedConstraint::Specified {
        value: "美国".into(),
        excerpt: "Creator location: 美国".into(),
    };
    assert_eq!(
        build_draft(&request, &sources(), proposal)
            .unwrap()
            .criteria["region"],
        ""
    );
}

#[tokio::test]
#[ignore = "one bounded tool-free native Codex planning call; no search or Campaign write"]
async fn native_codex_chinese_keyboard_queries_follow_content_language() {
    let mut request = input();
    request.brief = "我要卖键盘\nContent language: 中文\nPreferred search keywords: keyboard review, mechanical keyboard review, best keyboards".into();
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: Some(String::new()),
        language: Some("中文".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    let mut inventory = sources();
    let mut bilibili = inventory.sources[0].clone();
    bilibili.platform = "bilibili".into();
    bilibili.command = "onecrew-bilibili/search".into();
    inventory.sources.push(bilibili);
    let proposal = codex::plan(&request.brief, &inventory).await.unwrap();
    let draft = build_draft(&request, &inventory, proposal).unwrap();
    assert_eq!(draft.criteria["language"], "中文");
    assert_eq!(draft.scopes.len(), 2);
    for scope in &draft.scopes {
        assert!(
            scope.keywords[0]
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{} must begin with a Chinese query",
            scope.platform
        );
        assert!(scope.keywords.iter().any(|word| word.contains("键盘")));
    }
}

#[test]
fn explicit_search_scopes_override_ai_suggestions() {
    let mut request = input();
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec!["手动词".into()],
    }];
    let draft = build_draft(&request, &sources(), plan()).unwrap();
    assert_eq!(draft.scopes[0].keywords, vec!["手动词"]);
    request.scope_overrides[0].platform = "unregistered".into();
    assert_eq!(
        build_draft(&request, &sources(), plan()).unwrap_err(),
        DiscoveryDraftError::InvalidPlan
    );
}

#[test]
fn chinese_queries_require_target_language_including_manual_terms() {
    let mut request = input();
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: None,
        language: Some("中文".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    let mut proposal = plan();
    proposal.scopes[0].keywords = vec!["laptop review".into(), "ThinkPad 测评".into()];
    let draft = build_draft(&request, &sources(), proposal).unwrap();
    assert_eq!(
        draft.scopes[0].keywords,
        vec!["ThinkPad 测评", "laptop review"]
    );
    let mut proposal = plan();
    proposal.scopes[0].keywords = vec!["laptop review".into()];
    assert_eq!(
        build_draft(&request, &sources(), proposal).unwrap_err(),
        DiscoveryDraftError::InvalidScope
    );
    let proposal = plan();
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec!["laptop review".into()],
    }];
    assert_eq!(
        build_draft(&request, &sources(), proposal).unwrap().scopes[0].keywords,
        vec!["平价鞋测评"]
    );
}

#[test]
fn explicit_scope_survives_planner_omission_without_inventing_keywords() {
    let mut request = input();
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec!["用户填写的词".into()],
    }];
    let mut omitted = plan();
    omitted.scopes.clear();
    let draft = build_draft(&request, &sources(), omitted).unwrap();
    assert_eq!(draft.scopes.len(), 1);
    assert_eq!(
        draft.scopes[0].keywords,
        request.scope_overrides[0].keywords
    );
    assert_eq!(draft.scopes[0].profile_ref, request.profile_ref);
    // Without supplied terms, an omission still fails rather than guessing.
    request.scope_overrides.clear();
    let mut omitted = plan();
    omitted.scopes.clear();
    assert_eq!(
        build_draft(&request, &sources(), omitted).unwrap_err(),
        DiscoveryDraftError::IncompleteCoverage
    );
}

#[test]
fn supplied_scopes_cannot_override_exclusions_or_platform_term_rules() {
    let mut request = input();
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec!["manual".into()],
    }];
    request.brief.push_str("，不要 youtube");
    let mut excluded = plan();
    excluded.intent.original_request = request.brief.clone();
    excluded.scopes.clear();
    excluded
        .intent
        .excluded_platforms
        .push(onecrew_creator_runtime::QuotedPlatform {
            platform: "youtube".into(),
            excerpt: "不要 youtube".into(),
        });
    assert_eq!(
        build_draft(&request, &sources(), excluded).unwrap_err(),
        DiscoveryDraftError::InvalidScope
    );
    let mut inventory = sources();
    inventory.sources[0].allowed_values = vec!["allowed".into()];
    assert_eq!(
        build_draft(&input(), &inventory, plan()).unwrap_err(),
        DiscoveryDraftError::InvalidScope
    );
}

#[tokio::test]
async fn edited_many_source_plan_preserves_all_supplied_scopes_when_ai_omits_them() {
    let selected = "youtube, instagram, twitter, linkedin, linkedin-salesnav, facebook, pinterest, reddit, medium, archive, bluesky, jike, hackernews, linux-do, nowcoder, pixiv, stackoverflow, stackoverflow-users, hf-spaces, hf-models, hf-datasets, dribbble-designers, google, duckduckgo, yahoo";
    let mut request = input();
    let mut inventory = sources();
    let template = inventory.sources[0].clone();
    inventory.sources = selected
        .split(", ")
        .map(|name| {
            let mut source = template.clone();
            source.platform = name.into();
            source
        })
        .collect();
    request.scope_overrides = selected
        .split(", ")
        .map(|platform| DraftScopeOverride {
            platform: platform.into(),
            keywords: vec!["稳定币".into(), "USDT 中文".into()],
        })
        .collect();
    let mut omitted = plan();
    omitted.scopes.clear();
    let draft = build_draft(&request, &inventory, omitted).unwrap();
    assert_eq!(draft.scopes.len(), 25);
    for (scope, requested) in draft.scopes.iter().zip(&request.scope_overrides) {
        assert_eq!(scope.platform, requested.platform);
        assert_eq!(scope.keywords, requested.keywords);
    }
    let dir = tempfile::tempdir().unwrap();
    let platform = authenticated(dir.path());
    let creators = platform.creators().unwrap();
    let previous = creators
        .create_discovery_campaign_async(build_draft(&input(), &sources(), plan()).unwrap())
        .await
        .unwrap();
    let previous = creators
        .approve_discovery_campaign_async(previous.id.clone(), previous.revision)
        .await
        .unwrap();
    let mut edited = draft;
    edited.id = "edited-test".into();
    edited.brief = "虚拟货币".into();
    edited.criteria["category"] = json!("稳定币科普");
    edited.criteria["language"] = json!("中文");
    edited.target_count = 5;
    let saved = creators
        .create_discovery_campaign_async(edited)
        .await
        .unwrap();
    assert_eq!(saved.phase, "draft");
    assert_eq!(saved.used_calls, 0);
    assert_eq!(saved.scopes.len(), 25);
    assert_eq!(saved.criteria["category"], "稳定币科普");
    assert_eq!(saved.criteria["language"], "中文");
    assert_eq!(saved.target_count, 5);
    assert_eq!(
        creators
            .get_discovery_campaign_async(saved.id.clone())
            .await
            .unwrap(),
        saved
    );
    assert_eq!(
        creators
            .get_discovery_campaign_async(previous.id.clone())
            .await
            .unwrap(),
        previous
    );
}

fn sources() -> OpenCliDiscoverySources {
    OpenCliDiscoverySources {
        version: Some("1.8.8".into()),
        daemon_running: true,
        extension_connected: true,
        profiles: vec![OpenCliDiscoveryProfile {
            context_id: "browser".into(),
            alias: None,
        }],
        issue: None,
        sources: vec![OpenCliDiscoverySource {
            platform: "youtube".into(),
            command: "youtube/search".into(),
            browser_required: true,
            registered: true,
            scope_kind: "query".into(),
            allowed_values: vec![],
        }],
    }
}

fn plan() -> CodexDraftPlan {
    CodexDraftPlan {
        table_subject: String::new(),
        target_count: ProposedConstraint::Unspecified,
        max_calls: ProposedConstraint::Unspecified,
        contact_search_terms: vec![],
        intent: DiscoveryIntentProposal {
            original_request: input().brief,
            goal: "推广平价时尚鞋子".into(),
            category: ProposedConstraint::Specified {
                value: "鞋子".into(),
                excerpt: "鞋子".into(),
            },
            region: ProposedConstraint::Unspecified,
            language: ProposedConstraint::Unspecified,
            min_subscribers: ProposedConstraint::Unspecified,
            excluded_platforms: vec![],
            other_hard_conditions: vec![],
            soft_preferences: vec![],
            suggested_keywords: vec!["平价鞋测评".into()],
        },
        scopes: vec![DraftScope {
            platform: "youtube".into(),
            keywords: vec!["平价鞋测评".into()],
        }],
    }
}

fn authenticated(path: &std::path::Path) -> LocalPlatform {
    use onecrew_local_domain::*;
    let platform = LocalPlatform::open(path).unwrap();
    let metadata = CreatorSyncCredentialMetadata {
        installation_id: uuid::Uuid::new_v4(),
        credential_id: uuid::Uuid::new_v4(),
        access_user_id: "test-account".into(),
        access_device_id: "test-device".into(),
        source_instance_id: uuid::Uuid::new_v4(),
        access_pairing_id: "test-pairing".into(),
        client_platform: CreatorSyncClientPlatform::Macos,
        app_version: "0.6.27".into(),
        status: CreatorSyncCredentialStatus::Active,
        revision: 1,
        created_at: 100,
        updated_at: 101,
        last_seen_at: Some(101),
        revoked_at: None,
        protocol: CreatorSyncProtocol::ReacherCreatorSync,
        protocol_version: 2,
        capabilities: [CreatorSyncCapability::WriteChunkedQuarantine],
        accepted_bundle: CreatorSyncAcceptedBundle {
            format: "agent-platform.creator-snapshot".into(),
            format_version: 1,
            profile: CreatorSyncBundleProfile::PortablePublic,
            max_snapshot_bytes: 1024,
            max_chunk_bytes: 512,
            max_chunks: 2,
            max_namespace_staged_bytes: 2048,
            max_access_user_staged_bytes: 4096,
        },
    };
    platform
        .vault
        .save_creator_sync_credential(
            &metadata,
            &ServiceCredentialSecret::new(format!("{}_{}", metadata.credential_id, "A".repeat(43))),
        )
        .unwrap();
    platform
}

#[test]
fn planning_preserves_request_and_uses_published_defaults() {
    let draft = build_draft(&input(), &sources(), plan()).unwrap();
    assert_eq!(draft.id, input().id);
    assert_eq!(draft.brief, input().brief);
    assert_eq!(draft.criteria["min_subscribers"], 2000);
    assert_eq!(draft.criteria["region"], "");
    assert_eq!(draft.criteria["requires_ai_tool_demo"], false);
    assert_eq!(draft.criteria["requires_public_tool_url"], false);
    assert_eq!(draft.scopes[0].profile_ref, "browser");
    assert_eq!((draft.target_count, draft.max_calls), (30, Some(200)));
}

#[test]
fn understood_execution_settings_override_only_defaults_and_remain_quoted() {
    let mut request = input();
    request.brief = "在tiktok上搜索美妆博主5人，粉丝至少5000，最多读取20次".into();
    request.target_count = None;
    request.max_calls = None;
    let mut inventory = sources();
    inventory.sources[0].platform = "tiktok".into();
    let proposal = || {
        let mut p = plan();
        p.intent.original_request = request.brief.clone();
        p.intent.goal = "寻找美妆推广创作者".into();
        p.intent.category = ProposedConstraint::Specified { value: "美妆".into(), excerpt: "美妆".into() };
        p.intent.min_subscribers = ProposedConstraint::Specified { value: 5000, excerpt: "粉丝至少5000".into() };
        p.target_count = ProposedConstraint::Specified { value: 5, excerpt: "博主5人".into() };
        p.max_calls = ProposedConstraint::Specified { value: 20, excerpt: "最多读取20次".into() };
        p.scopes[0].platform = "tiktok".into();
        p.scopes[0].keywords = vec!["美妆".into()];
        p
    };
    let draft = build_draft(&request, &inventory, proposal()).unwrap();
    assert_eq!((draft.target_count, draft.max_calls), (5, Some(20)));
    assert_eq!(draft.criteria["min_subscribers"], 5000);
    assert_eq!(draft.criteria["category"], "美妆");
    let mut manual = request.clone();
    manual.target_count = Some(8);
    assert_eq!(build_draft(&manual, &inventory, proposal()).unwrap().target_count, 8);
    let mut invalid = proposal();
    invalid.target_count = ProposedConstraint::Specified { value: 5, excerpt: "不存在的要求".into() };
    assert_eq!(build_draft(&request, &inventory, invalid).unwrap_err(), DiscoveryDraftError::InvalidIntent);
}

#[tokio::test]
#[ignore = "authorized tool-free AI interpretation acceptance; no searches or Campaign writes"]
async fn native_codex_understands_full_sentence_before_planning() {
    for brief in [
        "在tiktok上搜索美妆博主5人",
        "我要推广口红，请找五位抖音美妆达人，粉丝至少5000，内容用中文",
        "手机支架",
    ] {
        let mut request = input();
        request.brief = format!("{brief}\nDefault search entries when no platform is requested: youtube");
        request.target_count = None;
        request.max_calls = None;
        let mut inventory = sources();
        for platform in ["tiktok", "douyin"] {
            let mut source = inventory.sources[0].clone();
            source.platform = platform.into();
            inventory.sources.push(source);
        }
        let proposal = codex::plan(&request.brief, &inventory).await.unwrap();
        let draft = build_draft(&request, &inventory, proposal).unwrap();
        let (platform, count, followers) = if brief.contains("tiktok") { ("tiktok", 5, 2000) }
            else if brief.contains("抖音") { ("douyin", 5, 5000) }
            else { ("youtube", 30, 2000) };
        assert_eq!(draft.target_count, count, "{brief}");
        assert_eq!(draft.criteria["min_subscribers"], followers, "{brief}");
        assert_eq!(draft.scopes.len(), 1, "{brief}");
        assert_eq!(draft.scopes[0].platform, platform, "{brief}");
        if brief.contains("抖音") {
            assert!(draft.criteria["category"].as_str().unwrap().contains("美妆"));
            assert!(draft.criteria["language"].as_str().unwrap().contains("中文"));
        }
    }
}

#[test]
fn planning_requires_all_registered_sources_unless_customer_explicitly_limits_them() {
    let mut inventory = sources();
    let mut instagram = inventory.sources[0].clone();
    instagram.platform = "instagram".into();
    instagram.command = "instagram/search".into();
    inventory.sources.push(instagram);
    assert_eq!(
        build_draft(&input(), &inventory, plan()).unwrap_err(),
        DiscoveryDraftError::IncompleteCoverage
    );

    let mut complete = plan();
    complete.scopes.insert(
        0,
        DraftScope {
            platform: "instagram".into(),
            keywords: vec!["shoe review".into()],
        },
    );
    let draft = build_draft(&input(), &inventory, complete).unwrap();
    assert_eq!(
        draft
            .scopes
            .iter()
            .map(|s| s.platform.as_str())
            .collect::<Vec<_>>(),
        vec!["instagram", "youtube"]
    );

    let mut request = input();
    request.brief.push_str("，只搜索 youtube");
    let mut limited = plan();
    limited.intent.original_request = request.brief.clone();
    limited
        .intent
        .excluded_platforms
        .push(onecrew_creator_runtime::QuotedPlatform {
            platform: "instagram".into(),
            excerpt: "只搜索 youtube".into(),
        });
    assert_eq!(
        build_draft(&request, &inventory, limited)
            .unwrap()
            .scopes
            .len(),
        1
    );
    let mut unquoted = plan();
    unquoted
        .intent
        .excluded_platforms
        .push(onecrew_creator_runtime::QuotedPlatform {
            platform: "instagram".into(),
            excerpt: "只搜索 youtube".into(),
        });
    assert!(build_draft(&input(), &inventory, unquoted).is_err());
}

#[test]
fn explicit_settings_override_ai_defaults_without_requiring_other_fields() {
    let mut request = input();
    request.min_followers_override = Some(10000);
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: Some("US".into()),
        language: None,
        category: None,
        requires_ai_tool_demo: true,
        requires_public_tool_url: false,
    });
    let draft = build_draft(&request, &sources(), plan()).unwrap();
    assert_eq!(draft.criteria["min_subscribers"], 10000);
    assert_eq!(draft.criteria["region"], "");
    assert_eq!(draft.criteria["language"], "");
    assert_eq!(draft.criteria["requires_ai_tool_demo"], true);
    request.min_followers_override = Some(0);
    assert_eq!(
        build_draft(&request, &sources(), plan()).unwrap().criteria["min_subscribers"],
        0
    );
    request.min_followers_override = Some(u64::MAX);
    assert_eq!(
        validate_input(&request),
        Err(DiscoveryDraftError::InvalidInput)
    );
}

#[test]
fn quoted_conditions_are_preserved_and_unquoted_conditions_cannot_create_a_draft() {
    let mut invented = plan();
    invented.intent.min_subscribers = ProposedConstraint::Specified {
        value: 10000,
        excerpt: "鞋子".into(),
    };
    assert_eq!(
        build_draft(&input(), &sources(), invented).unwrap_err(),
        DiscoveryDraftError::InvalidIntent
    );
    let mut unsupported = plan();
    unsupported
        .intent
        .other_hard_conditions
        .push(onecrew_creator_runtime::QuotedHardCondition {
            description: "保证便宜".into(),
            excerpt: "便宜".into(),
        });
    let saved = build_draft(&input(), &sources(), unsupported).unwrap();
    assert_eq!(saved.criteria["requirements"][0]["excerpt"], "便宜");
    assert_eq!(saved.criteria["contact_preference"], "public_email_first");
    let mut changed = plan();
    changed.intent.original_request = "别的需求".into();
    assert_eq!(
        build_draft(&input(), &sources(), changed).unwrap_err(),
        DiscoveryDraftError::InvalidIntent
    );
}

#[test]
fn additional_customer_conditions_survive_in_the_frozen_draft() {
    let mut request = input();
    request.brief = "找10位中国大陆的男明星按照粉丝排名".into();
    let mut proposal = plan();
    proposal.intent.original_request = request.brief.clone();
    proposal.intent.category = ProposedConstraint::Unspecified;
    proposal.intent.other_hard_conditions = ["男明星", "按照粉丝排名", "男明星"]
        .into_iter()
        .map(|excerpt| onecrew_creator_runtime::QuotedHardCondition {
            description: "untrusted model explanation".into(),
            excerpt: excerpt.into(),
        })
        .collect();
    proposal.intent.soft_preferences.clear();
    let saved = build_draft(&request, &sources(), proposal).unwrap();
    assert_eq!(saved.criteria["requirements"][0]["excerpt"], "男明星");
    assert_eq!(saved.criteria["requirements"][1]["excerpt"], "按照粉丝排名");
}

#[test]
fn fabricated_condition_excerpt_is_not_returned_as_a_clarification() {
    let mut proposal = plan();
    for excerpt in ["便宜", "not in the user request"] {
        proposal
            .intent
            .other_hard_conditions
            .push(onecrew_creator_runtime::QuotedHardCondition {
                description: "model output".into(),
                excerpt: excerpt.into(),
            });
    }
    assert_eq!(
        build_draft(&input(), &sources(), proposal).unwrap_err(),
        DiscoveryDraftError::InvalidIntent
    );
}

#[test]
fn empty_model_skeleton_is_reported_as_invalid_intent_not_a_saved_plan() {
    let mut empty = plan();
    empty.intent.original_request.clear();
    empty.intent.goal.clear();
    empty.intent.category = ProposedConstraint::Unspecified;
    empty.intent.suggested_keywords.clear();
    empty.scopes.clear();
    assert_eq!(
        build_draft(&input(), &sources(), empty).unwrap_err(),
        DiscoveryDraftError::InvalidIntent
    );
}

#[test]
fn supported_explicit_language_is_not_an_unsupported_hard_condition() {
    let mut request = input();
    request.brief.push_str("\nContent language: 英文");
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: None,
        language: Some("英文".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    let mut proposal = plan();
    proposal.intent.original_request = request.brief.clone();
    proposal
        .intent
        .other_hard_conditions
        .push(onecrew_creator_runtime::QuotedHardCondition {
            description: "English content".into(),
            excerpt: "Content language: 英文".into(),
        });
    proposal.scopes[0].keywords = vec!["affordable shoes review".into()];
    let draft = build_draft(&request, &sources(), proposal).unwrap();
    assert_eq!(draft.criteria["language"], "英文");
    assert_eq!(draft.criteria["requires_ai_tool_demo"], false);
}

#[tokio::test]
#[ignore = "bounded, tool-free planning call to the locally signed-in Codex; no Campaign write"]
async fn native_codex_keyboard_full_inventory_reproduction() {
    let mut request = input();
    request.brief = "我要卖键盘，找键盘测评或数码推荐达人。粉丝至少 8765，内容语言中文，目标 2 名推荐作者。\nContent language: 中文\nPreferred search keywords: keyboard review, mechanical keyboard review, best keyboards\nSearch only these entries: youtube, bilibili".into();
    request.target_count = Some(2);
    request.max_calls = Some(10000);
    let inventory = registered_inventory();
    let started = std::time::Instant::now();
    let phases = std::sync::Mutex::new(Vec::new());
    let proposal = codex::plan_with_progress(
        &request.brief,
        &inventory,
        &request.scope_overrides,
        &|phase| {
            phases
                .lock()
                .unwrap()
                .push((phase, started.elapsed().as_millis()));
        },
    )
    .await
    .unwrap();
    let draft = build_draft(&request, &inventory, proposal).unwrap();
    assert_eq!(draft.scopes.len(), 2);
    assert_eq!(draft.criteria["language"], "中文");
    assert_eq!(draft.criteria["min_subscribers"], 8765);
    for scope in draft.scopes {
        assert!(["youtube", "bilibili"].contains(&scope.platform.as_str()));
        assert!(
            scope.keywords[0]
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        );
    }
    eprintln!(
        "registered={}, phases={:?}, elapsed_ms={}",
        inventory.sources.len(),
        phases.into_inner().unwrap(),
        started.elapsed().as_millis()
    );
}

#[tokio::test]
#[ignore = "bounded, tool-free planning call to the locally signed-in Codex; no Campaign write"]
async fn native_codex_keyboard_acceptance_has_no_unsupported_conditions() {
    let mut request = input();
    request.brief = "我要卖键盘，找键盘测评或数码推荐达人。粉丝至少 8000，语言和地区不限，目标 2 名推荐作者，最多 20 次读取。\nSearch only these entries: youtube, bilibili".into();
    request.target_count = Some(2);
    request.max_calls = Some(20);
    let mut inventory = sources();
    let mut bilibili = inventory.sources[0].clone();
    bilibili.platform = "bilibili".into();
    inventory.sources.push(bilibili);
    let proposal = codex::plan(&request.brief, &inventory).await.unwrap();
    assert!(
        proposal.intent.other_hard_conditions.is_empty(),
        "unsupported conditions: {:?}",
        proposal.intent.other_hard_conditions
    );
    let draft = build_draft(&request, &inventory, proposal).unwrap();
    assert_eq!(draft.criteria["min_subscribers"], 8000);
    assert_eq!(draft.scopes.len(), 2);
}

#[tokio::test]
#[ignore = "bounded, tool-free planning call to the locally signed-in Codex; no Campaign write"]
async fn native_codex_phone_stand_with_language_plans_without_unsupported_conditions() {
    let mut request = input();
    request.brief = "手机支架\nContent language: 英文".into();
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: None,
        language: Some("英文".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    let proposal = codex::plan(&request.brief, &sources()).await.unwrap();
    let draft = build_draft(&request, &sources(), proposal).unwrap();
    assert_eq!(draft.criteria["language"], "英文");
    assert_eq!(draft.scopes.len(), 1);
}

#[test]
fn model_cannot_select_unknown_unregistered_excluded_duplicate_or_invalid_category_scopes() {
    let mut unknown = plan();
    unknown.scopes[0].platform = "unknown".into();
    assert!(build_draft(&input(), &sources(), unknown).is_err());
    let mut unregistered = sources();
    unregistered.sources[0].registered = false;
    assert!(build_draft(&input(), &unregistered, plan()).is_err());
    let mut duplicate = plan();
    duplicate.scopes.push(DraftScope {
        platform: "youtube".into(),
        keywords: vec!["shoe".into()],
    });
    assert!(build_draft(&input(), &sources(), duplicate).is_err());
    let mut excluded = plan();
    excluded
        .intent
        .excluded_platforms
        .push(onecrew_creator_runtime::QuotedPlatform {
            platform: "youtube".into(),
            excerpt: "鞋子".into(),
        });
    assert!(build_draft(&input(), &sources(), excluded).is_err());
    let mut scoped = sources();
    scoped.sources[0].allowed_values = vec!["allowed".into()];
    assert!(build_draft(&input(), &scoped, plan()).is_err());
}

#[tokio::test]
async fn unauthenticated_request_stops_before_native_planning() {
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    assert_eq!(
        platform.prepare_discovery_draft(input()).await.unwrap_err(),
        DiscoveryDraftError::AuthenticationRequired
    );
}

#[tokio::test]
async fn committed_draft_replays_without_browser_or_model_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let platform = authenticated(dir.path());
    let created = platform
        .creators()
        .unwrap()
        .create_discovery_campaign_async(build_draft(&input(), &sources(), plan()).unwrap())
        .await
        .unwrap();
    let replay = platform.prepare_discovery_draft(input()).await.unwrap();
    assert_eq!(created, replay);
    assert_eq!(replay.phase, "draft");
    assert_eq!(replay.used_calls, 0);
    for changed in [
        PrepareDiscoveryDraftInput {
            brief: "洗发水".into(),
            ..input()
        },
        PrepareDiscoveryDraftInput {
            profile_ref: "other".into(),
            ..input()
        },
        PrepareDiscoveryDraftInput {
            target_count: Some(200),
            ..input()
        },
        PrepareDiscoveryDraftInput {
            min_followers_override: Some(10000),
            ..input()
        },
        PrepareDiscoveryDraftInput {
            criteria_overrides: Some(DraftCriteriaOverrides {
                region: None,
                language: Some("中文".into()),
                category: None,
                requires_ai_tool_demo: false,
                requires_public_tool_url: false,
            }),
            ..input()
        },
    ] {
        assert_eq!(
            platform.prepare_discovery_draft(changed).await.unwrap_err(),
            DiscoveryDraftError::DraftConflict
        );
    }
    let _permit = platform.discovery_draft_gate.acquire().await.unwrap();
    assert_eq!(
        platform.prepare_discovery_draft(input()).await.unwrap_err(),
        DiscoveryDraftError::Busy
    );
    drop(_permit);
    drop(platform);
    let restarted = LocalPlatform::open(dir.path()).unwrap();
    assert_eq!(
        restarted.prepare_discovery_draft(input()).await.unwrap(),
        created
    );
    assert_eq!(
        restarted.check_draft_account("another-account").await,
        Err(DiscoveryDraftError::AccountChanged)
    );
}

#[test]
fn input_cannot_inject_command_model_path_account_or_approval() {
    for key in [
        "command",
        "model",
        "codex_path",
        "operator_account_id",
        "approved",
        "scopes",
    ] {
        let mut value = serde_json::to_value(input()).unwrap();
        value[key] = json!("untrusted");
        assert!(serde_json::from_value::<PrepareDiscoveryDraftInput>(value).is_err());
    }
    for changed in [
        PrepareDiscoveryDraftInput {
            id: "../file".into(),
            ..input()
        },
        PrepareDiscoveryDraftInput {
            max_calls: Some(0),
            ..input()
        },
        PrepareDiscoveryDraftInput {
            target_count: Some(10001),
            ..input()
        },
    ] {
        assert_eq!(
            validate_input(&changed),
            Err(DiscoveryDraftError::InvalidInput)
        );
    }
}

#[tokio::test]
#[ignore = "requires an explicitly authorized call to locally signed-in Codex"]
async fn native_codex_returns_a_valid_shoe_plan_without_plugin_transport() {
    let plan = codex::plan(&input().brief, &sources()).await.unwrap();
    let draft = build_draft(&input(), &sources(), plan).unwrap();
    assert_eq!(draft.scopes.len(), 1);
    assert_eq!(draft.scopes[0].platform, "youtube");
    assert_eq!(draft.criteria["min_subscribers"], 2000);
    assert_eq!(draft.criteria["requires_ai_tool_demo"], false);
}

#[tokio::test]
#[ignore = "uses actual MCP inventory and signed-in model; no provider read or Campaign write"]
async fn native_codex_preserves_exclusions_core_goal_and_email_preference() {
    let inventory_path = std::env::var("ONECREW_DRAFT_INVENTORY").expect("actual inventory path required");
    let inventory: OpenCliDiscoverySources = serde_json::from_str(
        &std::fs::read_to_string(inventory_path).unwrap()
    ).unwrap();
    assert!(inventory.extension_connected && inventory.daemon_running);
    let mut request = input();
    request.brief = "我要找美妆博主，帮我在tiktok, x 和 youtube上找粉丝数不少于5000的100个博主，排除赌博合作账号".into();
    if std::env::var_os("ONECREW_DRAFT_REFINE").is_some() {
        request.brief = format!(
            "原任务需求：\n我要找美妆博主，帮我在tiktok, x 和 youtube上找粉丝数不少于5000的100个博主\n已确认的计划设定：\n{}\n本次修改（仅覆盖明确修改的条件）：\n排除赌博合作账号，其他条件保持不变",
            json!({"criteria":{"category":"美妆","min_subscribers":5000,
                "requires_ai_tool_demo":true,"requires_public_tool_url":true},"target_count":100}),
        );
    }
    let all_sources = std::env::var_os("ONECREW_DRAFT_ALL_SOURCES").is_some();
    if all_sources {
        request.brief.push_str("\nSearch only these entries: ");
        request.brief.push_str(&inventory.sources.iter().filter(|source| source.registered)
            .map(|source| source.platform.as_str()).collect::<Vec<_>>().join(", "));
    }
    request.profile_ref = inventory.profiles.first().expect("actual browser profile").context_id.clone();
    request.opencli_version = inventory.version.clone().unwrap();
    request.target_count = None;
    request.max_calls = None;
    let proposal = codex::plan(&request.brief, &inventory).await.unwrap();
    let draft = build_draft(&request, &inventory, proposal).unwrap();
    assert_eq!(draft.target_count, 100);
    assert_eq!(draft.criteria["min_subscribers"], 5000);
    assert_eq!(draft.criteria["requires_ai_tool_demo"], false);
    assert_eq!(draft.criteria["requires_public_tool_url"], false);
    assert_eq!(draft.criteria["contact_preference"], "public_email_first");
    assert!(draft.criteria["requirements"].as_array().unwrap().iter().any(|item| {
        item["excerpt"].as_str().is_some_and(|quote| quote.contains("赌博"))
    }));
    let platforms: BTreeSet<_> = draft.scopes.iter().map(|scope| scope.platform.as_str()).collect();
    let expected = if all_sources {
        inventory.sources.iter().filter(|source| source.registered)
            .map(|source| source.platform.as_str()).collect()
    } else {
        BTreeSet::from(["tiktok", "twitter", "youtube"])
    };
    assert_eq!(platforms, expected);
}

#[tokio::test]
#[ignore = "bounded tool-free model validation; no Campaign write"]
async fn edited_stablecoin_plan_accepts_unrestricted_location_live() {
    let mut request = input();
    request.brief = "虚拟货币\nContent language: 中文\nContent category: 稳定币".into();
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: Some(String::new()),
        language: Some("中文".into()),
        category: Some("稳定币".into()),
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    validate_input(&request).unwrap();
    let plan = codex::plan(&request.brief, &sources()).await.unwrap();
    let draft = build_draft(&request, &sources(), plan).unwrap();
    assert_eq!(draft.criteria["region"], "");
    assert_eq!(draft.criteria["language"], "中文");
    assert_eq!(draft.criteria["category"], "稳定币");
}

#[tokio::test]
#[ignore = "bounded tool-free model with temporary Campaign store; no customer or provider write"]
async fn edited_stablecoin_many_sources_live() {
    let dir = tempfile::tempdir().unwrap();
    let platform = authenticated(dir.path());
    let sources: OpenCliDiscoverySources = platform.opencli_setup_report().await.into();
    let creators = platform.creators().unwrap();
    let previous = creators
        .create_discovery_campaign_async(build_draft(&input(), &self::sources(), plan()).unwrap())
        .await
        .unwrap();
    let previous = creators
        .approve_discovery_campaign_async(previous.id.clone(), previous.revision)
        .await
        .unwrap();
    let selected = "youtube, instagram, twitter, linkedin, linkedin-salesnav, facebook, pinterest, reddit, medium, archive, bluesky, jike, hackernews, linux-do, nowcoder, pixiv, stackoverflow, stackoverflow-users, hf-spaces, hf-models, hf-datasets, dribbble-designers, google, duckduckgo, yahoo";
    let keywords = "稳定币, USDT 中文, USDC 中文, USDT, USDC, 虚拟货币, 稳定币 中文 创作者, 稳定币 中文 分析, stablecoin 中文, 虚拟货币 中文, stablecoin Chinese, stablecoin, USDC 中文 分析, 稳定币 中文, 虚拟货币 中文 作者, 区块链 稳定币, 稳定币 科普, stablecoin infographic, 稳定币 图解, USDT 科普";
    let mut request = input();
    request.id = "edited-draft-test".into();
    request.profile_ref = sources
        .profiles
        .first()
        .expect("connected test browser context")
        .context_id
        .clone();
    request.brief = format!(
        "虚拟货币\nContent language: 中文\nContent category: 稳定币\nPreferred search keywords: {keywords}\nSearch only these entries: {selected}"
    );
    request.target_count = Some(5);
    request.max_calls = Some(10000);
    request.min_followers_override = Some(8000);
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: Some(String::new()),
        language: Some("中文".into()),
        category: Some("稳定币".into()),
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    request.scope_overrides = selected
        .split(", ")
        .map(|platform| DraftScopeOverride {
            platform: platform.into(),
            keywords: keywords.split(", ").map(String::from).collect(),
        })
        .collect();
    validate_input(&request).unwrap();
    let draft = platform
        .prepare_discovery_draft(request.clone())
        .await
        .unwrap();
    assert_eq!(draft.scopes.len(), 25);
    assert_eq!(draft.phase, "draft");
    assert_eq!(draft.used_calls, 0);
    assert_eq!(draft.target_count, 5);
    assert_eq!(draft.criteria["language"], "中文");
    assert_eq!(draft.criteria["category"], "稳定币");
    assert_eq!(draft.criteria["region"], "");
    assert!(request.scope_overrides.iter().all(|requested| {
        draft.scopes.iter().any(|scope| {
            scope.platform == requested.platform && scope.keywords == requested.keywords
        })
    }));
    assert_eq!(
        platform.prepare_discovery_draft(request).await.unwrap(),
        draft
    );
    assert_eq!(
        creators
            .get_discovery_campaign_async(previous.id.clone())
            .await
            .unwrap(),
        previous
    );
}

pub(super) fn registered_inventory() -> OpenCliDiscoverySources {
    inventory_from_fixture(include_str!("fixtures/registered-sources-2026-10-02.json"))
}

fn inventory_from_fixture(text: &str) -> OpenCliDiscoverySources {
    let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
    let mut inventory = sources();
    inventory.sources = fixture["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| OpenCliDiscoverySource {
            platform: source["platform"].as_str().unwrap().into(),
            command: source["command"].as_str().unwrap().into(),
            browser_required: source["browser_required"].as_bool().unwrap(),
            registered: source["registered"].as_bool().unwrap(),
            scope_kind: source["scope_kind"].as_str().unwrap().into(),
            allowed_values: source["allowed_values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().into())
                .collect(),
        })
        .collect();
    inventory
}

#[tokio::test]
#[ignore = "one tool-free live planning evaluation; no Campaign write or provider search"]
async fn live_mouse_queries_preserve_topic_across_retrieval_surfaces() {
    let mut request = input();
    request.brief = "寻找游戏鼠标的测评创作者\nContent language: 英文".into();
    let mut inventory =
        inventory_from_fixture(include_str!("fixtures/registered-sources-2026-10-11.json"));
    // Live source readback after the 0.6.88 adapter repair changed Pinterest's entry.
    // Preserve the older captured fixture rather than rewriting its history.
    inventory
        .sources
        .iter_mut()
        .find(|source| source.platform == "pinterest")
        .unwrap()
        .command = "onecrew-pinterest-reader/search-pins".into();
    let started = std::time::Instant::now();
    let proposal = codex::plan(&request.brief, &inventory).await.unwrap();
    assert_eq!(proposal.intent.original_request, request.brief);
    let draft = build_draft(&request, &inventory, proposal).unwrap();
    assert_eq!(draft.scopes.len(), 13);
    assert_eq!(draft.criteria["language"], "英文");
    for scope in &draft.scopes {
        assert!(!scope.keywords.is_empty());
        assert!(
            scope
                .keywords
                .iter()
                .all(|q| !q.chars().any(non_latin_script))
        );
        assert!(
            scope.keywords.iter().any(|q| {
                let q = q.to_lowercase();
                ["mouse", "mice", "gaming", "peripheral"]
                    .iter()
                    .any(|term| q.contains(term))
            }),
            "{} lost the requested product niche: {:?}",
            scope.platform,
            scope.keywords
        );
        if ["instagram", "linkedin", "pinterest"].contains(&scope.platform.as_str()) {
            assert!(
                !scope
                    .keywords
                    .iter()
                    .any(|q| q.eq_ignore_ascii_case("hardware reviewer")),
                "unqualified hardware reviewer reproduced wallet/irrelevant-name hits"
            );
        }
        if scope.platform == "reddit" {
            assert!(
                scope.keywords.iter().all(|query| {
                    query.to_lowercase().contains("\"gaming mouse\"")
                        && query.contains(" AND ")
                }),
                "Reddit queries still allow partial-topic natural-term matching: {:?}",
                scope.keywords
            );
        }
        eprintln!("{}: {:?}", scope.platform, scope.keywords);
    }
    eprintln!("planning elapsed_ms={}", started.elapsed().as_millis());
}

#[test]
fn manual_keywords_are_translated_before_english_draft_is_frozen() {
    let mut request = input();
    request
        .brief
        .push_str("\nContent language: 英文\nPreferred search keywords: 笔记本电脑测评");
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: None,
        language: Some("英文".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec!["笔记本电脑测评".into()],
    }];
    let mut proposal = plan();
    proposal.intent.original_request = request.brief.clone();
    proposal.scopes[0].keywords = vec!["laptop review".into()];
    let draft = build_draft(&request, &sources(), proposal).unwrap();
    assert_eq!(draft.scopes[0].keywords, vec!["laptop review"]);
}

#[test]
fn normalized_manual_terms_deduplicate_and_reject_wrong_language_fallback() {
    let mut request = input();
    request
        .brief
        .push_str("\nContent language: 英文\nPreferred search keywords: 笔记本测评, laptop review");
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: None,
        language: Some("英文".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec!["笔记本测评".into(), "laptop review".into()],
    }];
    let mut proposal = plan();
    proposal.intent.original_request = request.brief.clone();
    proposal.scopes[0].keywords = vec!["laptop review".into(), "laptop review".into()];
    proposal.intent.suggested_keywords = vec!["laptop review".into(), "laptop review".into()];
    assert_eq!(
        build_draft(&request, &sources(), proposal).unwrap().scopes[0].keywords,
        vec!["laptop review"]
    );
    let mut invalid = plan();
    invalid.intent.original_request = request.brief.clone();
    invalid.scopes[0].keywords = request.scope_overrides[0].keywords.clone();
    assert_eq!(
        build_draft(&request, &sources(), invalid).unwrap_err(),
        DiscoveryDraftError::InvalidScope
    );
}

#[tokio::test]
#[ignore = "one tool-free live AI planning call; no Campaign write or platform search"]
async fn live_manual_laptop_terms_follow_english_language() {
    let mut request = input();
    request.brief = "我要卖笔记本电脑\nContent language: 英文\nPreferred search keywords: 笔记本电脑测评, laptop review, 联想 ThinkPad 测评, 华硕 ASUS Zenbook 测评".into();
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: None,
        language: Some("英文".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec![
            "笔记本电脑测评".into(),
            "laptop review".into(),
            "联想 ThinkPad 测评".into(),
            "华硕 ASUS Zenbook 测评".into(),
        ],
    }];
    let inventory = sources();
    let proposal = codex::plan_with_progress(
        &request.brief,
        &inventory,
        &request.scope_overrides,
        &|_| {},
    )
    .await
    .unwrap();
    let draft = build_draft(&request, &inventory, proposal).unwrap();
    let queries = &draft.scopes[0].keywords;
    assert!(queries.iter().all(|q| !q.chars().any(non_latin_script)));
    assert!(queries.iter().any(|q| q.contains("ThinkPad")));
    assert!(queries.iter().any(|q| q.contains("Zenbook")));
    eprintln!("normalized queries: {queries:?}");
}

#[test]
fn multilingual_contact_terms_are_frozen_without_becoming_hard_conditions() {
    let mut proposal = plan();
    proposal.contact_search_terms = vec!["yhteystiedot".into(), "yhteistyö".into()];
    let draft = build_draft(&input(), &sources(), proposal).unwrap();
    assert_eq!(draft.criteria["contact_search_terms"][1], "yhteistyö");
    let mut invalid = plan();
    invalid.contact_search_terms = vec!["site:example.com".into()];
    assert_eq!(
        build_draft(&input(), &sources(), invalid).unwrap_err(),
        DiscoveryDraftError::InvalidPlan
    );
}

#[tokio::test]
#[ignore = "tool-free native multilingual planning; no Campaign or provider write"]
async fn live_finnish_contact_terms_share_existing_planning_call() {
    let mut request = input();
    request.brief = "我要卖笔记本电脑\nContent language: 芬兰语\nPreferred search keywords: ThinkPad laptop review".into();
    request.criteria_overrides = Some(DraftCriteriaOverrides {
        region: None,
        language: Some("芬兰语".into()),
        category: None,
        requires_ai_tool_demo: false,
        requires_public_tool_url: false,
    });
    request.scope_overrides = vec![DraftScopeOverride {
        platform: "youtube".into(),
        keywords: vec!["ThinkPad laptop review".into()],
    }];
    let proposal = codex::plan_with_progress(
        &request.brief,
        &sources(),
        &request.scope_overrides,
        &|_| {},
    )
    .await
    .unwrap();
    assert!(!proposal.contact_search_terms.is_empty());
    assert!(
        proposal
            .contact_search_terms
            .iter()
            .any(|term| term.to_lowercase().contains("yhte")
                || term.to_lowercase().contains("sähkö"))
    );
    let draft = build_draft(&request, &sources(), proposal).unwrap();
    eprintln!(
        "Finnish queries={:?}; contact terms={}",
        draft.scopes[0].keywords, draft.criteria["contact_search_terms"]
    );
}
