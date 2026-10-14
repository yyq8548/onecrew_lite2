//! Provider-neutral Email contracts shared by the Creator owner and concrete providers.

mod batch;
mod contracts;
mod error;
mod models;
mod platform;

pub use batch::{
    EmailBatchChunkResult, EmailBatchItem, EmailBatchItemOutcome, EmailBatchItemResult,
    EmailBatchRequest, EmailBatchResult, EmailBatchTemplate, EmailBatchValidation,
};
pub use contracts::{
    EmailAdministrationProvider, EmailAttachmentProvider, EmailHistoryProvider,
    EmailIdentityProvider, EmailProvider, EmailSendProvider,
};
pub use error::{EmailError, EmailSendFailureKind, ProviderErrorClass, ProviderFailure};
pub use models::{
    DOCUMENTED_SENT_EVENTS, EmailCapability, EmailReceipt, OutboundEmail, ProviderAttachmentAccess,
    ProviderAttachmentMetadata, ProviderEmailDomain, ProviderInboundEmail,
    ProviderInboundEmailPage, ProviderInboundEmailSummary, ProviderRateLimitStatus,
    ProviderSentEmail, ProviderSentEmailPage, ProviderSentEmailSummary, SentDelivery,
    is_documented_sent_event, sent_delivery,
};
pub use platform::{
    PlatformAttachmentAccess, PlatformEmailAttachment, PlatformEmailAttachmentError,
    PlatformEmailMessage, PlatformEmailPage, PlatformEmailReceipt, PlatformEmailRequest,
    PlatformEmailSendError, PlatformEmailSummary, PlatformPageBounds, PlatformSentEmailPage,
    PlatformSentEmailSummary,
};
