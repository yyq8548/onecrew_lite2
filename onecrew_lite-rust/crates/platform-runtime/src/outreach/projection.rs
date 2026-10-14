use super::{OutreachCompletion, OutreachPhase, OutreachReceiptReference};
use crate::{
    CallDisposition, ExecutedCallOutcome, OwnerCallCompletion, OwnerCallOutput, OwnerReviewContext,
    PresentationField, PresentationLocale, PresentationProjection, PresentationSection,
    ResultContinuationContext, ResultContinuationOutcomeSummary, ResultContinuationRequest,
    ResultFailure, ResultPresentationKind, ResultResolution, ResultResourceKind, ResultResourceRef,
    ResultStoreError, ResultUnknown, TerminalResult,
};
use onecrew_creator_runtime::CreatorOutreachRecord;

impl OutreachCompletion {
    /// Project the business outcome once for both peer adapters. Only a verified,
    /// committed Creator result authorizes a mixed batch to continue.
    ///
    /// # Errors
    /// Returns a result-store error if the owner presentation kind is invalid.
    pub fn into_owner_completion(
        mut self,
        context: &OwnerReviewContext,
        locale: PresentationLocale,
    ) -> Result<OwnerCallCompletion, ResultStoreError> {
        self.guidance = super::copy::translated(locale, &self.guidance)?.into();
        let complete = self.phase == OutreachPhase::Completed && self.creator_recorded;
        let (code, disposition, terminal) = if complete {
            let code = "outreach_completed";
            (
                code,
                CallDisposition::Succeeded,
                TerminalResult::Resolved(ResultResolution {
                    code: code.into(),
                    summary: self.guidance.clone(),
                }),
            )
        } else if self.phase == OutreachPhase::Rejected {
            let code = "outreach_refused";
            (
                code,
                CallDisposition::Failed { code: code.into() },
                TerminalResult::Failed(ResultFailure {
                    code: code.into(),
                    summary: self.guidance.clone(),
                    retryable: false,
                }),
            )
        } else {
            let code = match self.phase {
                OutreachPhase::RemoteAccepted => "outreach_remote_accepted",
                OutreachPhase::RecordPending => "outreach_record_pending",
                _ => "outreach_remote_effect_unknown",
            };
            (
                code,
                CallDisposition::Unknown { code: code.into() },
                TerminalResult::Unknown(ResultUnknown {
                    code: code.into(),
                    summary: self.guidance.clone(),
                }),
            )
        };
        let mut resources = Vec::new();
        if let Some(receipt) = &self.receipt {
            let (id, label) = match receipt {
                OutreachReceiptReference::Relationship { target_external_id } => {
                    (target_external_id, "Relationship stable target")
                }
                OutreachReceiptReference::ContentReply { external_reply_id } => {
                    (external_reply_id, "Public reply receipt")
                }
                OutreachReceiptReference::Message {
                    external_message_id,
                    ..
                } => (external_message_id, "Message receipt"),
            };
            resources.push(ResultResourceRef {
                kind: ResultResourceKind::ProviderObject,
                id: id.clone(),
                label: Some(super::copy::text(locale, label).into()),
            });
        }
        if let Some(CreatorOutreachRecord::DirectMessage(record)) = &self.recorded {
            resources.push(ResultResourceRef {
                kind: ResultResourceKind::Conversation,
                id: record.conversation.id.as_str().into(),
                label: Some(super::copy::text(locale, "Creator conversation").into()),
            });
        }
        let outcome = ExecutedCallOutcome {
            call_id: context.call_id().into(),
            operation_id: context.operation_id().into(),
            disposition: disposition.clone(),
            summary: self.guidance.clone(),
            resource_refs: resources.clone(),
        };
        let continuation = ResultContinuationRequest {
            message: self.guidance.clone(),
            requires_model_context: true,
            context: ResultContinuationContext::Execution {
                outcomes: vec![ResultContinuationOutcomeSummary {
                    call_id: outcome.call_id.clone(),
                    operation_id: outcome.operation_id.clone(),
                    disposition,
                    summary: outcome.summary.clone(),
                    resource_refs: resources,
                }],
            },
        };
        let presentation = PresentationProjection {
            kind: ResultPresentationKind::new("outreach_outcome")?,
            title: super::copy::text(locale, "Outreach result").into(),
            summary: Some(self.guidance.clone()),
            sections: vec![PresentationSection::Fields {
                heading: None,
                fields: vec![
                    PresentationField {
                        label: super::copy::text(locale, "Status").into(),
                        value: code.into(),
                    },
                    PresentationField {
                        label: super::copy::text(locale, "Idempotency key").into(),
                        value: self.idempotency_key.clone(),
                    },
                    PresentationField {
                        label: super::copy::text(locale, "Intent fingerprint").into(),
                        value: self.intent_fingerprint.clone(),
                    },
                    PresentationField {
                        label: super::copy::text(locale, "Creator recorded").into(),
                        value: self.creator_recorded.to_string(),
                    },
                ],
            }],
        };
        Ok(OwnerCallCompletion {
            outcome,
            presentation,
            terminal,
            continuation: Some(continuation),
            output: OwnerCallOutput::Outreach(Box::new(self)),
            continue_batch: complete,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResultOrigin;

    #[test]
    fn only_committed_creator_completion_can_continue_a_batch() {
        let context = OwnerReviewContext::new(
            ResultOrigin::new("projection-test").unwrap(),
            "call",
            "onecrew.outreach.message.send",
            None,
        )
        .unwrap();
        for phase in [
            OutreachPhase::RemoteEffectUnknown,
            OutreachPhase::RemoteAccepted,
            OutreachPhase::RecordPending,
            OutreachPhase::Rejected,
        ] {
            let projected = OutreachCompletion { idempotency_key: "key".into(), intent_fingerprint: format!("sha256:v1:{}", "0".repeat(64)), phase, creator_recorded: false, replayed: false, receipt: None, recorded: None, guidance: "The remote effect is unknown. Keep this key and inspect or resume this exact intent; do not send with a new key.".into(), failure: None }.into_owner_completion(&context, PresentationLocale::English).unwrap();
            assert!(!projected.continue_batch);
            assert!(!matches!(
                projected.outcome.disposition,
                CallDisposition::Succeeded
            ));
        }
    }
}
