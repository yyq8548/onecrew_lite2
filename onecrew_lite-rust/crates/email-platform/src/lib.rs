//! Compile-time Email toolkit facade.
//!
//! Provider-neutral contracts live in `onecrew-email-contracts`; the
//! current local Resend implementation lives in `onecrew-resend-platform`.
//! This crate only composes and re-exports those owners for Local adapters.

mod sender;

pub use onecrew_email_contracts::*;
pub use onecrew_gmail_platform::GmailPlatform;
pub use onecrew_resend_platform::{ResendAdmission, ResendPlatform};
pub use sender::{EmailSender, ResendApiKey, ResendProviderFactory};

#[cfg(test)]
mod tests {
    use super::{
        EmailAdministrationProvider, EmailAttachmentProvider, EmailHistoryProvider,
        EmailIdentityProvider, EmailProvider, EmailSendProvider, EmailSender, OutboundEmail,
    };

    #[test]
    fn facade_keeps_the_provider_composition_explicit() {
        fn assert_provider<T>()
        where
            T: EmailProvider
                + EmailIdentityProvider
                + EmailSendProvider
                + EmailHistoryProvider
                + EmailAttachmentProvider
                + EmailAdministrationProvider,
        {
        }

        assert_provider::<super::ResendPlatform>();
        // Names the constructor so its removal fails this test.
        let _constructor = EmailSender::from_provider;
    }

    #[test]
    fn outbound_email_requires_a_body() {
        let message = OutboundEmail {
            from_name: None,
            from_email: "sender@example.com".into(),
            to: vec!["recipient@example.com".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            idempotency_key: "test-email".into(),
            subject: "Hello".into(),
            text: None,
            html: None,
        };
        assert!(message.validate().is_err());
    }

    #[tokio::test]
    async fn facade_rejects_provider_resource_ids_before_dispatch() {
        let transport = std::sync::Arc::new(super::ResendAdmission::new().unwrap());
        let sender = EmailSender::from_resend_api_key("re_test".into(), transport).unwrap();
        assert!(sender.sent("../receiving").await.is_err());
        assert!(sender.received_message("").await.is_err());
        assert!(sender.received(Some("page?x=1"), 10).await.is_err());
        assert!(
            sender
                .received_attachment("ok-id", "../../emails")
                .await
                .is_err()
        );
        assert!(
            sender
                .received_details(&["good-id".into(), "bad/../id".into()])
                .await
                .is_err()
        );
    }
}
