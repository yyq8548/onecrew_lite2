use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use serde_json::json;

use crate::operation_registry::execution::{
    ClassifiedOwnerFailure, ExecuteEffectState, ExecuteFailureKind, ExecuteFailurePhase,
    ExecutionContext, OwnerEffectState, PublicFailureCode, RecoveryDirective, dispatch_prepared,
    prepare_invocation, project_execute_failure,
};
use crate::operation_registry::{
    descriptor::{ConnectionRequirement, EffectClass, EntryId, SurfaceId},
    registration::{OperationPolicy, RegistryBuilder},
};

use super::super::test_support::{TestInput, TestOutput, TestOwnerError, metadata, operation};

#[test]
fn prepare_rejects_invalid_invocation_without_calling_owner() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.prepare-invalid",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.prepare-invalid"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            move |_context, input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput {
                        accepted: !input.name.is_empty(),
                    })
                }
            },
            |_| {
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("test_rejected").unwrap(),
                    false,
                    None,
                )
            },
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let id = EntryId::parse("onecrew.test.prepare-invalid").unwrap();
    let entry = operation(&registry, &id);
    let failure = prepare_invocation(
        entry,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "prepare-invalid".to_owned(),
            connection: None,
            confirmed: false,
        },
        entry.input.revision(),
        json!({"name": 7}),
    )
    .err()
    .expect("invalid input must fail during prepare");

    assert_eq!(failure.phase, ExecuteFailurePhase::BeforeInvocation);
    assert_eq!(failure.kind, ExecuteFailureKind::InputSchemaRejected);
    assert!(failure.retry_safe);
    assert!(!failure.effect_may_have_occurred);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn prepared_dispatch_preserves_success_and_owner_failure_semantics() {
    let success_calls = Arc::new(AtomicUsize::new(0));
    let failure_calls = Arc::new(AtomicUsize::new(0));
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.prepare-failure",
            "onecrew.test.prepare-success",
        ]));

    let success_counter = Arc::clone(&success_calls);
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.prepare-success"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            move |_context, input| {
                let success_counter = Arc::clone(&success_counter);
                async move {
                    success_counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput {
                        accepted: input.name == "valid",
                    })
                }
            },
            |_| unreachable!("success binding cannot return an owner failure"),
        )
        .unwrap();

    let failure_counter = Arc::clone(&failure_calls);
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.prepare-failure"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            move |_context, _input| {
                let failure_counter = Arc::clone(&failure_counter);
                async move {
                    failure_counter.fetch_add(1, Ordering::SeqCst);
                    Err::<TestOutput, _>(TestOwnerError)
                }
            },
            |_| {
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("test_rejected").unwrap(),
                    true,
                    Some(RecoveryDirective::CorrectInput),
                )
                .with_provider_status(Some(409))
            },
        )
        .unwrap();

    let registry = builder.build().unwrap();
    let success_id = EntryId::parse("onecrew.test.prepare-success").unwrap();
    let failure_id = EntryId::parse("onecrew.test.prepare-failure").unwrap();
    let success_entry = operation(&registry, &success_id);
    let failure_entry = operation(&registry, &failure_id);
    let success = prepare_invocation(
        success_entry,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "prepare-success".to_owned(),
            connection: None,
            confirmed: false,
        },
        success_entry.input.revision(),
        json!({"name": "valid"}),
    )
    .unwrap();
    let rejected = prepare_invocation(
        failure_entry,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "prepare-failure".to_owned(),
            connection: None,
            confirmed: false,
        },
        failure_entry.input.revision(),
        json!({"name": "valid"}),
    )
    .unwrap();

    assert_eq!(success_calls.load(Ordering::SeqCst), 0);
    assert_eq!(failure_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        dispatch_prepared(success).await.unwrap(),
        json!({"accepted": true})
    );
    let failure = dispatch_prepared(rejected).await.unwrap_err();

    assert_eq!(success_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failure_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation);
    assert!(failure.retry_safe);
    assert!(!failure.effect_may_have_occurred);
    assert!(matches!(
        failure.kind,
        ExecuteFailureKind::OwnerRejected {
            ref code,
            effect: OwnerEffectState::KnownNoEffect,
            recovery: Some(RecoveryDirective::CorrectInput),
            provider_status: Some(409),
            provider_evidence: None,
            message: None,
            dm_failure: None,
        } if code.as_str() == "test_rejected"
    ));
    let projected = project_execute_failure(&failure);
    assert_eq!(projected.code.as_str(), "test_rejected");
    assert_eq!(projected.effect, ExecuteEffectState::KnownNoEffect);
    assert_eq!(projected.recovery, Some(RecoveryDirective::CorrectInput));
    assert_eq!(projected.provider_status, Some(409));
    assert!(projected.dm_failure.is_none());
}
