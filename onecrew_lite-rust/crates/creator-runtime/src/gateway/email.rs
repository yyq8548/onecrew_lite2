use crate::MailboxRefId;
use async_trait::async_trait;
use onecrew_email_contracts::{
    EmailBatchRequest, EmailBatchResult, PlatformAttachmentAccess, PlatformEmailAttachmentError,
    PlatformEmailMessage, PlatformEmailPage, PlatformEmailReceipt, PlatformEmailRequest,
    PlatformEmailSendError, PlatformSentEmailPage,
};

/// Provider-neutral outbound email capability. It is optional in the runtime
/// composition until a mailbox has been connected.
#[async_trait]
pub trait EmailGateway: Send + Sync {
    async fn send_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        request: PlatformEmailRequest,
    ) -> Result<PlatformEmailReceipt, PlatformEmailSendError>;

    /// Native batch sending is opt-in. An unavailable implementation reports
    /// a definite validation failure before any provider call.
    async fn send_many_email(
        &self,
        _mailbox_ref_id: &MailboxRefId,
        _request: EmailBatchRequest,
    ) -> Result<EmailBatchResult, PlatformEmailSendError> {
        Err(PlatformEmailSendError::Validation {
            message:
                "email provider does not expose native batch sending; no provider call occurred"
                    .into(),
        })
    }

    async fn sent_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String>;

    /// Read only replies in the provider conversation containing a locally sent message.
    async fn outreach_reply_ids(
        &self, _mailbox_ref_id: &MailboxRefId, _sent_message_id: &str,
    ) -> Result<Vec<String>, String> {
        Err("outreach_reply_read_unavailable".into())
    }

    async fn received_emails(
        &self,
        mailbox_ref_id: &MailboxRefId,
        after: Option<&str>,
        limit: u16,
    ) -> Result<PlatformEmailPage, String>;

    async fn received_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String>;

    async fn received_email_details(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String>;

    async fn sent_emails(
        &self,
        mailbox_ref_id: &MailboxRefId,
        after: Option<&str>,
        limit: u16,
    ) -> Result<PlatformSentEmailPage, String>;

    async fn sent_email_details(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String>;

    async fn received_attachment_access(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<PlatformAttachmentAccess, PlatformEmailAttachmentError>;

    async fn sent_attachments_access(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<Vec<PlatformAttachmentAccess>, PlatformEmailAttachmentError>;

    async fn download_email_attachment(
        &self,
        mailbox_ref_id: &MailboxRefId,
        access: &PlatformAttachmentAccess,
        limit: usize,
    ) -> Result<Vec<u8>, PlatformEmailAttachmentError>;
}

#[cfg(test)]
mod tests {
    use onecrew_email_contracts::PlatformEmailSendError;

    #[test]
    fn only_validation_and_explicit_4xx_are_terminal_send_rejections() {
        assert!(
            PlatformEmailSendError::Validation {
                message: "invalid message".into(),
            }
            .is_terminal_rejection()
        );
        assert!(
            PlatformEmailSendError::Rejected {
                status: 499,
                message: "rejected".into(),
            }
            .is_terminal_rejection()
        );
        assert!(
            !PlatformEmailSendError::Rejected {
                status: 503,
                message: "unavailable".into(),
            }
            .is_terminal_rejection()
        );
        assert!(
            !PlatformEmailSendError::Ambiguous {
                message: "timeout".into(),
            }
            .is_terminal_rejection()
        );
    }
}
