use super::*;
use onecrew_local_runtime::ToolkitId;

#[test]
fn phase_three_onecrew_read_retention_fixture_is_exact() {
    use onecrew_creator_runtime as creator_runtime;

    const FKR: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators];
    const FKO: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Compact, SurfaceId::Outreach];
    const FKOM: &[SurfaceId] = &[
        SurfaceId::Full,
        SurfaceId::Compact,
        SurfaceId::Outreach,
        SurfaceId::Mail,
    ];
    const FKM: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Compact, SurfaceId::Mail];
    const FKAM: &[SurfaceId] = &[
        SurfaceId::Full,
        SurfaceId::Compact,
        SurfaceId::Accounts,
        SurfaceId::Mail,
    ];
    const FM: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Mail];

    let retained = [
        ("onecrew.creator.content_reply.prepare", FKR),
        ("onecrew.creator.list", FKR),
        ("onecrew.creator.group.list", FKR),
        ("onecrew.creator.get", FKR),
        ("onecrew.creator.identity.resolve", FKR),
        ("onecrew.outreach.contact_route.last_contact_account", FKO),
        ("onecrew.outreach.conversation.get", FKO),
        ("onecrew.outreach.conversation.list", FKO),
        ("onecrew.outreach.action.status", FKO),
        ("onecrew.triage.list", FKOM),
        ("email.mailbox.get", FKAM),
        ("email.mailbox.sync_status", FKM),
        ("email.folder.list", FKM),
        ("email.thread.list", FKM),
        ("email.thread.get", FKM),
        ("email.message.get", FKM),
        ("email.template.list", FKM),
        ("email.template.get", FKM),
        ("email.template.render", FKM),
        ("email.draft.list", FKM),
        ("email.draft.get", FKM),
        ("email.direct.prepare", FM),
        ("email.manual.prepare", FM),
    ];
    assert_eq!(retained.len(), 23);

    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();

    let retained_ids = retained
        .iter()
        .map(|(id, _)| (*id).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(retained_ids.len(), retained.len());
    for (id, expected_surfaces) in retained {
        let operation = registry_operation(&registry, id);
        assert_eq!(operation.effect, EffectClass::ReadLocal, "{id}");
        assert_eq!(operation.secret, SecretClass::None, "{id}");
        assert_eq!(operation.approval, ApprovalClass::None, "{id}");
        assert_eq!(operation.connection, ConnectionRequirement::None, "{id}");
        assert!(operation.sensitive_input_paths.is_empty(), "{id}");
        assert_eq!(
            operation.metadata.surfaces.iter().collect::<Vec<_>>(),
            expected_surfaces,
            "{id}"
        );
    }

    macro_rules! assert_schema {
        ($id:literal, $input:ty, $output:ty) => {{
            let operation = registry_operation(&registry, $id);
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

    assert_schema!(
        "onecrew.creator.content_reply.prepare",
        creator_runtime::PrepareContentReplyInput,
        creator_runtime::ContentReplyPreparation
    );
    assert_schema!(
        "onecrew.creator.list",
        creator_runtime::ListCreatorsInput,
        creator_runtime::CreatorListPage
    );
    assert_schema!(
        "onecrew.creator.group.list",
        creator_runtime::ListCreatorGroupsInput,
        creator_runtime::CreatorGroupListResult
    );
    assert_schema!(
        "onecrew.creator.get",
        creator_runtime::CreatorIdInput,
        creator_runtime::CreatorRecord
    );
    assert_schema!(
        "onecrew.creator.identity.resolve",
        creator_runtime::ResolveCreatorIdentityInput,
        creator_runtime::IdentityResolution
    );
    assert_schema!(
        "onecrew.outreach.contact_route.last_contact_account",
        creator_runtime::ContactRouteLastContactAccountInput,
        Option<creator_runtime::ContactRouteLastContactAccount>
    );
    assert_schema!(
        "onecrew.outreach.conversation.get",
        creator_runtime::ConversationGetInput,
        Option<creator_runtime::ConversationView>
    );
    assert_schema!(
        "onecrew.outreach.conversation.list",
        creator_runtime::ListConversationsInput,
        creator_runtime::Page<creator_runtime::ConversationSummary>
    );
    assert_schema!(
        "onecrew.outreach.action.status",
        creator_runtime::OutreachActionStatusInput,
        creator_runtime::CreatorOutreachStatus
    );
    assert_schema!(
        "onecrew.triage.list",
        creator_runtime::ListReplyTriageInput,
        creator_runtime::Page<creator_runtime::ReplyTriageView>
    );
    assert_schema!(
        "email.mailbox.get",
        creator_runtime::MailboxGetInput,
        creator_runtime::MailboxGetOutcome
    );
    assert_schema!(
        "email.mailbox.sync_status",
        creator_runtime::MailboxGetInput,
        creator_runtime::MailboxSyncStatus
    );
    assert_schema!(
        "email.folder.list",
        creator_runtime::ListEmailFoldersInput,
        Vec<creator_runtime::EmailFolderSummary>
    );
    assert_schema!(
        "email.thread.list",
        creator_runtime::ListEmailThreadsInput,
        creator_runtime::Page<creator_runtime::EmailThreadSummary>
    );
    assert_schema!(
        "email.thread.get",
        creator_runtime::EmailThreadGetInput,
        creator_runtime::EmailThreadView
    );
    assert_schema!(
        "email.message.get",
        creator_runtime::EmailMessageGetInput,
        creator_runtime::EmailMessageView
    );
    assert_schema!(
        "email.template.list",
        creator_runtime::ListEmailTemplatesInput,
        creator_runtime::Page<creator_runtime::EmailTemplateSummary>
    );
    assert_schema!(
        "email.template.get",
        creator_runtime::EmailTemplateGetInput,
        creator_runtime::EmailTemplate
    );
    assert_schema!(
        "email.template.render",
        creator_runtime::RenderEmailTemplateInput,
        creator_runtime::RenderedEmailTemplate
    );
    assert_schema!(
        "email.draft.list",
        creator_runtime::ListEmailDraftsInput,
        creator_runtime::Page<creator_runtime::EmailDraftSummary>
    );
    assert_schema!(
        "email.draft.get",
        creator_runtime::EmailDraftGetInput,
        creator_runtime::EmailDraft
    );
    assert_schema!(
        "email.direct.prepare",
        creator_runtime::PrepareEmailInput,
        creator_runtime::EmailPreparation
    );
    assert_schema!(
        "email.manual.prepare",
        creator_runtime::PrepareManualEmailInput,
        creator_runtime::ManualEmailPreparation
    );
}

#[test]
fn onecrew_read_execution_matches_direct_owners_for_paging_keys_and_optional_inputs() {
    use onecrew_creator_runtime as creator_runtime;

    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let runtime = platform.creators().unwrap();
    let mailbox_ref_id = creator_runtime::MailboxRefId::new("33333333-3333-4333-8333-333333333333");
    runtime
        .sync_mailbox_ref(creator_runtime::MailboxRefInput {
            id: mailbox_ref_id.clone(),
            provider: "resend".into(),
            label: "Outreach".into(),
            email_address: "sender@example.com".into(),
            display_name: Some("Sender".into()),
            capabilities: vec![creator_runtime::EmailCapability::Send],
            available: false,
            last_error: Some("PRIVATE_LAST_ERROR_SENTINEL".into()),
        })
        .unwrap();
    let operator_accounts: Arc<dyn crate::accounts::OperatorAccountWorkflow> = Arc::new(
        crate::accounts::PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
    );
    let registry = crate::operation_registry::build_registry(
        crate::operation_registry::RegistryOwners {
            platform,
            creators: runtime.clone(),
            operator_accounts,
        },
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();

    let mailbox_arguments = json!({"mailbox_ref_id":mailbox_ref_id});
    let mailbox_input = serde_json::from_value(mailbox_arguments.clone()).unwrap();
    let cases = vec![
        (
            "onecrew.creator.list",
            json!({
                "limit":2,
                "cursor":null,
                "query":null,
                "stages":[],
                "group":null,
                "include_archived":false
            }),
            serde_json::to_value(
                runtime
                    .list_creators(creator_runtime::ListCreatorsInput {
                        limit: Some(2),
                        cursor: None,
                        query: None,
                        stages: Vec::new(),
                        group: None,
                        include_archived: false,
                        ..Default::default()
                    })
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "onecrew.creator.group.list",
            json!({
                "limit":2,
                "cursor":null,
                "query":null,
                "include_archived":true
            }),
            serde_json::to_value(
                runtime
                    .list_creator_groups(creator_runtime::ListCreatorGroupsInput {
                        limit: Some(2),
                        cursor: None,
                        query: None,
                        include_archived: true,
                    })
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "email.folder.list",
            json!({"mailbox_ref_id":null}),
            serde_json::to_value(
                runtime
                    .list_email_folders(creator_runtime::ListEmailFoldersInput {
                        mailbox_ref_id: None,
                    })
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "email.mailbox.get",
            mailbox_arguments,
            serde_json::to_value(runtime.get_mailbox(mailbox_input).unwrap()).unwrap(),
        ),
    ];

    for (id, arguments, expected) in cases {
        let actual = execute_local(&registry, id, arguments).unwrap();
        assert_eq!(actual, expected, "{id}");
        if id == "email.mailbox.get" {
            let wire = actual.to_string();
            assert!(wire.contains("mailbox_ref_id"));
            assert!(wire.contains("recorded_failure"));
            assert!(!wire.contains("last_error"));
            assert!(!wire.contains("PRIVATE_LAST_ERROR_SENTINEL"));
        }
    }
}

#[test]
fn onecrew_read_string_failures_are_bounded_known_no_effect_without_correct_input_guidance() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let failure = execute_local(
        &registry,
        "onecrew.creator.get",
        json!({"creator_id":"missing-creator"}),
    )
    .unwrap_err();

    assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation);
    assert!(!failure.retry_safe);
    assert!(!failure.effect_may_have_occurred);
    assert_eq!(
        failure.kind,
        ExecuteFailureKind::OwnerRejected {
            code: PublicFailureCode::parse("creator_read_unavailable").unwrap(),
            effect: OwnerEffectState::KnownNoEffect,
            recovery: None,
            provider_status: None,
            provider_evidence: None,
            message: None,
            dm_failure: None,
        }
    );
}

#[test]
fn link_hub_extract_is_an_open_world_read_without_a_connection_that_pins_its_output_keys() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let operation = registry_operation(&registry, "onecrew.link_hub.extract");
    assert_eq!(operation.effect, EffectClass::ReadOpenWorld);
    assert_eq!(operation.approval, ApprovalClass::None);
    assert_eq!(operation.connection, ConnectionRequirement::None);
    assert_eq!(operation.secret, SecretClass::None);
    assert_eq!(operation.metadata.toolkit, ToolkitId::OneCrew);
    assert_eq!(
        operation.metadata.surfaces.iter().collect::<Vec<_>>(),
        [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators]
    );
    assert_eq!(
        operation.input.document()["required"],
        json!(["url"]),
        "the URL is the only input"
    );

    // The extraction stays free of Creator vocabulary: the page's layers,
    // links and addresses as found, nothing classified.
    let output = operation.output.document();
    assert_eq!(output["required"], json!(["source_url", "links", "emails"]));
    assert_eq!(
        output["properties"]["links"]["items"]["required"],
        json!(["layer", "url", "text"])
    );
    assert_eq!(
        output["properties"]["emails"]["items"]["required"],
        json!(["layer", "value"])
    );
    let rendered = serde_json::to_string(output).unwrap();
    for absent in [
        "channel",
        "locator",
        "route",
        "contact",
        "candidate",
        "creator",
    ] {
        assert!(!rendered.contains(absent), "extract output names {absent}");
    }
    for layer in ["\"anchor\"", "\"json_ld\"", "\"hydration\""] {
        assert!(
            rendered.contains(layer),
            "extract output names the {layer} layer"
        );
    }
}

#[test]
fn link_hub_extract_refuses_unsupported_urls_before_any_request_and_echoes_only_the_host() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    for (url, code, forbidden, required) in [
        (
            "file:///etc/passwd",
            "link_hub_url_invalid",
            "passwd",
            "invalid link hub URL",
        ),
        (
            "https://user:pw@linktr.ee/creator?token=SECRET",
            "link_hub_url_invalid",
            "SECRET",
            "invalid link hub URL",
        ),
        (
            "https://example.com/creator?token=SECRET",
            "link_hub_host_unsupported",
            "SECRET",
            "unsupported link hub host: example.com",
        ),
    ] {
        let failure =
            execute_local(&registry, "onecrew.link_hub.extract", json!({"url": url})).unwrap_err();
        assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation, "{url}");
        assert!(!failure.effect_may_have_occurred, "{url}");
        let projected = project_execute_failure(&failure);
        assert_eq!(projected.code.as_str(), code, "{url}");
        assert_eq!(projected.effect, ExecuteEffectState::KnownNoEffect, "{url}");
        assert_eq!(
            projected.recovery,
            Some(RecoveryDirective::CorrectInput),
            "{url}"
        );
        let wire = serde_json::to_string(&projected).unwrap();
        assert!(!wire.contains(forbidden), "{url}: {wire}");
        assert!(wire.contains(required), "{url}: {wire}");
    }
}
