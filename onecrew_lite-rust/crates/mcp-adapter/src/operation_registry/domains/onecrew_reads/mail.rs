use onecrew_creator_runtime::{
    EmailDraft, EmailDraftGetInput, EmailDraftSummary, EmailFolderSummary, EmailMessageGetInput,
    EmailMessageView, EmailPreparation, EmailTemplate, EmailTemplateGetInput, EmailTemplateSummary,
    EmailThreadGetInput, EmailThreadSummary, EmailThreadView, ListEmailDraftsInput,
    ListEmailFoldersInput, ListEmailTemplatesInput, ListEmailThreadsInput, MailboxGetError,
    MailboxGetInput, MailboxGetOutcome, MailboxSyncStatus, ManualEmailPreparation, Page,
    PrepareEmailInput, PrepareManualEmailInput, RenderEmailTemplateInput, RenderedEmailTemplate,
};

use super::{
    super::RegistryOwners,
    shared::{classify_string_failure, metadata, read_policy},
};
use crate::operation_registry::{
    descriptor::SurfaceId,
    execution::{
        BlockingTaskFailure, BoundedPublicValue, ClassifiedOwnerFailure, PublicFailureCode,
        RecoveryDirective, run_blocking,
    },
    registration::{RegistryBuildError, RegistryBuilder},
};

const MAIL_SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Mail];
const MAILBOX_SURFACES: [SurfaceId; 4] = [
    SurfaceId::Full,
    SurfaceId::Compact,
    SurfaceId::Accounts,
    SurfaceId::Mail,
];
const DIRECT_SURFACES: [SurfaceId; 2] = [SurfaceId::Full, SurfaceId::Mail];
const FAILURE_CODE: &str = "mail_read_unavailable";

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<MailboxGetInput, MailboxGetOutcome, MailboxGetError, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_MAILBOX_GET,
            "mail",
            MAILBOX_SURFACES,
        ),
        read_policy().with_view_kind(
            crate::operation_registry::descriptor::OperationViewKind::RecordPreview,
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.get_mailbox(input)).await }
        },
        classify_mailbox_get_failure,
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<MailboxGetInput, MailboxSyncStatus, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC_STATUS,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move {
                creators
                    .mailbox_sync_status_async(input.mailbox_ref_id)
                    .await
            }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ListEmailFoldersInput, Vec<EmailFolderSummary>, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_FOLDER_LIST,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_email_folders_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ListEmailThreadsInput, Page<EmailThreadSummary>, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_email_threads_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<EmailThreadGetInput, EmailThreadView, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_THREAD_GET,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.get_email_thread_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<EmailMessageGetInput, EmailMessageView, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_MESSAGE_GET,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.get_email_message(input)).await }
        },
        classify_string_failure(FAILURE_CODE),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ListEmailTemplatesInput, Page<EmailTemplateSummary>, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_LIST,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_email_templates_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<EmailTemplateGetInput, EmailTemplate, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_GET,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.get_email_template_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<RenderEmailTemplateInput, RenderedEmailTemplate, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_RENDER,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.render_email_template_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ListEmailDraftsInput, Page<EmailDraftSummary>, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_DRAFT_LIST,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_email_drafts_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<EmailDraftGetInput, EmailDraft, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET,
            "mail",
            MAIL_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.get_email_draft_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<PrepareEmailInput, EmailPreparation, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_DIRECT_PREPARE,
            "mail",
            DIRECT_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.prepare_email(input)).await }
        },
        classify_string_failure(FAILURE_CODE),
    )?;

    let creators = owners.creators;
    builder.register_operation::<PrepareManualEmailInput, ManualEmailPreparation, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_MANUAL_PREPARE,
            "mail",
            DIRECT_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.prepare_manual_email(input)).await }
        },
        classify_string_failure(FAILURE_CODE),
    )
}

/// A lost blocking-pool result is the same read failure as the owner's own
/// lookup being unavailable; the mailbox read has no effect either way.
impl From<BlockingTaskFailure> for MailboxGetError {
    fn from(_failure: BlockingTaskFailure) -> Self {
        Self::LookupUnavailable
    }
}

fn classify_mailbox_get_failure(error: MailboxGetError) -> ClassifiedOwnerFailure {
    match error {
        MailboxGetError::LookupUnavailable => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("mailbox_lookup_unavailable")
                .expect("static mailbox failure code is valid"),
            false,
            Some(RecoveryDirective::ReadStatus {
                reference: BoundedPublicValue::parse("onecrew_connection_list")
                    .expect("static recovery reference is bounded"),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_lookup_unavailable_is_known_no_effect_with_inventory_recovery() {
        assert_eq!(
            classify_mailbox_get_failure(MailboxGetError::LookupUnavailable),
            ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse("mailbox_lookup_unavailable").unwrap(),
                false,
                Some(RecoveryDirective::ReadStatus {
                    reference: BoundedPublicValue::parse("onecrew_connection_list").unwrap(),
                }),
            )
        );
    }
}
