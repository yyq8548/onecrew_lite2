//! Typed effect state for a failed email send.
//!
//! Every email send failure answers one question before anything else: did the
//! message this call is responsible for reach the provider? That fact was only
//! ever stated in prose ("no provider send occurred"), so adapters could not
//! branch on it and reported every failure as an unknown outcome. It is a
//! typed contract here instead.

use std::fmt;

/// Whether the email this send call governs reached the provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmailSendEffect {
    /// No provider send occurred and this call started none. The failure is a
    /// rejection, a validation refusal, or a local step that runs before the
    /// provider is ever asked.
    NotSent,
    /// The provider accepted the message with a stable message id. Only later
    /// readback or local durable work failed, so the mail is already out and
    /// must not be sent again.
    Sent,
    /// The provider may or may not have accepted the message. This is the only
    /// genuine unknown-effect state; a blind resend can duplicate the mail.
    Unknown,
}

/// A failed email send: its effect state plus the owner's own explanation.
///
/// In the submitted-owner projection the message stays owner-side prose: that
/// path classifies on [`EmailSendEffect`] and emits bounded fixed copy instead,
/// because this text may interpolate repository and provider causes. Older
/// direct Web operations retain their own established message projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmailSendFailure {
    effect: EmailSendEffect,
    message: String,
}

impl EmailSendFailure {
    /// No provider send occurred, and this call started none.
    pub fn not_sent(message: impl Into<String>) -> Self {
        Self::new(EmailSendEffect::NotSent, message)
    }

    /// The provider accepted the message; the failure is in later work.
    pub fn sent(message: impl Into<String>) -> Self {
        Self::new(EmailSendEffect::Sent, message)
    }

    /// The provider may already hold the message.
    pub fn unknown(message: impl Into<String>) -> Self {
        Self::new(EmailSendEffect::Unknown, message)
    }

    fn new(effect: EmailSendEffect, message: impl Into<String>) -> Self {
        Self {
            effect,
            message: message.into(),
        }
    }

    pub fn effect(&self) -> EmailSendEffect {
        self.effect
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Take the owner explanation for a caller that owns its own outward
    /// string shape, such as the Web command host.
    pub fn into_message(self) -> String {
        self.message
    }
}

impl fmt::Display for EmailSendFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EmailSendFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_constructor_keeps_its_effect_and_explanation() {
        for (failure, effect) in [
            (
                EmailSendFailure::not_sent("nothing left OneCrew"),
                EmailSendEffect::NotSent,
            ),
            (
                EmailSendFailure::sent("the provider accepted it"),
                EmailSendEffect::Sent,
            ),
            (
                EmailSendFailure::unknown("the provider may hold it"),
                EmailSendEffect::Unknown,
            ),
        ] {
            assert_eq!(failure.effect(), effect);
            assert_eq!(failure.to_string(), failure.message());
            assert!(!failure.message().is_empty());
        }
    }
}
