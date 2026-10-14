use onecrew_creator_runtime::{EmailSendEffect, EmailSendFailure};

use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, ConfirmationSafety, ConnectionRequirement,
        EffectClass, EntryId, IdempotencyMode, IdempotencySafety, InstancePointer,
        OperationRegistration, OperationSafety, ReadbackMode, ReadbackSafety, SecretClass,
        SurfaceId, SurfaceSet,
    },
    execution::{
        BoundedPublicValue, ClassifiedOwnerFailure, ExecutionDeclaration, PublicFailureCode,
        RecoveryDirective,
    },
    registration::OperationPolicy,
};

pub(super) struct SafetySpec {
    pub(super) confirmation_mode: ConfirmationMode,
    pub(super) confirmation_parameters: &'static [&'static str],
    pub(super) idempotency_mode: IdempotencyMode,
    pub(super) idempotency_identity: &'static [&'static str],
    pub(super) readback_mode: ReadbackMode,
    pub(super) readback_operations: &'static [&'static str],
    pub(super) audit: AuditMode,
    pub(super) warning_output: &'static [&'static str],
    pub(super) recovery_output: &'static [&'static str],
}

/// Bind a canonical catalog identity to MCP-only discovery metadata.
pub(super) fn metadata(
    id: onecrew_local_runtime::OperationId,
    domain_id: &str,
    surfaces: impl IntoIterator<Item = SurfaceId>,
) -> OperationRegistration {
    OperationRegistration {
        id,
        domain_id: domain_id.to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new(surfaces).expect("OneCrew write operations have fixed surfaces"),
    }
}

pub(super) fn policy(
    effect: EffectClass,
    approval: ApprovalClass,
    approval_request: Option<ApprovalRequest>,
    safety: SafetySpec,
) -> OperationPolicy {
    OperationPolicy {
        effect,
        secret: SecretClass::None,
        approval,
        connection: ConnectionRequirement::None,
        sensitive_input_paths: Vec::new(),
        view_kind: crate::operation_registry::descriptor::OperationViewKind::SimpleOperation,
        approval_request,
        safety: OperationSafety {
            confirmation: ConfirmationSafety {
                mode: safety.confirmation_mode,
                parameter_pointers: pointers(safety.confirmation_parameters),
            },
            idempotency: IdempotencySafety {
                mode: safety.idempotency_mode,
                identity_pointers: pointers(safety.idempotency_identity),
            },
            readback: ReadbackSafety {
                mode: safety.readback_mode,
                operation_ids: safety
                    .readback_operations
                    .iter()
                    .map(|id| EntryId::parse(*id).expect("static readback operation ID is valid"))
                    .collect(),
            },
            audit: safety.audit,
            warnings_present: !safety.warning_output.is_empty(),
            recovery_present: !safety.recovery_output.is_empty(),
            warning_output_pointers: pointers(safety.warning_output),
            recovery_output_pointers: pointers(safety.recovery_output),
        },
        execution: ExecutionDeclaration::Sequential,
    }
}

pub(super) fn owner_guarded(effect: EffectClass, safety: SafetySpec) -> OperationPolicy {
    policy(effect, ApprovalClass::OwnerGuarded, None, safety)
}

pub(super) fn attended_delete(message: &str, safety: SafetySpec) -> OperationPolicy {
    policy(
        EffectClass::DeleteLocal,
        ApprovalClass::AttendedPrompt,
        Some(ApprovalRequest::new(message).expect("static approval prompt is valid")),
        safety,
    )
}

pub(super) fn unknown_string_failure(
    code: &'static str,
    recovery_operation: Option<&'static str>,
) -> impl Fn(String) -> ClassifiedOwnerFailure {
    move |_cause| {
        ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse(code).expect("static public failure code is valid"),
            recovery_operation.map(read_status),
        )
    }
}

/// Classify an email send by what the owner proved about the provider send.
///
/// Only a genuinely uncertain outcome stays unknown. A send the runtime proved
/// never reached the provider is an ordinary failure, and one it proved did
/// reach the provider is applied, so an Agent never reads "result unknown" for
/// mail that demonstrably never left. The owner's explanation stays below this
/// boundary: those strings interpolate repository and provider causes.
pub(super) fn email_send_failure(
    code: &'static str,
    recovery_operation: Option<&'static str>,
) -> impl Fn(EmailSendFailure) -> ClassifiedOwnerFailure {
    move |failure| {
        let code = PublicFailureCode::parse(code).expect("static public failure code is valid");
        let recovery = recovery_operation.map(read_status);
        match failure.effect() {
            EmailSendEffect::NotSent => {
                ClassifiedOwnerFailure::known_no_effect(code, true, recovery)
            }
            EmailSendEffect::Sent => ClassifiedOwnerFailure::applied(code, recovery),
            EmailSendEffect::Unknown => ClassifiedOwnerFailure::unknown(code, recovery),
        }
    }
}

fn pointers(values: &[&str]) -> Vec<InstancePointer> {
    values
        .iter()
        .map(|value| InstancePointer::parse(*value).expect("static safety pointer is valid"))
        .collect()
}

pub(super) fn read_status(operation_id: &str) -> RecoveryDirective {
    RecoveryDirective::ReadStatus {
        reference: BoundedPublicValue::parse(operation_id)
            .expect("static recovery operation reference is valid"),
    }
}

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::operation_registry::{
        descriptor::SurfaceId,
        execution::{
            ExecuteEffectState, ExecutionContext, project_classified_owner_failure,
            project_execute_failure,
        },
        registration::RegistryBuilder,
    };

    #[test]
    fn email_send_failures_separate_a_proven_no_send_from_a_genuine_unknown() {
        let classify = email_send_failure(
            "mail_operation_failed",
            Some(onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()),
        );
        for (failure, effect, retry_safe) in [
            (
                EmailSendFailure::not_sent("sentinel-provider-secret rejected the recipient"),
                ExecuteEffectState::KnownNoEffect,
                true,
            ),
            (
                EmailSendFailure::sent("sentinel-provider-secret readback is incomplete"),
                ExecuteEffectState::Applied,
                false,
            ),
            (
                EmailSendFailure::unknown("sentinel-provider-secret may hold the message"),
                ExecuteEffectState::Unknown,
                false,
            ),
        ] {
            let presentation = project_classified_owner_failure(classify(failure));
            assert_eq!(presentation.code.as_str(), "mail_operation_failed");
            assert_eq!(presentation.effect, effect);
            assert_eq!(presentation.retry_safe, retry_safe);
            assert_eq!(
                presentation.recovery,
                Some(read_status(
                    onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()
                ))
            );
            assert!(
                !serde_json::to_string(&presentation)
                    .unwrap()
                    .contains("sentinel-provider-secret"),
                "the owner explanation must stay below the public boundary"
            );
        }
    }

    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct EmptyInput {}

    #[derive(Serialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct EmptyOutput {}

    #[test]
    fn legacy_string_failure_is_secret_free_unknown_and_not_retryable() {
        let mut builder =
            RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
                "onecrew.test.legacy-failure",
            ]));
        builder
            .register_operation::<EmptyInput, EmptyOutput, String, _, _, _>(
                metadata(
                    onecrew_local_runtime::OperationId::from_static("onecrew.test.legacy-failure"),
                    "test",
                    [SurfaceId::Full],
                ),
                owner_guarded(
                    EffectClass::WriteLocal,
                    SafetySpec {
                        confirmation_mode: ConfirmationMode::None,
                        confirmation_parameters: &[],
                        idempotency_mode: IdempotencyMode::None,
                        idempotency_identity: &[],
                        readback_mode: ReadbackMode::None,
                        readback_operations: &[],
                        audit: AuditMode::None,
                        warning_output: &[],
                        recovery_output: &[],
                    },
                ),
                |_context, _input| async {
                    Err::<EmptyOutput, _>("sentinel-provider-secret".to_owned())
                },
                unknown_string_failure(
                    "mail_operation_failed",
                    Some(onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()),
                ),
            )
            .unwrap();
        let registry = builder.build().unwrap();
        let id = EntryId::parse("onecrew.test.legacy-failure").unwrap();
        let crate::operation_registry::descriptor::RegistryEntry::Operation(operation) =
            registry.entry(&id, SurfaceId::Full).unwrap()
        else {
            unreachable!()
        };
        let prepared = crate::operation_registry::execution::prepare_invocation(
            operation,
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: "request-1".to_owned(),
                connection: None,
                confirmed: false,
            },
            operation.input.revision(),
            serde_json::json!({}),
        )
        .unwrap();
        let failure = tokio::runtime::Runtime::new().unwrap().block_on(
            crate::operation_registry::execution::dispatch_prepared(prepared),
        );
        let presentation = project_execute_failure(&failure.unwrap_err());
        assert_eq!(presentation.code.as_str(), "mail_operation_failed");
        assert_eq!(presentation.effect, ExecuteEffectState::Unknown);
        assert!(!presentation.retry_safe);
        assert_eq!(
            presentation.recovery,
            Some(read_status(
                onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()
            ))
        );
        assert!(
            !serde_json::to_string(&presentation)
                .unwrap()
                .contains("sentinel-provider-secret")
        );
    }
}
