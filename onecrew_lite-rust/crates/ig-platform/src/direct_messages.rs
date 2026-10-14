//! Instagram direct messages.
//!
//! This provider owns the target kinds Instagram accepts, how a thread is
//! located when the caller has no thread id, and the stable-id check it
//! performs immediately before a stable-ID write. Failures never expose raw
//! provider or credential diagnostics.

use std::sync::Arc;

use async_trait::async_trait;
use onecrew_dm_contracts::{
    DirectMessageProvider, DmCapability, DmMessage, DmReadError, DmReceipt, DmSendError,
    DmSendRequest, DmTarget, DmTargetKind, DmThread,
};

use crate::{IgAccount, IgError, IgThread};

/// Supplies the live Instagram session for one profile, per call.
#[async_trait]
pub trait IgAccountSource: Send + Sync {
    async fn open_ig_account(&self, profile_id: &str) -> Result<IgAccount, String>;
}

pub struct IgDirectMessages {
    source: Arc<dyn IgAccountSource>,
    profile_id: String,
}

impl IgDirectMessages {
    pub fn new(source: Arc<dyn IgAccountSource>, profile_id: String) -> Self {
        Self { source, profile_id }
    }

    async fn account(&self) -> Result<IgAccount, String> {
        self.source.open_ig_account(&self.profile_id).await
    }
}

#[async_trait]
impl DirectMessageProvider for IgDirectMessages {
    async fn capability(&self, target: &DmTarget) -> Result<DmCapability, DmReadError> {
        if !accepts(target.kind) {
            return Err(local_read("unsupported_target", ACCOUNT_TARGET));
        }
        // Instagram exposes no direct-message permission probe, so nothing is
        // known before the send is attempted. Following is still part of its
        // first-contact rule.
        Ok(DmCapability::unknown(true))
    }

    async fn send(&self, request: DmSendRequest) -> Result<DmReceipt, DmSendError> {
        if !accepts(request.target.kind) {
            return Err(DmSendError::refused(local_diagnostic(
                "unsupported_target",
                ACCOUNT_TARGET,
            )));
        }
        let recipient = handle(&request.target);
        if recipient.is_empty() || request.body.trim().is_empty() {
            return Err(DmSendError::refused(local_diagnostic(
                "recipient_or_body_missing",
                "Instagram recipient and message body are required",
            )));
        }
        let account = self.account().await.map_err(|_| {
            DmSendError::refused(local_diagnostic(
                "session_unavailable",
                "Instagram session could not be opened",
            ))
        })?;
        let target = account
            .user(recipient)
            .await
            .map_err(|error| DmSendError::refused(evidence(&error)))?;
        if target.id != request.target.external_id.trim() {
            return Err(DmSendError::refused(local_diagnostic(
                "target_identity_changed",
                "Instagram direct-message target stable ID changed",
            )));
        }
        let external_message_id = account
            .send_dm(&target.id, request.body.trim())
            .await
            .map_err(send_error)?;
        if external_message_id.trim().is_empty() {
            return Err(DmSendError::unknown(local_diagnostic(
                "receipt_identity_missing",
                "Instagram returned an empty message receipt",
            )));
        }
        Ok(DmReceipt {
            external_message_id,
            // Instagram answers a send with an item id and no thread id, and
            // an inbox scan here would trade a post-write read failure for a
            // lost receipt. The thread id stays an explicit absence; the
            // readback looks the thread up by peer and surfaces its errors.
            external_thread_id: None,
        })
    }

    async fn thread(
        &self,
        target: &DmTarget,
        external_thread_id: Option<&str>,
        limit: u16,
    ) -> Result<Option<DmThread>, DmReadError> {
        if !accepts(target.kind) {
            return Err(local_read(
                "unsupported_target",
                "Instagram conversations require an account target",
            ));
        }
        let account = self.account().await.map_err(|_| {
            local_read(
                "session_unavailable",
                "Instagram session could not be opened",
            )
        })?;
        let current = account.who_am_i().await.map_err(read_error)?;
        let thread_id = match external_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(thread_id) => thread_id.to_owned(),
            None => {
                let Some(thread_id) = account
                    .inbox(100)
                    .await
                    .map_err(read_error)?
                    .into_iter()
                    .find(|thread| {
                        single_peer_id(&thread.users, &current.id)
                            == Some(target.external_id.trim())
                    })
                    .map(|thread| thread.thread_id)
                else {
                    return Ok(None);
                };
                thread_id
            }
        };
        let read = account
            .thread(&thread_id, u32::from(limit.clamp(1, 100)))
            .await
            .map_err(read_error)?;
        let thread = thread(read, &current.id)?;
        if thread.peer_external_id != target.external_id.trim() {
            return Err(local_read(
                "target_identity_changed",
                "Instagram conversation target stable ID changed",
            ));
        }
        Ok(Some(thread))
    }
}

const ACCOUNT_TARGET: &str = "Instagram direct messages require an account target";

fn send_error(error: IgError) -> DmSendError {
    let diagnostic = evidence(&error);
    match crate::IgWriteFailure::from(&error) {
        crate::IgWriteFailure::Refused => DmSendError::refused(diagnostic),
        crate::IgWriteFailure::Unknown => DmSendError::unknown(diagnostic),
    }
}

fn read_error(error: IgError) -> DmReadError {
    DmReadError {
        diagnostic: evidence(&error),
    }
}

/// The protocol-native evidence of an Instagram failure, shared by reads and
/// direct messages: the account API's HTTP status and `error_type` when
/// Instagram answered with them, a provider record without native facts when
/// none was retained, or a local code for a precondition or an unusable
/// response. It carries no response body, challenge payload, session material,
/// proxy address or transport text.
pub fn evidence(error: &IgError) -> onecrew_dm_contracts::DmDiagnostic {
    use onecrew_dm_contracts::DmDiagnostic;
    match error {
        IgError::Session(_) => local_diagnostic(
            "session_unavailable",
            "Instagram session could not be opened",
        ),
        IgError::InvalidHeader => local_diagnostic(
            "invalid_header",
            "Instagram request headers could not be constructed",
        ),
        IgError::Network(_) => DmDiagnostic::provider(
            "instagram",
            "instagram_private_api",
            "Instagram request did not produce a usable response",
        ),
        IgError::Api(_) => local_diagnostic(
            "response_unusable",
            "Instagram response did not contain the required evidence",
        ),
        IgError::Challenge(_) => DmDiagnostic::provider(
            "instagram",
            "instagram_private_api",
            "Instagram reported a challenge; no native code was retained",
        ),
        IgError::Response { status, code } => {
            let mut diagnostic = DmDiagnostic::provider(
                "instagram",
                "instagram_private_api",
                "Instagram returned an unsuccessful account API response",
            );
            diagnostic.http_status = Some(*status);
            diagnostic.code = code.clone();
            diagnostic
        }
    }
}

fn local_diagnostic(
    code: &'static str,
    message: &'static str,
) -> onecrew_dm_contracts::DmDiagnostic {
    onecrew_dm_contracts::DmDiagnostic::local("instagram", "instagram_private_api", code, message)
}

fn local_read(code: &'static str, message: &'static str) -> DmReadError {
    DmReadError {
        diagnostic: local_diagnostic(code, message),
    }
}

const fn accepts(kind: DmTargetKind) -> bool {
    matches!(
        kind,
        DmTargetKind::User | DmTargetKind::Creator | DmTargetKind::Business
    )
}

fn handle(target: &DmTarget) -> &str {
    target.handle.trim().trim_start_matches('@')
}

fn thread(thread: IgThread, current_user_id: &str) -> Result<DmThread, DmReadError> {
    if current_user_id.is_empty() || thread.thread_id.trim().is_empty() {
        return Err(local_read(
            "thread_identity_missing",
            "Instagram thread identity is unavailable",
        ));
    }
    let peer = single_peer_id(&thread.users, current_user_id)
        .ok_or_else(|| {
            local_read(
                "peer_identity_ambiguous",
                "Instagram readback requires exactly one identified peer",
            )
        })?
        .to_owned();
    Ok(DmThread {
        external_thread_id: thread.thread_id,
        peer_external_id: peer,
        messages: thread
            .messages
            .into_iter()
            .map(|message| {
                if message.item_id.trim().is_empty() {
                    return Err(local_read(
                        "message_identity_missing",
                        "Instagram message identity is unavailable",
                    ));
                }
                let provider = serde_json::to_value(&message).map_err(|_| {
                    local_read(
                        "message_encoding_failed",
                        "Instagram message could not be encoded",
                    )
                })?;
                let sent_at = send_time(message.timestamp).ok_or_else(|| {
                    local_read(
                        "message_time_missing",
                        "Instagram message carries no usable send time",
                    )
                })?;
                Ok(DmMessage {
                    external_message_id: message.item_id,
                    outgoing: message.user_id == current_user_id,
                    body: message.text,
                    sent_at,
                    provider,
                })
            })
            .collect::<Result<Vec<_>, DmReadError>>()?,
    })
}

fn single_peer_id<'a>(users: &'a [crate::IgUser], current_user_id: &str) -> Option<&'a str> {
    if current_user_id.is_empty() {
        return None;
    }
    let mut peers = users.iter().filter(|user| user.id != current_user_id);
    let peer = peers.next()?;
    (!peer.id.is_empty() && peers.next().is_none()).then_some(peer.id.as_str())
}

/// Instagram states a thread item's `timestamp` in microseconds, so that is the
/// one unit read here. A value that is not a time in that unit is refused
/// rather than rescaled until it looks plausible: guessing the unit is how a
/// message ends up dated years away from when it was sent.
fn send_time(timestamp: Option<i64>) -> Option<u64> {
    let seconds = u64::try_from(timestamp?).ok()? / 1_000_000;
    (EARLIEST_PLAUSIBLE_SECONDS..=LATEST_PLAUSIBLE_SECONDS)
        .contains(&seconds)
        .then_some(seconds)
}

/// 2001-09-09, comfortably before Instagram existed.
const EARLIEST_PLAUSIBLE_SECONDS: u64 = 1_000_000_000;
/// 2100-01-01.
const LATEST_PLAUSIBLE_SECONDS: u64 = 4_102_444_800;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instagram_accepts_only_account_targets() {
        assert!(accepts(DmTargetKind::User));
        assert!(accepts(DmTargetKind::Creator));
        assert!(accepts(DmTargetKind::Business));
        assert!(!accepts(DmTargetKind::Channel));
        assert!(!accepts(DmTargetKind::Bot));
    }

    #[test]
    fn a_send_time_is_read_as_microseconds_and_anything_else_is_refused() {
        // 2023-11-14T22:13:20Z, as Instagram states it.
        assert_eq!(send_time(Some(1_700_000_000_000_000)), Some(1_700_000_000));

        for unusable in [
            None,
            // The same instant in milliseconds and in seconds: rescaling either
            // one until it looked plausible is exactly the guess that dated
            // messages decades wrong.
            Some(1_700_000_000_000),
            Some(1_700_000_000),
            Some(-1_700_000_000_000_000),
            Some(0),
        ] {
            assert_eq!(
                send_time(unusable),
                None,
                "{unusable:?} is not a microsecond timestamp"
            );
        }
    }

    /// Text an Instagram failure may carry: a response body, a bearer token,
    /// session cookies and a credentialed proxy address.
    fn sensitive_text() -> String {
        "body=SENTINEL Authorization: Bearer SENTINEL; Cookie: sessionid=SENTINEL; \
         csrftoken=SENTINEL via https://proxy_user:proxy_pass@gate.example:8080/?token=SENTINEL"
            .to_owned()
    }

    /// Every Instagram failure shape with its evidence; each shape that has
    /// text carries sensitive text.
    fn every_error_with_its_evidence() -> Vec<(IgError, onecrew_dm_contracts::DmDiagnostic)> {
        use onecrew_dm_contracts::DmDiagnostic;
        let provider =
            |message| DmDiagnostic::provider("instagram", "instagram_private_api", message);
        let response = |status, code: Option<&str>| DmDiagnostic {
            http_status: Some(status),
            code: code.map(str::to_owned),
            ..provider("Instagram returned an unsuccessful account API response")
        };
        let cases = vec![
            (
                IgError::Session(sensitive_text()),
                local_diagnostic(
                    "session_unavailable",
                    "Instagram session could not be opened",
                ),
            ),
            (
                IgError::Network(sensitive_text()),
                provider("Instagram request did not produce a usable response"),
            ),
            (
                IgError::Api(sensitive_text()),
                local_diagnostic(
                    "response_unusable",
                    "Instagram response did not contain the required evidence",
                ),
            ),
            (
                IgError::Challenge(sensitive_text()),
                provider("Instagram reported a challenge; no native code was retained"),
            ),
            (
                IgError::Response {
                    status: 429,
                    code: Some("rate_limit_error".into()),
                },
                response(429, Some("rate_limit_error")),
            ),
            (
                IgError::Response {
                    status: 404,
                    code: None,
                },
                response(404, None),
            ),
            (
                IgError::InvalidHeader,
                local_diagnostic(
                    "invalid_header",
                    "Instagram request headers could not be constructed",
                ),
            ),
        ];
        for (error, _) in &cases {
            covered(error);
        }
        cases
    }

    /// Names every Instagram failure shape, so a new shape fails to compile
    /// here until it gains an arm. The match neither adds the shape to
    /// `every_error_with_its_evidence` nor notices a sample removed from it;
    /// the list's author keeps every sample and its evidence.
    fn covered(error: &IgError) {
        match error {
            IgError::Session(_)
            | IgError::Network(_)
            | IgError::Api(_)
            | IgError::Challenge(_)
            | IgError::Response { .. }
            | IgError::InvalidHeader => {}
        }
    }

    #[test]
    fn evidence_of_every_error_keeps_its_native_facts_and_drops_its_text() {
        for (error, expected) in every_error_with_its_evidence() {
            let actual = evidence(&error);
            assert_eq!(actual, expected, "{error:?}");
            assert_eq!(actual.platform, "instagram");
            assert_eq!(actual.protocol, "instagram_private_api");
            assert!(actual.rpc_code.is_none());
            for text in [
                serde_json::to_string(&actual).unwrap(),
                format!("{actual:?}"),
            ] {
                for absent in [
                    "SENTINEL",
                    "body=",
                    "Bearer",
                    "sessionid",
                    "csrftoken",
                    "proxy_user",
                    "proxy_pass",
                    "gate.example",
                    "token=",
                ] {
                    assert!(!text.contains(absent), "{absent}: {text}");
                }
            }
        }
    }

    #[test]
    fn instagram_has_no_probe_so_capability_is_unknown_and_follow_relevant() {
        let capability = DmCapability::unknown(true);
        assert_eq!(
            capability.state,
            onecrew_dm_contracts::DmCapabilityState::Unknown
        );
        assert!(capability.follow_relevant);
    }
}
