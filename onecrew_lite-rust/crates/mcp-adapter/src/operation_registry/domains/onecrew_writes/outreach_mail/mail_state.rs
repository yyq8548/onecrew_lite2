use onecrew_creator_runtime::{
    DeleteEmailTemplateInput, EmailDraft, EmailDraftMutationInput, EmailTemplate, EmailThreadView,
    SaveEmailDraftInput, SaveEmailTemplateInput, UpdateEmailThreadDispositionInput,
};

use super::{
    super::super::RegistryOwners,
    shared::{SafetySpec, attended_delete, metadata, owner_guarded, unknown_string_failure},
};
use crate::operation_registry::{
    descriptor::{
        AuditMode, ConfirmationMode, EffectClass, IdempotencyMode, ReadbackMode, SurfaceId,
    },
    registration::{RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Mail];
const FAILURE_CODE: &str = "mail_operation_failed";

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<EmailDraftMutationInput, EmailDraft, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_DRAFT_RESTORE,
            "mail",
            SURFACES,
        ),
        owner_guarded(
            EffectClass::WriteLocal,
            local_readback(
                const { &[onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str()] },
            ),
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.restore_email_draft_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str()),
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<SaveEmailDraftInput, EmailDraft, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_DRAFT_SAVE,
            "mail",
            SURFACES,
        ),
        owner_guarded(
            EffectClass::WriteLocal,
            local_readback(
                const { &[onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str()] },
            ),
        )
        .with_view_kind(crate::operation_registry::descriptor::OperationViewKind::OutreachDraft),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.save_email_draft_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str()),
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<EmailDraftMutationInput, EmailDraft, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_DRAFT_TRASH,
            "mail",
            SURFACES,
        ),
        owner_guarded(
            EffectClass::WriteLocal,
            local_readback(
                const { &[onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str()] },
            ),
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.trash_email_draft_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str()),
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<DeleteEmailTemplateInput, bool, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_DELETE,
            "mail",
            SURFACES,
        ),
        attended_delete(
            "Delete the selected mail template? This cannot be undone.",
            SafetySpec {
                confirmation_mode: ConfirmationMode::AttendedPrompt,
                confirmation_parameters: &[],
                idempotency_mode: IdempotencyMode::None,
                idempotency_identity: &[],
                readback_mode: ReadbackMode::FollowUpOperations,
                readback_operations: const {
                    &[onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_LIST.as_str()]
                },
                audit: AuditMode::None,
                warning_output: &[],
                recovery_output: &[],
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.delete_email_template_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_LIST.as_str()),
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<SaveEmailTemplateInput, EmailTemplate, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_SAVE,
            "mail",
            SURFACES,
        ),
        owner_guarded(
            EffectClass::WriteLocal,
            local_readback(
                const { &[onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_GET.as_str()] },
            ),
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.save_email_template_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_GET.as_str()),
        )),
    )?;

    let creators = owners.creators;
    builder.register_operation::<UpdateEmailThreadDispositionInput, EmailThreadView, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_THREAD_UPDATE_DISPOSITION,
            "mail",
            SURFACES,
        ),
        owner_guarded(
            EffectClass::WriteLocal,
            local_readback(
                const { &[onecrew_local_runtime::OperationId::EMAIL_THREAD_GET.as_str()] },
            ),
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.update_email_thread_disposition_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_THREAD_GET.as_str()),
        )),
    )
}

fn local_readback(readback_operations: &'static [&'static str]) -> SafetySpec {
    SafetySpec {
        confirmation_mode: ConfirmationMode::None,
        confirmation_parameters: &[],
        idempotency_mode: IdempotencyMode::None,
        idempotency_identity: &[],
        readback_mode: ReadbackMode::InlineAndFollowUp,
        readback_operations,
        audit: AuditMode::None,
        warning_output: &[],
        recovery_output: &[],
    }
}
