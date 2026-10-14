//! Telegram direct messages.
//!
//! This provider owns the target kinds Telegram accepts, its thread-id rule
//! (the peer username), and the human-user check it performs before a read. A
//! failed write preserves effect uncertainty without raw provider diagnostics.
//! Every call opens and closes its own connection, as every other Telegram
//! operation does.

use std::sync::Arc;

use async_trait::async_trait;
use onecrew_dm_contracts::{
    DirectMessageProvider, DmCapability, DmMessage, DmReadError, DmReceipt, DmSendError,
    DmSendRequest, DmTarget, DmTargetKind, DmThread,
};

use crate::{TgAccount, TgError, TgUserTarget};

/// Supplies a connected Telegram account for one profile, per call.
#[async_trait]
pub trait TgAccountSource: Send + Sync {
    async fn open_tg_account(&self, profile_id: &str) -> Result<TgAccount, String>;
}

pub struct TgDirectMessages {
    source: Arc<dyn TgAccountSource>,
    profile_id: String,
}

impl TgDirectMessages {
    pub fn new(source: Arc<dyn TgAccountSource>, profile_id: String) -> Self {
        Self { source, profile_id }
    }

    async fn account(&self) -> Result<TgAccount, String> {
        self.source.open_tg_account(&self.profile_id).await
    }
}

#[async_trait]
impl DirectMessageProvider for TgDirectMessages {
    async fn capability(&self, target: &DmTarget) -> Result<DmCapability, DmReadError> {
        if target.kind != DmTargetKind::User {
            return Err(local_read("unsupported_target", USER_TARGET));
        }
        // Telegram exposes no direct-message permission probe, and following is
        // not part of its contact rule, so no follow confirmation is asked for.
        Ok(DmCapability::unknown(false))
    }

    async fn send(&self, request: DmSendRequest) -> Result<DmReceipt, DmSendError> {
        if request.target.kind != DmTargetKind::User {
            return Err(DmSendError::refused(local_diagnostic(
                "unsupported_target",
                USER_TARGET,
            )));
        }
        let recipient = handle(&request.target);
        if recipient.is_empty() || request.body.trim().is_empty() {
            return Err(DmSendError::refused(local_diagnostic(
                "recipient_or_body_missing",
                "Telegram recipient and message body are required",
            )));
        }
        let external_id = request.target.external_id.trim();
        if external_id.is_empty() {
            return Err(DmSendError::refused(local_diagnostic(
                "target_identity_missing",
                "Telegram direct message requires a resolved User target ID",
            )));
        }
        let target = TgUserTarget {
            external_id: external_id.to_owned(),
            username: recipient.to_owned(),
        };
        let account = self.account().await.map_err(|_| {
            DmSendError::refused(local_diagnostic(
                "session_unavailable",
                "Telegram session could not be opened",
            ))
        })?;
        let result = account
            .send_dm(&target, request.body.trim())
            .await
            .map_err(send_error);
        account.close().await;
        let external_message_id = result?;
        if external_message_id.trim().is_empty() {
            return Err(DmSendError::unknown(local_diagnostic(
                "receipt_identity_missing",
                "Telegram returned an empty message receipt",
            )));
        }
        Ok(DmReceipt {
            external_message_id,
            // Telegram has no thread id of its own; the peer username is the
            // key this provider reads a conversation back with.
            external_thread_id: Some(recipient.to_owned()),
        })
    }

    async fn thread(
        &self,
        target: &DmTarget,
        _external_thread_id: Option<&str>,
        limit: u16,
    ) -> Result<Option<DmThread>, DmReadError> {
        if target.kind != DmTargetKind::User {
            return Err(local_read(
                "unsupported_target",
                "Telegram conversations require a resolved User target",
            ));
        }
        let recipient = handle(target);
        if recipient.is_empty() {
            return Err(local_read(
                "recipient_missing",
                "Telegram conversation recipient is required",
            ));
        }
        let account = self.account().await.map_err(|_| {
            local_read(
                "session_unavailable",
                "Telegram session could not be opened",
            )
        })?;
        let result = async {
            // Telegram marks each message as outgoing itself, so reading a
            // thread needs no identity round trip.
            let user_target = TgUserTarget {
                external_id: target.external_id.trim().to_owned(),
                username: recipient.to_owned(),
            };
            let messages = account
                .user_thread(&user_target, usize::from(limit.clamp(1, 200)))
                .await
                .map_err(read_error)?;
            if messages.is_empty() {
                return Ok(None);
            }
            Ok(Some(DmThread {
                external_thread_id: recipient.to_owned(),
                peer_external_id: user_target.external_id,
                messages: messages
                    .into_iter()
                    .map(|message| {
                        if message.id.trim().is_empty() {
                            return Err(local_read(
                                "message_identity_missing",
                                "Telegram message identity is unavailable",
                            ));
                        }
                        let provider = serde_json::to_value(&message).map_err(|_| {
                            local_read(
                                "message_encoding_failed",
                                "Telegram message could not be encoded",
                            )
                        })?;
                        // Telegram states the date in seconds. One it cannot
                        // state fails the read rather than being stamped with a
                        // time nobody reported.
                        let sent_at = u64::try_from(message.date).map_err(|_| {
                            local_read(
                                "message_time_missing",
                                "Telegram message carries no usable send time",
                            )
                        })?;
                        Ok(DmMessage {
                            external_message_id: message.id,
                            outgoing: message.outgoing,
                            body: message.text,
                            sent_at,
                            provider,
                        })
                    })
                    .collect::<Result<Vec<_>, DmReadError>>()?,
            }))
        }
        .await;
        account.close().await;
        result
    }
}

const USER_TARGET: &str = "Telegram direct messages require a resolved User target";

fn send_error(error: TgError) -> DmSendError {
    let diagnostic = evidence(&error);
    match error {
        TgError::Auth(_)
        | TgError::Permission(_)
        | TgError::Session(_)
        | TgError::Rpc {
            code: 401 | 403, ..
        } => DmSendError::refused(diagnostic),
        TgError::Network(_) | TgError::Other(_) | TgError::Rpc { .. } => {
            DmSendError::unknown(diagnostic)
        }
    }
}

fn read_error(error: TgError) -> DmReadError {
    DmReadError {
        diagnostic: evidence(&error),
    }
}

/// The protocol-native evidence of a Telegram failure, shared by reads and
/// direct messages: an MTProto RPC error keeps Telegram's own RPC code and
/// name, a request without a usable RPC response is provider evidence without
/// either, and every other failure is a local code. It never carries the
/// transport's text, so no proxy address, phone number, session or peer
/// authorization reaches it.
pub fn evidence(error: &TgError) -> onecrew_dm_contracts::DmDiagnostic {
    use onecrew_dm_contracts::DmDiagnostic;
    match error {
        TgError::Session(_) => local_diagnostic(
            "session_unavailable",
            "Telegram session could not be opened",
        ),
        TgError::Auth(_) => local_diagnostic(
            "authorization_unavailable",
            "Telegram authorization could not be established",
        ),
        TgError::Permission(_) => local_diagnostic(
            "permission_unavailable",
            "Telegram permission could not be established",
        ),
        TgError::Network(_) => DmDiagnostic::provider(
            "telegram",
            "mtproto",
            "Telegram request did not produce a usable RPC response",
        ),
        TgError::Other(_) => local_diagnostic(
            "target_unavailable",
            "Telegram target could not be resolved and validated",
        ),
        TgError::Rpc { code, name } => {
            let mut diagnostic = DmDiagnostic::provider(
                "telegram",
                "mtproto",
                "Telegram returned an MTProto RPC error",
            );
            diagnostic.rpc_code = Some(*code);
            diagnostic.code = name.clone();
            diagnostic
        }
    }
}

pub(crate) fn local_diagnostic(
    code: &'static str,
    message: &'static str,
) -> onecrew_dm_contracts::DmDiagnostic {
    onecrew_dm_contracts::DmDiagnostic::local("telegram", "mtproto", code, message)
}

fn local_read(code: &'static str, message: &'static str) -> DmReadError {
    DmReadError {
        diagnostic: local_diagnostic(code, message),
    }
}

fn handle(target: &DmTarget) -> &str {
    target.handle.trim().trim_start_matches('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_telegram_errors_keep_unknown_and_never_echo_provider_text() {
        for error in [
            TgError::Network("SECRET".into()),
            TgError::Other("SECRET".into()),
        ] {
            let error = send_error(error);
            assert_eq!(
                error.kind(),
                onecrew_dm_contracts::DmSendFailureKind::Unknown
            );
            assert!(!error.to_string().contains("SECRET"));
        }
        for (code, name) in [
            (420, "FLOOD_WAIT"),
            (500, "INTERNAL"),
            (999, "NEW_NATIVE_ERROR"),
        ] {
            let error = send_error(TgError::Rpc {
                code,
                name: Some(name.into()),
            });
            assert_eq!(
                error.kind(),
                onecrew_dm_contracts::DmSendFailureKind::Unknown
            );
            assert_eq!(error.diagnostic.platform, "telegram");
            assert_eq!(error.diagnostic.protocol, "mtproto");
            assert_eq!(error.diagnostic.rpc_code, Some(code));
            assert_eq!(error.diagnostic.code.as_deref(), Some(name));
            assert_eq!(error.diagnostic.http_status, None);
        }
        assert_eq!(
            send_error(TgError::Permission("SECRET".into())).kind(),
            onecrew_dm_contracts::DmSendFailureKind::Refused
        );
    }

    #[test]
    fn telegram_rpc_read_diagnostics_do_not_treat_http_or_missing_codes_as_equivalent() {
        let native = read_error(TgError::Rpc {
            code: 429,
            name: Some("UNKNOWN_NATIVE_NAME".into()),
        })
        .diagnostic;
        assert_eq!(native.rpc_code, Some(429));
        assert_eq!(native.http_status, None);
        assert_eq!(native.code.as_deref(), Some("UNKNOWN_NATIVE_NAME"));
        let absent = read_error(TgError::Network("login 429 timeout SECRET".into())).diagnostic;
        assert_eq!(absent.rpc_code, None);
        assert_eq!(absent.code, None);
        assert!(!serde_json::to_string(&absent).unwrap().contains("SECRET"));
    }

    const PROXY_URL: &str = "socks5://proxy_user:proxy_pass@gate.example:1080";
    const PHONE: &str = "+15551234567";

    /// Every Telegram failure shape, each carrying a credentialed proxy address
    /// and a phone number wherever the shape has text.
    fn every_error() -> Vec<TgError> {
        let text = || format!("via {PROXY_URL} for {PHONE} SENTINEL");
        let errors = vec![
            TgError::Session(text()),
            TgError::Auth(text()),
            TgError::Permission(text()),
            TgError::Network(text()),
            TgError::Other(text()),
            TgError::Rpc {
                code: 420,
                name: Some("FLOOD_WAIT".into()),
            },
            TgError::Rpc {
                code: 500,
                name: None,
            },
        ];
        for error in &errors {
            covered(error);
        }
        errors
    }

    /// Names every Telegram failure shape, so a new shape fails to compile
    /// here until it gains an arm. The match does not add the shape to
    /// `every_error`; its author must add that sample and its evidence.
    fn covered(error: &TgError) {
        match error {
            TgError::Session(_)
            | TgError::Auth(_)
            | TgError::Permission(_)
            | TgError::Network(_)
            | TgError::Other(_)
            | TgError::Rpc { .. } => {}
        }
    }

    #[test]
    fn every_telegram_failure_has_its_evidence_without_proxy_or_phone() {
        use onecrew_dm_contracts::DmDiagnostic;
        let rpc = |code, name: Option<&str>| DmDiagnostic {
            rpc_code: Some(code),
            code: name.map(str::to_owned),
            ..DmDiagnostic::provider(
                "telegram",
                "mtproto",
                "Telegram returned an MTProto RPC error",
            )
        };
        let expected = vec![
            local_diagnostic(
                "session_unavailable",
                "Telegram session could not be opened",
            ),
            local_diagnostic(
                "authorization_unavailable",
                "Telegram authorization could not be established",
            ),
            local_diagnostic(
                "permission_unavailable",
                "Telegram permission could not be established",
            ),
            DmDiagnostic::provider(
                "telegram",
                "mtproto",
                "Telegram request did not produce a usable RPC response",
            ),
            local_diagnostic(
                "target_unavailable",
                "Telegram target could not be resolved and validated",
            ),
            rpc(420, Some("FLOOD_WAIT")),
            rpc(500, None),
        ];
        let errors = every_error();
        assert_eq!(errors.len(), expected.len());
        for (error, expected) in errors.iter().zip(expected) {
            let evidence = evidence(error);
            assert_eq!(evidence, expected, "{error:?}");
            assert_eq!(evidence.http_status, None, "{error:?}");
            for view in [
                format!("{evidence:?}"),
                serde_json::to_string(&evidence).unwrap(),
            ] {
                for absent in [
                    "proxy_user",
                    "proxy_pass",
                    "gate.example",
                    "socks5://",
                    PHONE,
                    "SENTINEL",
                ] {
                    assert!(!view.contains(absent), "{absent}: {view}");
                }
            }
        }
    }

    #[test]
    fn following_is_not_part_of_the_telegram_contact_rule() {
        assert!(!DmCapability::unknown(false).follow_relevant);
    }
}
