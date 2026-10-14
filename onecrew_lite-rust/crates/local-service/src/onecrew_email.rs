use onecrew_creator_runtime::{
    DeleteEmailTemplateInput, EmailDraft, EmailDraftGetInput, EmailDraftMutationInput,
    EmailDraftSendResult, EmailDraftSummary, EmailFolderSummary, EmailSendFailure, EmailTemplate,
    EmailTemplateGetInput, EmailTemplateSummary, EmailThreadGetInput, EmailThreadMutationInput,
    EmailThreadSummary, EmailThreadView, ListEmailDraftsInput, ListEmailFoldersInput,
    ListEmailTemplatesInput, ListEmailThreadsInput, MailboxDetail, MailboxGetInput,
    MailboxSyncOutcome, MailboxSyncStatus, Page, RenderEmailTemplateInput, RenderedEmailTemplate,
    SaveEmailDraftInput, SaveEmailTemplateInput, SendEmailDraftInput, SyncMailboxRequest,
    UpdateEmailThreadDispositionInput,
};

use crate::ServiceState;

pub(super) async fn onecrew_gmail_bulk_recipients(state: &ServiceState) -> Result<Vec<onecrew_creator_runtime::GmailBulkRecipient>,String> {
    state.platform().await?.creators()?.gmail_bulk_recipients().await
}

pub(super) async fn onecrew_gmail_bulk_list(state: &ServiceState) -> Result<Vec<onecrew_creator_runtime::GmailBulkBatch>,String> {
    state.platform().await?.creators()?.list_gmail_bulk().await
}
pub(super) async fn onecrew_gmail_bulk_prepare(state: &ServiceState,input:onecrew_creator_runtime::PrepareGmailBulkInput) -> Result<onecrew_creator_runtime::GmailBulkBatch,String> {
    state.platform().await?.creators()?.prepare_gmail_bulk(input).await
}
pub(super) async fn onecrew_gmail_bulk_control(state: &ServiceState,input:onecrew_creator_runtime::GmailBulkControlInput) -> Result<(),String> {
    state.platform().await?.creators()?.control_gmail_bulk(input).await
}

pub(super) async fn onecrew_gmail_connect_url(state: &ServiceState) -> Result<String, String> {
    state.platform().await?.gmail_connect_url().await
}
pub(super) async fn onecrew_gmail_refresh(
    state: &ServiceState,
) -> Result<Vec<onecrew_local_runtime::AccessMailConnection>, String> {
    state.platform().await?.refresh_gmail_mailboxes().await
}

pub(super) async fn onecrew_mail_access_connections(
    state: &ServiceState,
) -> Result<Vec<onecrew_local_runtime::AccessMailConnection>, String> {
    state.platform().await?.mail_access_connections().await
}

pub(super) async fn onecrew_email_mailbox_list(
    state: &ServiceState,
) -> Result<Vec<MailboxDetail>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_mailboxes_async()
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_mailbox_sync(
    state: &ServiceState,
    input: SyncMailboxRequest,
) -> Result<MailboxSyncOutcome, String> {
    state
        .platform()
        .await?
        .creators()?
        .sync_mailbox_full(input)
        .await
}

pub(super) async fn onecrew_email_mailbox_sync_status(
    state: &ServiceState,
    input: MailboxGetInput,
) -> Result<MailboxSyncStatus, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .mailbox_sync_status_async(input.mailbox_ref_id)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_thread_list(
    state: &ServiceState,
    input: ListEmailThreadsInput,
) -> Result<Page<EmailThreadSummary>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_email_threads_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_folder_list(
    state: &ServiceState,
    input: ListEmailFoldersInput,
) -> Result<Vec<EmailFolderSummary>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_email_folders_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_thread_update_disposition(
    state: &ServiceState,
    input: UpdateEmailThreadDispositionInput,
) -> Result<EmailThreadView, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .update_email_thread_disposition_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_thread_mark_read(
    state: &ServiceState,
    input: EmailThreadMutationInput,
) -> Result<EmailThreadView, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .mark_email_thread_read_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_thread_get(
    state: &ServiceState,
    input: EmailThreadGetInput,
) -> Result<EmailThreadView, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .get_email_thread_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_template_list(
    state: &ServiceState,
    input: ListEmailTemplatesInput,
) -> Result<Page<EmailTemplateSummary>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_email_templates_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_template_get(
    state: &ServiceState,
    input: EmailTemplateGetInput,
) -> Result<EmailTemplate, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .get_email_template_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_template_save(
    state: &ServiceState,
    input: SaveEmailTemplateInput,
) -> Result<EmailTemplate, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .save_email_template_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_template_delete(
    state: &ServiceState,
    input: DeleteEmailTemplateInput,
) -> Result<bool, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .delete_email_template_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_template_render(
    state: &ServiceState,
    input: RenderEmailTemplateInput,
) -> Result<RenderedEmailTemplate, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .render_email_template_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_draft_list(
    state: &ServiceState,
    input: ListEmailDraftsInput,
) -> Result<Page<EmailDraftSummary>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_email_drafts_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_draft_get(
    state: &ServiceState,
    input: EmailDraftGetInput,
) -> Result<EmailDraft, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .get_email_draft_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_draft_save(
    state: &ServiceState,
    input: SaveEmailDraftInput,
) -> Result<EmailDraft, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .save_email_draft_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_draft_trash(
    state: &ServiceState,
    input: EmailDraftMutationInput,
) -> Result<EmailDraft, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .trash_email_draft_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_draft_restore(
    state: &ServiceState,
    input: EmailDraftMutationInput,
) -> Result<EmailDraft, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .restore_email_draft_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_email_draft_send(
    state: &ServiceState,
    input: SendEmailDraftInput,
) -> Result<EmailDraftSendResult, String> {
    state
        .platform()
        .await?
        .creators()?
        .send_email_draft(input)
        .await
        .map_err(EmailSendFailure::into_message)
}
