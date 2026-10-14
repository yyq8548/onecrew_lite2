//! Provider-neutral capability facets used by the Email toolkit.
//!
//! Concrete providers implement only the facets they advertise. Creator owns the
//! business workflow and persistence; these traits carry provider operations
//! and never expose provider transport, credential, or wire types.

use async_trait::async_trait;

use crate::{
    EmailBatchRequest, EmailBatchResult, EmailCapability, EmailError, EmailReceipt, OutboundEmail,
    ProviderAttachmentAccess, ProviderEmailDomain, ProviderInboundEmail, ProviderInboundEmailPage,
    ProviderSentEmail, ProviderSentEmailPage,
};

/// Identity and capability projection for one provider connection.
pub trait EmailIdentityProvider: Send + Sync {
    fn capabilities(&self) -> Vec<EmailCapability>;
}

/// Single-send and optional native batch execution.
#[async_trait]
pub trait EmailSendProvider: Send + Sync {
    async fn send(&self, message: &OutboundEmail) -> Result<EmailReceipt, EmailError>;

    /// Native batch is optional. Providers that do not implement it must
    /// return `UnsupportedCapability` rather than silently fan out.
    async fn send_many(
        &self,
        _request: &EmailBatchRequest,
    ) -> Result<EmailBatchResult, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "native batch sending is unavailable".into(),
        ))
    }
}

/// History, detail, and incremental provider reads.
#[async_trait]
pub trait EmailHistoryProvider: Send + Sync {
    async fn sent(&self, _provider_message_id: &str) -> Result<ProviderSentEmail, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "sent message readback is unavailable".into(),
        ))
    }
    async fn sent_page(
        &self,
        _after: Option<&str>,
        _limit: u16,
    ) -> Result<ProviderSentEmailPage, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "sent message listing is unavailable".into(),
        ))
    }
    async fn received(
        &self,
        _after: Option<&str>,
        _limit: u16,
    ) -> Result<ProviderInboundEmailPage, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "inbox listing is unavailable".into(),
        ))
    }
    async fn received_message(
        &self,
        _provider_message_id: &str,
    ) -> Result<ProviderInboundEmail, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "received message readback is unavailable".into(),
        ))
    }
    async fn received_details(
        &self,
        provider_message_ids: &[String],
    ) -> Vec<(String, Result<ProviderInboundEmail, EmailError>)> {
        provider_message_ids
            .iter()
            .cloned()
            .map(|id| {
                (
                    id,
                    Err(EmailError::UnsupportedCapability(
                        "received message readback is unavailable".into(),
                    )),
                )
            })
            .collect()
    }
    async fn sent_details(
        &self,
        provider_message_ids: &[String],
    ) -> Vec<(String, Result<ProviderSentEmail, EmailError>)> {
        provider_message_ids
            .iter()
            .cloned()
            .map(|id| {
                (
                    id,
                    Err(EmailError::UnsupportedCapability(
                        "sent message readback is unavailable".into(),
                    )),
                )
            })
            .collect()
    }
}

/// On-demand attachment capabilities. Access URLs remain short-lived values.
#[async_trait]
pub trait EmailAttachmentProvider: Send + Sync {
    async fn sent_attachments(
        &self,
        _provider_message_id: &str,
    ) -> Result<Vec<ProviderAttachmentAccess>, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "sent attachment access is unavailable".into(),
        ))
    }
    async fn received_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<ProviderAttachmentAccess, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "received attachment access is unavailable".into(),
        ))
    }
    async fn download(&self, _url: &str, _limit: usize) -> Result<Vec<u8>, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "attachment download is unavailable".into(),
        ))
    }
}

/// Provider administration capabilities such as verified sending domains.
#[async_trait]
pub trait EmailAdministrationProvider: Send + Sync {
    async fn domains(&self) -> Result<Vec<ProviderEmailDomain>, EmailError> {
        Err(EmailError::UnsupportedCapability(
            "domain administration is unavailable".into(),
        ))
    }
}

/// A concrete provider is composed from explicit capability facets.
pub trait EmailProvider:
    EmailIdentityProvider
    + EmailSendProvider
    + EmailHistoryProvider
    + EmailAttachmentProvider
    + EmailAdministrationProvider
{
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SendOnlyProvider;

    impl EmailIdentityProvider for SendOnlyProvider {
        fn capabilities(&self) -> Vec<EmailCapability> {
            vec![EmailCapability::Send]
        }
    }

    #[async_trait]
    impl EmailSendProvider for SendOnlyProvider {
        async fn send(&self, _message: &OutboundEmail) -> Result<EmailReceipt, EmailError> {
            Ok(EmailReceipt {
                provider: "test".into(),
                message_id: None,
                accepted: true,
            })
        }
    }

    // The empty facet impls exercise the fail-closed defaults: a future
    // send-only provider need not write unrelated history, attachment, or
    // administration methods merely to compose an EmailProvider.
    impl EmailHistoryProvider for SendOnlyProvider {}
    impl EmailAttachmentProvider for SendOnlyProvider {}
    impl EmailAdministrationProvider for SendOnlyProvider {}
    impl EmailProvider for SendOnlyProvider {}

    #[test]
    fn optional_facets_can_be_composed_without_unrelated_method_bodies() {
        fn assert_provider<T: EmailProvider>() {}
        assert_provider::<SendOnlyProvider>();
    }
}
