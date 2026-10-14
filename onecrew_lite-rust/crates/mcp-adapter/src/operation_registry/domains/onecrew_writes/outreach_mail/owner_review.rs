//! Public policy and safe preparation failures for sealed outreach reviews.
use super::shared::{SafetySpec, policy, read_status};
use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, EffectClass, IdempotencyMode, ReadbackMode,
    },
    execution::{ClassifiedOwnerFailure, PublicFailureCode, RecoveryDirective},
    registration::OperationPolicy,
};
use onecrew_local_runtime::OutreachError;

pub(super) fn review_policy(prompt: &str) -> OperationPolicy {
    policy(
        EffectClass::ExternalWrite,
        ApprovalClass::AttendedPrompt,
        Some(ApprovalRequest::new(prompt).expect("static review prompt")),
        SafetySpec {
            confirmation_mode: ConfirmationMode::AttendedPrompt,
            confirmation_parameters: &[],
            idempotency_mode: IdempotencyMode::OwnerInputIdentity,
            idempotency_identity: &["/idempotency_key"],
            readback_mode: ReadbackMode::InlineAndFollowUp,
            readback_operations: const {
                &[onecrew_local_runtime::OperationId::ONECREW_OUTREACH_ACTION_STATUS.as_str()]
            },
            audit: AuditMode::OwnerRecorded,
            warning_output: &[],
            recovery_output: &[],
        },
    )
}

pub(in crate::operation_registry::domains::onecrew_writes) fn classify_prepare_failure(
    error: OutreachError,
) -> ClassifiedOwnerFailure {
    use onecrew_creator_runtime::CreatorOutreachError;
    let message = error.to_string();
    if let OutreachError::DirectMessage(failure) = error {
        return ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("direct_message_failed").expect("static direct message code"),
            false,
            None,
        )
        .with_message(failure.diagnostic.message)
        .with_dm_failure(failure);
    }
    if matches!(&error, OutreachError::ProviderRead(_)) {
        return ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("outreach_provider_read_failed")
                .expect("static provider read code"),
            false,
            None,
        )
        .with_message(&message);
    }
    if matches!(&error, OutreachError::ProviderStateCommitUnknown) {
        return ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("provider_state_commit_unknown")
                .expect("static provider state code"),
            Some(read_status("onecrew_connection_list")),
        )
        .with_message(&message);
    }
    if let OutreachError::Target(error) = &error {
        use onecrew_creator_runtime::CreatorTargetResolutionError;
        return match error {
            CreatorTargetResolutionError::CommitUnknown => ClassifiedOwnerFailure::unknown(
                PublicFailureCode::parse("contact_target_commit_unknown")
                    .expect("static target code"),
                Some(read_status(
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str(),
                )),
            ),
            CreatorTargetResolutionError::ContextUnavailable => {
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("contact_target_context_unavailable")
                        .expect("static target code"),
                    false,
                    Some(RecoveryDirective::CorrectInput),
                )
            }
            CreatorTargetResolutionError::InvalidEvidence => {
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("contact_target_evidence_invalid")
                        .expect("static target code"),
                    false,
                    Some(RecoveryDirective::CorrectInput),
                )
            }
            CreatorTargetResolutionError::Storage => ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse("contact_target_storage_unavailable")
                    .expect("static target code"),
                false,
                Some(read_status(
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str(),
                )),
            ),
        }
        .with_message(&message);
    }
    let (code, unresolved) = match &error {
        OutreachError::WrongPlatform => ("conversation_platform_mismatch", false),
        OutreachError::UnfollowedConfirmationRequired => {
            ("unfollowed_recipient_confirmation_required", false)
        }
        OutreachError::IdentityMismatch => ("outreach_identity_mismatch", false),
        OutreachError::CapabilityUnavailable => ("outreach_capability_unavailable", false),
        OutreachError::Creator(CreatorOutreachError::IntentConflict) => {
            ("outreach_intent_conflict", false)
        }
        OutreachError::Creator(CreatorOutreachError::RetiredActionUnresolved) => {
            ("retired_action_unresolved", true)
        }
        OutreachError::Creator(
            CreatorOutreachError::CommitUnknown
            | CreatorOutreachError::ActionInProgress
            | CreatorOutreachError::RecordUnavailable,
        ) => ("outreach_prior_effect_unresolved", true),
        _ => ("outreach_preparation_failed", false),
    };
    let code = PublicFailureCode::parse(code).expect("static outreach code");
    if unresolved {
        ClassifiedOwnerFailure::unknown(
            code,
            Some(read_status(
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_ACTION_STATUS.as_str(),
            )),
        )
    } else {
        ClassifiedOwnerFailure::known_no_effect(code, false, Some(RecoveryDirective::CorrectInput))
    }
    .with_message(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dm_preparation_keeps_native_evidence_without_inventing_recovery() {
        for message in [
            "login 429 timeout",
            "sent successfully",
            "Unrecognized provider failure",
        ] {
            let mut diagnostic =
                onecrew_local_runtime::DmDiagnostic::provider("x", "x_chat", message);
            diagnostic.code = Some("451".into());
            diagnostic.http_status = Some(403);
            let failure = onecrew_local_runtime::DmFailure {
                stage: "capability",
                diagnostic,
            };
            let classified =
                classify_prepare_failure(OutreachError::DirectMessage(failure.clone()));
            let wire =
                crate::operation_registry::execution::project_classified_owner_failure(classified);
            assert_eq!(wire.dm_failure.as_deref(), Some(&failure));
            assert_eq!(
                wire.effect,
                crate::operation_registry::execution::ExecuteEffectState::KnownNoEffect
            );
            assert!(!wire.retry_safe);
            assert_eq!(wire.recovery, None);
            let json = serde_json::to_value(&wire).unwrap();
            assert_eq!(json["dm_failure"]["stage"], "capability");
            assert_eq!(json["dm_failure"]["diagnostic"]["code"], "451");
        }
        let wire = crate::operation_registry::execution::project_classified_owner_failure(
            classify_prepare_failure(OutreachError::ProviderRead("target")),
        );
        assert_eq!(wire.recovery, None);
        assert!(
            serde_json::to_value(wire)
                .unwrap()
                .get("dm_failure")
                .is_none()
        );
    }

    #[test]
    fn provider_state_commit_unknown_preserves_uncertainty_and_connection_readback() {
        let error = OutreachError::ProviderStateCommitUnknown;
        let message = error.to_string();
        let classified = classify_prepare_failure(error);
        assert_eq!(
            classified,
            ClassifiedOwnerFailure::unknown(
                PublicFailureCode::parse("provider_state_commit_unknown").unwrap(),
                Some(read_status("onecrew_connection_list")),
            )
            .with_message(&message)
        );
        let wire =
            crate::operation_registry::execution::project_classified_owner_failure(classified);
        assert_eq!(
            wire.effect,
            crate::operation_registry::execution::ExecuteEffectState::Unknown
        );
        assert!(!wire.retry_safe);
    }

    #[test]
    fn contact_target_commit_unknown_preserves_effect_and_uses_creator_readback() {
        let error = onecrew_creator_runtime::CreatorTargetResolutionError::CommitUnknown;
        assert_eq!(
            classify_prepare_failure(OutreachError::Target(error)),
            ClassifiedOwnerFailure::unknown(
                PublicFailureCode::parse("contact_target_commit_unknown").unwrap(),
                Some(read_status(
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()
                ))
            )
            .with_message(&error.to_string())
        );
        for error in [
            onecrew_creator_runtime::CreatorTargetResolutionError::ContextUnavailable,
            onecrew_creator_runtime::CreatorTargetResolutionError::InvalidEvidence,
            onecrew_creator_runtime::CreatorTargetResolutionError::Storage,
        ] {
            let failure = classify_prepare_failure(OutreachError::Target(error));
            let projection =
                crate::operation_registry::execution::project_classified_owner_failure(failure);
            assert_eq!(
                projection.effect,
                crate::operation_registry::execution::ExecuteEffectState::KnownNoEffect
            );
        }
    }
}
