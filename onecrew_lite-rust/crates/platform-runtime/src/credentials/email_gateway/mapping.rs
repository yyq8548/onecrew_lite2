use onecrew_email_contracts::{
    EmailError, EmailReceipt, EmailSendFailureKind, ProviderInboundEmail, ProviderInboundEmailPage,
    ProviderSentEmail, ProviderSentEmailPage,
};
use onecrew_email_contracts::{
    PlatformEmailMessage, PlatformEmailPage, PlatformEmailReceipt, PlatformEmailSendError,
    PlatformEmailSummary, PlatformSentEmailPage, PlatformSentEmailSummary,
};

pub(super) fn normalize_email_address(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    let Some((local, domain)) = value.split_once('@') else {
        return Err("Resend sender email_address is invalid".into());
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
        || value.chars().any(char::is_whitespace)
    {
        return Err("Resend sender email_address is invalid".into());
    }
    Ok(value)
}

pub(super) fn platform_email_page(
    value: ProviderInboundEmailPage,
    mailbox_email_address: &str,
) -> PlatformEmailPage {
    // Raw boundary before filtering: the engine's watermark must move even
    // when this mailbox matches nothing on the page.
    let bounds = value
        .messages
        .first()
        .zip(value.messages.last())
        .map(
            |(newest, oldest)| onecrew_email_contracts::PlatformPageBounds {
                newest_provider_message_id: newest.provider_message_id.clone(),
                newest_created_at: newest.created_at.clone(),
                oldest_created_at: oldest.created_at.clone(),
            },
        );
    PlatformEmailPage {
        messages: value
            .messages
            .into_iter()
            .filter(|message| inbound_summary_is_for_mailbox(message, mailbox_email_address))
            .map(|message| PlatformEmailSummary {
                provider_message_id: message.provider_message_id,
                created_at: message.created_at,
            })
            .collect(),
        has_more: value.has_more,
        next_cursor: value.next_cursor,
        bounds,
    }
}

fn inbound_summary_is_for_mailbox(
    message: &onecrew_email_contracts::ProviderInboundEmailSummary,
    mailbox_email_address: &str,
) -> bool {
    message
        .to
        .iter()
        .chain(message.cc.iter())
        .chain(message.bcc.iter())
        .chain(message.received_for.iter())
        .any(|address| same_recipient(address, mailbox_email_address))
}

fn same_recipient(left: &str, right: &str) -> bool {
    canonical_recipient(left)
        .zip(canonical_recipient(right))
        .is_some_and(|(left, right)| left == right)
}

fn canonical_recipient(value: &str) -> Option<String> {
    let value = value.trim();
    let address = match (value.rfind('<'), value.rfind('>')) {
        (Some(start), Some(end)) if start < end => &value[start + 1..end],
        _ => value,
    };
    normalize_email_address(address).ok()
}

/// Team-scoped sent pagination filtered to the configured mailbox: another
/// sender identity sharing the Resend team is never imported merely because
/// it shares the key. The global cursor is preserved.
pub(super) fn platform_sent_page(
    value: ProviderSentEmailPage,
    mailbox_email_address: &str,
) -> PlatformSentEmailPage {
    let bounds = value
        .messages
        .first()
        .zip(value.messages.last())
        .map(
            |(newest, oldest)| onecrew_email_contracts::PlatformPageBounds {
                newest_provider_message_id: newest.provider_message_id.clone(),
                newest_created_at: newest.created_at.clone(),
                oldest_created_at: oldest.created_at.clone(),
            },
        );
    PlatformSentEmailPage {
        messages: value
            .messages
            .into_iter()
            .filter(|message| same_recipient(&message.from, mailbox_email_address))
            .map(|message| PlatformSentEmailSummary {
                provider_message_id: message.provider_message_id,
                internet_message_id: message.internet_message_id,
                from: message.from,
                to: message.to,
                cc: message.cc,
                bcc: message.bcc,
                reply_to: message.reply_to,
                subject: message.subject,
                last_event: message.last_event,
                created_at: message.created_at,
            })
            .collect(),
        has_more: value.has_more,
        next_cursor: value.next_cursor,
        bounds,
    }
}

pub(super) fn platform_sent_email(value: ProviderSentEmail) -> PlatformEmailMessage {
    PlatformEmailMessage {
        provider_message_id: value.provider_message_id,
        internet_message_id: value.internet_message_id,
        from: value.from,
        to: value.to,
        cc: value.cc,
        bcc: value.bcc,
        received_for: Vec::new(),
        reply_to: value.reply_to,
        subject: value.subject,
        text: value.text,
        html: value.html,
        headers: serde_json::json!({}),
        last_event: value.last_event,
        created_at: value.created_at,
        inbound: false,
        // Sent detail embeds no attachment metadata on the wire; Phase 6
        // fetches it through the sent-attachments endpoint when needed.
        attachments: Vec::new(),
    }
}

pub(super) fn platform_inbound_email(value: ProviderInboundEmail) -> PlatformEmailMessage {
    PlatformEmailMessage {
        provider_message_id: value.provider_message_id,
        internet_message_id: Some(value.internet_message_id),
        from: value.from,
        to: value.to,
        cc: value.cc,
        bcc: value.bcc,
        received_for: value.received_for,
        reply_to: value.reply_to,
        subject: value.subject,
        text: value.text,
        html: value.html,
        headers: value.headers,
        last_event: "received".into(),
        created_at: value.created_at,
        inbound: true,
        attachments: value
            .attachments
            .into_iter()
            .map(
                |attachment| onecrew_email_contracts::PlatformEmailAttachment {
                    provider_attachment_id: attachment.provider_attachment_id,
                    filename: attachment.filename,
                    content_type: attachment.content_type,
                    content_id: attachment.content_id,
                    content_disposition: attachment.content_disposition,
                    size_bytes: attachment.size,
                },
            )
            .collect(),
    }
}

/// Maps a provider failure into the typed attachment outcome the port
/// promises. Retention expiry / missing objects become `Unavailable`; the
/// quota family stays distinct from transient rate limiting.
pub(super) fn platform_attachment_error(
    error: &EmailError,
) -> onecrew_email_contracts::PlatformEmailAttachmentError {
    use onecrew_email_contracts::PlatformEmailAttachmentError as AttachmentError;
    use onecrew_email_contracts::ProviderErrorClass;
    let message = error.to_string();
    match error {
        EmailError::OversizedResponse { .. } => AttachmentError::TooLarge { message },
        EmailError::Transport(_) => AttachmentError::Transient { message },
        EmailError::Provider(failure) => match failure.class {
            ProviderErrorClass::NotFound => AttachmentError::Unavailable { message },
            ProviderErrorClass::RateLimited => AttachmentError::RateLimited { message },
            ProviderErrorClass::DailyQuotaExceeded | ProviderErrorClass::MonthlyQuotaExceeded => {
                AttachmentError::QuotaExhausted { message }
            }
            ProviderErrorClass::Auth => AttachmentError::AuthenticationLost { message },
            ProviderErrorClass::ProviderFault => AttachmentError::Transient { message },
            _ => AttachmentError::Failed { message },
        },
        _ => AttachmentError::Failed { message },
    }
}

pub(super) fn platform_attachment_access(
    value: onecrew_email_contracts::ProviderAttachmentAccess,
) -> onecrew_email_contracts::PlatformAttachmentAccess {
    onecrew_email_contracts::PlatformAttachmentAccess {
        metadata: onecrew_email_contracts::PlatformEmailAttachment {
            provider_attachment_id: value.metadata.provider_attachment_id,
            filename: value.metadata.filename,
            content_type: value.metadata.content_type,
            content_id: value.metadata.content_id,
            content_disposition: value.metadata.content_disposition,
            size_bytes: value.metadata.size,
        },
        download_url: value.download_url,
        expires_at: value.expires_at,
    }
}

/// Records a credential-health write that failed after the provider already
/// accepted the send.
///
/// Health is local bookkeeping about a key, not evidence about this message.
/// The provider's acceptance is the send's conclusion, so a failed health
/// write is logged and goes no further: reporting it as an ambiguous outcome
/// would invite the caller to retry a message that is already out.
pub(super) fn note_accepted_send_health_error(health_error: Option<String>) {
    if let Some(health_error) = health_error {
        log::warn!(
            "Resend accepted the request but credential health persistence failed: {health_error}"
        );
    }
}

/// Projects an accepted send, carrying the provider receipt through unchanged.
///
/// The return type has no failure arm on purpose: once the provider has taken
/// the message, nothing this adapter does afterwards can unmake that.
pub(super) fn platform_accepted_send(
    receipt: EmailReceipt,
    health_error: Option<String>,
) -> PlatformEmailReceipt {
    note_accepted_send_health_error(health_error);
    PlatformEmailReceipt {
        provider: receipt.provider,
        external_message_id: receipt.message_id,
        accepted: receipt.accepted,
    }
}

pub(super) fn platform_email_send_error(
    error: EmailError,
    health_error: Option<String>,
) -> PlatformEmailSendError {
    let kind = error.send_failure_kind();
    let mut message = error.to_string();
    if let Some(health_error) = health_error {
        message.push_str(&format!(
            "; failed to persist credential health: {health_error}"
        ));
    }
    match kind {
        EmailSendFailureKind::Validation => PlatformEmailSendError::Validation { message },
        EmailSendFailureKind::Rejected { status } => {
            PlatformEmailSendError::Rejected { status, message }
        }
        EmailSendFailureKind::Ambiguous => PlatformEmailSendError::Ambiguous { message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_snapshot_mapping_is_lossless() {
        let mapped = platform_inbound_email(ProviderInboundEmail {
            provider_message_id: "provider-id".into(),
            internet_message_id: "<message@example.com>".into(),
            from: "creator@example.com".into(),
            to: vec!["reach@example.com".into()],
            cc: vec![],
            bcc: vec![],
            received_for: vec!["Envelope@Example.com".into()],
            reply_to: vec!["reply@example.com".into()],
            subject: "Reply".into(),
            text: Some("Hi".into()),
            html: None,
            headers: serde_json::json!({"references": "<root@example.com>"}),
            created_at: "2026-08-05T00:00:00Z".into(),
            attachments: Vec::new(),
        });
        assert_eq!(mapped.provider_message_id, "provider-id");
        assert_eq!(
            mapped.internet_message_id.as_deref(),
            Some("<message@example.com>")
        );
        assert_eq!(mapped.reply_to, ["reply@example.com"]);
        assert!(mapped.bcc.is_empty());
        assert_eq!(mapped.received_for, ["Envelope@Example.com"]);
        assert!(mapped.inbound);
    }

    #[test]
    fn receiving_page_is_scoped_to_the_selected_mailbox_and_keeps_global_cursor() {
        let summary = |id: &str,
                       to: Vec<&str>,
                       received_for: Vec<&str>|
         -> onecrew_email_contracts::ProviderInboundEmailSummary {
            onecrew_email_contracts::ProviderInboundEmailSummary {
                provider_message_id: id.into(),
                internet_message_id: format!("<{id}@mailer.example.net>"),
                from: "Creator <creator@example.net>".into(),
                to: to.into_iter().map(str::to_owned).collect(),
                cc: Vec::new(),
                bcc: Vec::new(),
                received_for: received_for.into_iter().map(str::to_owned).collect(),
                subject: "Reply".into(),
                created_at: "2026-08-05T00:00:00Z".into(),
                attachments: Vec::new(),
            }
        };
        let page = platform_email_page(
            ProviderInboundEmailPage {
                messages: vec![
                    summary("selected-visible", vec!["Partnership@Example.com"], vec![]),
                    summary("another-mailbox", vec!["billing@example.com"], vec![]),
                    summary(
                        "selected-envelope",
                        vec!["forwarder@example.net"],
                        vec!["partnership@example.com"],
                    ),
                ],
                has_more: true,
                next_cursor: Some("provider-global-cursor".into()),
            },
            "partnership@example.com",
        );

        assert_eq!(
            page.messages
                .iter()
                .map(|message| message.provider_message_id.as_str())
                .collect::<Vec<_>>(),
            ["selected-visible", "selected-envelope"]
        );
        assert!(page.has_more);
        assert_eq!(page.next_cursor.as_deref(), Some("provider-global-cursor"));
    }

    #[test]
    fn an_accepted_send_stays_accepted_when_credential_health_persistence_fails() {
        // The provider already took the message. Downgrading that to an
        // ambiguous outcome because a local health row would not write leaves
        // the claim unresolved and sends the same mail a second time on retry.
        let receipt = platform_accepted_send(
            EmailReceipt {
                provider: "resend".into(),
                message_id: Some("provider-message-1".into()),
                accepted: true,
            },
            Some("vault lock poisoned".into()),
        );

        assert!(receipt.accepted);
        assert_eq!(
            receipt.external_message_id.as_deref(),
            Some("provider-message-1")
        );
        assert_eq!(receipt.provider, "resend");
    }

    #[test]
    fn send_errors_remain_typed_across_the_adapter_boundary() {
        assert!(matches!(
            platform_email_send_error(EmailError::InvalidMessage("bad".into()), None),
            PlatformEmailSendError::Validation { .. }
        ));
        assert!(matches!(
            platform_email_send_error(
                EmailError::Provider(onecrew_email_contracts::ProviderFailure {
                    status: 422,
                    class: onecrew_email_contracts::ProviderErrorClass::Validation,
                    message: "rejected".into(),
                    rate_limit: onecrew_email_contracts::ProviderRateLimitStatus {
                        limit: None,
                        remaining: None,
                        reset_seconds: None,
                        retry_after_seconds: None,
                    },
                }),
                None
            ),
            PlatformEmailSendError::Rejected { status: 422, .. }
        ));
        assert!(matches!(
            platform_email_send_error(
                EmailError::Transport("timeout".into()),
                Some("health write failed".into())
            ),
            PlatformEmailSendError::Ambiguous { message }
                if message.contains("health write failed")
        ));
    }
}
