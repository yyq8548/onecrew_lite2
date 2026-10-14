//! Resend adapter for the provider-neutral Email contracts.
//!
//! [`ResendPlatform`] implements the `onecrew-email-contracts` identity, send,
//! history, attachment, and administration traits over one shared Resend
//! transport. Single and batch sends go through `onecrew-resend-client`
//! (`resend-rs`); sent and received reads, attachments, signed downloads, and
//! domains use this crate's private read-compatibility decoder and the typed
//! [`wire`] contract. Provider resource IDs and cursors are validated before
//! they enter a request path or query. This crate owns no Creator policy or
//! persistence.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::JoinSet;

use crate::read_compat::ResendReadCompat;
use crate::wire::{
    WireDomain, WireInboundEmail, WireInboundEmailSummary, WireList, WireSentEmail,
    WireSentEmailSummary,
};
use onecrew_email_contracts::{
    EmailAdministrationProvider, EmailAttachmentProvider, EmailBatchRequest, EmailBatchResult,
    EmailCapability, EmailError, EmailHistoryProvider, EmailIdentityProvider, EmailProvider,
    EmailReceipt, EmailSendProvider, OutboundEmail, ProviderAttachmentAccess, ProviderEmailDomain,
    ProviderInboundEmail, ProviderInboundEmailPage, ProviderSentEmail, ProviderSentEmailPage,
};
use onecrew_resend_client::ResendClient;
use onecrew_resend_transport::{ResendOpPriority, ResendTransport};
mod read_compat;
pub mod wire;

pub use onecrew_resend_transport::ResendTransport as ResendAdmission;
pub use onecrew_resend_transport::SharedResendTransport;
pub use wire::WireErrorClass;

/// How many detail fetches the orchestrator keeps queued at once. The shared
/// quota bounds actual in-flight HTTP below this; the window additionally
/// bounds buffered response bodies.
const DETAIL_WINDOW: usize = 8;

pub struct ResendPlatform {
    api_key: String,
    client: ResendClient,
    read_compat: ResendReadCompat,
}

impl ResendPlatform {
    pub fn new(api_key: String, transport: Arc<ResendTransport>) -> Result<Self, EmailError> {
        if api_key.trim().is_empty() {
            return Err(EmailError::InvalidConfig(
                "Resend API key is required".into(),
            ));
        }
        let client = ResendClient::new(api_key.clone(), Arc::clone(&transport))?;
        let read_compat = ResendReadCompat::new(Arc::clone(&transport));
        Ok(Self {
            api_key,
            client,
            read_compat,
        })
    }

    fn list_query(after: Option<&str>, limit: u16) -> Vec<(&'static str, String)> {
        let mut query = vec![("limit", limit.clamp(1, 100).to_string())];
        if let Some(after) = after.map(str::trim).filter(|value| !value.is_empty()) {
            query.push(("after", after.to_owned()));
        }
        query
    }
}

impl EmailIdentityProvider for ResendPlatform {
    fn capabilities(&self) -> Vec<EmailCapability> {
        vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
            EmailCapability::Reply,
        ]
    }
}

#[async_trait]
impl EmailSendProvider for ResendPlatform {
    async fn send(&self, message: &OutboundEmail) -> Result<EmailReceipt, EmailError> {
        self.client.send(message).await
    }

    async fn send_many(&self, request: &EmailBatchRequest) -> Result<EmailBatchResult, EmailError> {
        self.client.send_many(request).await
    }
}

#[async_trait]
impl EmailHistoryProvider for ResendPlatform {
    async fn sent(&self, provider_message_id: &str) -> Result<ProviderSentEmail, EmailError> {
        validate_provider_id(provider_message_id)?;
        let detail: WireSentEmail = self
            .read_compat
            .get_json(
                &self.api_key,
                ResendOpPriority::User,
                &format!("/emails/{provider_message_id}"),
                &[],
            )
            .await?;
        Ok(detail.into())
    }

    async fn sent_page(
        &self,
        after: Option<&str>,
        limit: u16,
    ) -> Result<ProviderSentEmailPage, EmailError> {
        validate_after_cursor(after)?;
        let page: WireList<WireSentEmailSummary> = self
            .read_compat
            .get_json(
                &self.api_key,
                ResendOpPriority::Background,
                "/emails",
                &Self::list_query(after, limit),
            )
            .await?;
        Ok(page.into())
    }

    async fn received(
        &self,
        after: Option<&str>,
        limit: u16,
    ) -> Result<ProviderInboundEmailPage, EmailError> {
        validate_after_cursor(after)?;
        let page: WireList<WireInboundEmailSummary> = self
            .read_compat
            .get_json(
                &self.api_key,
                ResendOpPriority::Background,
                "/emails/receiving",
                &Self::list_query(after, limit),
            )
            .await?;
        Ok(page.into())
    }

    async fn received_message(
        &self,
        provider_message_id: &str,
    ) -> Result<ProviderInboundEmail, EmailError> {
        validate_provider_id(provider_message_id)?;
        let detail: WireInboundEmail = self
            .read_compat
            .get_json(
                &self.api_key,
                ResendOpPriority::Background,
                &format!("/emails/receiving/{provider_message_id}"),
                &[],
            )
            .await?;
        Ok(detail.into())
    }

    async fn received_details(
        &self,
        provider_message_ids: &[String],
    ) -> Vec<(String, Result<ProviderInboundEmail, EmailError>)> {
        let mut results: Vec<Option<(String, Result<ProviderInboundEmail, EmailError>)>> =
            provider_message_ids.iter().map(|_| None).collect();
        let mut tasks: JoinSet<(usize, String, Result<ProviderInboundEmail, EmailError>)> =
            JoinSet::new();
        let mut next = 0;

        while next < provider_message_ids.len() || !tasks.is_empty() {
            while next < provider_message_ids.len() && tasks.len() < DETAIL_WINDOW {
                let index = next;
                let id = provider_message_ids[index].clone();
                next += 1;
                if let Err(error) = validate_provider_id(&id) {
                    results[index] = Some((id, Err(error)));
                    continue;
                }
                let read_compat = self.read_compat.clone();
                let api_key = self.api_key.clone();
                tasks.spawn(async move {
                    let outcome: Result<WireInboundEmail, EmailError> = read_compat
                        .get_json(
                            &api_key,
                            ResendOpPriority::Background,
                            &format!("/emails/receiving/{id}"),
                            &[],
                        )
                        .await;
                    (index, id, outcome.map(Into::into))
                });
            }
            if let Some(joined) = tasks.join_next().await {
                let (index, id, outcome) = joined.expect("received detail task panicked");
                results[index] = Some((id, outcome));
            }
        }

        results
            .into_iter()
            .map(|slot| slot.expect("every requested detail produces one outcome"))
            .collect()
    }

    async fn sent_details(
        &self,
        provider_message_ids: &[String],
    ) -> Vec<(String, Result<ProviderSentEmail, EmailError>)> {
        let mut results: Vec<Option<(String, Result<ProviderSentEmail, EmailError>)>> =
            provider_message_ids.iter().map(|_| None).collect();
        let mut tasks: JoinSet<(usize, String, Result<ProviderSentEmail, EmailError>)> =
            JoinSet::new();
        let mut next = 0;

        while next < provider_message_ids.len() || !tasks.is_empty() {
            while next < provider_message_ids.len() && tasks.len() < DETAIL_WINDOW {
                let index = next;
                let id = provider_message_ids[index].clone();
                next += 1;
                if let Err(error) = validate_provider_id(&id) {
                    results[index] = Some((id, Err(error)));
                    continue;
                }
                let read_compat = self.read_compat.clone();
                let api_key = self.api_key.clone();
                tasks.spawn(async move {
                    let outcome: Result<WireSentEmail, EmailError> = read_compat
                        .get_json(
                            &api_key,
                            ResendOpPriority::Background,
                            &format!("/emails/{id}"),
                            &[],
                        )
                        .await;
                    (index, id, outcome.map(Into::into))
                });
            }
            if let Some(joined) = tasks.join_next().await {
                let (index, id, outcome) = joined.expect("sent detail task panicked");
                results[index] = Some((id, outcome));
            }
        }

        results
            .into_iter()
            .map(|slot| slot.expect("every requested detail produces one outcome"))
            .collect()
    }
}

#[async_trait]
impl EmailAttachmentProvider for ResendPlatform {
    async fn sent_attachments(
        &self,
        provider_message_id: &str,
    ) -> Result<Vec<ProviderAttachmentAccess>, EmailError> {
        validate_provider_id(provider_message_id)?;
        let page: WireList<crate::wire::WireAttachment> = self
            .read_compat
            .get_json(
                &self.api_key,
                ResendOpPriority::User,
                &format!("/emails/{provider_message_id}/attachments"),
                &[],
            )
            .await?;
        Ok(page.data.into_iter().map(Into::into).collect())
    }

    async fn received_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<ProviderAttachmentAccess, EmailError> {
        validate_provider_id(provider_message_id)?;
        validate_provider_id(provider_attachment_id)?;
        let attachment: crate::wire::WireAttachment = self
            .read_compat
            .get_json(
                &self.api_key,
                ResendOpPriority::User,
                &format!(
                    "/emails/receiving/{provider_message_id}/attachments/{provider_attachment_id}"
                ),
                &[],
            )
            .await?;
        Ok(attachment.into())
    }

    async fn download(&self, url: &str, limit: usize) -> Result<Vec<u8>, EmailError> {
        self.read_compat.download_signed(url, limit).await
    }
}

#[async_trait]
impl EmailAdministrationProvider for ResendPlatform {
    async fn domains(&self) -> Result<Vec<ProviderEmailDomain>, EmailError> {
        let page: WireList<WireDomain> = self
            .read_compat
            .get_json(&self.api_key, ResendOpPriority::User, "/domains", &[])
            .await?;
        Ok(page.data.into_iter().map(Into::into).collect())
    }
}

impl EmailProvider for ResendPlatform {}

/// Provider resource ids (email ids, attachment ids, and cursors) participate
/// in request paths or queries. Keep the concrete provider boundary safe even
/// when callers bypass the higher-level EmailSender facade.
fn validate_provider_id(value: &str) -> Result<(), EmailError> {
    if value.trim().is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(EmailError::InvalidMessage(
            "provider resource id is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_after_cursor(after: Option<&str>) -> Result<(), EmailError> {
    if let Some(after) = after.filter(|value| !value.trim().is_empty()) {
        validate_provider_id(after)?;
    }
    Ok(())
}
