//! On-demand attachment access: durable metadata stays canonical in the
//! repository; content access mints a fresh short-lived provider capability
//! every time and consumes it through the bounded transport. No signed URL
//! is ever persisted, logged, audited, or projected, and no long-term binary
//! archive exists. Cancellation is a dropped future: no partial local write
//! can occur because nothing is written locally.

use crate::{
    EmailAttachmentContent, EmailAttachmentReadError, EmailAttachmentReadInput,
    EmailAttachmentView, EmailDirection, EmailId, MAX_EMAIL_ATTACHMENT_BYTES,
};
use onecrew_email_contracts::PlatformEmailAttachmentError;

impl crate::CreatorRuntime {
    /// Durable attachment metadata for one email, straight from the
    /// repository. No provider call happens here.
    pub fn list_email_attachments(
        &self,
        email_id: &EmailId,
    ) -> Result<Vec<EmailAttachmentView>, String> {
        self.repository.email_attachments(email_id)
    }

    /// Resolves the authoritative email, asks the provider for a fresh
    /// access capability, and downloads the content bounded in memory.
    /// Retention expiry marks durable availability evidence and returns a
    /// typed `Unavailable`; the email and its metadata remain local.
    pub async fn read_email_attachment(
        &self,
        input: EmailAttachmentReadInput,
    ) -> Result<EmailAttachmentContent, EmailAttachmentReadError> {
        let gateway =
            self.gateways
                .email
                .as_ref()
                .ok_or_else(|| EmailAttachmentReadError::Failed {
                    message: "email gateway is not configured".into(),
                })?;
        let (mailbox_ref_id, provider_message_id, direction) = self
            .repository
            .email_provider_coordinates(&input.email_id)
            .map_err(|message| EmailAttachmentReadError::UnknownEmail { message })?;

        let access = match direction {
            EmailDirection::Inbound => {
                match gateway
                    .received_attachment_access(
                        &mailbox_ref_id,
                        &provider_message_id,
                        &input.provider_attachment_id,
                    )
                    .await
                {
                    Ok(access) => access,
                    Err(error) => {
                        return Err(self.attachment_provider_error(&input, error));
                    }
                }
            }
            EmailDirection::Outbound => {
                // Sent detail embeds no attachment metadata on the wire; the
                // listing both serves access and backfills durable metadata.
                let accesses = match gateway
                    .sent_attachments_access(&mailbox_ref_id, &provider_message_id)
                    .await
                {
                    Ok(accesses) => accesses,
                    Err(error) => {
                        return Err(self.attachment_provider_error(&input, error));
                    }
                };
                let metadata: Vec<onecrew_email_contracts::PlatformEmailAttachment> = accesses
                    .iter()
                    .map(|access| access.metadata.clone())
                    .collect();
                self.repository
                    .record_email_attachments(&input.email_id, direction, &metadata)
                    .map_err(|message| EmailAttachmentReadError::Failed { message })?;
                match accesses.into_iter().find(|access| {
                    access.metadata.provider_attachment_id == input.provider_attachment_id
                }) {
                    Some(access) => access,
                    None => {
                        return Err(self.attachment_provider_error(
                            &input,
                            PlatformEmailAttachmentError::Unavailable {
                                message: "the provider no longer lists this attachment".into(),
                            },
                        ));
                    }
                }
            }
        };

        let bytes = gateway
            .download_email_attachment(&mailbox_ref_id, &access, MAX_EMAIL_ATTACHMENT_BYTES)
            .await
            .map_err(|error| self.attachment_provider_error(&input, error))?;

        let metadata = self
            .repository
            .email_attachments(&input.email_id)
            .map_err(|message| EmailAttachmentReadError::Failed { message })?
            .into_iter()
            .find(|view| view.provider_attachment_id == input.provider_attachment_id)
            .unwrap_or(EmailAttachmentView {
                provider_attachment_id: access.metadata.provider_attachment_id.clone(),
                filename: access.metadata.filename.clone(),
                content_type: access.metadata.content_type.clone(),
                content_id: access.metadata.content_id.clone(),
                content_disposition: access.metadata.content_disposition.clone(),
                size_bytes: access.metadata.size_bytes,
                direction,
                available: true,
            });
        Ok(EmailAttachmentContent { metadata, bytes })
    }

    /// Converts a provider outcome into the typed read error, recording
    /// durable unavailability evidence when the provider no longer holds the
    /// content.
    fn attachment_provider_error(
        &self,
        input: &EmailAttachmentReadInput,
        error: PlatformEmailAttachmentError,
    ) -> EmailAttachmentReadError {
        if matches!(error, PlatformEmailAttachmentError::Unavailable { .. }) {
            // Best effort: the read outcome stays typed even if the evidence
            // write fails.
            if let Err(record) = self
                .repository
                .mark_email_attachment_unavailable(&input.email_id, &input.provider_attachment_id)
            {
                log::warn!("recording an unavailable email attachment failed: {record}");
            }
        }
        error.into()
    }
}
