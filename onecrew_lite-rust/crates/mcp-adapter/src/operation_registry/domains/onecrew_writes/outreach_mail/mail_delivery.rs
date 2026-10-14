use onecrew_creator_runtime::{
    EmailActionResult, EmailBatchActionResult, EmailDraftSendResult, EmailSendFailure,
    MailboxSyncOutcome, ManualEmailActionResult, ReconcileEmailBatchInput, SendEmailDraftInput,
    SendEmailInput, SendEmailManyInput, SendManualEmailInput, SyncMailboxRequest,
};

use super::{
    super::super::RegistryOwners,
    shared::{
        SafetySpec, email_send_failure, metadata, owner_guarded, policy, unknown_string_failure,
    },
};
use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, EffectClass, IdempotencyMode, ReadbackMode,
        SurfaceId,
    },
    registration::{OperationPolicy, RegistryBuildError, RegistryBuilder},
};

const MAIL_SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Mail];
const DIRECT_SURFACES: [SurfaceId; 2] = [SurfaceId::Full, SurfaceId::Mail];
const FAILURE_CODE: &str = "mail_operation_failed";
const THREAD_READBACK: &[&str] =
    const { &[onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()] };
const DRAFT_READBACK: &[&str] = const {
    &[
        onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
    ]
};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<SyncMailboxRequest, MailboxSyncOutcome, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC,
            "mail",
            MAIL_SURFACES,
        ),
        owner_guarded(
            EffectClass::SyncOpenWorld,
            SafetySpec {
                confirmation_mode: ConfirmationMode::None,
                confirmation_parameters: &[],
                idempotency_mode: IdempotencyMode::None,
                idempotency_identity: &[],
                readback_mode: ReadbackMode::InlineAndFollowUp,
                readback_operations: const {
                    &[
                        onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC_STATUS.as_str(),
                        onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
                        onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str(),
                    ]
                },
                audit: AuditMode::None,
                warning_output: &[
                    "/inbound/failure",
                    "/outbound/failure",
                    "/inbound/missing_provider_message_ids",
                    "/outbound/missing_provider_message_ids",
                    "/pending_triage",
                ],
                recovery_output: &[
                    "/inbound/failure",
                    "/outbound/failure",
                    "/inbound/missing_provider_message_ids",
                    "/outbound/missing_provider_message_ids",
                ],
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.sync_mailbox_full(input).await }
        },
        unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC_STATUS.as_str()),
        ),
    )?;

    let platform = owners.platform.clone();
    builder
        .register_owner_review_operation::<SendManualEmailInput, ManualEmailActionResult, EmailSendFailure, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_MANUAL_SEND,
            "mail",
            DIRECT_SURFACES,
        ),
        review_policy("Send the prepared manual email to the explicitly selected address?", &[]),
        move |context, input| {
            let platform = platform.clone();
            async move {
                platform
                    .prepare_email_manual_send(input, review_context(context)?)
                    .await
            }
        },
        email_send_failure(FAILURE_CODE, None),
    )?;

    let platform = owners.platform.clone();
    builder
        .register_owner_review_operation::<SendEmailInput, EmailActionResult, EmailSendFailure, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_DIRECT_SEND,
            "mail",
            DIRECT_SURFACES,
        ),
        review_policy(
            "Send the prepared direct email to the selected Creator route?",
            THREAD_READBACK,
        ),
        move |context, input| {
            let platform = platform.clone();
            async move {
                platform
                    .prepare_email_direct_send(input, review_context(context)?)
                    .await
            }
        },
        email_send_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()),
        ),
    )?;

    let platform = owners.platform.clone();
    builder
        .register_owner_review_operation::<SendEmailManyInput, EmailBatchActionResult, EmailSendFailure, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::EMAIL_SEND_MANY,
            "mail",
            MAIL_SURFACES,
        ),
        review_policy(
            "Send the prepared email batch to all listed recipients?",
            THREAD_READBACK,
        ),
        move |context, input| {
            let platform = platform.clone();
            async move {
                platform
                    .prepare_email_batch_send(input, review_context(context)?)
                    .await
            }
        },
        email_send_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()),
        ),
    )?;

    let platform = owners.platform.clone();
    builder
        .register_owner_review_operation::<ReconcileEmailBatchInput, EmailBatchActionResult, EmailSendFailure, _, _, _>(
            metadata(
                onecrew_local_runtime::OperationId::EMAIL_SEND_MANY_RECONCILE,
                "mail",
                MAIL_SURFACES,
            ),
            review_policy(
                "Reconcile the remotely accepted email batch without resending?",
                THREAD_READBACK,
            ),
            move |context, input| {
                let platform = platform.clone();
                async move {
                    platform
                        .prepare_email_batch_reconcile(input, review_context(context)?)
                        .await
                }
            },
            email_send_failure(
                FAILURE_CODE,
                Some(onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()),
            ),
        )?;

    let platform = owners.platform;
    builder
        .register_owner_review_operation::<SendEmailDraftInput, EmailDraftSendResult, EmailSendFailure, _, _, _>(
            metadata(
                onecrew_local_runtime::OperationId::EMAIL_DRAFT_SEND,
                "mail",
                MAIL_SURFACES,
            ),
            review_policy("Send the selected saved email draft?", DRAFT_READBACK),
            move |context, input| {
                let platform = platform.clone();
                async move {
                    platform
                        .prepare_email_draft_send(input, review_context(context)?)
                        .await
                }
            },
            email_send_failure(
                FAILURE_CODE,
                Some(onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str()),
            ),
        )
}

/// An email send is an owner review, exactly like an outreach direct message.
/// The confirmed intent is the sealed grant the review mints, so there is no
/// confirmation parameter: a `/confirmed` pointer here was only ever validated
/// at registration and never read at execution, which let a resumed call
/// re-send the Agent's original unconfirmed arguments.
fn review_policy(
    prompt: &'static str,
    readback_operations: &'static [&'static str],
) -> OperationPolicy {
    policy(
        EffectClass::ExternalWrite,
        ApprovalClass::AttendedPrompt,
        Some(ApprovalRequest::new(prompt).expect("static review prompt")),
        SafetySpec {
            confirmation_mode: ConfirmationMode::AttendedPrompt,
            confirmation_parameters: &[],
            idempotency_mode: IdempotencyMode::OwnerInputIdentity,
            idempotency_identity: &["/idempotency_key"],
            readback_mode: if readback_operations.is_empty() {
                ReadbackMode::InlineOwnerVerified
            } else {
                ReadbackMode::InlineAndFollowUp
            },
            readback_operations,
            audit: AuditMode::OwnerRecorded,
            warning_output: &[],
            recovery_output: &[],
        },
    )
}

/// Only V2's validated call path carries the server-owned review context. A
/// rejection here happens before any claim, so nothing was sent.
fn review_context(
    context: crate::operation_registry::execution::BindingContext,
) -> Result<onecrew_local_runtime::OwnerReviewContext, EmailSendFailure> {
    context
        .owner_review_context()
        .map_err(|error| EmailSendFailure::not_sent(error.to_string()))
}
