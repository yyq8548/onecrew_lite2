mod attachments;
mod drafts;
mod mailboxes;
mod queries;
mod recovery;
mod sending;
mod sync_state;
mod templates;
mod threads;

pub use attachments::{
    EmailAttachmentContent, EmailAttachmentReadError, EmailAttachmentReadInput,
    EmailAttachmentView, MAX_EMAIL_ATTACHMENT_BYTES,
};
pub use drafts::{
    EmailDraft, EmailDraftGetInput, EmailDraftMutationInput, EmailDraftSendResult,
    EmailDraftSummary, ListEmailDraftsInput, SaveEmailDraftInput, SendEmailDraftInput,
};
pub(crate) use mailboxes::MailboxRecord;
pub use mailboxes::{
    EmailAuthConnection, EmailAuthConnectionDeletionClaim, EmailAuthConnectionId,
    EmailProviderKind, EmailRecoveryStage, EmailSecretRecord, EmailSecretRecordId,
    MailboxAuthBinding, MailboxCredentialDeletionBlockers, MailboxDetail,
    MailboxEmailRecoveryBlocker, MailboxGetError, MailboxGetOutcome, MailboxIdentityMapping,
    MailboxIdentityMappingInput, MailboxIssue, MailboxProjectionKind, MailboxProvider,
    MailboxRefInput, MailboxSendingDraftBlocker, resend_auth_connection_id,
};
pub use queries::{
    EMAIL_THREAD_MESSAGE_PAGE_LIMITS, EmailFolderSummary, EmailMessageGetInput,
    EmailThreadGetInput, EmailThreadMutationInput, EmailThreadSort, ListEmailFoldersInput,
    ListEmailThreadsInput, MAIL_PAGE_LIMITS, MailboxGetInput, UpdateEmailThreadDispositionInput,
};
pub use recovery::{
    CommitEmailActionInput, CommitEmailActionResult, CommitEmailDraftActionInput,
    CommitManualEmailActionInput, CommitManualEmailActionResult, EmailActionKind, EmailAddress,
    EmailDraftSendClaim, EmailDraftSendFingerprint, EmailMessageInput, EmailSnapshotInput,
    RemoteAcceptedEmailAction, RemoteAcceptedEmailContext, RemoteEmailRecovery,
    RemoteStartedEmailAction, ReserveEmailDraftSendInput,
};
pub(crate) use recovery::{
    decode_persisted_email_action_result, decode_persisted_manual_email_action_result,
    encode_persisted_email_action_result, encode_persisted_manual_email_action_result,
    persisted_mailbox_snapshot,
};
pub use sending::{
    EmailActionResult, EmailPreparation, ManualEmailActionResult, ManualEmailPreparation,
    PrepareEmailInput, PrepareManualEmailInput, SendEmailInput, SendManualEmailInput,
};
pub(crate) use sync_state::EmailSyncIncrementalContinuation;
pub use sync_state::{
    EmailAttachmentInput, EmailOrigin, EmailSyncAdvance, EmailSyncCheckpoint, EmailSyncFailure,
    EmailSyncIncrementalContinuationMutation, EmailSyncPageCommit, EmailSyncPageOutcome,
    EmailSyncRunMode, EmailSyncSentMutations, EmailSyncState, EmailSyncStateView,
    EmailSyncThreadMerge, MailboxDirectionOutcome, MailboxSyncOutcome, MailboxSyncProgress,
    MailboxSyncStatus, ReceivedSyncReport, SentSyncReport, SyncMailboxRequest,
};
pub use templates::{
    DeleteEmailTemplateInput, EmailTemplate, EmailTemplateGetInput, EmailTemplateSummary,
    ListEmailTemplatesInput, RenderEmailTemplateInput, RenderedEmailTemplate,
    SaveEmailTemplateInput,
};
pub use threads::{
    EMAIL_PREVIEW_MAX_LENGTH, EmailMessage, EmailMessagePreview, EmailMessageView, EmailThread,
    EmailThreadSummary, EmailThreadView,
};

#[cfg(test)]
mod projection_tests {
    use crate::models::{
        EMAIL_PREVIEW_MAX_LENGTH, EmailDirection, EmailDraft, EmailDraftId, EmailDraftState,
        EmailId, EmailMessage, EmailThread, EmailThreadFolder, EmailThreadId, MailboxRefId,
    };

    fn message() -> EmailMessage {
        EmailMessage {
            id: EmailId::new("email-1"),
            thread_id: EmailThreadId::new("thread-1"),
            provider_message_id: "provider-1".into(),
            internet_message_id: Some("<m@example.com>".into()),
            direction: EmailDirection::Inbound,
            from: "from@example.com".into(),
            to: vec!["to@example.com".into()],
            cc: Vec::new(),
            bcc: vec!["hidden@example.com".into()],
            received_for: vec!["routing@example.com".into()],
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            subject: "Subject".into(),
            text: Some("body".into()),
            html: Some("<p>x</p>".into()),
            delivery_status: "unread".into(),
            headers: serde_json::json!({"x-raw": "internal"}),
            occurred_at: 1,
        }
    }

    #[test]
    fn public_email_projections_omit_internal_fields() {
        let view = serde_json::to_value(message().view()).unwrap();
        for field in ["bcc", "received_for", "headers"] {
            assert!(view.get(field).is_none(), "view leaked {field}");
        }
        assert_eq!(view["text"], "body");

        let preview = serde_json::to_value(message().preview()).unwrap();
        for field in ["bcc", "received_for", "headers", "text", "html"] {
            assert!(preview.get(field).is_none(), "preview leaked {field}");
        }
        assert_eq!(preview["has_html"], true);

        let thread = serde_json::to_value(EmailThread {
            id: EmailThreadId::new("thread-1"),
            mailbox_ref_id: MailboxRefId::new("mailbox-1"),
            creator_id: None,
            contact_route_id: None,
            provider_thread_key: "key".into(),
            subject: "Subject".into(),
            unread_count: 0,
            folder: EmailThreadFolder::Inbox,
            created_at: 1,
            updated_at: 1,
        })
        .unwrap();
        assert!(
            thread.get("cursor").is_none(),
            "thread leaked provider cursor"
        );
    }

    #[test]
    fn draft_summary_omits_bodies_and_the_pending_send_key() {
        let draft = EmailDraft {
            id: EmailDraftId::new("draft-1"),
            mailbox_ref_id: MailboxRefId::new("mailbox-1"),
            to: "to@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Subject".into(),
            text: "x".repeat(400),
            html: None,
            template_id: None,
            template_variables: Default::default(),
            state: EmailDraftState::Draft,
            sending: true,
            pending_send_idempotency_key: Some("draft-key-1".into()),
            revision: 1,
            created_at: 1,
            updated_at: 1,
        };
        let summary = serde_json::to_value(draft.summary()).unwrap();
        for field in [
            "text",
            "html",
            "template_variables",
            "pending_send_idempotency_key",
        ] {
            assert!(summary.get(field).is_none(), "summary leaked {field}");
        }
        assert_eq!(
            summary["preview_text"].as_str().unwrap().chars().count(),
            EMAIL_PREVIEW_MAX_LENGTH
        );
        assert_eq!(summary["sending"], true);
    }
}
