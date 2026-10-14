use crate::{
    ActionFailure, ActionFailureKind, CommitEmailActionInput, CommitEmailDraftActionInput,
    CommitManualEmailActionInput, CreatorChangeScope, CreatorRepositoryCallError,
    EmailActionResult, EmailCapability, EmailDirection, EmailGateway, EmailSendFailure,
    EmailSnapshotInput, MailboxDetail, ManualEmailActionResult, RecoverableActionClaim,
    RemoteAcceptedEmailAction, RemoteAcceptedEmailContext, RemoteStartedEmailAction,
    decode_persisted_email_action_result, decode_persisted_manual_email_action_result,
};
use onecrew_email_contracts::{PlatformEmailRequest, PlatformEmailSendError, SentDelivery};

use super::{
    EMAIL_ACTION, MANUAL_EMAIL_ACTION, RecoveredEmailAction, VerifiedEmailAction, ensure_mailbox,
    failed_replay, provider_message_input, validate_remote_accepted_email_action,
    validate_remote_started_email_action, verify_sent_readback,
};

impl crate::CreatorRuntime {
    /// Hands one frozen request to the provider. The caller resolves the
    /// gateway first, because a missing gateway is a local precondition that
    /// starts no provider call at all — not a send whose outcome is unknown.
    pub(crate) async fn accept_email_once(
        gateway: &dyn EmailGateway,
        mailbox: &MailboxDetail,
        request: &PlatformEmailRequest,
    ) -> Result<String, PlatformEmailSendError> {
        let receipt = gateway
            .send_email(&mailbox.mailbox_ref_id, request.clone())
            .await?;
        let provider_message_id = receipt
            .external_message_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if !receipt.accepted || provider_message_id.is_none() {
            return Err(PlatformEmailSendError::Ambiguous {
                message: "email provider did not accept the email with a stable message id".into(),
            });
        }
        Ok(provider_message_id.unwrap().to_owned())
    }

    pub(super) async fn resume_creator_email_claim(
        &self,
        claim: RecoverableActionClaim,
        replayed: bool,
    ) -> Result<EmailActionResult, EmailSendFailure> {
        match claim {
            RecoverableActionClaim::Completed(value) => {
                // A completed claim is a delivered send; only decoding it failed.
                let mut result =
                    decode_persisted_email_action_result(value).map_err(EmailSendFailure::sent)?;
                result.replayed = true;
                Ok(result)
            }
            // A durable `failed` claim records a terminal provider rejection.
            RecoverableActionClaim::Failed(value) => {
                Err(EmailSendFailure::not_sent(failed_replay(value)))
            }
            RecoverableActionClaim::RemoteStarted(value) => {
                // The claim was staged but never handed to the provider.
                let recovery: RemoteStartedEmailAction = serde_json::from_value(value)
                    .map_err(|error| EmailSendFailure::not_sent(error.to_string()))?;
                match self
                    .resume_remote_started_email_action(recovery, replayed)
                    .await?
                {
                    RecoveredEmailAction::Creator(result) => Ok(*result),
                    _ => Err(EmailSendFailure::sent(
                        "email recovery returned the wrong action kind",
                    )),
                }
            }
            RecoverableActionClaim::RemoteAccepted(value) => {
                // The provider already accepted this request.
                let recovery: RemoteAcceptedEmailAction = serde_json::from_value(value)
                    .map_err(|error| EmailSendFailure::sent(error.to_string()))?;
                match self
                    .recover_remote_accepted_email_action(recovery, replayed)
                    .await?
                {
                    RecoveredEmailAction::Creator(result) => Ok(*result),
                    _ => Err(EmailSendFailure::sent(
                        "email recovery returned the wrong action kind",
                    )),
                }
            }
            RecoverableActionClaim::RemoteVerified(value) => {
                let commit: CommitEmailActionInput = serde_json::from_value(value)
                    .map_err(|error| EmailSendFailure::sent(error.to_string()))?;
                match self
                    .complete_verified_email_action_segment(
                        VerifiedEmailAction::Creator(Box::new(commit)),
                        replayed,
                    )
                    .await?
                {
                    RecoveredEmailAction::Creator(result) => Ok(*result),
                    _ => Err(EmailSendFailure::sent(
                        "email recovery returned the wrong action kind",
                    )),
                }
            }
        }
    }

    pub(super) async fn resume_manual_email_claim(
        &self,
        claim: RecoverableActionClaim,
        replayed: bool,
    ) -> Result<ManualEmailActionResult, EmailSendFailure> {
        match claim {
            RecoverableActionClaim::Completed(value) => {
                let mut result = decode_persisted_manual_email_action_result(value)
                    .map_err(EmailSendFailure::sent)?;
                result.replayed = true;
                Ok(result)
            }
            RecoverableActionClaim::Failed(value) => {
                Err(EmailSendFailure::not_sent(failed_replay(value)))
            }
            RecoverableActionClaim::RemoteStarted(value) => {
                let recovery: RemoteStartedEmailAction = serde_json::from_value(value)
                    .map_err(|error| EmailSendFailure::not_sent(error.to_string()))?;
                match self
                    .resume_remote_started_email_action(recovery, replayed)
                    .await?
                {
                    RecoveredEmailAction::Manual(result) => Ok(result),
                    _ => Err(EmailSendFailure::sent(
                        "manual email recovery returned the wrong action kind",
                    )),
                }
            }
            RecoverableActionClaim::RemoteAccepted(value) => {
                let recovery: RemoteAcceptedEmailAction = serde_json::from_value(value)
                    .map_err(|error| EmailSendFailure::sent(error.to_string()))?;
                match self
                    .recover_remote_accepted_email_action(recovery, replayed)
                    .await?
                {
                    RecoveredEmailAction::Manual(result) => Ok(result),
                    _ => Err(EmailSendFailure::sent(
                        "manual email recovery returned the wrong action kind",
                    )),
                }
            }
            RecoverableActionClaim::RemoteVerified(value) => {
                let commit: CommitManualEmailActionInput = serde_json::from_value(value)
                    .map_err(|error| EmailSendFailure::sent(error.to_string()))?;
                match self
                    .complete_verified_email_action_segment(
                        VerifiedEmailAction::Manual(commit),
                        replayed,
                    )
                    .await?
                {
                    RecoveredEmailAction::Manual(result) => Ok(result),
                    _ => Err(EmailSendFailure::sent(
                        "manual email recovery returned the wrong action kind",
                    )),
                }
            }
        }
    }

    pub(crate) async fn resume_remote_started_email_action(
        &self,
        recovery: RemoteStartedEmailAction,
        replayed: bool,
    ) -> Result<RecoveredEmailAction, EmailSendFailure> {
        validate_remote_started_email_action(&recovery).map_err(EmailSendFailure::not_sent)?;
        let mailbox_runtime = self.clone();
        let mailbox_ref_id = recovery.mailbox.id.clone();
        let current_mailbox = self
            .repository_read_segment(move || mailbox_runtime.require_mailbox_ref(&mailbox_ref_id))
            .await
            .map_err(|error| email_pre_provider_repository_error("load send mailbox", error))?;
        ensure_mailbox(&current_mailbox, EmailCapability::Send).map_err(|error| {
            EmailSendFailure::not_sent(format!(
                "staged email provider retry is unavailable: {error}. Retry the same action and idempotency key after the mailbox credential is available"
            ))
        })?;
        // Resolved before the provider is asked: with no gateway there is no
        // send to be uncertain about, and the claim stays remote_started for
        // the same action and idempotency key.
        let gateway = self.gateways.email.as_ref().ok_or_else(|| {
            EmailSendFailure::not_sent(
                "staged email provider retry is unavailable: the email gateway is not configured. \
                 Retry the same action and idempotency key after it is configured; no provider \
                 send occurred",
            )
        })?;
        if current_mailbox.provider == crate::MailboxProvider::Gmail {
            let runtime = self.clone();
            let key = recovery.request.idempotency_key.clone();
            let reserved = self.repository_write_segment(move || {
                runtime.repository.reserve_gmail_send_attempt(&key)
            }).await.map_err(|error| EmailSendFailure::unknown(error.to_string()))?;
            if !reserved {
                return Err(EmailSendFailure::unknown("A Gmail send was already attempted. Check sent mail; automatic resend is disabled."));
            }
        }
        let provider_message_id = match Self::accept_email_once(
            gateway.as_ref(),
            &current_mailbox,
            &recovery.request,
        )
        .await
        {
            Ok(provider_message_id) => provider_message_id,
            Err(error) if !error.is_terminal_rejection() => {
                let failure = ActionFailure {
                    kind: ActionFailureKind::RemoteWriteUnverified,
                    message: if current_mailbox.provider == crate::MailboxProvider::Gmail {
                        format!("{}. Gmail may have accepted this email. Check sent mail; automatic resend is disabled.", error.message())
                    } else { format!(
                        "{}. The frozen request remains remote_started; retry the same action and idempotency key",
                        error.message()
                    ) },
                    remote_receipt: Some(serde_json::to_value(&error).unwrap_or_default()),
                };
                // The provider neither accepted nor terminally rejected the
                // request, so the message may still be in flight.
                return Err(EmailSendFailure::unknown(
                    serde_json::to_string(&failure).unwrap_or(failure.message),
                ));
            }
            Err(error) => {
                let draft_id = match &recovery.context {
                    RemoteAcceptedEmailContext::Draft { draft_id, .. } => Some(draft_id.clone()),
                    _ => None,
                };
                let failure_runtime = self.clone();
                let idempotency_key = recovery.request.idempotency_key.clone();
                let action = recovery.action.as_str().to_owned();
                let failure = ActionFailure {
                    kind: ActionFailureKind::RemoteWriteRejected,
                    message: error.to_string(),
                    remote_receipt: Some(serde_json::to_value(&error).unwrap_or_default()),
                };
                let rendered_failure =
                    serde_json::to_string(&failure).unwrap_or_else(|_| failure.message.clone());
                let failure_value = serde_json::to_value(&failure).unwrap_or_default();
                let failure_message = failure.message.clone();
                let failure_receipt = failure.remote_receipt.clone();
                let rendered_for_work = rendered_failure.clone();
                let rendered = self
                    .repository_write_segment(move || {
                        let rendered = if let Some(draft_id) = draft_id {
                            let fallback_error = failure_runtime
                                .repository
                                .fail_and_release_email_draft_send(
                                    &draft_id,
                                    &idempotency_key,
                                    failure_value,
                                )?;
                            failure_runtime.publish_changes([
                                CreatorChangeScope::EmailDrafts,
                                CreatorChangeScope::EmailFolders,
                            ]);
                            if let Some(persist_error) = fallback_error {
                                format!(
                                    "{rendered_for_work}; failed to persist terminal action failure: \
                                     {persist_error}; idempotency claim and draft reservation \
                                     were released for retry"
                                )
                            } else {
                                rendered_for_work.clone()
                            }
                        } else {
                            failure_runtime.fail_email_action_for(
                                &idempotency_key,
                                &action,
                                ActionFailureKind::RemoteWriteRejected,
                                failure_message,
                                failure_receipt,
                            )
                        };
                        Ok::<_, String>(rendered)
                    })
                    .await
                    .map_err(|error| email_rejection_repository_error(&rendered_failure, error))?;
                // The provider terminally rejected the request: nothing was sent.
                return Err(EmailSendFailure::not_sent(rendered));
            }
        };
        let accepted = recovery.accept(provider_message_id);
        let accepted_provider_message_id = accepted.provider_message_id.clone();
        let stage_runtime = self.clone();
        let staged = accepted.clone();
        self.repository_write_segment(move || {
            stage_runtime.stage_remote_accepted_action(&staged)?;
            match &staged.context {
                RemoteAcceptedEmailContext::Draft { .. } => stage_runtime.publish_changes([
                    CreatorChangeScope::EmailDrafts,
                    CreatorChangeScope::EmailFolders,
                ]),
                RemoteAcceptedEmailContext::Creator { .. }
                | RemoteAcceptedEmailContext::Manual { .. } => stage_runtime.publish_changes([
                    CreatorChangeScope::EmailFolders,
                    CreatorChangeScope::EmailThreads,
                ]),
            };
            Ok::<_, String>(())
        })
        .await
        .map_err(|error| {
            email_remote_accepted_repository_error(
                "stage accepted send",
                &accepted_provider_message_id,
                error,
            )
        })?;
        self.recover_remote_accepted_email_action(accepted, replayed)
            .await
    }

    pub(crate) fn stage_remote_accepted_action(
        &self,
        recovery: &RemoteAcceptedEmailAction,
    ) -> Result<(), String> {
        validate_remote_accepted_email_action(recovery)?;
        self.repository
            .stage_remote_accepted_action(
                &recovery.request.idempotency_key,
                recovery.action.as_str(),
                &recovery.request_fingerprint,
                serde_json::to_value(recovery).map_err(|error| error.to_string())?,
            )
            .map_err(|error| {
                format!(
                    "The provider accepted email {} but durable recovery staging failed: {error}. Automatic resend is disabled for this idempotency key",
                    recovery.provider_message_id
                )
            })
    }

    /// Every failure below follows provider acceptance carrying a stable
    /// message id, so the request must never be resent. The remaining work is
    /// one provider-status read and the local durable commit.
    pub(crate) async fn recover_remote_accepted_email_action(
        &self,
        recovery: RemoteAcceptedEmailAction,
        replayed: bool,
    ) -> Result<RecoveredEmailAction, EmailSendFailure> {
        validate_remote_accepted_email_action(&recovery).map_err(EmailSendFailure::sent)?;
        let RemoteAcceptedEmailAction {
            recovery,
            provider_message_id,
        } = recovery;
        let accepted_provider_message_id = provider_message_id.clone();
        let mailbox_runtime = self.clone();
        let mailbox_ref_id = recovery.mailbox.id.clone();
        let current_mailbox = self
            .repository_read_segment(move || mailbox_runtime.require_mailbox_ref(&mailbox_ref_id))
            .await
            .map_err(|error| {
                email_remote_accepted_repository_error(
                    "load readback mailbox",
                    &accepted_provider_message_id,
                    error,
                )
            })?;
        ensure_mailbox(&current_mailbox, EmailCapability::SentReadback).map_err(|error| {
            EmailSendFailure::sent(format!(
                "accepted email readback recovery is unavailable: {error}. Retry the same action and idempotency key after the mailbox credential is available; no provider send will occur"
            ))
        })?;
        let gateway = self.gateways.email.as_ref().ok_or_else(|| {
            EmailSendFailure::sent("accepted email readback recovery requires the email gateway; retry the same action and idempotency key after it is configured; no provider send will occur")
        })?;
        let readback = gateway
            .sent_email(&current_mailbox.mailbox_ref_id, &provider_message_id)
            .await
            .map_err(|error| {
                EmailSendFailure::sent(format!(
                    "accepted email readback recovery failed: {error}. Retry the same action and idempotency key; no provider send will occur"
                ))
            })?;
        let delivery = verify_sent_readback(&provider_message_id, &readback).map_err(|error| {
            EmailSendFailure::sent(format!(
                "accepted email readback is not yet verifiable: {error}. Retry the same action and idempotency key; no provider send will occur"
            ))
        })?;
        match delivery {
            // Accepted messages that are still queued, scheduled, delayed, or
            // in a future provider state are real outbound facts. Persist the
            // provider's exact event after this one detail read; do not turn a
            // progressing send into a recording failure or claim delivery.
            SentDelivery::Sent | SentDelivery::Advancing => {}
            // The provider will not deliver this message, so there is nothing
            // left to wait for and no reason to read the status again. No
            // local success facts are written. The durable claim stays
            // remote_accepted on purpose: once a send has been accepted,
            // `fail_action` refuses to mark its claim failed, because the mail
            // did leave OneCrew and the record must not say otherwise.
            SentDelivery::Failed => {
                return Err(EmailSendFailure::sent(format!(
                    "The provider accepted email {provider_message_id} but reports delivery status {}; it was not delivered, and retrying this action and idempotency key cannot change that. Send again under a new idempotency key if this recipient should be retried; no provider send will occur",
                    readback.last_event
                )));
            }
        }
        let subject = readback.subject.clone();
        let delivery_status = readback.last_event.clone();
        let message = provider_message_input(
            readback,
            EmailDirection::Outbound,
            crate::models::EmailOrigin::ConfirmedSend,
            recovery.in_reply_to.clone(),
            recovery.references.clone(),
            &delivery_status,
        )
        .map_err(|error| {
            EmailSendFailure::sent(format!(
                "accepted email readback could not be normalized: {error}. Retry the same action and idempotency key; no provider send will occur"
            ))
        })?;
        let snapshot_owner = match &recovery.context {
            RemoteAcceptedEmailContext::Creator { preparation } => (
                Some(preparation.creator.creator.id.clone()),
                Some(preparation.route.route.id.clone()),
            ),
            RemoteAcceptedEmailContext::Manual { .. } => (None, None),
            RemoteAcceptedEmailContext::Draft {
                creator_id,
                contact_route_id,
                ..
            } => (creator_id.clone(), contact_route_id.clone()),
        };
        let snapshot = EmailSnapshotInput {
            mailbox_ref_id: recovery.mailbox.id.clone(),
            creator_id: snapshot_owner.0,
            contact_route_id: snapshot_owner.1,
            provider_thread_key: recovery.provider_thread_key.clone(),
            subject,
            messages: vec![message],
        };
        let verified_provider_message_id = provider_message_id.clone();
        let verified = match recovery.context {
            RemoteAcceptedEmailContext::Creator { preparation } => {
                VerifiedEmailAction::Creator(Box::new(CommitEmailActionInput {
                    creator_id: preparation.creator.creator.id.clone(),
                    contact_locator_id: preparation.route.locator.id.clone(),
                    preparation: *preparation,
                    snapshot,
                    verified_provider_message_id: provider_message_id,
                    idempotency_key: recovery.request.idempotency_key,
                    request_fingerprint: recovery.request_fingerprint,
                }))
            }
            RemoteAcceptedEmailContext::Manual { preparation } => {
                VerifiedEmailAction::Manual(CommitManualEmailActionInput {
                    recipient: recovery.expected_recipient.into_string(),
                    preparation,
                    snapshot,
                    verified_provider_message_id: provider_message_id,
                    idempotency_key: recovery.request.idempotency_key,
                    request_fingerprint: recovery.request_fingerprint,
                })
            }
            RemoteAcceptedEmailContext::Draft {
                draft_id,
                expected_revision,
                creator_id,
                contact_locator_id,
                ..
            } => VerifiedEmailAction::Draft(CommitEmailDraftActionInput {
                draft_id,
                expected_revision,
                creator_id,
                contact_locator_id,
                recipient: recovery.expected_recipient.into_string(),
                snapshot,
                verified_provider_message_id: provider_message_id,
                idempotency_key: recovery.request.idempotency_key,
                request_fingerprint: recovery.request_fingerprint,
            }),
        };
        let finish_runtime = self.clone();
        self.repository_write_segment(move || {
            match &verified {
                VerifiedEmailAction::Creator(commit) => {
                    finish_runtime.repository.stage_remote_verified_action(
                        &commit.idempotency_key,
                        EMAIL_ACTION,
                        &commit.request_fingerprint,
                        serde_json::to_value(commit).map_err(|error| error.to_string())?,
                    )?;
                }
                VerifiedEmailAction::Manual(commit) => {
                    finish_runtime.repository.stage_remote_verified_action(
                        &commit.idempotency_key,
                        MANUAL_EMAIL_ACTION,
                        &commit.request_fingerprint,
                        serde_json::to_value(commit).map_err(|error| error.to_string())?,
                    )?;
                }
                VerifiedEmailAction::Draft(commit) => {
                    finish_runtime
                        .repository
                        .stage_verified_email_draft_action(commit)?;
                }
            }
            finish_runtime.complete_verified_email_action(verified, replayed)
        })
        .await
        .map_err(|error| {
            email_remote_accepted_repository_error(
                "finish verified send",
                &verified_provider_message_id,
                error,
            )
        })
    }

    pub(crate) async fn complete_verified_email_action_segment(
        &self,
        verified: VerifiedEmailAction,
        replayed: bool,
    ) -> Result<RecoveredEmailAction, EmailSendFailure> {
        let runtime = self.clone();
        self.repository_write_segment(move || {
            runtime.complete_verified_email_action(verified, replayed)
        })
        .await
        .map_err(|error| email_remote_verified_repository_error("recover verified send", error))
    }

    pub(crate) fn complete_verified_email_action(
        &self,
        verified: VerifiedEmailAction,
        replayed: bool,
    ) -> Result<RecoveredEmailAction, String> {
        match verified {
            VerifiedEmailAction::Creator(commit) => {
                let mut result = self.repository.commit_email_action(*commit).map_err(|error| {
                    format!(
                        "verified email local recovery commit failed: {error}. Retry the same action and idempotency key; no provider send will occur"
                    )
                })?;
                result.replayed = replayed;
                self.publish_changes([
                    CreatorChangeScope::EmailFolders,
                    CreatorChangeScope::EmailThreads,
                ]);
                Ok(RecoveredEmailAction::Creator(Box::new(result)))
            }
            VerifiedEmailAction::Manual(commit) => {
                let mut result = self
                    .repository
                    .commit_manual_email_action(commit)
                    .map_err(|error| {
                        format!(
                            "verified manual email local recovery commit failed: {error}. Retry the same action and idempotency key; no provider send will occur"
                        )
                })?;
                result.replayed = replayed;
                self.publish_changes([
                    CreatorChangeScope::EmailFolders,
                    CreatorChangeScope::EmailThreads,
                ]);
                Ok(RecoveredEmailAction::Manual(result))
            }
            VerifiedEmailAction::Draft(commit) => {
                let mut result = self
                    .repository
                    .commit_email_draft_action(commit)
                    .map_err(|error| {
                        format!(
                            "verified draft email local recovery commit failed: {error}. Retry the same draft, revision, and idempotency key; no provider send will occur"
                        )
                })?;
                result.replayed = replayed;
                self.publish_changes([
                    CreatorChangeScope::EmailDrafts,
                    CreatorChangeScope::EmailFolders,
                    CreatorChangeScope::EmailThreads,
                ]);
                Ok(RecoveredEmailAction::Draft(result))
            }
        }
    }

    pub(crate) fn fail_email_action_for(
        &self,
        key: &str,
        action: &str,
        kind: ActionFailureKind,
        message: String,
        remote_receipt: Option<serde_json::Value>,
    ) -> String {
        let failure = ActionFailure {
            kind,
            message,
            remote_receipt,
        };
        let rendered =
            serde_json::to_string(&failure).unwrap_or_else(|_| "email action failed".into());
        match self.repository.fail_action(
            key,
            action,
            serde_json::to_value(&failure).unwrap_or_default(),
        ) {
            Ok(()) => rendered,
            Err(persist_error) => match self.repository.cancel_action(key, action) {
                Ok(()) => format!(
                    "{rendered}; failed to persist terminal action failure: {persist_error}; idempotency claim was released for retry"
                ),
                Err(release_error) => format!(
                    "{rendered}; failed to persist terminal action failure: {persist_error}; failed to release idempotency claim: {release_error}"
                ),
            },
        }
    }
}

/// These reads run before the provider is asked, so no send can have occurred.
fn email_pre_provider_repository_error(
    stage: &str,
    error: CreatorRepositoryCallError<String>,
) -> EmailSendFailure {
    match error {
        CreatorRepositoryCallError::Owner(error) => EmailSendFailure::not_sent(error),
        CreatorRepositoryCallError::Dispatch(error) => EmailSendFailure::not_sent(format!(
            "email {stage} repository read failed: {error}; no provider send occurred in this \
             attempt; retry only with the same action and idempotency key"
        )),
    }
}

/// The provider terminally rejected the request; only recording that failed.
fn email_rejection_repository_error(
    rendered_failure: &str,
    error: CreatorRepositoryCallError<String>,
) -> EmailSendFailure {
    EmailSendFailure::not_sent(format!(
        "{rendered_failure}; local rejection recovery did not finish: {error}; retry only with \
         the same action and idempotency key so the stored receipt can be reconciled"
    ))
}

/// The provider already accepted the mail; the local follow-up failed.
fn email_remote_accepted_repository_error(
    stage: &str,
    provider_message_id: &str,
    error: CreatorRepositoryCallError<String>,
) -> EmailSendFailure {
    EmailSendFailure::sent(format!(
        "The provider accepted email {provider_message_id}, but {stage} failed: {error}; do not send \
         again or change the idempotency key; retry the same action and key for durable readback"
    ))
}

/// Delivery was already verified against the provider; only local recovery failed.
fn email_remote_verified_repository_error(
    stage: &str,
    error: CreatorRepositoryCallError<String>,
) -> EmailSendFailure {
    EmailSendFailure::sent(format!(
        "provider email delivery was already verified, but {stage} failed: {error}; no provider \
         send is allowed; retry the same action and idempotency key for local recovery"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::email::{canonical_email_request_fingerprint, persisted_mailbox_snapshot};
    use crate::{
        CreatorGateways, CreatorRuntime, EmailActionKind, EmailAddress, EmailSendEffect,
        MailboxRefId, MailboxRefInput, ManualEmailPreparation, RemoteEmailRecovery,
    };

    /// A Send-capable Resend mailbox on a runtime that has no email gateway.
    fn runtime_without_an_email_gateway() -> (tempfile::TempDir, CreatorRuntime, MailboxDetail) {
        let directory = tempfile::tempdir().unwrap();
        let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
        let mailbox = runtime
            .sync_mailbox_ref(MailboxRefInput {
                id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
                provider: "resend".into(),
                label: "Mailbox fixture".into(),
                email_address: "sender@example.com".into(),
                display_name: Some("Sender".into()),
                capabilities: vec![EmailCapability::Send],
                available: true,
                last_error: None,
            })
            .unwrap();
        (directory, runtime, mailbox)
    }

    fn staged_manual_send(mailbox: MailboxDetail) -> RemoteStartedEmailAction {
        let request = PlatformEmailRequest {
            to: vec!["creator@example.net".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            subject: "Hello".into(),
            text: Some("Body".into()),
            html: None,
            idempotency_key: "email-gateway-missing".into(),
        };
        RemoteStartedEmailAction {
            recovery: RemoteEmailRecovery {
                action: EmailActionKind::Manual,
                request_fingerprint: "manual-fingerprint".into(),
                canonical_request_fingerprint: canonical_email_request_fingerprint(&request)
                    .unwrap(),
                mailbox: persisted_mailbox_snapshot(mailbox.clone()),
                expected_recipient: EmailAddress::parse("creator@example.net").unwrap(),
                request,
                provider_thread_key: "thread-key".into(),
                in_reply_to: None,
                references: Vec::new(),
                context: RemoteAcceptedEmailContext::Manual {
                    preparation: ManualEmailPreparation {
                        mailbox,
                        to: "creator@example.net".into(),
                        confirmation_required: false,
                        confirmation_reason: String::new(),
                    },
                },
            },
        }
    }

    #[tokio::test]
    async fn a_missing_email_gateway_is_a_known_unsent_send_not_an_unknown_one() {
        let (_directory, runtime, mailbox) = runtime_without_an_email_gateway();

        let failure = match runtime
            .resume_remote_started_email_action(staged_manual_send(mailbox), false)
            .await
        {
            Ok(_) => panic!("a runtime without an email gateway cannot have sent anything"),
            Err(failure) => failure,
        };

        // Without a gateway no provider call is ever made, so the effect is
        // known. Reporting Unknown here would tell the caller that a message
        // OneCrew never handed over might still be in flight.
        assert_eq!(failure.effect(), EmailSendEffect::NotSent);
        assert!(
            failure
                .message()
                .contains("email gateway is not configured"),
            "{failure}"
        );
    }
}
