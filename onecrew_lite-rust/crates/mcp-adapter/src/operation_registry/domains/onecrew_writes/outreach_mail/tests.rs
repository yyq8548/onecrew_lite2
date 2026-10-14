use std::{collections::BTreeSet, sync::Arc};

use crate::operation_registry::{
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, EffectClass, EntryId, IdempotencyMode,
        ReadbackMode, RegistryEntry, SurfaceId,
    },
    execution::{ExecuteFailureKind, ExecuteFailurePhase, ExecutionContext},
    registration::OperationRegistry,
};

const FKO: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Compact, SurfaceId::Outreach];
const FKOM: &[SurfaceId] = &[
    SurfaceId::Full,
    SurfaceId::Compact,
    SurfaceId::Outreach,
    SurfaceId::Mail,
];
const FKM: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Compact, SurfaceId::Mail];
const FM: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Mail];

fn production_registry() -> OperationRegistry {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let operator_accounts = Arc::new(
        crate::accounts::PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
    );
    crate::operation_registry::build_registry(
        crate::operation_registry::RegistryOwners {
            creators: platform.creators().unwrap(),
            platform,
            operator_accounts,
        },
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap()
}

fn operation<'a>(
    registry: &'a OperationRegistry,
    id: &str,
) -> &'a crate::operation_registry::descriptor::OperationEntry {
    let id = EntryId::parse(id).unwrap();
    let RegistryEntry::Operation(operation) = registry.entry(&id, SurfaceId::Full).unwrap() else {
        unreachable!()
    };
    operation
}

#[test]
fn outreach_mail_write_retention_fixture_is_exact() {
    let expected = [
        (
            "onecrew.outreach.contact_target.resolve",
            EffectClass::SyncOpenWorld,
            FKO,
        ),
        (
            "onecrew.outreach.relationship.get",
            EffectClass::SyncOpenWorld,
            FKO,
        ),
        (
            "onecrew.outreach.relationship.prepare",
            EffectClass::SyncOpenWorld,
            FKO,
        ),
        (
            "onecrew.outreach.relationship.update",
            EffectClass::ExternalWrite,
            FKO,
        ),
        (
            "onecrew.outreach.message.prepare",
            EffectClass::SyncOpenWorld,
            FKO,
        ),
        (
            "onecrew.outreach.message.send",
            EffectClass::ExternalWrite,
            FKO,
        ),
        ("x.message.send", EffectClass::ExternalWrite, FKO),
        ("x.conversation.sync", EffectClass::SyncOpenWorld, FKO),
        ("instagram.message.send", EffectClass::ExternalWrite, FKO),
        (
            "instagram.conversation.sync",
            EffectClass::SyncOpenWorld,
            FKO,
        ),
        ("telegram.message.send", EffectClass::ExternalWrite, FKO),
        (
            "telegram.conversation.sync",
            EffectClass::SyncOpenWorld,
            FKO,
        ),
        ("onecrew.triage.record", EffectClass::WriteLocal, FKOM),
        ("onecrew.triage.acknowledge", EffectClass::WriteLocal, FKOM),
        ("email.draft.restore", EffectClass::WriteLocal, FKM),
        ("email.draft.save", EffectClass::WriteLocal, FKM),
        ("email.draft.send", EffectClass::ExternalWrite, FKM),
        ("email.draft.trash", EffectClass::WriteLocal, FKM),
        ("email.mailbox.sync", EffectClass::SyncOpenWorld, FKM),
        ("email.manual.send", EffectClass::ExternalWrite, FM),
        ("email.direct.send", EffectClass::ExternalWrite, FM),
        ("email.send_many", EffectClass::ExternalWrite, FKM),
        ("email.send_many.reconcile", EffectClass::ExternalWrite, FKM),
        ("email.template.delete", EffectClass::DeleteLocal, FKM),
        ("email.template.save", EffectClass::WriteLocal, FKM),
        (
            "email.thread.update_disposition",
            EffectClass::WriteLocal,
            FKM,
        ),
    ];
    assert_eq!(expected.len(), 26);
    assert_eq!(
        expected
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<BTreeSet<_>>()
            .len(),
        expected.len()
    );

    let registry = production_registry();
    for (id, effect, surfaces) in expected {
        let entry = operation(&registry, id);
        assert_eq!(entry.effect, effect, "{id}");
        assert_eq!(
            entry.metadata.surfaces.iter().collect::<Vec<_>>(),
            surfaces,
            "{id}"
        );
        assert_eq!(
            entry.approval,
            if matches!(
                id,
                "email.template.delete"
                    | "onecrew.outreach.relationship.update"
                    | "onecrew.outreach.message.send"
                    | "x.message.send"
                    | "instagram.message.send"
                    | "telegram.message.send"
                    | "email.draft.send"
                    | "email.manual.send"
                    | "email.direct.send"
                    | "email.send_many"
                    | "email.send_many.reconcile"
            ) {
                ApprovalClass::AttendedPrompt
            } else {
                ApprovalClass::OwnerGuarded
            },
            "{id}"
        );
        assert!(matches!(
            entry.connection,
            crate::operation_registry::descriptor::ConnectionRequirement::None
        ));
        assert!(matches!(
            entry.secret,
            crate::operation_registry::descriptor::SecretClass::None
        ));
        assert!(entry.sensitive_input_paths.is_empty());
    }
}

#[test]
fn external_write_safety_keeps_confirmation_idempotency_readback_and_audit() {
    let registry = production_registry();
    for id in [
        "onecrew.outreach.relationship.update",
        "onecrew.outreach.message.send",
        "x.message.send",
        "instagram.message.send",
        "telegram.message.send",
    ] {
        let entry = operation(&registry, id);
        let safety = &entry.safety;
        assert_eq!(
            safety.confirmation.mode,
            ConfirmationMode::AttendedPrompt,
            "{id}"
        );
        assert!(safety.confirmation.parameter_pointers.is_empty(), "{id}");
        assert_eq!(
            entry.binding.output_class(),
            crate::operation_registry::execution::BindingOutputClass::OwnerReview
        );
        assert!(
            entry.input.document()["properties"]
                .get("confirmed")
                .is_none(),
            "{id} has no wire confirmation bypass"
        );
        assert_eq!(
            entry.output.document(),
            crate::operation_registry::schema::CompiledSchema::for_output::<
                onecrew_local_runtime::OutreachCompletion,
            >()
            .unwrap()
            .document()
        );
        assert_eq!(
            safety.idempotency.mode,
            IdempotencyMode::OwnerInputIdentity,
            "{id}"
        );
        assert!(!safety.idempotency.identity_pointers.is_empty(), "{id}");
        assert_eq!(
            safety.readback.mode,
            ReadbackMode::InlineAndFollowUp,
            "{id}"
        );
        assert!(!safety.readback.operation_ids.is_empty(), "{id}");
        assert_eq!(safety.audit, AuditMode::OwnerRecorded, "{id}");
    }

    for id in [
        "onecrew.outreach.relationship.update",
        "onecrew.outreach.message.send",
        "x.message.send",
        "instagram.message.send",
        "telegram.message.send",
    ] {
        assert_eq!(
            operation(&registry, id)
                .safety
                .readback
                .operation_ids
                .iter()
                .map(EntryId::as_str)
                .collect::<Vec<_>>(),
            ["onecrew.outreach.action.status"],
            "{id}"
        );
    }

    for id in [
        "email.draft.send",
        "email.manual.send",
        "email.direct.send",
        "email.send_many",
        "email.send_many.reconcile",
    ] {
        let entry = operation(&registry, id);
        assert_eq!(entry.approval, ApprovalClass::AttendedPrompt, "{id}");
        assert!(entry.approval_request.is_some(), "{id}");
        assert_eq!(
            entry.safety.confirmation.mode,
            ConfirmationMode::AttendedPrompt,
            "{id}"
        );
        assert!(
            entry.safety.confirmation.parameter_pointers.is_empty(),
            "{id}"
        );
        assert_eq!(
            entry.binding.output_class(),
            crate::operation_registry::execution::BindingOutputClass::OwnerReview,
            "{id}"
        );
        assert!(
            entry.input.document()["properties"]
                .get("confirmed")
                .is_none(),
            "{id} has no wire confirmation bypass"
        );
        assert_eq!(
            entry.safety.idempotency.mode,
            IdempotencyMode::OwnerInputIdentity,
            "{id}"
        );
        assert!(
            !entry.safety.idempotency.identity_pointers.is_empty(),
            "{id}"
        );
        assert_eq!(entry.safety.audit, AuditMode::OwnerRecorded, "{id}");
        let prompt = &entry
            .approval_request
            .as_ref()
            .expect("attended email send has a prompt")
            .message;
        assert!(!prompt.contains("operation_id"), "{id}");
        assert!(!prompt.contains("confirmed"), "{id}");
    }

    let manual = operation(&registry, "email.manual.send");
    assert_eq!(
        manual.safety.readback.mode,
        ReadbackMode::InlineOwnerVerified
    );
    assert!(manual.safety.readback.operation_ids.is_empty());

    for id in [
        "email.draft.send",
        "email.direct.send",
        "email.send_many",
        "email.send_many.reconcile",
    ] {
        assert!(
            operation(&registry, id)
                .safety
                .readback
                .operation_ids
                .iter()
                .all(|operation_id| operation_id.as_str().starts_with("email.")),
            "{id}"
        );
    }
}

#[test]
fn email_send_operations_require_a_confirmed_result_submission() {
    let registry = production_registry();
    for id in [
        "email.draft.send",
        "email.manual.send",
        "email.direct.send",
        "email.send_many",
        "email.send_many.reconcile",
    ] {
        let operation = operation(&registry, id);
        let failure = crate::operation_registry::approval::preflight(
            operation.approval,
            operation.approval_request.as_ref(),
            false,
        )
        .expect_err("an email send needs a live attended approval, not a wire argument");
        assert_eq!(
            failure,
            crate::operation_registry::approval::ApprovalFailure::ChannelUnavailable,
            "{id}"
        );
    }
}

#[test]
fn template_delete_requires_a_live_attended_approval_before_owner_invocation() {
    let registry = production_registry();
    let operation = operation(&registry, "email.template.delete");
    assert_eq!(operation.approval, ApprovalClass::AttendedPrompt);
    assert_eq!(
        operation.safety.confirmation.mode,
        ConfirmationMode::AttendedPrompt
    );
    assert!(operation.approval_request.is_some());

    let prepared = crate::operation_registry::execution::prepare_invocation(
        operation,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "request-1".to_owned(),
            connection: None,
            confirmed: false,
        },
        operation.input.revision(),
        serde_json::json!({
            "template_id": "template-that-does-not-exist",
            "expected_revision": 1
        }),
    );
    let failure = prepared.and_then(|prepared| {
        tokio::runtime::Runtime::new().unwrap().block_on(
            crate::operation_registry::execution::dispatch_prepared(prepared),
        )
    });
    assert_eq!(
        failure.as_ref().unwrap_err().phase,
        ExecuteFailurePhase::BeforeInvocation
    );
    assert!(matches!(
        failure.unwrap_err().kind,
        ExecuteFailureKind::Approval(
            crate::operation_registry::approval::ApprovalFailure::ChannelUnavailable
        )
    ));
}
