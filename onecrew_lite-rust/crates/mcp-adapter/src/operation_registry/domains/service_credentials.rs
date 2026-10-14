use onecrew_local_runtime::{
    RenameYoutubeCredentialInput, ResendCredentialConnectInput, ResendCredentialDeleteInput,
    ResendCredentialDeleteOutcome, ResendCredentialDeletionError, RotateResendCredentialInput,
    RotateYoutubeCredentialInput, ServiceCredentialConnectResult, ServiceCredentialMutationError,
    ServiceCredentialProjection, YoutubeApiKeyConnectInput, YoutubeCredentialDeleteError,
    YoutubeCredentialDeleteInput, YoutubeCredentialDeleteOutcome,
};

use super::RegistryOwners;
use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, ConfirmationSafety, ConnectionRequirement,
        EffectClass, IdempotencyMode, IdempotencySafety, InstancePointer, OperationRegistration,
        OperationSafety, ReadbackMode, ReadbackSafety, SecretClass, SurfaceId, SurfaceSet,
    },
    execution::{
        BlockingTaskFailure, BoundedPublicValue, ClassifiedOwnerFailure, ExecutionDeclaration,
        PublicFailureCode, RecoveryDirective, run_blocking,
    },
    registration::{OperationPolicy, RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 4] = [
    SurfaceId::Full,
    SurfaceId::Compact,
    SurfaceId::Accounts,
    SurfaceId::Mail,
];

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let platform = owners.platform.clone();
    builder.register_operation::<YoutubeApiKeyConnectInput, ServiceCredentialConnectResult, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_CONNECT),
        policy(EffectClass::WriteLocal, SecretClass::VaultBound, false, IdempotencyMode::None, &[], &["/api_key"], false, None),
        move |_context, input| { let platform = platform.clone(); async move { platform.connect_youtube_api_key_typed(input).await } },
        classify_mutation,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<RotateYoutubeCredentialInput, ServiceCredentialConnectResult, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_ROTATE),
        policy(EffectClass::WriteLocal, SecretClass::VaultBound, true, IdempotencyMode::None, &[], &["/api_key"], false, Some("Rotate this YouTube credential?")),
        move |_context, input| { let platform = platform.clone(); async move { platform.rotate_youtube_credential_typed(input).await } },
        classify_mutation,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<RenameYoutubeCredentialInput, ServiceCredentialProjection, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_RENAME),
        policy(EffectClass::WriteLocal, SecretClass::None, true, IdempotencyMode::OwnerInputIdentity, &["/credential_id", "/label"], &[], false, Some("Rename this YouTube credential?")),
        move |_context, input| { let platform = platform.clone(); async move { run_blocking(move || platform.rename_youtube_credential_typed(input)).await } },
        classify_mutation,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<YoutubeCredentialDeleteInput, YoutubeCredentialDeleteOutcome, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_DELETE),
        policy(EffectClass::DeleteLocal, SecretClass::None, true, IdempotencyMode::OwnerRequestId, &["/idempotency_key"], &[], true, Some("Delete this YouTube credential?")),
        move |_context, input| { let platform = platform.clone(); async move { run_blocking(move || platform.delete_youtube_credential(input)).await } },
        classify_youtube_delete,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<ResendCredentialConnectInput, ServiceCredentialConnectResult, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::EMAIL_CREDENTIAL_CONNECT),
        policy(EffectClass::WriteLocal, SecretClass::VaultBound, false, IdempotencyMode::None, &[], &["/api_key"], false, None),
        move |_context, input| { let platform = platform.clone(); async move { platform.connect_resend_credential_typed(input).await } },
        classify_mutation,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<RotateResendCredentialInput, ServiceCredentialConnectResult, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::EMAIL_CREDENTIAL_ROTATE),
        policy(EffectClass::WriteLocal, SecretClass::VaultBound, true, IdempotencyMode::None, &[], &["/api_key"], false, Some("Rotate this Resend credential?")),
        move |_context, input| { let platform = platform.clone(); async move { platform.rotate_resend_credential_typed(input).await } },
        classify_mutation,
    )?;

    let platform = owners.platform;
    builder.register_operation::<ResendCredentialDeleteInput, ResendCredentialDeleteOutcome, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::EMAIL_CREDENTIAL_DELETE),
        policy(EffectClass::DeleteLocal, SecretClass::None, true, IdempotencyMode::OwnerInputIdentity, &["/credential_id"], &[], false, Some("Delete this Resend credential?")),
        move |_context, input| { let platform = platform.clone(); async move { run_blocking(move || platform.delete_resend_credential_outcome(input)).await } },
        classify_resend_delete,
    )
}

fn metadata(id: onecrew_local_runtime::OperationId) -> OperationRegistration {
    OperationRegistration {
        id,
        domain_id: "service_credentials".to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new(SURFACES).expect("credential operations have fixed surfaces"),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "shared policy helper keeps credential safety fields explicit"
)]
fn policy(
    effect: EffectClass,
    secret: SecretClass,
    confirmed: bool,
    idempotency_mode: IdempotencyMode,
    identity_pointers: &[&str],
    sensitive_input_paths: &[&str],
    owner_audited: bool,
    approval_prompt: Option<&str>,
) -> OperationPolicy {
    let attended = confirmed && approval_prompt.is_some();
    OperationPolicy {
        effect,
        secret,
        approval: if attended {
            ApprovalClass::AttendedPrompt
        } else if confirmed {
            ApprovalClass::OwnerGuarded
        } else {
            ApprovalClass::None
        },
        connection: ConnectionRequirement::None,
        sensitive_input_paths: sensitive_input_paths
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        view_kind: if secret == SecretClass::None {
            crate::operation_registry::descriptor::OperationViewKind::SimpleOperation
        } else {
            crate::operation_registry::descriptor::OperationViewKind::ReadOnly
        },
        approval_request: approval_prompt.filter(|_| attended).map(|prompt| {
            ApprovalRequest::new(prompt).expect("static credential approval prompt is valid")
        }),
        safety: OperationSafety {
            confirmation: ConfirmationSafety {
                mode: if attended {
                    ConfirmationMode::AttendedPromptAndOwnerParameters
                } else if confirmed {
                    ConfirmationMode::OwnerParameters
                } else {
                    ConfirmationMode::None
                },
                parameter_pointers: confirmed
                    .then(|| pointer("/confirmed"))
                    .into_iter()
                    .collect(),
            },
            idempotency: IdempotencySafety {
                mode: idempotency_mode,
                identity_pointers: identity_pointers
                    .iter()
                    .map(|value| pointer(value))
                    .collect(),
            },
            readback: ReadbackSafety {
                mode: ReadbackMode::InlineOwnerVerified,
                operation_ids: Vec::new(),
            },
            audit: if owner_audited {
                AuditMode::OwnerRecorded
            } else {
                AuditMode::None
            },
            warnings_present: false,
            recovery_present: false,
            warning_output_pointers: Vec::new(),
            recovery_output_pointers: Vec::new(),
        },
        execution: ExecutionDeclaration::Sequential,
    }
}

fn pointer(value: &str) -> InstancePointer {
    InstancePointer::parse(value).expect("static credential pointer is valid")
}

fn failure(code: &str, recovery: Option<RecoveryDirective>) -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse(code).expect("static credential failure code is valid"),
        false,
        recovery,
    )
}

/// Blocking-pool join failures map to each credential owner's existing
/// internal/runtime variant beside its classifier; the public codes stay
/// `credential_operation_unavailable` / `credential_delete_unavailable`.
impl From<BlockingTaskFailure> for ServiceCredentialMutationError {
    fn from(_failure: BlockingTaskFailure) -> Self {
        Self::Internal
    }
}

impl From<BlockingTaskFailure> for YoutubeCredentialDeleteError {
    fn from(_failure: BlockingTaskFailure) -> Self {
        Self::Internal
    }
}

impl From<BlockingTaskFailure> for ResendCredentialDeletionError {
    fn from(failure: BlockingTaskFailure) -> Self {
        Self::Runtime {
            message: String::from(failure),
        }
    }
}

fn classify_mutation(error: ServiceCredentialMutationError) -> ClassifiedOwnerFailure {
    match error {
        ServiceCredentialMutationError::ConfirmationRequired => failure(
            "confirmation_required",
            Some(RecoveryDirective::Confirm {
                parameter: bounded("confirmed"),
            }),
        ),
        ServiceCredentialMutationError::InvalidInput => failure(
            "credential_input_invalid",
            Some(RecoveryDirective::CorrectInput),
        ),
        ServiceCredentialMutationError::NotFound => {
            failure("credential_not_found", Some(connection_list()))
        }
        ServiceCredentialMutationError::WrongKind => {
            failure("credential_wrong_kind", Some(connection_list()))
        }
        ServiceCredentialMutationError::Conflict => {
            failure("credential_conflict", Some(connection_list()))
        }
        ServiceCredentialMutationError::ValidationRejected => failure(
            "credential_validation_rejected",
            Some(RecoveryDirective::CorrectInput),
        ),
        ServiceCredentialMutationError::Recoverable => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("credential_recovery_required").unwrap(),
            Some(connection_list()),
        ),
        ServiceCredentialMutationError::Internal => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("credential_operation_unavailable").unwrap(),
            Some(connection_list()),
        ),
    }
}

fn classify_youtube_delete(error: YoutubeCredentialDeleteError) -> ClassifiedOwnerFailure {
    match error {
        YoutubeCredentialDeleteError::ConfirmationRequired => failure(
            "confirmation_required",
            Some(RecoveryDirective::Confirm {
                parameter: bounded("confirmed"),
            }),
        ),
        YoutubeCredentialDeleteError::IdempotencyKeyInvalid
        | YoutubeCredentialDeleteError::InvalidCredentialId => failure(
            "credential_input_invalid",
            Some(RecoveryDirective::CorrectInput),
        ),
        YoutubeCredentialDeleteError::IdempotencyConflict => failure(
            "idempotency_conflict",
            Some(RecoveryDirective::CorrectInput),
        ),
        YoutubeCredentialDeleteError::NotFound | YoutubeCredentialDeleteError::WrongKind => {
            failure("credential_not_found", Some(connection_list()))
        }
        YoutubeCredentialDeleteError::OutreachInProgress => {
            failure("outreach_action_unresolved", Some(connection_list()))
                .with_message(YoutubeCredentialDeleteError::OutreachInProgress.safe_message())
        }
        YoutubeCredentialDeleteError::RecoverableSameKeyRetry => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("credential_delete_recoverable").unwrap(),
            Some(RecoveryDirective::ReuseIdempotencyKey),
        ),
        YoutubeCredentialDeleteError::Internal => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("credential_delete_unavailable").unwrap(),
            Some(connection_list()),
        ),
    }
}

fn classify_resend_delete(error: ResendCredentialDeletionError) -> ClassifiedOwnerFailure {
    match error {
        ResendCredentialDeletionError::InvalidRequest { message }
            if message.contains("explicit confirmation") =>
        {
            failure(
                "confirmation_required",
                Some(RecoveryDirective::Confirm {
                    parameter: bounded("confirmed"),
                }),
            )
        }
        ResendCredentialDeletionError::InvalidRequest { .. } => failure(
            "credential_input_invalid",
            Some(RecoveryDirective::CorrectInput),
        ),
        ResendCredentialDeletionError::Blocked { .. } => {
            failure("credential_delete_blocked", Some(connection_list()))
        }
        ResendCredentialDeletionError::Runtime { .. } => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("credential_delete_unavailable").unwrap(),
            Some(connection_list()),
        ),
    }
}

fn connection_list() -> RecoveryDirective {
    RecoveryDirective::ReadStatus {
        reference: bounded("onecrew_connection_list"),
    }
}

fn bounded(value: &str) -> BoundedPublicValue {
    BoundedPublicValue::parse(value).expect("static credential recovery value is bounded")
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_local_runtime::ToolkitId;

    #[test]
    fn unresolved_outreach_blocks_youtube_deletion_before_any_credential_effect() {
        assert_eq!(
            classify_youtube_delete(YoutubeCredentialDeleteError::OutreachInProgress),
            failure("outreach_action_unresolved", Some(connection_list()))
                .with_message(YoutubeCredentialDeleteError::OutreachInProgress.safe_message())
        );
    }

    #[test]
    fn exact_legacy_credential_mutations_have_toolkit_owned_ids() {
        let operations = [
            (
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_CONNECT.as_str(),
                ToolkitId::Youtube,
            ),
            (
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_ROTATE.as_str(),
                ToolkitId::Youtube,
            ),
            (
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_RENAME.as_str(),
                ToolkitId::Youtube,
            ),
            (
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_DELETE.as_str(),
                ToolkitId::Youtube,
            ),
            (
                onecrew_local_runtime::OperationId::EMAIL_CREDENTIAL_CONNECT.as_str(),
                ToolkitId::Email,
            ),
            (
                onecrew_local_runtime::OperationId::EMAIL_CREDENTIAL_ROTATE.as_str(),
                ToolkitId::Email,
            ),
            (
                onecrew_local_runtime::OperationId::EMAIL_CREDENTIAL_DELETE.as_str(),
                ToolkitId::Email,
            ),
        ];
        assert_eq!(operations.len(), 7);
        for (id, toolkit) in operations {
            let metadata = metadata(onecrew_local_runtime::OperationId::from_static(id));
            assert_eq!(
                metadata.id.as_str().split('.').next(),
                Some(toolkit.as_str())
            );
            assert!(metadata.aliases.is_empty());
        }
    }

    #[test]
    fn api_keys_are_sensitive_but_never_idempotency_identity() {
        let policy = policy(
            EffectClass::WriteLocal,
            SecretClass::VaultBound,
            true,
            IdempotencyMode::None,
            &[],
            &["/api_key"],
            false,
            Some("Rotate this credential?"),
        );
        assert_eq!(policy.sensitive_input_paths, ["/api_key"]);
        assert!(policy.safety.idempotency.identity_pointers.is_empty());
        assert_eq!(policy.approval, ApprovalClass::AttendedPrompt);
        assert_eq!(
            policy.safety.confirmation.mode,
            ConfirmationMode::AttendedPromptAndOwnerParameters
        );
        assert_eq!(
            policy.safety.confirmation.parameter_pointers,
            [pointer("/confirmed")]
        );
        assert!(policy.approval_request.is_some());
    }
}
