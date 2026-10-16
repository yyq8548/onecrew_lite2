use onecrew_creator_runtime::{EmailGateway, MailboxRefId};
use onecrew_email_contracts::{
    EmailBatchRequest, EmailBatchResult, EmailCapability, PlatformAttachmentAccess,
    PlatformEmailAttachmentError, PlatformEmailMessage, PlatformEmailPage,
    PlatformEmailReceipt, PlatformEmailRequest, PlatformEmailSendError, PlatformSentEmailPage,
};
use std::sync::Arc;

use super::services::CredentialServices;

const UNSUPPORTED_MAILBOX: &str = "lite_mailbox_provider_unsupported";

pub(crate) struct RuntimeEmailGateway {
    services: Arc<CredentialServices>,
}

impl RuntimeEmailGateway {
    pub(crate) fn new(services: Arc<CredentialServices>) -> Self {
        Self { services }
    }
}

#[async_trait::async_trait]
impl EmailGateway for RuntimeEmailGateway {
    async fn send_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        request: PlatformEmailRequest,
    ) -> Result<PlatformEmailReceipt, PlatformEmailSendError> {
        let capability = if request.in_reply_to.is_some() || !request.references.is_empty() {
            EmailCapability::Reply
        } else {
            EmailCapability::Send
        };
        let mailbox = self.services.gmail_mailbox(mailbox_ref_id, capability).await
            .map_err(|message| PlatformEmailSendError::Validation { message })?
            .ok_or_else(|| PlatformEmailSendError::Validation {
                message: UNSUPPORTED_MAILBOX.into(),
            })?;
        mailbox.platform.send(mailbox.token.expose_secret(), &mailbox.email, &request).await
    }

    async fn send_many_email(
        &self,
        _mailbox_ref_id: &MailboxRefId,
        _request: EmailBatchRequest,
    ) -> Result<EmailBatchResult, PlatformEmailSendError> {
        Err(PlatformEmailSendError::Validation {
            message: "lite_email_batch_send_unsupported_use_gmail_bulk".into(),
        })
    }

    async fn sent_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String> {
        let mailbox = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::SentReadback)
            .await?.ok_or(UNSUPPORTED_MAILBOX)?;
        mailbox.platform.sent(mailbox.token.expose_secret(), provider_message_id).await
    }

    async fn outreach_reply_ids(
        &self,
        mailbox_ref_id: &MailboxRefId,
        sent_id: &str,
    ) -> Result<Vec<String>, String> {
        let mailbox = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox)
            .await?.ok_or("outreach_reply_read_unavailable")?;
        mailbox.platform.outreach_reply_ids(mailbox.token.expose_secret(), sent_id).await
    }

    async fn received_emails(
        &self,
        mailbox_ref_id: &MailboxRefId,
        after: Option<&str>,
        limit: u16,
    ) -> Result<PlatformEmailPage, String> {
        let mailbox = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox)
            .await?.ok_or(UNSUPPORTED_MAILBOX)?;
        mailbox.platform.inbox(mailbox.token.expose_secret(), after, limit).await
    }

    async fn received_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String> {
        let mailbox = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox)
            .await?.ok_or(UNSUPPORTED_MAILBOX)?;
        mailbox.platform.received(mailbox.token.expose_secret(), provider_message_id, &mailbox.email).await
    }

    async fn sent_emails(
        &self,
        _mailbox_ref_id: &MailboxRefId,
        _after: Option<&str>,
        _limit: u16,
    ) -> Result<PlatformSentEmailPage, String> {
        Err("lite_sent_email_listing_unsupported".into())
    }

    async fn sent_email_details(
        &self,
        _mailbox_ref_id: &MailboxRefId,
        _provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String> {
        Err("lite_sent_email_batch_details_unsupported".into())
    }

    async fn received_email_details(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String> {
        if provider_message_ids.len() > 100 {
            return Err("gmail_detail_batch_too_large".into());
        }
        let mailbox = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox)
            .await?.ok_or(UNSUPPORTED_MAILBOX)?;
        let mut results = Vec::with_capacity(provider_message_ids.len());
        for chunk in provider_message_ids.chunks(4) {
            let outcomes = futures::future::join_all(chunk.iter().map(|id|
                mailbox.platform.received(mailbox.token.expose_secret(), id, &mailbox.email)
            )).await;
            results.extend(chunk.iter().cloned().zip(outcomes));
        }
        Ok(results)
    }

    async fn received_attachment_access(
        &self,
        _mailbox_ref_id: &MailboxRefId,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<PlatformAttachmentAccess, PlatformEmailAttachmentError> {
        Err(PlatformEmailAttachmentError::Failed {
            message: "lite_email_attachment_access_unsupported".into(),
        })
    }

    async fn sent_attachments_access(
        &self,
        _mailbox_ref_id: &MailboxRefId,
        _provider_message_id: &str,
    ) -> Result<Vec<PlatformAttachmentAccess>, PlatformEmailAttachmentError> {
        Err(PlatformEmailAttachmentError::Failed {
            message: "lite_email_attachment_access_unsupported".into(),
        })
    }

    async fn download_email_attachment(
        &self,
        _mailbox_ref_id: &MailboxRefId,
        _access: &PlatformAttachmentAccess,
        _limit: usize,
    ) -> Result<Vec<u8>, PlatformEmailAttachmentError> {
        Err(PlatformEmailAttachmentError::Failed {
            message: "lite_email_attachment_download_unsupported".into(),
        })
    }
}
