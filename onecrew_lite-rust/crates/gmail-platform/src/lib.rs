//! Gmail send, readback and read-only Inbox facade. OAuth remains Access-owned.
mod client;
mod mime;
mod transport;

use onecrew_email_contracts::{
    PlatformEmailMessage, PlatformEmailReceipt, PlatformEmailRequest, PlatformEmailSendError,
};

pub struct GmailPlatform(client::GmailClient);

impl GmailPlatform {
    pub async fn outreach_reply_ids(&self, token: &str, sent_id: &str) -> Result<Vec<String>, String> {
        self.0.outreach_reply_ids(token, sent_id).await
    }

    pub async fn inbox(
        &self,
        token: &str,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<onecrew_email_contracts::PlatformEmailPage, String> {
        use onecrew_email_contracts::{PlatformEmailPage, PlatformEmailSummary};
        let page = self.0.inbox(token, cursor, limit).await?;
        let mut messages = Vec::with_capacity(page.messages.len());
        for chunk in page.messages.chunks(4) {
            let times = futures::future::try_join_all(
                chunk
                    .iter()
                    .map(|message| self.0.message_time(token, &message.id)),
            )
            .await?;
            for (message, created_at) in chunk.iter().zip(times) {
                messages.push(PlatformEmailSummary {
                    provider_message_id: message.id.clone(),
                    created_at,
                });
            }
        }
        Ok(PlatformEmailPage {
            messages,
            has_more: page.next_page_token.is_some(),
            next_cursor: page.next_page_token,
            // Gmail has no documented chronological page bounds or 30-day
            // retention floor. Do not show a fabricated backfill percentage.
            bounds: None,
        })
    }
    pub async fn received(
        &self,
        token: &str,
        id: &str,
        recipient: &str,
    ) -> Result<PlatformEmailMessage, String> {
        mime::decode_inbox(self.0.sent(token, id).await?, id, recipient)
    }
    pub fn new() -> Result<Self, String> {
        Ok(Self(client::GmailClient::new()?))
    }

    /// One attempt only: transport ambiguity is returned to the durable send owner.
    pub async fn send(
        &self,
        token: &str,
        from: &str,
        request: &PlatformEmailRequest,
    ) -> Result<PlatformEmailReceipt, PlatformEmailSendError> {
        let raw = mime::encode(from, request)
            .map_err(|message| PlatformEmailSendError::Validation { message })?;
        let id = self.0.send(token, raw).await?;
        Ok(PlatformEmailReceipt {
            provider: "gmail".into(),
            external_message_id: Some(id),
            accepted: true,
        })
    }

    pub async fn sent(&self, token: &str, id: &str) -> Result<PlatformEmailMessage, String> {
        let message = self.0.sent(token, id).await?;
        mime::decode(message, id)
    }
}
