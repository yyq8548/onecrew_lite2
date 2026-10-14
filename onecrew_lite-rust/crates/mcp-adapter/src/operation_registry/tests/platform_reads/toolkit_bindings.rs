//! The profile, channel and user-posts reads bind to the runtime's toolkit
//! functions: connection admission runs inside the toolkit, the output is a
//! typed envelope around the provider's own object, and no candidate or
//! identity projection is attached. No provider is reachable here, so every
//! execution stops at admission or input validation.
use super::*;

fn production_registry() -> (
    tempfile::TempDir,
    crate::operation_registry::OperationRegistry,
) {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    (directory, registry)
}

fn owner_rejected(
    failure: &crate::operation_registry::execution::ExecuteFailure,
) -> (String, Option<RecoveryDirective>, Option<String>) {
    match &failure.kind {
        ExecuteFailureKind::OwnerRejected {
            code,
            recovery,
            message,
            ..
        } => (
            code.as_str().to_owned(),
            recovery.clone(),
            message.as_ref().map(|message| message.as_str().to_owned()),
        ),
        other => panic!("expected an owner rejection, got {other:?}"),
    }
}

#[test]
fn toolkit_reads_return_the_provider_object_under_an_open_envelope_without_identity_resolution() {
    let (_directory, registry) = production_registry();
    for (id, envelope_key) in [
        ("x.profile.get", "profile"),
        ("instagram.profile.get", "profile"),
        ("youtube.channel.get", "channel"),
        ("x.user_posts.list", "page"),
    ] {
        let operation = registry_operation(&registry, id);
        assert_eq!(operation.effect, EffectClass::ReadOpenWorld, "{id}");
        assert_eq!(operation.approval, ApprovalClass::None, "{id}");
        let output = operation.output.document();
        assert_eq!(output["properties"][envelope_key]["type"], "object", "{id}");
        assert_eq!(output["required"], json!([envelope_key]), "{id}");
        assert_eq!(output["additionalProperties"], json!(false), "{id}");
        let rendered = serde_json::to_string(output).unwrap();
        for absent in [
            "identity_resolution",
            "already_saved_creator_id",
            "candidate_id",
            "discovery_id",
            "hits",
        ] {
            assert!(!rendered.contains(absent), "{id} output names {absent}");
        }
        assert_eq!(
            operation
                .metadata
                .summary
                .contains("onecrew.creator.identity.resolve"),
            envelope_key != "page",
            "{id}: the profile and channel gets point to identity resolution for the already-saved question"
        );
    }
}

#[test]
fn toolkit_reads_admit_the_connection_inside_the_toolkit_and_keep_the_connection_codes() {
    let (_directory, registry) = production_registry();
    let operator = |toolkit, id: &str| ConnectionRef::OperatorAccount {
        toolkit,
        operator_account_id: id.to_owned(),
    };
    let unknown = uuid_like();
    for (id, connection, arguments, code, recovery, message) in [
        (
            "x.profile.get",
            operator(OperatorToolkitId::X, "not-a-uuid"),
            json!({"handle": "someone"}),
            "connection_id_invalid",
            Some(RecoveryDirective::CorrectInput),
            "connection identifier is invalid",
        ),
        (
            "x.profile.get",
            operator(OperatorToolkitId::X, &unknown),
            json!({"handle": "someone"}),
            "connection_not_found_or_mismatch",
            Some(RecoveryDirective::CorrectInput),
            "connection was not found or does not match the requested kind and platform",
        ),
        (
            "instagram.profile.get",
            operator(OperatorToolkitId::Instagram, &unknown),
            json!({"handle": "someone"}),
            "connection_not_found_or_mismatch",
            Some(RecoveryDirective::CorrectInput),
            "connection was not found or does not match the requested kind and platform",
        ),
        (
            "youtube.channel.get",
            ConnectionRef::ServiceCredential {
                toolkit: ServiceToolkitId::Youtube,
                credential_id: unknown.clone(),
            },
            json!({"channel_id": "UC_x5XG1OV2P6uZZ5FSM9Ttw", "depth": "light"}),
            "connection_not_found_or_mismatch",
            Some(RecoveryDirective::CorrectInput),
            "connection was not found or does not match the requested kind and platform",
        ),
        (
            "x.user_posts.list",
            operator(OperatorToolkitId::X, &unknown),
            json!({"external_user_id": "12345"}),
            "connection_not_found_or_mismatch",
            Some(RecoveryDirective::CorrectInput),
            "connection was not found or does not match the requested kind and platform",
        ),
        (
            "telegram.community.get",
            operator(OperatorToolkitId::Telegram, &unknown),
            json!({"locator":{"type":"public_username","username":"community"}}),
            "connection_not_found_or_mismatch",
            Some(RecoveryDirective::CorrectInput),
            "connection was not found or does not match the requested kind and platform",
        ),
        (
            "telegram.community.administrators.list",
            operator(OperatorToolkitId::Telegram, &unknown),
            json!({"locator":{"type":"stable_community","external_id":"123","kind":"supergroup"}}),
            "connection_not_found_or_mismatch",
            Some(RecoveryDirective::CorrectInput),
            "connection was not found or does not match the requested kind and platform",
        ),
    ] {
        let failure = execute_registered(&registry, id, &connection, arguments).unwrap_err();
        assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation, "{id}");
        assert!(!failure.effect_may_have_occurred, "{id}");
        assert_eq!(
            owner_rejected(&failure),
            (code.to_owned(), recovery, Some(message.to_owned())),
            "{id}"
        );
    }
}

#[test]
fn telegram_community_reads_use_closed_locator_inputs_and_require_operator_connections() {
    let (_directory, registry) = production_registry();
    for id in [
        "telegram.community.get",
        "telegram.community.administrators.list",
    ] {
        let operation = registry_operation(&registry, id);
        assert_eq!(operation.effect, EffectClass::ReadOpenWorld);
        assert_eq!(operation.approval, ApprovalClass::None);
        assert_eq!(
            operation.connection,
            ConnectionRequirement::OperatorAccount {
                toolkit: OperatorToolkitId::Telegram
            }
        );
        assert!(!operation.binding.supports_search_plan());
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(operation.input.document())
            .unwrap();
        for valid in [
            json!({"locator":{"type":"public_username","username":"community"}}),
            json!({"locator":{"type":"stable_community","external_id":"123","kind":"channel"}}),
            json!({"locator":{"type":"stable_community","external_id":"456","kind":"supergroup"}}),
        ] {
            assert!(validator.is_valid(&valid), "{id}: {valid}");
        }
        for invalid in [
            json!({"locator":{"type":"public_username","username":"community","access_hash":"secret"}}),
            json!({"locator":{"type":"stable_community","external_id":"123","kind":"basic_group"}}),
            json!({"locator":{"type":"public_username","username":"community"},"confirmed":true}),
            json!({"creator_id":"old-direct-persist-target"}),
        ] {
            assert!(!validator.is_valid(&invalid), "{id}: {invalid}");
        }
    }
}

#[test]
fn toolkit_reads_reject_empty_provider_input_before_admission() {
    let (_directory, registry) = production_registry();
    let x = ConnectionRef::OperatorAccount {
        toolkit: OperatorToolkitId::X,
        operator_account_id: "not-a-uuid".to_owned(),
    };
    for (id, arguments, message) in [
        (
            "x.profile.get",
            json!({"handle": " @ "}),
            "X handle is required",
        ),
        (
            "x.user_posts.list",
            json!({"external_user_id": "  ", "cursor": null}),
            "X user id is required",
        ),
    ] {
        let failure = execute_registered(&registry, id, &x, arguments).unwrap_err();
        assert_eq!(
            owner_rejected(&failure),
            (
                "platform_read_input_invalid".to_owned(),
                Some(RecoveryDirective::CorrectInput),
                Some(message.to_owned()),
            ),
            "{id}"
        );
    }
    let youtube = ConnectionRef::ServiceCredential {
        toolkit: ServiceToolkitId::Youtube,
        credential_id: "not-a-uuid".to_owned(),
    };
    let failure = execute_registered(
        &registry,
        "youtube.channel.get",
        &youtube,
        json!({"channel_id": "@handle", "depth": "light"}),
    )
    .unwrap_err();
    assert_eq!(
        owner_rejected(&failure).0,
        "platform_read_input_invalid",
        "a handle is not a channel id"
    );

    let projected = project_execute_failure(&failure);
    let wire = serde_json::to_value(&projected).unwrap();
    assert_eq!(wire["code"], "platform_read_input_invalid");
    assert_eq!(
        wire["message"],
        "YouTube channel id must be a stable 24-character UC identifier"
    );
}

fn uuid_like() -> String {
    // A well-formed identifier that no vault row carries.
    "00000000-0000-4000-8000-000000000042".to_owned()
}
