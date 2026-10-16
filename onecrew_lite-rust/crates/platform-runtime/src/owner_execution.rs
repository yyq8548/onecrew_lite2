//! Dispatch sealed owner grants; peer adapters only project the resulting facts.
use std::{fmt, sync::Arc};

use onecrew_creator_runtime::{EmailSendEffect, EmailSendFailure};

use crate::{
    CallDisposition, CreatorSaveError, DmFailure, LocalPlatform, OutreachError,
    OwnerCallCompletion, OwnerReviewKind, PresentationLocale, ResultBinding, ResultFailure,
    ResultSnapshot, ResultState, ResultStoreError, ResultTerminalUpdate, ResultUnknown,
    SubmittedOwnerGrant, TerminalResult,
};

/// Safe failure projection for the protocol after a submitted owner call.
/// Only safe transient DM evidence accompanies the owner's exact effect fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerExecutionFailure {
    pub code: &'static str,
    pub message: &'static str,
    pub effect: OwnerExecutionEffect,
    pub dm_failure: Option<Box<DmFailure>>,
}

/// What the submitted owner proved about the governed effect before failing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerExecutionEffect {
    /// The owner proved that the governed effect did not happen.
    KnownNoEffect,
    /// The owner proved that the governed effect happened; only later work failed.
    Applied,
    /// The owner cannot prove whether the governed effect happened.
    Unknown,
}

impl fmt::Display for OwnerExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}
impl std::error::Error for OwnerExecutionFailure {}

impl LocalPlatform {
    /// Consume the store-issued grant without reconstructing intent from peer
    /// JSON. The admitted Host work future owns execution through its terminal
    /// or next-review transition.
    ///
    /// # Errors
    /// Reports a safe owner failure and its exact effect classification. Peers
    /// must keep the current result and intent key for applied or unknown effects.
    pub async fn execute_submitted_owner(
        self: &Arc<Self>,
        grant: SubmittedOwnerGrant,
    ) -> Result<OwnerCallCompletion, OwnerExecutionFailure> {
        if matches!(grant.kind(), OwnerReviewKind::Outreach | OwnerReviewKind::Email) {
            return Err(OwnerExecutionFailure {
                code: crate::RETIRED_OPERATION_CODE,
                message: "This sending operation has been retired. Use Gmail bulk mail from the Mail page.",
                effect: OwnerExecutionEffect::KnownNoEffect,
                dm_failure: None,
            });
        }
        let context = grant.context().clone();
        let locale = grant.locale();
        match grant.kind() {
            OwnerReviewKind::CreatorSave => self
                .execute_creator_save_submitted(grant)
                .await
                .map_err(creator_save_failure),
            OwnerReviewKind::Outreach => self
                .execute_outreach_submitted(grant)
                .await
                .map_err(outreach_failure)?
                .into_owner_completion(&context, locale)
                .map_err(|_| owner_completion_projection_failure()),
            OwnerReviewKind::Email => self
                .execute_email_submitted(grant)
                .await
                .map_err(email_failure)?
                .into_owner_completion(&context, locale)
                .map_err(|_| owner_completion_projection_failure()),
            OwnerReviewKind::CreatorBatch => {
                let result = self
                    .execute_manual_creator_batch_submitted(grant)
                    .await
                    .map_err(creator_batch_failure)?;
                crate::creator_batch::into_owner_completion(result, &context)
                    .map_err(|_| owner_completion_projection_failure())
            }
        }
    }

    /// Close a Web singleton's submitted review after a safe execution error.
    /// This preserves its reviewed fields and never updates a mixed batch: its
    /// adapter owns the accumulated prefix and must supply the full terminal.
    ///
    /// # Errors
    /// Rejects a stale, non-submitted, or non-singleton result without mutation.
    pub fn fail_singleton_owner_execution(
        &self,
        binding: &ResultBinding,
        failure: &OwnerExecutionFailure,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        let mut snapshot = self.read_result(binding)?;
        if !matches!(snapshot.state, ResultState::Submitted { .. })
            || snapshot.batch.outcomes.len() != 1
        {
            return Err(ResultStoreError::InvalidState(
                "submitted_singleton_required",
            ));
        }
        let effect_may_have_occurred = failure.effect != OwnerExecutionEffect::KnownNoEffect;
        let summary = match (snapshot.locale, effect_may_have_occurred) {
            (PresentationLocale::SimplifiedChinese, true) => {
                "写入结果暂时无法核实。请保留原操作键并查询状态，不要重新发送。"
            }
            (PresentationLocale::SimplifiedChinese, false) => {
                "此次写入在发送前被拒绝。请核对输入和当前状态。"
            }
            (PresentationLocale::English, _) => failure.message,
        }
        .to_owned();
        snapshot.batch.outcomes[0].summary = summary.clone();
        snapshot.batch.outcomes[0].disposition = if effect_may_have_occurred {
            CallDisposition::Unknown {
                code: failure.code.into(),
            }
        } else {
            CallDisposition::Failed {
                code: failure.code.into(),
            }
        };
        snapshot.presentation.summary = Some(summary.clone());
        let terminal = if effect_may_have_occurred {
            TerminalResult::Unknown(ResultUnknown {
                code: failure.code.into(),
                summary,
            })
        } else {
            TerminalResult::Failed(ResultFailure {
                code: failure.code.into(),
                summary,
                retryable: false,
            })
        };
        self.transition_result_terminal(
            binding,
            ResultTerminalUpdate {
                terminal,
                batch: snapshot.batch,
                presentation: snapshot.presentation,
                continuation: None,
            },
        )
    }
}

fn owner_completion_projection_failure() -> OwnerExecutionFailure {
    OwnerExecutionFailure {
        code: "owner_completion_projection_failed",
        message: "The write result could not be displayed; inspect its durable status without resending.",
        effect: OwnerExecutionEffect::Unknown,
        dm_failure: None,
    }
}

fn creator_save_failure(error: CreatorSaveError) -> OwnerExecutionFailure {
    let effect = uncertain_if(!matches!(
        error,
        CreatorSaveError::Expired
            | CreatorSaveError::InvalidBatchSize
            | CreatorSaveError::ConflictingIntents
            | CreatorSaveError::Input { .. }
    ));
    OwnerExecutionFailure {
        code: "creator_save_execution_failed",
        message: "Creator save did not finish. Keep this result and check its state before preparing another save.",
        effect,
        dm_failure: None,
    }
}

/// Carry the Creator owner's own effect evidence outward unchanged. Provider
/// acceptance stays distinct from a genuinely unknown outcome so peers can
/// state that the mail was sent without claiming later recording completed.
fn email_failure(error: EmailSendFailure) -> OwnerExecutionFailure {
    let (code, message, effect) = match error.effect() {
        EmailSendEffect::NotSent => (
            "email_send_rejected",
            "The email was rejected before it reached the provider. Check the input and the current mailbox state.",
            OwnerExecutionEffect::KnownNoEffect,
        ),
        EmailSendEffect::Sent => (
            "email_send_recording_incomplete",
            "The provider accepted the email, but provider readback or local recording did not finish. Do not send again.",
            OwnerExecutionEffect::Applied,
        ),
        EmailSendEffect::Unknown => (
            "email_send_outcome_unknown",
            "The email outcome is unavailable. Keep this idempotency key; do not resend.",
            OwnerExecutionEffect::Unknown,
        ),
    };
    OwnerExecutionFailure {
        code,
        message,
        effect,
        dm_failure: None,
    }
}

/// The Creator owner reports a batch rejection as one opaque string, which
/// covers both a refusal that preceded the idempotency claim and a failure after
/// some rows had already committed. Nothing below this boundary distinguishes
/// the two, so the outcome stays unknown rather than being re-classified here: a
/// batch reported as "nothing written" that had in fact created Creators would
/// invite a second submission of the same people.
fn creator_batch_failure(_error: String) -> OwnerExecutionFailure {
    OwnerExecutionFailure {
        code: "creator_batch_outcome_unknown",
        message: "The Creator batch outcome is unavailable. Keep this idempotency key and read the Creators before submitting it again.",
        effect: OwnerExecutionEffect::Unknown,
        dm_failure: None,
    }
}

fn outreach_failure(error: OutreachError) -> OwnerExecutionFailure {
    use onecrew_creator_runtime::CreatorOutreachError;
    let effect = uncertain_if(matches!(
        &error,
        OutreachError::Creator(
            CreatorOutreachError::IntentConflict
                | CreatorOutreachError::ActionInProgress
                | CreatorOutreachError::Storage
                | CreatorOutreachError::CommitUnknown
                | CreatorOutreachError::RecordUnavailable
                | CreatorOutreachError::RetiredActionUnresolved
        ) | OutreachError::Result(_)
            | OutreachError::Encoding
            | OutreachError::ProviderStateCommitUnknown
    ));
    let dm_failure = match error {
        OutreachError::DirectMessage(failure) => Some(Box::new(failure)),
        _ => None,
    };
    if effect == OwnerExecutionEffect::Unknown {
        OwnerExecutionFailure {
            code: "outreach_execution_unavailable",
            message: "The write outcome is unavailable. Keep this intent key and inspect action status without resending.",
            effect,
            dm_failure,
        }
    } else {
        OwnerExecutionFailure {
            code: "outreach_preflight_rejected",
            message: "Outreach admission rejected the write before it was sent. Check the input and current connection or target state.",
            effect,
            dm_failure,
        }
    }
}

fn uncertain_if(may_have_occurred: bool) -> OwnerExecutionEffect {
    if may_have_occurred {
        OwnerExecutionEffect::Unknown
    } else {
        OwnerExecutionEffect::KnownNoEffect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outreach_execution_preserves_dm_stage_without_reinterpreting_text_or_effect() {
        for message in ["login 429 timeout", "sent successfully", "Provider failure"] {
            let mut diagnostic =
                crate::DmDiagnostic::provider("instagram", "ig_private_api", message);
            diagnostic.code = Some("challenge_required".into());
            diagnostic.http_status = Some(400);
            let dm_failure = DmFailure {
                stage: "capability",
                diagnostic,
            };
            let failure = outreach_failure(OutreachError::DirectMessage(dm_failure.clone()));
            assert_eq!(failure.dm_failure.as_deref(), Some(&dm_failure));
            assert_eq!(failure.code, "outreach_preflight_rejected");
            assert_eq!(failure.effect, OwnerExecutionEffect::KnownNoEffect);
        }
        for error in [
            OutreachError::ProviderStateCommitUnknown,
            OutreachError::Creator(
                onecrew_creator_runtime::CreatorOutreachError::RecordUnavailable,
            ),
            OutreachError::Creator(
                onecrew_creator_runtime::CreatorOutreachError::RetiredActionUnresolved,
            ),
        ] {
            let failure = outreach_failure(error);
            assert_eq!(failure.effect, OwnerExecutionEffect::Unknown);
            assert!(failure.dm_failure.is_none());
        }
    }

    #[test]
    fn email_failure_preserves_sent_separately_from_unknown() {
        for (failure, code, effect) in [
            (
                EmailSendFailure::not_sent("preflight rejected"),
                "email_send_rejected",
                OwnerExecutionEffect::KnownNoEffect,
            ),
            (
                EmailSendFailure::sent("provider accepted"),
                "email_send_recording_incomplete",
                OwnerExecutionEffect::Applied,
            ),
            (
                EmailSendFailure::unknown("provider outcome unavailable"),
                "email_send_outcome_unknown",
                OwnerExecutionEffect::Unknown,
            ),
        ] {
            let failure = email_failure(failure);
            assert_eq!(failure.code, code);
            assert_eq!(failure.effect, effect);
            assert!(failure.dm_failure.is_none());
        }
    }
}
