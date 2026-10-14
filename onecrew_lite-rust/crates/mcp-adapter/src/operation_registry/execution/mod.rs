//! Typed execution and public failure projection facade.
mod binding;
mod blocking;
mod failure;
mod invocation;

pub(crate) use binding::{
    BindingContext, BindingOutput, BindingOutputClass, ErasedBinding, ExecutionDeclaration,
    SearchScopeCompiler, SearchScopePurpose, typed_binding, typed_owner_review_binding,
};
pub(crate) use blocking::{BlockingTaskFailure, run_blocking};
#[cfg(test)]
pub(crate) use failure::OwnerEffectState;
pub(crate) use failure::{
    BoundedPublicValue, ClassifiedOwnerFailure, ExecuteEffectState, ExecuteFailure,
    ExecuteFailureKind, ExecuteFailurePhase, ExecuteFailurePresentation, PublicFailureCode,
    RecoveryDirective, project_classified_owner_failure, project_execute_failure,
};
#[cfg(test)]
pub(crate) use invocation::dispatch_prepared;
pub(crate) use invocation::{
    ExecutionContext, PreparedInvocation, dispatch_prepared_output, prepare_invocation,
};

#[cfg(test)]
mod test_support;

/// Preserve the typed owner classifier while distinguishing admission from a lost started result.
pub(crate) fn classify_repository_call<E>(
    owner: impl Fn(E) -> ClassifiedOwnerFailure,
) -> impl Fn(onecrew_creator_runtime::CreatorRepositoryCallError<E>) -> ClassifiedOwnerFailure {
    move |error| match error {
        onecrew_creator_runtime::CreatorRepositoryCallError::Owner(error) => owner(error),
        onecrew_creator_runtime::CreatorRepositoryCallError::Dispatch(error) => {
            let code = PublicFailureCode::parse("creator_repository_dispatch_failed")
                .expect("static failure code");
            if error.known_no_effect() {
                ClassifiedOwnerFailure::known_no_effect(code, true, None)
            } else {
                ClassifiedOwnerFailure::unknown(code, None)
            }
        }
    }
}

#[cfg(test)]
mod repository_failure_tests {
    use super::*;
    use onecrew_creator_runtime::{CreatorRepositoryCallError, CreatorRepositoryDispatchFailure};
    #[test]
    fn repository_admission_and_started_work_preserve_effect_classification() {
        let classify = classify_repository_call(|_: ()| {
            ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse("owner_rejected").unwrap(),
                false,
                None,
            )
        });
        for error in [
            CreatorRepositoryDispatchFailure::Closed,
            CreatorRepositoryDispatchFailure::Overloaded,
            CreatorRepositoryDispatchFailure::CancelledBeforeDispatch,
        ] {
            assert_eq!(
                classify(CreatorRepositoryCallError::Dispatch(error)),
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("creator_repository_dispatch_failed").unwrap(),
                    true,
                    None
                )
            );
        }
        assert_eq!(
            classify(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::OutcomeUnknown
            )),
            ClassifiedOwnerFailure::unknown(
                PublicFailureCode::parse("creator_repository_dispatch_failed").unwrap(),
                None
            )
        );
        assert_eq!(
            classify(CreatorRepositoryCallError::Owner(())),
            ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse("owner_rejected").unwrap(),
                false,
                None
            )
        );
    }
}
