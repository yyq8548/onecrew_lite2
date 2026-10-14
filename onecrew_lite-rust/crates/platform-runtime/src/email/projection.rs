use onecrew_creator_runtime::{EmailBatchActionResult, EmailBatchOutcome};

use super::EmailCallOutput;
use crate::{
    CallDisposition, ExecutedCallOutcome, OwnerCallCompletion, OwnerCallOutput, OwnerReviewContext,
    PresentationLocale, PresentationProjection, ResultContinuationContext,
    ResultContinuationOutcomeSummary, ResultContinuationRequest, ResultPresentationKind,
    ResultResolution, ResultStoreError, ResultUnknown, TerminalResult,
};

impl EmailCallOutput {
    /// Project one completed send for both peer adapters.
    ///
    /// This copy is deliberately not routed through the outreach copy catalog:
    /// that catalog states direct-message wording, and stamping it on an email
    /// result would describe the wrong action.
    ///
    /// # Errors
    /// Returns a result-store error if the owner presentation kind is invalid.
    pub(crate) fn into_owner_completion(
        self,
        context: &OwnerReviewContext,
        locale: PresentationLocale,
    ) -> Result<OwnerCallCompletion, ResultStoreError> {
        let (code, disposition, summary) = match &self {
            Self::Batch(result) => {
                let (accepted, rejected, unknown) = batch_counts(result);
                if unknown > 0 {
                    (
                        "email_batch_outcome_unknown",
                        CallDisposition::Unknown {
                            code: "email_batch_outcome_unknown".into(),
                        },
                        super::copy::batch_unknown(locale, accepted, rejected, unknown),
                    )
                } else {
                    (
                        "email_batch_completed",
                        CallDisposition::Succeeded,
                        super::copy::batch_completed(locale, accepted, rejected, self.replayed()),
                    )
                }
            }
            _ => (
                "email_sent",
                CallDisposition::Succeeded,
                super::copy::sent(locale, self.replayed()).to_owned(),
            ),
        };
        let succeeded = matches!(disposition, CallDisposition::Succeeded);
        let terminal = if succeeded {
            TerminalResult::Resolved(ResultResolution {
                code: code.into(),
                summary: summary.clone(),
            })
        } else {
            TerminalResult::Unknown(ResultUnknown {
                code: code.into(),
                summary: summary.clone(),
            })
        };
        let outcome = ExecutedCallOutcome {
            call_id: context.call_id().into(),
            operation_id: context.operation_id().into(),
            disposition: disposition.clone(),
            summary: summary.clone(),
            resource_refs: Vec::new(),
        };
        let continuation = ResultContinuationRequest {
            message: summary.clone(),
            requires_model_context: true,
            context: ResultContinuationContext::Execution {
                outcomes: vec![ResultContinuationOutcomeSummary {
                    call_id: outcome.call_id.clone(),
                    operation_id: outcome.operation_id.clone(),
                    disposition,
                    summary: outcome.summary.clone(),
                    resource_refs: Vec::new(),
                }],
            },
        };
        Ok(OwnerCallCompletion {
            outcome,
            presentation: PresentationProjection {
                kind: ResultPresentationKind::new("email_outcome")?,
                title: super::copy::title(locale).to_owned(),
                summary: Some(summary),
                sections: Vec::new(),
            },
            terminal,
            continuation: Some(continuation),
            output: OwnerCallOutput::Email(Box::new(self)),
            // Only a batch with no uncertain item may let a mixed peer batch
            // continue; an unknown outcome stops it so nothing is resent.
            continue_batch: succeeded,
        })
    }
}

fn batch_counts(result: &EmailBatchActionResult) -> (usize, usize, usize) {
    let mut accepted = 0;
    let mut rejected = 0;
    let mut unknown = 0;
    for row in &result.outcomes {
        match &row.outcome {
            EmailBatchOutcome::Accepted { .. } => accepted += 1,
            EmailBatchOutcome::Rejected { .. } => rejected += 1,
            EmailBatchOutcome::Unknown { .. } => unknown += 1,
        }
    }
    (accepted, rejected, unknown)
}

#[cfg(test)]
mod tests {
    use onecrew_creator_runtime::{
        AuditEvent, AuditEventId, EmailBatchActionResult, EmailBatchItemResult, EmailBatchOutcome,
        EmailCapability, EmailDirection, EmailId, EmailMessageView, EmailThread, EmailThreadFolder,
        EmailThreadId, MailboxDetail, MailboxProvider, MailboxRefId, ManualEmailActionResult,
        ManualEmailPreparation,
    };

    use super::*;
    use crate::ResultOrigin;

    const PROVIDER_MESSAGE_ID: &str = "provider-message-uuid-secret";

    fn context() -> OwnerReviewContext {
        OwnerReviewContext::new(
            ResultOrigin::new("email-projection-test").unwrap(),
            "call.email",
            "email.manual.send",
            None,
        )
        .unwrap()
    }

    fn manual_output(replayed: bool) -> EmailCallOutput {
        let mailbox_ref_id = MailboxRefId::new("mailbox-1");
        let thread_id = EmailThreadId::new("thread-1");
        let email_id = EmailId::new("email-1");
        EmailCallOutput::Manual(Box::new(ManualEmailActionResult {
            preparation: ManualEmailPreparation {
                mailbox: MailboxDetail {
                    mailbox_ref_id: mailbox_ref_id.clone(),
                    provider: MailboxProvider::Resend,
                    label: "Primary".into(),
                    email_address: "sender@example.test".into(),
                    display_name: None,
                    capabilities: vec![EmailCapability::Send],
                    available: true,
                    issues: Vec::new(),
                    created_at: 1,
                    updated_at: 1,
                },
                to: "recipient@example.test".into(),
                confirmation_required: true,
                confirmation_reason: "External write".into(),
            },
            thread: EmailThread {
                id: thread_id.clone(),
                mailbox_ref_id: mailbox_ref_id.clone(),
                creator_id: None,
                contact_route_id: None,
                provider_thread_key: "provider-thread-secret".into(),
                subject: "Subject".into(),
                unread_count: 0,
                folder: EmailThreadFolder::Sent,
                created_at: 1,
                updated_at: 1,
            },
            email: EmailMessageView {
                id: email_id.clone(),
                thread_id,
                provider_message_id: PROVIDER_MESSAGE_ID.into(),
                internet_message_id: None,
                direction: EmailDirection::Outbound,
                from: "sender@example.test".into(),
                to: vec!["recipient@example.test".into()],
                cc: Vec::new(),
                reply_to: Vec::new(),
                in_reply_to: None,
                references: Vec::new(),
                subject: "Subject".into(),
                text: Some("Body".into()),
                html: None,
                delivery_status: "queued".into(),
                occurred_at: 1,
            },
            audit_event: AuditEvent {
                id: AuditEventId::new("audit-1"),
                kind: "manual_email_sent".into(),
                creator_id: None,
                account_ref_id: None,
                resolved_target_id: None,
                mailbox_ref_id: Some(mailbox_ref_id),
                contact_locator_id: None,
                email_id: Some(email_id),
                payload: serde_json::json!({"provider_message_id": PROVIDER_MESSAGE_ID}),
                created_at: 1,
            },
            replayed,
        }))
    }

    fn visible_completion_json(completion: &OwnerCallCompletion) -> String {
        serde_json::to_string(&serde_json::json!({
            "outcome": &completion.outcome,
            "presentation": &completion.presentation,
            "terminal": &completion.terminal,
            "continuation": &completion.continuation,
        }))
        .unwrap()
    }

    #[test]
    fn single_completion_hides_provider_receipt_metadata_from_the_card() {
        let completion = manual_output(true)
            .into_owner_completion(&context(), PresentationLocale::SimplifiedChinese)
            .unwrap();
        let visible = visible_completion_json(&completion);

        for forbidden in [
            PROVIDER_MESSAGE_ID,
            "provider-message",
            "provider_message",
            "Provider message",
            "Last event",
            "queued",
            "Replayed",
        ] {
            assert!(
                !visible.contains(forbidden),
                "card leaked {forbidden}: {visible}"
            );
        }
        assert!(completion.outcome.resource_refs.is_empty());
        assert!(completion.presentation.sections.is_empty());
        assert_eq!(completion.presentation.title, "邮件结果");
        assert_eq!(
            completion.presentation.summary.as_deref(),
            Some("OneCrew 返回了此前已记录的发送结果；未再次发送邮件。")
        );
    }

    #[test]
    fn batch_completion_hides_internal_fingerprints_and_replay_state() {
        let output = EmailCallOutput::Batch(Box::new(EmailBatchActionResult {
            logical_fingerprint: "logical-fingerprint-secret".into(),
            provider_payload_fingerprint: "provider-fingerprint-secret".into(),
            chunks: Vec::new(),
            outcomes: vec![EmailBatchItemResult {
                input_index: 0,
                creator_id: None,
                contact_route_id: None,
                payload_fingerprint: "payload-fingerprint-secret".into(),
                outcome: EmailBatchOutcome::Accepted {
                    provider_message_id: PROVIDER_MESSAGE_ID.into(),
                },
            }],
            replayed: true,
        }));
        let completion = output
            .into_owner_completion(&context(), PresentationLocale::English)
            .unwrap();
        let visible = visible_completion_json(&completion);

        for forbidden in [
            PROVIDER_MESSAGE_ID,
            "logical-fingerprint-secret",
            "provider-fingerprint-secret",
            "payload-fingerprint-secret",
            "Logical fingerprint",
            "Replayed",
        ] {
            assert!(
                !visible.contains(forbidden),
                "card leaked {forbidden}: {visible}"
            );
        }
        assert!(completion.presentation.sections.is_empty());
        assert!(completion.outcome.summary.contains("1 accepted"));
    }

    #[test]
    fn unknown_batch_stops_continuation_and_keeps_localized_no_resend_guidance() {
        let output = EmailCallOutput::Batch(Box::new(EmailBatchActionResult {
            logical_fingerprint: "logical-fingerprint-secret".into(),
            provider_payload_fingerprint: "provider-fingerprint-secret".into(),
            chunks: Vec::new(),
            outcomes: vec![EmailBatchItemResult {
                input_index: 0,
                creator_id: None,
                contact_route_id: None,
                payload_fingerprint: "payload-fingerprint-secret".into(),
                outcome: EmailBatchOutcome::Unknown {
                    message: "provider outcome secret".into(),
                },
            }],
            replayed: false,
        }));
        let completion = output
            .into_owner_completion(&context(), PresentationLocale::SimplifiedChinese)
            .unwrap();
        let summary = completion.outcome.summary.as_str();

        assert!(matches!(
            completion.outcome.disposition,
            CallDisposition::Unknown { .. }
        ));
        assert!(matches!(completion.terminal, TerminalResult::Unknown(_)));
        assert!(!completion.continue_batch);
        assert!(summary.contains("结果不确定 1 封"));
        assert!(summary.contains("相同幂等键"));
        assert_eq!(completion.presentation.summary.as_deref(), Some(summary));
        let continuation = completion.continuation.as_ref().unwrap();
        assert_eq!(continuation.message, summary);
        let ResultContinuationContext::Execution { outcomes } = &continuation.context else {
            panic!("email completion keeps an execution continuation")
        };
        assert!(matches!(
            outcomes[0].disposition,
            CallDisposition::Unknown { .. }
        ));
        assert!(outcomes[0].resource_refs.is_empty());
        let visible = visible_completion_json(&completion);
        for forbidden in [
            PROVIDER_MESSAGE_ID,
            "provider outcome secret",
            "logical-fingerprint-secret",
            "provider-fingerprint-secret",
            "payload-fingerprint-secret",
        ] {
            assert!(
                !visible.contains(forbidden),
                "card leaked {forbidden}: {visible}"
            );
        }
    }
}
