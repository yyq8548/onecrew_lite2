//! Runtime Email port implementation. Mailbox ownership is resolved before
//! the credential blocking segment; provider awaits hold only owned values.

use onecrew_email_contracts::{
    EmailBatchRequest, EmailBatchResult, PlatformEmailMessage, PlatformEmailPage,
    PlatformEmailReceipt, PlatformEmailRequest, PlatformEmailSendError, PlatformSentEmailPage,
};
use onecrew_email_contracts::{
    EmailCapability, EmailError, OutboundEmail, ProviderInboundEmail, ProviderSentEmail,
};
use uuid::Uuid;

use super::services::{CredentialServices, blocking_segment};
use onecrew_creator_runtime::{EmailGateway, MailboxRefId};
use std::sync::Arc;

mod mapping;

use self::mapping::{
    note_accepted_send_health_error, platform_accepted_send, platform_attachment_access,
    platform_attachment_error, platform_email_page, platform_email_send_error,
    platform_inbound_email, platform_sent_email, platform_sent_page,
};

fn failed_attachment(message: String) -> onecrew_email_contracts::PlatformEmailAttachmentError {
    onecrew_email_contracts::PlatformEmailAttachmentError::Failed { message }
}

pub(crate) struct RuntimeEmailGateway {
    services: Arc<CredentialServices>,
}

impl RuntimeEmailGateway {
    pub(crate) fn new(services: Arc<CredentialServices>) -> Self {
        Self { services }
    }

    async fn record_success(&self, id: Uuid) -> Result<(), String> {
        let services = Arc::clone(&self.services);
        blocking_segment(
            "Email credential health update was interrupted".to_owned(),
            move || services.mark_resend_success(id),
        )
        .await
    }

    async fn record_failure(&self, id: Uuid, error: &EmailError) -> Result<(), String> {
        let services = Arc::clone(&self.services);
        let error = error.clone();
        blocking_segment(
            "Email credential health update was interrupted".to_owned(),
            move || services.mark_resend_failure(id, &error),
        )
        .await
    }

    async fn finish_sent_message_read(
        &self,
        credential_id: Uuid,
        result: Result<ProviderSentEmail, EmailError>,
    ) -> Result<PlatformEmailMessage, String> {
        match result {
            Ok(message) => {
                self.record_success(credential_id).await?;
                Ok(platform_sent_email(message))
            }
            Err(error) => {
                self.record_failure(credential_id, &error).await?;
                Err(error.to_string())
            }
        }
    }

    async fn finish_inbound_message_read(
        &self,
        credential_id: Uuid,
        result: Result<ProviderInboundEmail, EmailError>,
    ) -> Result<PlatformEmailMessage, String> {
        match result {
            Ok(message) => {
                self.record_success(credential_id).await?;
                Ok(platform_inbound_email(message))
            }
            Err(error) => {
                self.record_failure(credential_id, &error).await?;
                Err(error.to_string())
            }
        }
    }
}

#[async_trait::async_trait]
impl EmailGateway for RuntimeEmailGateway {
    async fn send_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        request: PlatformEmailRequest,
    ) -> Result<PlatformEmailReceipt, PlatformEmailSendError> {
        let required_capability = if request.in_reply_to.is_some() || !request.references.is_empty()
        {
            EmailCapability::Reply
        } else {
            EmailCapability::Send
        };
        if let Some(mailbox) = self
            .services
            .gmail_mailbox(mailbox_ref_id, required_capability)
            .await
            .map_err(|message| PlatformEmailSendError::Validation { message })?
        {
            return mailbox
                .platform
                .send(mailbox.token.expose_secret(), &mailbox.email, &request)
                .await;
        }
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), required_capability)
            .await
            .map_err(|message| PlatformEmailSendError::Validation { message })?;
        let result = mailbox
            .sender
            .send(&OutboundEmail {
                from_name: mailbox.display_name.clone(),
                from_email: mailbox.email_address.clone(),
                to: request.to,
                cc: request.cc,
                bcc: request.bcc,
                reply_to: request.reply_to,
                in_reply_to: request.in_reply_to,
                references: request.references,
                idempotency_key: request.idempotency_key,
                subject: request.subject,
                text: request.text,
                html: request.html,
            })
            .await;
        match result {
            // Resend accepted the message, so this send is settled as a
            // success. Credential health is written afterwards and reports
            // only on the key, so it can never revise that conclusion.
            Ok(receipt) => Ok(platform_accepted_send(
                receipt,
                self.record_success(mailbox.credential_id).await.err(),
            )),
            Err(error) => {
                let health_error = self
                    .record_failure(mailbox.credential_id, &error)
                    .await
                    .err();
                Err(platform_email_send_error(error, health_error))
            }
        }
    }

    async fn send_many_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        request: EmailBatchRequest,
    ) -> Result<EmailBatchResult, PlatformEmailSendError> {
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::Send)
            .await
            .map_err(|message| PlatformEmailSendError::Validation { message })?;
        request
            .validate()
            .map_err(|error| PlatformEmailSendError::Validation {
                message: error.to_string(),
            })?;
        match mailbox.sender.send_many(&request).await {
            // The provider already accepted the batch and returned its
            // per-item outcomes; a failed health write leaves them untouched.
            Ok(result) => {
                note_accepted_send_health_error(
                    self.record_success(mailbox.credential_id).await.err(),
                );
                Ok(result)
            }
            Err(error) => {
                let health_error = self
                    .record_failure(mailbox.credential_id, &error)
                    .await
                    .err();
                Err(platform_email_send_error(error, health_error))
            }
        }
    }

    async fn sent_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String> {
        if let Some(mailbox) = self
            .services
            .gmail_mailbox(mailbox_ref_id, EmailCapability::SentReadback)
            .await?
        {
            return mailbox
                .platform
                .sent(mailbox.token.expose_secret(), provider_message_id)
                .await;
        }
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::SentReadback)
            .await?;
        let result = mailbox.sender.sent(provider_message_id).await;
        self.finish_sent_message_read(mailbox.credential_id, result)
            .await
    }

    async fn outreach_reply_ids(&self, mailbox_ref_id: &MailboxRefId, sent_id: &str) -> Result<Vec<String>, String> {
        let mailbox = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox).await?
            .ok_or("outreach_reply_read_unavailable")?;
        mailbox.platform.outreach_reply_ids(mailbox.token.expose_secret(), sent_id).await
    }

    async fn received_emails(
        &self,
        mailbox_ref_id: &MailboxRefId,
        after: Option<&str>,
        limit: u16,
    ) -> Result<PlatformEmailPage, String> {
        if let Some(mailbox) = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox).await? {
            return mailbox.platform.inbox(mailbox.token.expose_secret(), after, limit).await;
        }
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::Inbox)
            .await?;
        match mailbox.sender.received(after, limit).await {
            Ok(page) => {
                let page = platform_email_page(page, &mailbox.email_address);
                self.record_success(mailbox.credential_id).await?;
                Ok(page)
            }
            Err(error) => {
                self.record_failure(mailbox.credential_id, &error).await?;
                Err(error.to_string())
            }
        }
    }

    async fn received_email(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String> {
        if let Some(mailbox) = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox).await? {
            return mailbox.platform.received(mailbox.token.expose_secret(), provider_message_id, &mailbox.email).await;
        }
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::Inbox)
            .await?;
        let result = mailbox.sender.received_message(provider_message_id).await;
        self.finish_inbound_message_read(mailbox.credential_id, result)
            .await
    }

    async fn sent_emails(
        &self,
        mailbox_ref_id: &MailboxRefId,
        after: Option<&str>,
        limit: u16,
    ) -> Result<PlatformSentEmailPage, String> {
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::SentReadback)
            .await?;
        match mailbox.sender.sent_page(after, limit).await {
            Ok(page) => {
                let page = platform_sent_page(page, &mailbox.email_address);
                self.record_success(mailbox.credential_id).await?;
                Ok(page)
            }
            Err(error) => {
                self.record_failure(mailbox.credential_id, &error).await?;
                Err(error.to_string())
            }
        }
    }

    async fn sent_email_details(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String> {
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::SentReadback)
            .await?;
        let outcomes = mailbox
            .sender
            .sent_details(provider_message_ids)
            .await
            .map_err(|error| error.to_string())?;
        let mut first_failure = None;
        let mut any_success = false;
        let results = outcomes
            .into_iter()
            .map(|(id, outcome)| match outcome {
                Ok(message) => {
                    any_success = true;
                    (id, Ok(platform_sent_email(message)))
                }
                Err(error) => {
                    if first_failure.is_none() {
                        first_failure = Some(error.clone());
                    }
                    (id, Err(error.to_string()))
                }
            })
            .collect();
        if any_success || first_failure.is_none() {
            self.record_success(mailbox.credential_id).await?;
        } else if let Some(error) = first_failure {
            self.record_failure(mailbox.credential_id, &error).await?;
        }
        Ok(results)
    }

    async fn received_email_details(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String> {
        if let Some(mailbox) = self.services.gmail_mailbox(mailbox_ref_id, EmailCapability::Inbox).await? {
            if provider_message_ids.len() > 100 { return Err("gmail_detail_batch_too_large".into()); }
            let mut results = Vec::with_capacity(provider_message_ids.len());
            for chunk in provider_message_ids.chunks(4) {
                let outcomes = futures::future::join_all(chunk.iter().map(|id| mailbox.platform.received(mailbox.token.expose_secret(), id, &mailbox.email))).await;
                results.extend(chunk.iter().cloned().zip(outcomes));
            }
            return Ok(results);
        }
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::Inbox)
            .await?;
        let outcomes = mailbox
            .sender
            .received_details(provider_message_ids)
            .await
            .map_err(|error| error.to_string())?;
        // Operation-scoped health: any success proves the credential works;
        // an all-failed batch records the first failure once.
        let mut first_failure = None;
        let mut any_success = false;
        let results = outcomes
            .into_iter()
            .map(|(id, outcome)| match outcome {
                Ok(message) => {
                    any_success = true;
                    (id, Ok(platform_inbound_email(message)))
                }
                Err(error) => {
                    if first_failure.is_none() {
                        first_failure = Some(error.clone());
                    }
                    (id, Err(error.to_string()))
                }
            })
            .collect();
        if any_success || first_failure.is_none() {
            self.record_success(mailbox.credential_id).await?;
        } else if let Some(error) = first_failure {
            self.record_failure(mailbox.credential_id, &error).await?;
        }
        Ok(results)
    }

    async fn received_attachment_access(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<
        onecrew_email_contracts::PlatformAttachmentAccess,
        onecrew_email_contracts::PlatformEmailAttachmentError,
    > {
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::Inbox)
            .await
            .map_err(failed_attachment)?;
        match mailbox
            .sender
            .received_attachment(provider_message_id, provider_attachment_id)
            .await
        {
            Ok(access) => {
                self.record_success(mailbox.credential_id)
                    .await
                    .map_err(failed_attachment)?;
                Ok(platform_attachment_access(access))
            }
            Err(error) => {
                self.record_failure(mailbox.credential_id, &error)
                    .await
                    .map_err(failed_attachment)?;
                Err(platform_attachment_error(&error))
            }
        }
    }

    async fn sent_attachments_access(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<
        Vec<onecrew_email_contracts::PlatformAttachmentAccess>,
        onecrew_email_contracts::PlatformEmailAttachmentError,
    > {
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::SentReadback)
            .await
            .map_err(failed_attachment)?;
        match mailbox.sender.sent_attachments(provider_message_id).await {
            Ok(accesses) => {
                self.record_success(mailbox.credential_id)
                    .await
                    .map_err(failed_attachment)?;
                Ok(accesses
                    .into_iter()
                    .map(platform_attachment_access)
                    .collect())
            }
            Err(error) => {
                self.record_failure(mailbox.credential_id, &error)
                    .await
                    .map_err(failed_attachment)?;
                Err(platform_attachment_error(&error))
            }
        }
    }

    async fn download_email_attachment(
        &self,
        mailbox_ref_id: &MailboxRefId,
        access: &onecrew_email_contracts::PlatformAttachmentAccess,
        limit: usize,
    ) -> Result<Vec<u8>, onecrew_email_contracts::PlatformEmailAttachmentError> {
        let mailbox = self
            .services
            .resend_mailbox(mailbox_ref_id.as_str(), EmailCapability::Inbox)
            .await
            .map_err(failed_attachment)?;
        let provider_access = onecrew_email_contracts::ProviderAttachmentAccess {
            metadata: onecrew_email_contracts::ProviderAttachmentMetadata {
                provider_attachment_id: access.metadata.provider_attachment_id.clone(),
                filename: access.metadata.filename.clone(),
                content_type: access.metadata.content_type.clone(),
                content_id: access.metadata.content_id.clone(),
                content_disposition: access.metadata.content_disposition.clone(),
                size: access.metadata.size_bytes,
            },
            download_url: access.download_url.clone(),
            expires_at: access.expires_at.clone(),
        };
        match mailbox
            .sender
            .download_attachment(&provider_access, limit)
            .await
        {
            Ok(bytes) => {
                self.record_success(mailbox.credential_id)
                    .await
                    .map_err(failed_attachment)?;
                Ok(bytes)
            }
            Err(error) => {
                self.record_failure(mailbox.credential_id, &error)
                    .await
                    .map_err(failed_attachment)?;
                Err(platform_attachment_error(&error))
            }
        }
    }
}
