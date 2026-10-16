//! Compile-time Email toolkit facade.
//!
//! Provider-neutral contracts live in `onecrew-email-contracts`; the
//! Gmail implementation lives in `onecrew-gmail-platform`.

pub use onecrew_email_contracts::*;
pub use onecrew_gmail_platform::GmailPlatform;

#[cfg(test)]
mod tests {
    use super::OutboundEmail;

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

}
