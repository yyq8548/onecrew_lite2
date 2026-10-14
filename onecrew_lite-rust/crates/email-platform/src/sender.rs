use std::sync::Arc;

use onecrew_email_contracts::{
    EmailBatchRequest, EmailBatchResult, EmailBatchValidation, EmailCapability, EmailError,
    EmailProvider, EmailReceipt, OutboundEmail, ProviderAttachmentAccess, ProviderEmailDomain,
    ProviderInboundEmail, ProviderInboundEmailPage, ProviderSentEmail, ProviderSentEmailPage,
};
use onecrew_resend_platform::{ResendAdmission, ResendPlatform};

/// Secret-bearing Resend input accepted only by the compile-time facade
/// factory. It intentionally has no Debug/Serialize implementation.
pub struct ResendApiKey(String);

impl ResendApiKey {
    pub fn new(value: impl Into<String>) -> Result<Self, EmailError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(EmailError::InvalidConfig(
                "Resend API key is required".into(),
            ));
        }
        Ok(Self(value))
    }
}

/// Shipped provider factory. Adding a provider is an explicit, typed API
/// change; callers cannot select one by an arbitrary string.
#[derive(Clone)]
pub struct ResendProviderFactory {
    transport: Arc<ResendAdmission>,
}

impl ResendProviderFactory {
    pub fn new(transport: Arc<ResendAdmission>) -> Self {
        Self { transport }
    }

    pub fn build(&self, api_key: ResendApiKey) -> Result<EmailSender, EmailError> {
        Ok(EmailSender::from_provider(Arc::new(ResendPlatform::new(
            api_key.0,
            Arc::clone(&self.transport),
        )?)))
    }
}

/// Compile-time facade over one provider-neutral capability object.
#[derive(Clone)]
pub struct EmailSender {
    provider: Arc<dyn EmailProvider>,
}

impl EmailSender {
    /// Compose a sender from a provider-neutral capability object. Generic
    /// callers do not depend on Resend configuration or transport types.
    pub fn from_provider(provider: Arc<dyn EmailProvider>) -> Self {
        Self { provider }
    }

    /// Local adapter factory for the shipped Resend platform owner.
    pub fn from_resend_api_key(
        api_key: String,
        transport: Arc<ResendAdmission>,
    ) -> Result<Self, EmailError> {
        ResendProviderFactory::new(transport).build(ResendApiKey::new(api_key)?)
    }

    pub async fn send(&self, message: &OutboundEmail) -> Result<EmailReceipt, EmailError> {
        message.validate()?;
        self.provider.send(message).await
    }

    pub async fn send_many(
        &self,
        request: &EmailBatchRequest,
    ) -> Result<EmailBatchResult, EmailError> {
        request.validate()?;
        // Strict mode is atomic: no provider call may occur when any item is
        // invalid. Permissive mode intentionally leaves invalid-item
        // projection to the provider-neutral result contract.
        if request.validation == EmailBatchValidation::Strict {
            for item in &request.items {
                item.validate()?;
            }
        }
        self.provider.send_many(request).await
    }

    pub fn capabilities(&self) -> Vec<EmailCapability> {
        self.provider.capabilities()
    }

    pub async fn sent(&self, provider_message_id: &str) -> Result<ProviderSentEmail, EmailError> {
        validate_provider_id(provider_message_id)?;
        self.provider.sent(provider_message_id).await
    }

    pub async fn sent_page(
        &self,
        after: Option<&str>,
        limit: u16,
    ) -> Result<ProviderSentEmailPage, EmailError> {
        if let Some(after) = after.filter(|value| !value.trim().is_empty()) {
            validate_provider_id(after)?;
        }
        self.provider.sent_page(after, limit).await
    }

    pub async fn received(
        &self,
        after: Option<&str>,
        limit: u16,
    ) -> Result<ProviderInboundEmailPage, EmailError> {
        if let Some(after) = after.filter(|value| !value.trim().is_empty()) {
            validate_provider_id(after)?;
        }
        self.provider.received(after, limit).await
    }

    pub async fn received_message(
        &self,
        provider_message_id: &str,
    ) -> Result<ProviderInboundEmail, EmailError> {
        validate_provider_id(provider_message_id)?;
        self.provider.received_message(provider_message_id).await
    }

    pub async fn received_details(
        &self,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<ProviderInboundEmail, EmailError>)>, EmailError> {
        for id in provider_message_ids {
            validate_provider_id(id)?;
        }
        Ok(self.provider.received_details(provider_message_ids).await)
    }

    pub async fn sent_details(
        &self,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<ProviderSentEmail, EmailError>)>, EmailError> {
        for id in provider_message_ids {
            validate_provider_id(id)?;
        }
        Ok(self.provider.sent_details(provider_message_ids).await)
    }

    pub async fn sent_attachments(
        &self,
        provider_message_id: &str,
    ) -> Result<Vec<ProviderAttachmentAccess>, EmailError> {
        validate_provider_id(provider_message_id)?;
        self.provider.sent_attachments(provider_message_id).await
    }

    pub async fn received_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<ProviderAttachmentAccess, EmailError> {
        validate_provider_id(provider_message_id)?;
        validate_provider_id(provider_attachment_id)?;
        self.provider
            .received_attachment(provider_message_id, provider_attachment_id)
            .await
    }

    pub async fn download_attachment(
        &self,
        access: &ProviderAttachmentAccess,
        limit: usize,
    ) -> Result<Vec<u8>, EmailError> {
        self.provider.download(&access.download_url, limit).await
    }

    pub async fn domains(&self) -> Result<Vec<ProviderEmailDomain>, EmailError> {
        self.provider.domains().await
    }
}

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
