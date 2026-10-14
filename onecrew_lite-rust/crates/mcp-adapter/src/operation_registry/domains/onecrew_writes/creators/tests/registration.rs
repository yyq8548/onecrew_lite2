use crate::operation_registry::descriptor::{
    ApprovalClass, AuditMode, ConfirmationMode, EffectClass, IdempotencyMode, ReadbackMode,
    SecretClass, SurfaceId,
};
use crate::operation_registry::domains::onecrew_writes::creators::test_support::{
    execute_current, operation, registry,
};
use crate::operation_registry::execution::ExecutionContext;
use crate::operation_registry::schema::CompiledSchema;
use onecrew_creator_runtime::{
    BulkMoveCreatorsToGroupInput, CreateCreatorGroupInput, CreatorGroup, DeleteCreatorGroupInput,
    DeleteCreatorGroupResult, ManualCreatorBatchCreateInput, ManualCreatorBatchCreateResult,
    MoveCreatorsToGroupResult, SetCreatorGroupInput, UpdateCreatorGroupInput,
};
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn target_increase_requires_confirmation_and_trusted_account_with_closed_input() {
    use crate::operation_registry::execution::project_execute_failure;
    use serde_json::json;
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = registry(platform);
    let target = operation(&registry, "onecrew.discovery.target.increase");
    assert_eq!(target.effect, EffectClass::WriteLocal);
    assert_eq!(target.approval, ApprovalClass::AttendedPrompt);
    assert_eq!(
        target.safety.idempotency.mode,
        IdempotencyMode::OwnerInputIdentity
    );
    assert_eq!(target.safety.idempotency.identity_pointers.len(), 1);
    let input = json!({"campaign_id":"campaign", "expected_revision":1,
        "expected_target_count":30, "new_target_count":200, "decision_id":"target-200"});
    for field in ["operator_account_id", "verdict"] {
        let mut forged = input.clone();
        forged[field] = json!("forged");
        assert!(
            execute_current(
                target,
                ExecutionContext {
                    surface: SurfaceId::Full,
                    request_id: "target-forged".into(),
                    connection: None,
                    confirmed: true,
                },
                forged
            )
            .is_err()
        );
    }
    assert!(
        execute_current(
            target,
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: "target-unconfirmed".into(),
                connection: None,
                confirmed: false,
            },
            input.clone()
        )
        .is_err()
    );
    let error = execute_current(
        target,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "target-no-account".into(),
            connection: None,
            confirmed: true,
        },
        input,
    )
    .unwrap_err();
    assert_eq!(
        project_execute_failure(&error).code.as_str(),
        "library_identity_unavailable"
    );
}

#[test]
fn public_page_read_is_local_scoped_and_preserves_the_owner_receipt() {
    use onecrew_creator_runtime::*;
    use serde_json::json;
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let owner = platform.creators().unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "page-peer".into(),
            brief: "AI tool evidence".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 30,
            max_calls: Some(5),
        })
        .unwrap();
    owner.approve_discovery_campaign("page-peer", 1).unwrap();
    let claim = owner
        .claim_discovery_search("page-peer", 1, "youtube", "dedicated")
        .unwrap();
    let source = "https://www.youtube.com/watch?v=LR04bU_yV5k";
    let url = "https://example.com/tool";
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: source.into(),
                title: "Demo".into(),
                author_label: "Creator".into(),
                author_id: None,
                author_url: None,
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "page-peer".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let claim = owner
        .claim_discovery_video("page-peer", 1, "dedicated", &hit.id)
        .unwrap();
    owner.finish_discovery_video(FinishDiscoveryVideoInput {claim,stop_code:None,candidate:Some(UpsertDiscoveryCandidateInput {
        campaign_id:"page-peer".into(),expected_revision:1,platform:"youtube".into(),source_profile_ref:"dedicated".into(),
        platform_account_id:"UC1234567890".into(),display_name:"Creator".into(),profile_url:"https://www.youtube.com/channel/UC1234567890".into(),
        evidence:json!({"source_urls":[source],"video_description":format!("Use {url}"),"public_links":[url]}),
    })}).unwrap();
    let candidate = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "page-peer".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let claim = owner
        .claim_discovery_public_page(DiscoveryPublicPageInput {
            campaign_id: "page-peer".into(),
            expected_revision: 1,
            profile_ref: "dedicated".into(),
            candidate_id: candidate.id.clone(),
            requested_url: url.into(),
        })
        .unwrap();
    let registry = registry(platform);
    let read = operation(&registry, "onecrew.discovery.public_page.get");
    assert_eq!(read.effect, EffectClass::ReadLocal);
    assert_eq!(
        read.input.document(),
        CompiledSchema::for_input::<DiscoveryPublicPageReadInput>()
            .unwrap()
            .document()
    );
    let context = |surface| ExecutionContext {
        surface,
        request_id: "page-local".into(),
        connection: None,
        confirmed: false,
    };
    let input = json!({"campaign_id":"page-peer","candidate_id":candidate.id,"requested_url":url});
    let reading = execute_current(read, context(SurfaceId::Full), input.clone()).unwrap();
    assert_eq!(reading["status"], "reading");
    assert!(reading["document_url"].is_null());
    let receipt = owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some(url.into()),
            title: "Tool".into(),
            markdown_excerpt: "Observed tool page".into(),
            truncated: true,
            stop_code: None,
        })
        .unwrap();
    let review = operation(&registry, "onecrew.discovery.review_material.get");
    assert_eq!(review.effect, EffectClass::ReadLocal);
    assert_eq!(
        review.input.document(),
        CompiledSchema::for_input::<DiscoveryReviewMaterialInput>()
            .unwrap()
            .document()
    );
    let review_input =
        json!({"campaign_id":"page-peer","expected_revision":1,"search_hit_id":hit.id});
    let material = owner
        .prepare_discovery_review_material("page-peer", 1, &hit.id)
        .unwrap();
    for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
        assert_eq!(
            execute_current(review, context(surface), review_input.clone()).unwrap(),
            serde_json::to_value(&material).unwrap()
        );
    }
    for wrong in [
        json!({"campaign_id":"page-peer","expected_revision":0,"search_hit_id":hit.id}),
        json!({"campaign_id":"page-peer","expected_revision":2,"search_hit_id":hit.id}),
        json!({"campaign_id":"page-peer","expected_revision":1,"search_hit_id":hit.id,"verdict":"qualified"}),
    ] {
        assert!(execute_current(review, context(SurfaceId::Full), wrong).is_err());
    }
    for surface in [
        SurfaceId::Accounts,
        SurfaceId::Outreach,
        SurfaceId::Mail,
        SurfaceId::Skills,
    ] {
        assert!(
            registry
                .entry(
                    &crate::operation_registry::descriptor::EntryId::parse(
                        "onecrew.discovery.review_material.get"
                    )
                    .unwrap(),
                    surface
                )
                .is_none()
        );
    }
    for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
        for _ in 0..2 {
            assert_eq!(
                execute_current(read, context(surface), input.clone()).unwrap(),
                serde_json::to_value(&receipt).unwrap()
            );
        }
    }
    for field in ["campaign_id", "candidate_id", "requested_url", "unexpected"] {
        let mut wrong = input.clone();
        wrong[field] = json!("wrong");
        assert!(execute_current(read, context(SurfaceId::Full), wrong).is_err());
    }
    for surface in [
        SurfaceId::Accounts,
        SurfaceId::Outreach,
        SurfaceId::Mail,
        SurfaceId::Skills,
    ] {
        assert!(
            registry
                .entry(
                    &crate::operation_registry::descriptor::EntryId::parse(
                        "onecrew.discovery.public_page.get"
                    )
                    .unwrap(),
                    surface
                )
                .is_none()
        );
    }
    assert_eq!(
        owner
            .get_discovery_campaign("page-peer")
            .unwrap()
            .used_calls,
        3
    );
    assert_eq!(
        owner
            .count_discovery_candidates("page-peer")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn identity_read_projects_exact_owner_receipt_without_spending_budget() {
    use onecrew_creator_runtime::*;
    use serde_json::json;
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let owner = platform.creators().unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "identity-peer".into(),
            brief: "Read-only receipt parity".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":10000}),
            target_count: 30,
            max_calls: Some(3),
            scopes: vec![DiscoveryPlatformScope {
                platform: "twitter".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
        })
        .unwrap();
    owner
        .approve_discovery_campaign("identity-peer", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("identity-peer", 1, "twitter", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://x.com/i/status/123".into(),
                title: "Demo".into(),
                author_label: "Creator".into(),
                author_id: Some("creator".into()),
                author_url: Some("https://x.com/creator".into()),
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "identity-peer".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let claim = owner
        .claim_discovery_identity("identity-peer", 1, &hit.id)
        .unwrap();
    let receipt = owner
        .finish_discovery_identity(FinishDiscoveryIdentityInput {
            claim,
            account_id: Some("987".into()),
            observed_source_url: Some("https://x.com/creator/status/123".into()),
            stop_code: None,
        })
        .unwrap();
    let registry = registry(platform);
    let read = operation(&registry, "onecrew.discovery.identity.get");
    assert_eq!(read.effect, EffectClass::ReadLocal);
    for _ in 0..2 {
        let result = execute_current(
            read,
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: "identity-local".into(),
                connection: None,
                confirmed: false,
            },
            json!({"campaign_id":"identity-peer","search_hit_id":hit.id}),
        )
        .unwrap();
        assert_eq!(result, serde_json::to_value(&receipt).unwrap());
    }
    for input in [
        json!({"campaign_id":"other","search_hit_id":hit.id}),
        json!({"campaign_id":"identity-peer","search_hit_id":hit.id,"account_id":"forged"}),
    ] {
        assert!(
            execute_current(
                read,
                ExecutionContext {
                    surface: SurfaceId::Full,
                    request_id: "identity-invalid".into(),
                    connection: None,
                    confirmed: false
                },
                input
            )
            .is_err()
        );
    }
    assert_eq!(
        owner
            .get_discovery_campaign("identity-peer")
            .unwrap()
            .used_calls,
        2
    );
    assert_eq!(
        owner
            .count_discovery_candidates("identity-peer")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn discovery_campaign_mcp_reads_the_same_durable_draft_as_the_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let creators = platform.creators().unwrap();
    let registry = registry(platform);
    let sources = operation(&registry, "onecrew.discovery.sources.get");
    assert_eq!(sources.effect, EffectClass::ReadLocal);
    let inventory = execute_current(
        sources,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "campaign-sources".into(),
            connection: None,
            confirmed: false,
        },
        serde_json::json!({}),
    )
    .unwrap();
    assert!(inventory["sources"].is_array());
    assert!(inventory["profiles"].is_array());
    assert!(inventory["extension_connected"].is_boolean());
    for source in inventory["sources"].as_array().unwrap() {
        assert!(source["platform"].is_string());
        assert!(source["registered"].is_boolean());
        assert!(source["scope_kind"].is_string());
        assert!(source["allowed_values"].is_array());
    }
    let create = operation(&registry, "onecrew.discovery.campaign.create");
    assert_eq!(create.effect, EffectClass::WriteLocal);
    let draft = execute_current(create, ExecutionContext {
        surface: SurfaceId::Full, request_id: "campaign-create".into(),
        connection: None, confirmed: false,
    }, serde_json::json!({
        "id": "mcp-draft", "brief": "AI tool creators", "opencli_version": "1.8.8",
        "criteria": {"min_subscribers": 10000},
        "scopes": [{"platform": "youtube", "profile_ref": "dedicated", "keywords": ["AI tool demo"]}],
        "target_count": 30, "max_calls": 80
    })).unwrap();
    assert_eq!(draft["phase"], "draft");
    assert_eq!(
        creators
            .get_discovery_campaign("mcp-draft")
            .unwrap()
            .target_count,
        30
    );
    let readback = execute_current(
        operation(&registry, "onecrew.discovery.campaign.get"),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "campaign-get".into(),
            connection: None,
            confirmed: false,
        },
        serde_json::json!({"campaign_id": "mcp-draft"}),
    )
    .unwrap();
    assert_eq!(readback["criteria"]["min_subscribers"], 10000);
    let approve = operation(&registry, "onecrew.discovery.campaign.approve");
    assert_eq!(approve.approval, ApprovalClass::AttendedPrompt);
    assert_eq!(
        approve.safety.confirmation.mode,
        ConfirmationMode::AttendedPrompt
    );
    for id in [
        "onecrew.discovery.advance",
        "onecrew.discovery.search_once",
        "onecrew.discovery.youtube.search_once",
        "onecrew.discovery.youtube.verify_hit_once",
    ] {
        let read = operation(&registry, id);
        assert_eq!(read.effect, EffectClass::WriteLocal);
        assert_eq!(read.safety.readback.mode, ReadbackMode::InlineAndFollowUp);
    }
    let advance = operation(&registry, "onecrew.discovery.advance");
    assert!(
        execute_current(
            advance,
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: "unapproved-advance".into(),
                connection: None,
                confirmed: false,
            },
            serde_json::json!({"campaign_id": "mcp-draft", "expected_revision": 1})
        )
        .is_err()
    );
    assert_eq!(
        creators
            .get_discovery_campaign("mcp-draft")
            .unwrap()
            .used_calls,
        0
    );
}

#[test]
fn transcript_mcp_readback_matches_durable_owner_without_more_provider_budget() {
    use onecrew_creator_runtime::{
        CreateDiscoveryCampaignInput, DiscoveryPlatformScope, DiscoverySearchHitInput,
        DiscoveryTranscriptReadInput, DiscoveryTranscriptSegment, FinishDiscoverySearchInput,
        FinishDiscoveryTranscriptInput, FinishDiscoveryVideoInput, ListDiscoverySearchHitsInput,
        UpsertDiscoveryCandidateInput,
    };
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let creators = platform.creators().unwrap();
    let registry = registry(platform);
    creators
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "readback".into(),
            brief: "AI creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_followers":10000}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 30,
            max_calls: Some(10),
        })
        .unwrap();
    creators.approve_discovery_campaign("readback", 1).unwrap();
    let claim = creators
        .claim_discovery_search("readback", 1, "youtube", "dedicated")
        .unwrap();
    let source = "https://www.youtube.com/watch?v=LR04bU_yV5k";
    creators
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: source.into(),
                title: "Demo".into(),
                author_label: "Creator".into(),
                author_id: Some("UC1234567890".into()),
                author_url: Some("https://www.youtube.com/channel/UC1234567890".into()),
            }],
        })
        .unwrap();
    let hit = creators
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "readback".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
        let hits = execute_current(
            operation(&registry, "onecrew.discovery.hits.list"),
            ExecutionContext {
                surface,
                request_id: "author-readback".into(),
                connection: None,
                confirmed: false,
            },
            serde_json::json!({"campaign_id":"readback","after_id":null,"limit":10}),
        )
        .unwrap();
        assert_eq!(hits, serde_json::to_value(vec![hit.clone()]).unwrap());
        assert!(hit.author_observed_at.is_some());
    }
    let claim = creators
        .claim_discovery_video("readback", 1, "dedicated", &hit.id)
        .unwrap();
    creators
        .finish_discovery_video(FinishDiscoveryVideoInput {
            claim,
            stop_code: None,
            candidate: Some(UpsertDiscoveryCandidateInput {
                campaign_id: "readback".into(),
                expected_revision: 1,
                platform: "youtube".into(),
                source_profile_ref: "dedicated".into(),
                platform_account_id: "UC1234567890".into(),
                display_name: "Creator".into(),
                profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
                evidence: serde_json::json!({"source_urls":[source]}),
            }),
        })
        .unwrap();
    let claim = creators
        .claim_discovery_transcript("readback", 1, "dedicated", &hit.id)
        .unwrap();
    let saved = creators
        .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
            claim,
            truncated: true,
            stop_code: None,
            segments: vec![DiscoveryTranscriptSegment {
                timestamp: "1:23".into(),
                speaker: "".into(),
                text: "I open the tool.".into(),
            }],
        })
        .unwrap();
    let read = operation(&registry, "onecrew.discovery.transcript.get");
    assert_eq!(read.effect, EffectClass::ReadLocal);
    assert_eq!(
        read.input.document(),
        CompiledSchema::for_input::<DiscoveryTranscriptReadInput>()
            .unwrap()
            .document()
    );
    for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
        let result = execute_current(
            read,
            ExecutionContext {
                surface,
                request_id: "transcript-get".into(),
                connection: None,
                confirmed: false,
            },
            serde_json::json!({"campaign_id":"readback","search_hit_id":hit.id}),
        )
        .unwrap();
        assert_eq!(result, serde_json::to_value(&saved).unwrap());
    }
    assert_eq!(
        creators
            .get_discovery_campaign("readback")
            .unwrap()
            .used_calls,
        3
    );
    assert_eq!(
        creators
            .count_discovery_candidates("readback")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn profile_mcp_readback_preserves_native_receipt_without_provider_or_budget() {
    use onecrew_creator_runtime::*;
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let creators = platform.creators().unwrap();
    let registry = registry(platform);
    creators
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "profile-readback".into(),
            brief: "AI creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_followers":10000}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "bilibili".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 30,
            max_calls: Some(10),
        })
        .unwrap();
    creators
        .approve_discovery_campaign("profile-readback", 1)
        .unwrap();
    let claim = creators
        .claim_discovery_search("profile-readback", 1, "bilibili", "dedicated")
        .unwrap();
    creators
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.bilibili.com/video/BV1RYqHYnEw8".into(),
                title: "Demo".into(),
                author_label: "Creator".into(),
                author_id: Some("123".into()),
                author_url: Some("https://space.bilibili.com/123".into()),
            }],
        })
        .unwrap();
    let hit = creators
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "profile-readback".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let claim = creators
        .claim_discovery_profile("profile-readback", 1, &hit.id)
        .unwrap();
    let saved = creators.finish_discovery_profile(FinishDiscoveryProfileInput {claim,
        native_profile:Some(serde_json::json!({"mid":"123","profile_url":"https://space.bilibili.com/123","follower_count":15000})),stop_code:None}).unwrap();
    let read = operation(&registry, "onecrew.discovery.profile.get");
    assert_eq!(read.effect, EffectClass::ReadLocal);
    assert_eq!(
        read.input.document(),
        CompiledSchema::for_input::<DiscoveryProfileReadInput>()
            .unwrap()
            .document()
    );
    for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
        let result = execute_current(
            read,
            ExecutionContext {
                surface,
                request_id: "profile-get".into(),
                connection: None,
                confirmed: false,
            },
            serde_json::json!({"campaign_id":"profile-readback","search_hit_id":hit.id}),
        )
        .unwrap();
        assert_eq!(result, serde_json::to_value(&saved).unwrap());
    }
    for surface in [
        SurfaceId::Accounts,
        SurfaceId::Outreach,
        SurfaceId::Mail,
        SurfaceId::Skills,
    ] {
        assert!(
            registry
                .entry(
                    &crate::operation_registry::descriptor::EntryId::parse(
                        "onecrew.discovery.profile.get"
                    )
                    .unwrap(),
                    surface
                )
                .is_none()
        );
    }
    assert_eq!(
        creators
            .get_discovery_campaign("profile-readback")
            .unwrap()
            .used_calls,
        2
    );
    assert_eq!(
        creators
            .count_discovery_candidates("profile-readback")
            .unwrap()
            .qualified,
        0
    );
}

const EXPECTED_IDS: [&str; 19] = [
    "onecrew.creator.content_reply.send",
    "onecrew.creator.archive",
    "onecrew.creator.candidate.save",
    "onecrew.creator.candidate.batch.save",
    "onecrew.creator.contact_route.add",
    "onecrew.creator.contact_route.delete.confirm",
    "onecrew.creator.contact_route.delete.prepare",
    "onecrew.creator.contact_route.update",
    "onecrew.creator.delete",
    "onecrew.creator.export",
    "onecrew.creator.manual.create",
    "onecrew.creator.batch.create",
    "onecrew.creator.update",
    "onecrew.creator.stage.update",
    "onecrew.creator.group.create",
    "onecrew.creator.group.update",
    "onecrew.creator.group.delete",
    "onecrew.creator.group.set",
    "onecrew.creator.group.bulk_move",
];

#[test]
fn fixture_registers_exactly_the_19_creator_write_ids_on_current_surfaces() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let platform = Arc::new(
        onecrew_local_runtime::LocalPlatform::open(directory.path()).expect("local platform"),
    );
    let registry = registry(platform);
    let expected = EXPECTED_IDS.into_iter().collect::<BTreeSet<_>>();
    let actual = registry
        .operation_ids_and_surfaces()
        .into_iter()
        .filter(|(id, _)| expected.contains(id.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<BTreeSet<_>>(),
        expected
    );
    for (id, surfaces) in actual {
        assert_eq!(
            surfaces,
            vec![SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators],
            "surfaces for {id}"
        );
        let operation = operation(&registry, id.as_str());
        assert_eq!(operation.secret, SecretClass::None);
        assert_eq!(
            operation.connection,
            crate::operation_registry::descriptor::ConnectionRequirement::None
        );
    }

    let attended = EXPECTED_IDS
        .into_iter()
        .filter(|id| operation(&registry, id).approval == ApprovalClass::AttendedPrompt)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        attended,
        BTreeSet::from([
            "onecrew.creator.batch.create",
            "onecrew.creator.candidate.save",
            "onecrew.creator.candidate.batch.save",
            "onecrew.creator.contact_route.delete.confirm",
            "onecrew.creator.content_reply.send",
            "onecrew.creator.delete",
            "onecrew.creator.group.delete",
        ])
    );
    for id in EXPECTED_IDS {
        let operation = operation(&registry, id);
        if attended.contains(id) {
            let prompt = &operation
                .approval_request
                .as_ref()
                .expect("attended operation has a static prompt")
                .message;
            assert!(!prompt.contains("operation_id"));
            assert!(!prompt.contains("creator_id"));
        } else {
            assert_eq!(operation.approval, ApprovalClass::OwnerGuarded);
            assert!(operation.approval_request.is_none());
        }
    }
}

#[test]
fn discovery_review_is_account_bound_and_has_idempotent_readback() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = registry(platform);
    let submit = operation(&registry, "onecrew.discovery.review.submit");
    let get = operation(&registry, "onecrew.discovery.review.get");
    assert_eq!(submit.effect, EffectClass::WriteLocal);
    assert_eq!(get.effect, EffectClass::ReadLocal);
    assert_eq!(
        submit.safety.idempotency.mode,
        IdempotencyMode::OwnerInputIdentity
    );
    assert_eq!(submit.safety.readback.mode, ReadbackMode::InlineAndFollowUp);
    assert_eq!(
        submit.safety.readback.operation_ids[0].as_str(),
        "onecrew.discovery.review.get"
    );
    for schema in [submit.input.document(), get.input.document()] {
        let properties = schema["properties"].as_object().unwrap();
        assert!(!properties.contains_key("operator_account_id"));
        assert!(!properties.contains_key("verdict"));
    }
}

#[test]
fn identity_review_contracts_exclude_forged_facts_and_preserve_audit_readback() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = registry(platform);
    for id in [
        "onecrew.discovery.identity.material.get",
        "onecrew.discovery.identity.groups.get",
        "onecrew.discovery.identity.review.get",
        "onecrew.discovery.identity.review.submit",
        "onecrew.discovery.identity.review.revoke",
    ] {
        let entry = operation(&registry, id);
        let schema = entry.input.document();
        assert_eq!(schema["additionalProperties"], false);
        let properties = schema["properties"].as_object().unwrap();
        for forbidden in [
            "operator_account_id",
            "native_author_id",
            "groups",
            "verdict",
        ] {
            assert!(!properties.contains_key(forbidden), "{id}: {forbidden}");
        }
        if id.ends_with(".get") {
            assert_eq!(entry.effect, EffectClass::ReadLocal);
        } else {
            assert_eq!(entry.effect, EffectClass::WriteLocal);
            assert_eq!(entry.safety.audit, AuditMode::OwnerRecorded);
            assert_eq!(
                entry.safety.idempotency.mode,
                IdempotencyMode::OwnerInputIdentity
            );
            assert_eq!(entry.safety.readback.mode, ReadbackMode::InlineAndFollowUp);
            assert_eq!(
                entry.safety.readback.operation_ids[0].as_str(),
                "onecrew.discovery.identity.review.get"
            );
        }
    }
}

#[test]
fn safety_contracts_construct_with_owner_pointers_readbacks_and_typed_recovery() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let platform = Arc::new(
        onecrew_local_runtime::LocalPlatform::open(directory.path()).expect("local platform"),
    );
    let registry = registry(platform);

    let reply = operation(&registry, "onecrew.creator.content_reply.send");
    assert_eq!(reply.effect, EffectClass::ExternalWrite);
    assert_eq!(
        reply.safety.confirmation.mode,
        ConfirmationMode::AttendedPrompt
    );
    assert!(reply.safety.confirmation.parameter_pointers.is_empty());
    assert_eq!(
        reply.binding.output_class(),
        crate::operation_registry::execution::BindingOutputClass::OwnerReview
    );
    assert!(
        reply.input.document()["properties"]
            .get("confirmed")
            .is_none()
    );
    assert_eq!(
        reply.safety.idempotency.mode,
        IdempotencyMode::OwnerInputIdentity
    );
    assert_eq!(reply.safety.audit, AuditMode::OwnerRecorded);
    assert_eq!(
        reply.safety.readback.mode,
        ReadbackMode::InlineOwnerVerified
    );
    assert!(reply.approval_request.is_some());

    let delete = operation(&registry, "onecrew.creator.contact_route.delete.confirm");
    assert_eq!(
        delete.safety.confirmation.mode,
        ConfirmationMode::AttendedPromptAndOwnerParameters
    );
    assert_eq!(
        delete.safety.confirmation.parameter_pointers[0].as_str(),
        "/operation_id"
    );
    assert_eq!(
        delete.safety.warning_output_pointers[0].as_str(),
        "/remote_not_deleted"
    );
    assert_eq!(
        delete.safety.recovery_output_pointers[0].as_str(),
        "/next_tool"
    );

    for id in [
        "onecrew.creator.candidate.save",
        "onecrew.creator.candidate.batch.save",
    ] {
        let candidate = operation(&registry, id);
        assert_eq!(candidate.effect, EffectClass::WriteLocal);
        assert_eq!(
            candidate.safety.confirmation.mode,
            ConfirmationMode::AttendedPrompt
        );
        assert!(candidate.safety.confirmation.parameter_pointers.is_empty());
        assert!(candidate.approval_request.is_some());
        assert_eq!(
            candidate.binding.output_class(),
            crate::operation_registry::execution::BindingOutputClass::OwnerReview
        );
        assert_eq!(
            candidate.output.document(),
            CompiledSchema::for_output::<onecrew_local_runtime::CreatorSaveCallResult>()
                .unwrap()
                .document()
        );
    }
    for id in [
        "onecrew.creator.candidate.save.confirm",
        "onecrew.creator.candidate.batch.save.confirm",
        "onecrew.creator.community_administrators.discover",
    ] {
        for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
            assert!(
                registry
                    .entry(
                        &crate::operation_registry::descriptor::EntryId::parse(id).unwrap(),
                        surface
                    )
                    .is_none()
            );
        }
    }
    // The batch is written only by consuming the grant its review mints, so no
    // input field carries confirmation. A `/confirmed` pointer here was checked
    // at registration and never read at execution, which let the model's own
    // flag both refuse a batch the user had confirmed and authorize one they had
    // not yet seen.
    let batch_create = operation(&registry, "onecrew.creator.batch.create");
    assert_eq!(
        batch_create.safety.confirmation.mode,
        ConfirmationMode::AttendedPrompt
    );
    assert!(
        batch_create
            .safety
            .confirmation
            .parameter_pointers
            .is_empty()
    );
    assert_eq!(
        batch_create.binding.output_class(),
        crate::operation_registry::execution::BindingOutputClass::OwnerReview
    );
    assert!(
        batch_create.input.document()["properties"]
            .get("confirmed")
            .is_none(),
        "the Creator batch has no wire confirmation bypass"
    );
    assert!(batch_create.approval_request.is_some());
    assert_eq!(
        batch_create.safety.idempotency.mode,
        IdempotencyMode::OwnerInputIdentity
    );
    assert_eq!(batch_create.safety.audit, AuditMode::OwnerRecorded);

    let group_contracts = [
        (
            "onecrew.creator.group.create",
            EffectClass::WriteLocal,
            ApprovalClass::OwnerGuarded,
            ConfirmationMode::None,
            IdempotencyMode::None,
            &[][..],
            "onecrew.creator.group.list",
        ),
        (
            "onecrew.creator.group.update",
            EffectClass::WriteLocal,
            ApprovalClass::OwnerGuarded,
            ConfirmationMode::None,
            IdempotencyMode::OwnerInputIdentity,
            &["/group_id", "/expected_revision", "/name"][..],
            "onecrew.creator.group.list",
        ),
        (
            "onecrew.creator.group.delete",
            EffectClass::DeleteLocal,
            ApprovalClass::AttendedPrompt,
            ConfirmationMode::AttendedPrompt,
            IdempotencyMode::OwnerInputIdentity,
            &["/group_id", "/expected_revision"][..],
            "onecrew.creator.group.list",
        ),
        (
            "onecrew.creator.group.set",
            EffectClass::WriteLocal,
            ApprovalClass::OwnerGuarded,
            ConfirmationMode::None,
            IdempotencyMode::OwnerInputIdentity,
            &["/creator_id", "/destination"][..],
            "onecrew.creator.get",
        ),
        (
            "onecrew.creator.group.bulk_move",
            EffectClass::WriteLocal,
            ApprovalClass::OwnerGuarded,
            ConfirmationMode::None,
            IdempotencyMode::OwnerInputIdentity,
            &["/creator_ids", "/destination"][..],
            "onecrew.creator.list",
        ),
    ];
    for (id, effect, approval, confirmation, idempotency, pointers, readback) in group_contracts {
        let operation = operation(&registry, id);
        assert_eq!(operation.effect, effect, "{id}");
        assert_eq!(operation.approval, approval, "{id}");
        assert_eq!(operation.safety.confirmation.mode, confirmation, "{id}");
        assert_eq!(operation.safety.idempotency.mode, idempotency, "{id}");
        assert_eq!(
            operation
                .safety
                .idempotency
                .identity_pointers
                .iter()
                .map(|pointer| pointer.as_str())
                .collect::<Vec<_>>(),
            pointers,
            "{id}"
        );
        assert_eq!(
            operation.safety.readback.mode,
            ReadbackMode::InlineAndFollowUp
        );
        assert_eq!(
            operation.safety.readback.operation_ids[0].as_str(),
            readback
        );
    }

    macro_rules! assert_group_schema {
        ($id:literal, $input:ty, $output:ty) => {{
            let operation = operation(&registry, $id);
            assert_eq!(
                operation.input.document(),
                CompiledSchema::for_input::<$input>().unwrap().document(),
                "{} input",
                $id
            );
            assert_eq!(
                operation.output.document(),
                CompiledSchema::for_output::<$output>().unwrap().document(),
                "{} output",
                $id
            );
        }};
    }
    assert_group_schema!(
        "onecrew.creator.group.create",
        CreateCreatorGroupInput,
        CreatorGroup
    );
    assert_group_schema!(
        "onecrew.creator.group.update",
        UpdateCreatorGroupInput,
        CreatorGroup
    );
    assert_group_schema!(
        "onecrew.creator.group.delete",
        DeleteCreatorGroupInput,
        DeleteCreatorGroupResult
    );
    assert_group_schema!(
        "onecrew.creator.group.set",
        SetCreatorGroupInput,
        MoveCreatorsToGroupResult
    );
    assert_group_schema!(
        "onecrew.creator.group.bulk_move",
        BulkMoveCreatorsToGroupInput,
        MoveCreatorsToGroupResult
    );
    assert_group_schema!(
        "onecrew.creator.batch.create",
        ManualCreatorBatchCreateInput,
        ManualCreatorBatchCreateResult
    );
    let batch_schema = operation(&registry, "onecrew.creator.batch.create")
        .input
        .document();
    assert_eq!(
        batch_schema["properties"]["items"]["minItems"], 1,
        "manual Creator batch requires one item"
    );
    assert_eq!(
        batch_schema["properties"]["items"]["maxItems"], 50,
        "manual Creator batch business limit is independent of MCP call count"
    );
    for id in [
        "onecrew.creator.group.create",
        "onecrew.creator.group.update",
    ] {
        assert_eq!(
            operation(&registry, id).input.document()["properties"]["name"]["maxLength"],
            64,
            "{id} name bound"
        );
    }
    let bulk_input = operation(&registry, "onecrew.creator.group.bulk_move")
        .input
        .document();
    assert_eq!(bulk_input["properties"]["creator_ids"]["minItems"], 1);
    assert_eq!(bulk_input["properties"]["creator_ids"]["maxItems"], 100);
}

#[test]
fn creator_save_wire_schemas_are_typed_bounded_and_require_server_result_context() {
    use crate::operation_registry::execution::{
        ExecutionContext, dispatch_prepared_output, prepare_invocation, project_execute_failure,
    };
    use onecrew_local_runtime::{CreatorSaveCandidateInput, PrepareCreatorSaveInput};
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = registry(Arc::clone(&platform));
    let candidate = serde_json::json!({
        "intent": "save_or_update",
        "target": {"platform":"x","user_id":"1","handle":"person0"},
        "connection": {"platform":"x","account_ref_id":"00000000-0000-0000-0000-000000000001"},
        "provenance": {"kind":"direct_selection","source_url":"https://x.com/person0","description":"Explicit user selection"},
        "notes":null,"tags":null,"quality":{}
    });
    for (id, arguments, expected_schema) in [
        (
            "onecrew.creator.candidate.save",
            candidate.clone(),
            CompiledSchema::for_input::<CreatorSaveCandidateInput>().unwrap(),
        ),
        (
            "onecrew.creator.candidate.batch.save",
            serde_json::json!({"candidates":[candidate]}),
            CompiledSchema::for_input::<PrepareCreatorSaveInput>().unwrap(),
        ),
    ] {
        let operation = operation(&registry, id);
        assert_eq!(operation.input.document(), expected_schema.document());
        let prepared = prepare_invocation(
            operation,
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: "unbound-prepare".into(),
                connection: None,
                confirmed: false,
            },
            operation.input.revision(),
            arguments,
        )
        .unwrap();
        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(dispatch_prepared_output(prepared))
            .unwrap_err();
        assert_eq!(
            project_execute_failure(&error).code.as_str(),
            "creator_save_result_context_required"
        );
    }
    let batch = operation(&registry, "onecrew.creator.candidate.batch.save")
        .input
        .document();
    assert_eq!(batch["properties"]["candidates"]["minItems"], 1);
    assert_eq!(batch["properties"]["candidates"]["maxItems"], 50);
    assert!(
        platform
            .creators()
            .unwrap()
            .list_creators(Default::default())
            .unwrap()
            .items
            .is_empty()
    );
}
