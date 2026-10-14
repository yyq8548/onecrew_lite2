use super::shared::{CREATOR_SURFACES, SafetySpec, metadata, policy};
use crate::operation_registry::{
    descriptor::EffectClass,
    domains::RegistryOwners,
    execution::{BindingContext, ClassifiedOwnerFailure, PublicFailureCode, RecoveryDirective},
    registration::{RegistryBuildError, RegistryBuilder},
};
use onecrew_local_runtime::{
    CreatorSaveCallResult, CreatorSaveCandidateInput, CreatorSaveError, OwnerReviewContext,
    PrepareCreatorSaveInput,
};

pub(super) fn register_candidate_and_contact(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let platform = owners.platform.clone();
    builder.register_owner_review_operation::<CreatorSaveCandidateInput, CreatorSaveCallResult, PrepareBindingError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            confirmation: super::shared::ConfirmationSpec::Attended("Review the selected Creator rows and routes before saving."),
            readback: super::shared::ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] }),
            audit: crate::operation_registry::descriptor::AuditMode::OwnerRecorded,
            ..SafetySpec::local()
        }),
        move |context, input| {
            let platform = platform.clone();
            async move {
                let context = preparation_context(context)?;
                platform.prepare_creator_save_review(PrepareCreatorSaveInput { candidates: vec![input] }, context)
                    .await.map_err(PrepareBindingError::Owner)
            }
        },
        classify_prepare_failure,
    )?;
    let platform = owners.platform.clone();
    builder.register_owner_review_operation::<PrepareCreatorSaveInput, CreatorSaveCallResult, PrepareBindingError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_BATCH_SAVE, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            confirmation: super::shared::ConfirmationSpec::Attended("Review the selected Creator rows and routes before saving."),
            readback: super::shared::ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] }),
            audit: crate::operation_registry::descriptor::AuditMode::OwnerRecorded,
            ..SafetySpec::local()
        }),
        move |context, input| {
            let platform = platform.clone();
            async move {
                let context = preparation_context(context)?;
                platform.prepare_creator_save_review(input, context).await.map_err(PrepareBindingError::Owner)
            }
        },
        classify_prepare_failure,
    )?;
    super::contact_routes::register_contact_route_writes(builder, owners)
}

enum PrepareBindingError {
    MissingResultContext,
    Owner(CreatorSaveError),
}

fn preparation_context(context: BindingContext) -> Result<OwnerReviewContext, PrepareBindingError> {
    context
        .owner_review_context()
        .map_err(|_| PrepareBindingError::MissingResultContext)
}

fn classify_prepare_failure(error: PrepareBindingError) -> ClassifiedOwnerFailure {
    match error {
        PrepareBindingError::MissingResultContext => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("creator_save_result_context_required").expect("static code"),
            false,
            Some(RecoveryDirective::CorrectInput),
        ),
        PrepareBindingError::Owner(error) => classify_owner_prepare_failure(error),
    }
}

fn classify_owner_prepare_failure(error: CreatorSaveError) -> ClassifiedOwnerFailure {
    use onecrew_creator_runtime::CreatorSavePreparationErrorKind as Kind;
    let effect = error.effect();
    // Only these owner variants carry closed validation text or an already-safe
    // toolkit message. Raw repository strings never enter this projection.
    let (code, message, correct_input) = match error {
        error @ (CreatorSaveError::InvalidBatchSize
        | CreatorSaveError::ConflictingIntents
        | CreatorSaveError::Input { .. }) => {
            ("creator_save_input_invalid", error.to_string(), true)
        }
        error @ CreatorSaveError::IdentityMismatch => {
            ("creator_save_identity_mismatch", error.to_string(), true)
        }
        error @ CreatorSaveError::SourceMismatch => {
            ("creator_save_source_mismatch", error.to_string(), true)
        }
        CreatorSaveError::ProviderRead { code, message } => (
            "creator_save_provider_read_failed",
            format!("{code}: {message}"),
            false,
        ),
        CreatorSaveError::Preparation(error) => {
            let (code, correct_input) = match error.kind {
                Kind::Tags(_) | Kind::InvalidFields => {
                    ("creator_save_canonical_input_invalid", true)
                }
                Kind::InvalidSource => ("creator_save_source_invalid", true),
                Kind::Identity => ("creator_save_identity_prepare_failed", false),
                Kind::TelegramEligibility(_) => ("creator_save_telegram_eligibility_invalid", true),
                Kind::ContactNormalization => ("creator_save_contact_invalid", false),
                Kind::Storage => ("creator_save_storage_prepare_failed", false),
            };
            (code, error.to_string(), correct_input)
        }
        CreatorSaveError::PreparationDispatch(error) => (
            "creator_save_preparation_dispatch_failed",
            error.to_string(),
            false,
        ),
        CreatorSaveError::CreatorUnavailable => (
            "creator_save_owner_unavailable",
            "The Creator owner is unavailable for save preparation.".into(),
            false,
        ),
        CreatorSaveError::Result(_) => (
            "creator_save_review_failed",
            "The prepared Creator review could not be retained.".into(),
            false,
        ),
        error @ CreatorSaveError::Expired => {
            ("creator_save_review_expired", error.to_string(), true)
        }
        CreatorSaveError::WorkerUnavailable => (
            "creator_save_worker_unavailable",
            "The save preparation worker's result is unavailable.".into(),
            false,
        ),
    };
    let code = PublicFailureCode::parse(code).expect("static code");
    let recovery = correct_input.then_some(RecoveryDirective::CorrectInput);
    let failure = match effect {
        onecrew_local_runtime::CreatorSaveEffect::NoMutation => {
            ClassifiedOwnerFailure::known_no_effect(code, false, recovery)
        }
        // Earlier fresh reads may have committed credential health even when a
        // later preparation segment did not start. Preserve that uncertainty.
        onecrew_local_runtime::CreatorSaveEffect::LocalEffectsPossible => {
            ClassifiedOwnerFailure::unknown(code, recovery)
        }
    };
    failure.with_message(&format!(
        "{}. No Creator save was submitted.",
        message.trim_end_matches('.')
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_registry::execution::{
        ExecuteEffectState, project_classified_owner_failure,
    };
    use onecrew_creator_runtime::{
        CreatorRepositoryDispatchFailure, CreatorSavePreparationError,
        CreatorSavePreparationErrorKind, CreatorTagsError,
    };

    #[test]
    fn prepare_failures_preserve_safe_causes_and_effect_boundaries() {
        let invalid = project_classified_owner_failure(classify_owner_prepare_failure(
            CreatorSaveError::Input {
                candidate_index: 4,
                error: onecrew_local_runtime::CreatorSaveInputError::Tags(
                    CreatorTagsError::TooMany,
                ),
            },
        ));
        assert_eq!(invalid.code.as_str(), "creator_save_input_invalid");
        assert_eq!(invalid.effect, ExecuteEffectState::KnownNoEffect);
        assert_eq!(invalid.recovery, Some(RecoveryDirective::CorrectInput));
        let wire = serde_json::to_value(invalid).unwrap();
        assert_eq!(
            wire["message"],
            "Candidate 5: Creator supports at most 3 tags. No Creator save was submitted."
        );

        for (error, code, expected_message) in [
            (
                CreatorSaveError::ProviderRead {
                    code: "youtube_channel_read".into(),
                    message: "YouTube read quota is exhausted".into(),
                },
                "creator_save_provider_read_failed",
                "youtube_channel_read: YouTube read quota is exhausted",
            ),
            (
                CreatorSaveError::PreparationDispatch(CreatorRepositoryDispatchFailure::Overloaded),
                "creator_save_preparation_dispatch_failed",
                "Creator repository capacity is temporarily full",
            ),
        ] {
            let failure = project_classified_owner_failure(classify_owner_prepare_failure(error));
            assert_eq!(failure.code.as_str(), code);
            assert_eq!(failure.effect, ExecuteEffectState::Unknown);
            assert!(!failure.retry_safe);
            assert!(failure.recovery.is_none());
            let wire = serde_json::to_value(failure).unwrap();
            assert!(wire["message"].as_str().unwrap().contains(expected_message));
            assert!(
                !wire["message"]
                    .as_str()
                    .unwrap()
                    .contains("credential health may have changed")
            );
        }

        let storage = project_classified_owner_failure(classify_owner_prepare_failure(
            CreatorSaveError::Preparation(CreatorSavePreparationError {
                row_id: Some("private-row-sentinel".into()),
                kind: CreatorSavePreparationErrorKind::Storage,
            }),
        ));
        assert_eq!(storage.code.as_str(), "creator_save_storage_prepare_failed");
        assert_eq!(storage.effect, ExecuteEffectState::Unknown);
        assert!(storage.recovery.is_none());
        assert!(
            !serde_json::to_string(&storage)
                .unwrap()
                .contains("private-row-sentinel")
        );
    }
}
