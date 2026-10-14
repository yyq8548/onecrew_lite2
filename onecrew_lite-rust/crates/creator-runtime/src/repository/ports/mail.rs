use crate::{
    AcknowledgeReplyTriageInput, CommitEmailActionInput, CommitEmailDraftActionInput,
    CommitManualEmailActionInput, ConversationId, CreatorId, DeleteEmailTemplateInput,
    EmailActionResult, EmailAuthConnectionDeletionClaim, EmailAuthConnectionId, EmailDraft,
    EmailDraftId, EmailDraftMutationInput, EmailDraftSendClaim, EmailDraftSendResult,
    EmailDraftState, EmailFolder, EmailFolderSummary, EmailMessage, EmailSnapshotInput,
    EmailTemplate, EmailTemplateId, EmailThread, EmailThreadId, EmailThreadSummary,
    ListReplyTriageInput, MailboxAuthBinding, MailboxDetail, MailboxIdentityMapping,
    MailboxIdentityMappingInput, MailboxRefId, MailboxRefInput, ManualEmailActionResult, Page,
    RecordReplyTriageInput, RemoteStartedEmailAction, ReplyTriage, ReplyTriageBrief,
    ReplyTriageView, ReserveEmailDraftSendInput, SaveEmailDraftInput, SaveEmailTemplateInput,
};

pub trait MailboxRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_mailbox_ref(&self, input: MailboxRefInput) -> Result<MailboxDetail, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn reconcile_mailbox_refs(
        &self,
        inputs: Vec<MailboxRefInput>,
    ) -> Result<Vec<MailboxDetail>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn find_mailbox_ref(&self, id: &MailboxRefId) -> Result<Option<MailboxDetail>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_mailbox_refs(&self) -> Result<Vec<MailboxDetail>, String>;
    /// Persist and read the provider-neutral mailbox/auth projection. The
    /// secret record is referenced by opaque id and is never returned here.
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_mailbox_identity_mapping(
        &self,
        input: MailboxIdentityMappingInput,
    ) -> Result<MailboxIdentityMapping, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn resolve_mailbox_auth_binding(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<MailboxAuthBinding, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn begin_email_auth_connection_deletion(
        &self,
        auth_connection_id: &EmailAuthConnectionId,
    ) -> Result<EmailAuthConnectionDeletionClaim, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn finish_email_auth_connection_deletion(
        &self,
        auth_connection_id: &EmailAuthConnectionId,
    ) -> Result<(), String>;
}

pub trait EmailRepository: Send + Sync {
    /// One provider seed per conversation authored locally, scoped to the mailbox.
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn outreach_reply_seeds(&self, mailbox_ref_id: &MailboxRefId) -> Result<Vec<(EmailThreadId, String)>, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn persist_email_snapshot(
        &self,
        input: EmailSnapshotInput,
    ) -> Result<(EmailThread, Vec<EmailMessage>), String>;
    /// Commits one provider page atomically: snapshots plus, only when the
    /// page completed all required work, the direction's durable advance.
    #[cfg_attr(any(), repository_execution(write))]
    fn commit_email_sync_page(
        &self,
        input: crate::EmailSyncPageCommit,
    ) -> Result<crate::EmailSyncPageOutcome, String>;
    /// Reads the durable public boundary and private Phase A continuation in
    /// one statement snapshot. This evolves the original sync-state port;
    /// private continuation does not add another repository operation.
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn email_sync_checkpoint(
        &self,
        mailbox_ref_id: &MailboxRefId,
        direction: crate::EmailDirection,
    ) -> Result<crate::EmailSyncCheckpoint, String>;
    /// Reads both durable direction states and pending inbound triage from
    /// one short committed SQLite snapshot.
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn mailbox_sync_readback(
        &self,
        mailbox_ref_id: &MailboxRefId,
        triage_limit: u32,
    ) -> Result<
        (
            Option<crate::EmailSyncState>,
            Option<crate::EmailSyncState>,
            crate::PendingReplyTriageDebt,
        ),
        String,
    >;
    /// The subset of the given provider ids whose full detail is already
    /// local for this mailbox.
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn complete_email_provider_ids(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<String>, String>;
    /// Applies sent list-metadata delivery-status updates in place; returns
    /// thread ids whose status actually changed.
    #[cfg_attr(any(), repository_execution(write))]
    fn reconcile_sent_statuses(
        &self,
        mailbox_ref_id: &MailboxRefId,
        updates: &[(String, String)],
    ) -> Result<Vec<EmailThreadId>, String>;
    /// Threads containing RFC evidence naming the given Message-ID, oldest
    /// first.
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn find_email_threads_referencing(
        &self,
        mailbox_ref_id: &MailboxRefId,
        internet_message_id: &str,
    ) -> Result<Vec<EmailThreadId>, String>;
    /// Merges RFC-proven sibling threads into the target, preserving stable
    /// message ids and every referencing fact.
    #[cfg_attr(any(), repository_execution(write))]
    fn merge_email_threads(
        &self,
        target: &EmailThreadId,
        sources: &[EmailThreadId],
    ) -> Result<(), String>;
    /// Reads one thread record by id.
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String>;
    /// Durable attachment metadata for one email.
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn email_attachments(
        &self,
        email_id: &crate::EmailId,
    ) -> Result<Vec<crate::EmailAttachmentView>, String>;
    /// The provider coordinates of one email: owning mailbox, provider
    /// message id, and direction.
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn email_provider_coordinates(
        &self,
        email_id: &crate::EmailId,
    ) -> Result<(MailboxRefId, String, crate::EmailDirection), String>;
    /// Upserts provider-listed attachment metadata (sent emails embed none
    /// at import time); never touches URLs.
    #[cfg_attr(any(), repository_execution(write))]
    fn record_email_attachments(
        &self,
        email_id: &crate::EmailId,
        direction: crate::EmailDirection,
        metadata: &[onecrew_email_contracts::PlatformEmailAttachment],
    ) -> Result<(), String>;
    /// Records durable evidence that the provider no longer holds this
    /// attachment's content.
    #[cfg_attr(any(), repository_execution(write))]
    fn mark_email_attachment_unavailable(
        &self,
        email_id: &crate::EmailId,
        provider_attachment_id: &str,
    ) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_email_threads(
        &self,
        mailbox_ref_id: Option<&MailboxRefId>,
        folder: EmailFolder,
        sort: crate::EmailThreadSort,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<EmailThreadSummary>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_email_folder_counts(
        &self,
        mailbox_ref_id: Option<&MailboxRefId>,
    ) -> Result<Vec<EmailFolderSummary>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_email_thread_record(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn get_email_thread_messages(
        &self,
        thread_id: &EmailThreadId,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<crate::EmailMessageView>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn find_email_thread_by_internet_message_ids(
        &self,
        mailbox_ref_id: &MailboxRefId,
        internet_message_ids: &[String],
    ) -> Result<Option<EmailThread>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn get_email_message(&self, email_id: &crate::EmailId) -> Result<EmailMessage, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn archive_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn unarchive_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn trash_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn restore_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String>;
    /// Clear the thread's inbound unread marks. This is attention state, not a
    /// thread event, so it must not move the thread's `updated_at`.
    #[cfg_attr(any(), repository_execution(write))]
    fn mark_email_thread_read(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn email_batch_preflight_snapshot(
        &self,
        mailbox_ref_id: &MailboxRefId,
        creator_ids: &[CreatorId],
        template_ids: &[EmailTemplateId],
    ) -> Result<crate::EmailBatchPreflightSnapshot, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn commit_email_action(
        &self,
        input: CommitEmailActionInput,
    ) -> Result<EmailActionResult, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn commit_manual_email_action(
        &self,
        input: CommitManualEmailActionInput,
    ) -> Result<ManualEmailActionResult, String>;
    /// Atomically persist all provider-readback-verified batch items and
    /// complete the existing idempotency claim. Rejected/unknown items are
    /// retained only in the action result and never become local messages.
    #[cfg_attr(any(), repository_execution(write))]
    fn commit_email_batch_action(
        &self,
        _input: crate::EmailBatchCommitInput,
    ) -> Result<crate::EmailBatchActionResult, String> {
        Err("native email batch persistence is unavailable".into())
    }
}

pub trait EmailTemplateRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_email_templates(
        &self,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<crate::EmailTemplateSummary>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_email_template(&self, template_id: &EmailTemplateId) -> Result<EmailTemplate, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn save_email_template(
        &self,
        input: SaveEmailTemplateInput,
        variables: Vec<String>,
    ) -> Result<EmailTemplate, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn delete_email_template(&self, input: DeleteEmailTemplateInput) -> Result<bool, String>;
}

pub trait EmailDraftRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_email_drafts(
        &self,
        mailbox_ref_id: Option<&MailboxRefId>,
        state: Option<EmailDraftState>,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<crate::EmailDraftSummary>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_email_draft(&self, draft_id: &EmailDraftId) -> Result<EmailDraft, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn save_email_draft(&self, input: SaveEmailDraftInput) -> Result<EmailDraft, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn set_email_draft_state(
        &self,
        input: EmailDraftMutationInput,
        state: EmailDraftState,
    ) -> Result<EmailDraft, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn find_email_draft_send(
        &self,
        input: ReserveEmailDraftSendInput,
    ) -> Result<Option<EmailDraftSendClaim>, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_remote_started_email_draft_send(
        &self,
        input: ReserveEmailDraftSendInput,
        recovery: RemoteStartedEmailAction,
    ) -> Result<EmailDraftSendClaim, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn stage_verified_email_draft_action(
        &self,
        input: &CommitEmailDraftActionInput,
    ) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn release_email_draft_send(
        &self,
        draft_id: &EmailDraftId,
        idempotency_key: &str,
    ) -> Result<(), String>;
    /// Atomically persist a terminal send failure and release the draft.
    /// `Some(error)` means failure persistence failed and the same transaction
    /// instead deleted the claim before releasing the reservation for retry.
    #[cfg_attr(any(), repository_execution(write))]
    fn fail_and_release_email_draft_send(
        &self,
        draft_id: &EmailDraftId,
        idempotency_key: &str,
        failure: serde_json::Value,
    ) -> Result<Option<String>, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn commit_email_draft_action(
        &self,
        input: CommitEmailDraftActionInput,
    ) -> Result<EmailDraftSendResult, String>;
}

pub trait ReplyTriageRepository: Send + Sync {
    /// Record one agent verdict about one inbound message. Judging the same
    /// message again replaces the reading and clears any acknowledgement.
    #[cfg_attr(any(), repository_execution(write))]
    fn record_reply_triage(&self, input: RecordReplyTriageInput) -> Result<ReplyTriage, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn acknowledge_reply_triage(
        &self,
        input: AcknowledgeReplyTriageInput,
    ) -> Result<ReplyTriage, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_reply_triage(
        &self,
        input: ListReplyTriageInput,
    ) -> Result<Page<ReplyTriageView>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn latest_triage_for_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<ReplyTriageBrief>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn latest_triage_for_email_thread(
        &self,
        thread_id: &EmailThreadId,
    ) -> Result<Option<ReplyTriageBrief>, String>;
    /// 一个邮箱里还没有人判断过的入站邮件。同步把它们收了进来,但没
    /// 人说过它们是什么意思——这是欠着的账,不是刚到的那一页。
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn pending_email_triage(
        &self,
        mailbox_ref_id: &MailboxRefId,
        limit: u32,
    ) -> Result<crate::PendingReplyTriageDebt, String>;
    /// 同一笔账在私信这边:一条会话里还没判断过的入站消息。
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn pending_conversation_triage(
        &self,
        conversation_id: &ConversationId,
        limit: u32,
    ) -> Result<crate::PendingReplyTriageDebt, String>;
}
