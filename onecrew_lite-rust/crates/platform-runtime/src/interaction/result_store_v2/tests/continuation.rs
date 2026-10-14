use std::{
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

use super::*;

fn terminal_confirmation(store: &ResultMemoryStore) -> ResultSnapshot {
    let available = present(
        store,
        store
            .mint_from_execute(confirmation_input(b"owner:continuation"))
            .unwrap(),
    );
    let submitted = store.submit(&available.binding).unwrap();
    let SubmittedOwnerAction {
        binding,
        owner_payload,
        ..
    } = submitted;
    assert_eq!(
        owner_payload.into_generic_bytes().unwrap(),
        b"owner:continuation"
    );
    let mut update = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: "Completed.".to_owned(),
    }));
    update.continuation = Some(execution_continuation_request());
    store.terminal(&binding, update).unwrap()
}

fn claim_pending(
    store: &ResultMemoryStore,
    terminal: &ResultSnapshot,
) -> ClaimedResultContinuation {
    store
        .claim_continuation(ClaimResultContinuationInput {
            binding: terminal.binding.clone(),
            expected_continuation_revision: terminal.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap()
}

#[test]
fn claim_is_one_shot_and_request_is_hidden_until_claim() {
    let store = ResultMemoryStore::new();
    let terminal = terminal_confirmation(&store);
    assert_eq!(
        terminal.continuation.state,
        ResultContinuationState::Pending
    );
    assert_eq!(terminal.continuation.continuation_revision, 2);
    let serialized = serde_json::to_string(&terminal).unwrap();
    assert!(!serialized.contains("confirmed operation finished"));
    assert!(!serialized.contains("continuation_request"));

    let claim_input = ClaimResultContinuationInput {
        binding: terminal.binding.clone(),
        expected_continuation_revision: terminal.continuation.continuation_revision,
        mode: ResultContinuationClaimMode::Pending,
    };
    let claimed = store.claim_continuation(claim_input.clone()).unwrap();
    assert_eq!(claimed.binding, terminal.binding);
    assert_eq!(claimed.continuation_revision, 3);
    assert!(claimed.lease_id.as_str().starts_with("rcl_"));
    assert!(claimed.dispatch_not_after_ms > claimed.dispatch_started_at_ms);
    assert_eq!(claimed.request, execution_continuation_request());
    assert_eq!(
        store.claim_continuation(claim_input).unwrap_err(),
        ResultStoreError::ContinuationRevisionConflict {
            current_revision: claimed.continuation_revision,
        }
    );
    let current = store.current(&claimed.binding).unwrap();
    assert_eq!(current.binding.revision, terminal.binding.revision);
    assert_eq!(
        current.continuation.state,
        ResultContinuationState::Dispatching
    );
    assert_eq!(
        current.continuation.dispatch_started_at_ms,
        Some(claimed.dispatch_started_at_ms)
    );
}

#[test]
fn concurrent_claim_race_has_exactly_one_dispatch_lease() {
    let store = ResultMemoryStore::new();
    let terminal = terminal_confirmation(&store);
    let input = ClaimResultContinuationInput {
        binding: terminal.binding,
        expected_continuation_revision: terminal.continuation.continuation_revision,
        mode: ResultContinuationClaimMode::Pending,
    };
    let barrier = Arc::new(Barrier::new(2));
    let first_store = store.clone();
    let first_input = input.clone();
    let first_barrier = barrier.clone();
    let first = std::thread::spawn(move || {
        first_barrier.wait();
        first_store.claim_continuation(first_input)
    });
    let second_store = store.clone();
    let second = std::thread::spawn(move || {
        barrier.wait();
        second_store.claim_continuation(input)
    });
    let outcomes = [first.join().unwrap(), second.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                Err(ResultStoreError::ContinuationRevisionConflict { .. })
            ))
            .count(),
        1
    );
}

#[test]
fn completion_requires_exact_revision_and_lease_and_unknown_cannot_retry() {
    let store = ResultMemoryStore::new();
    let terminal = terminal_confirmation(&store);
    let claimed = claim_pending(&store, &terminal);
    assert_eq!(
        store
            .complete_continuation(CompleteResultContinuationInput {
                binding: claimed.binding.clone(),
                expected_continuation_revision: claimed.continuation_revision - 1,
                lease_id: claimed.lease_id.clone(),
                outcome: ResultContinuationCompletionOutcome::Delivered,
            })
            .unwrap_err(),
        ResultStoreError::ContinuationRevisionConflict {
            current_revision: claimed.continuation_revision,
        }
    );
    assert_eq!(
        store
            .complete_continuation(CompleteResultContinuationInput {
                binding: claimed.binding.clone(),
                expected_continuation_revision: claimed.continuation_revision,
                lease_id: ResultContinuationLeaseId::mint(),
                outcome: ResultContinuationCompletionOutcome::Delivered,
            })
            .unwrap_err(),
        ResultStoreError::ContinuationLeaseMismatch
    );

    let completed = store
        .complete_continuation(CompleteResultContinuationInput {
            binding: claimed.binding.clone(),
            expected_continuation_revision: claimed.continuation_revision,
            lease_id: claimed.lease_id,
            outcome: ResultContinuationCompletionOutcome::DeliveryUnknown,
        })
        .unwrap();
    assert_eq!(
        completed.continuation.state,
        ResultContinuationState::DeliveryUnknown
    );
    assert_eq!(
        store
            .claim_continuation(ClaimResultContinuationInput {
                binding: completed.binding,
                expected_continuation_revision: completed.continuation.continuation_revision,
                mode: ResultContinuationClaimMode::RetryPaused,
            })
            .unwrap_err(),
        ResultStoreError::InvalidState("continuation_not_claimable")
    );
}

#[test]
fn paused_delivery_requires_explicit_retry_and_reuses_the_same_request() {
    let store = ResultMemoryStore::new();
    let terminal = terminal_confirmation(&store);
    let claimed = claim_pending(&store, &terminal);
    let paused = store
        .complete_continuation(CompleteResultContinuationInput {
            binding: claimed.binding.clone(),
            expected_continuation_revision: claimed.continuation_revision,
            lease_id: claimed.lease_id,
            outcome: ResultContinuationCompletionOutcome::Paused,
        })
        .unwrap();
    assert_eq!(paused.continuation.state, ResultContinuationState::Paused);
    assert_eq!(
        store
            .claim_continuation(ClaimResultContinuationInput {
                binding: paused.binding.clone(),
                expected_continuation_revision: paused.continuation.continuation_revision,
                mode: ResultContinuationClaimMode::Pending,
            })
            .unwrap_err(),
        ResultStoreError::InvalidState("continuation_retry_requires_explicit_mode")
    );
    let retried = store
        .claim_continuation(ClaimResultContinuationInput {
            binding: paused.binding,
            expected_continuation_revision: paused.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::RetryPaused,
        })
        .unwrap();
    assert_eq!(retried.request, execution_continuation_request());
    let delivered = store
        .complete_continuation(CompleteResultContinuationInput {
            binding: retried.binding,
            expected_continuation_revision: retried.continuation_revision,
            lease_id: retried.lease_id,
            outcome: ResultContinuationCompletionOutcome::Delivered,
        })
        .unwrap();
    assert_eq!(
        delivered.continuation.state,
        ResultContinuationState::Delivered
    );
}

#[test]
fn expired_dispatch_becomes_delivery_unknown_without_implicit_retry() {
    let store = ResultMemoryStore::new();
    let terminal = terminal_confirmation(&store);
    let claimed = claim_pending(&store, &terminal);
    let after_lease =
        Instant::now() + Duration::from_millis(RESULT_CONTINUATION_DISPATCH_LEASE_MS + 1);
    let recovered = store.current_at(&claimed.binding, after_lease).unwrap();
    assert_eq!(
        recovered.continuation.state,
        ResultContinuationState::DeliveryUnknown
    );
    assert_eq!(
        recovered.continuation.continuation_revision,
        claimed.continuation_revision + 1
    );
    assert_eq!(
        store
            .claim_continuation(ClaimResultContinuationInput {
                binding: recovered.binding,
                expected_continuation_revision: recovered.continuation.continuation_revision,
                mode: ResultContinuationClaimMode::RetryPaused,
            })
            .unwrap_err(),
        ResultStoreError::InvalidState("continuation_not_claimable")
    );
}

#[test]
fn terminal_continuation_is_atomic_and_direct_terminal_mint_has_none() {
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(choice_input()).unwrap());
    let submitted = store.select(&available.binding, choice_answers()).unwrap();
    let before = store.current(&submitted.binding).unwrap();
    let mut mismatched = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: "Completed.".to_owned(),
    }));
    let mut wrong_request = choice_continuation_request();
    if let ResultContinuationContext::Choice { answers } = &mut wrong_request.context {
        let ResultChoiceAnswer::Option { option_id, .. } = &mut answers[0] else {
            panic!("option fixture")
        };
        *option_id = "email".to_owned();
    }
    mismatched.continuation = Some(wrong_request);
    assert_eq!(
        store.terminal(&submitted.binding, mismatched).unwrap_err(),
        ResultStoreError::InvalidInput("choice_continuation_answers")
    );
    assert_eq!(store.current(&submitted.binding).unwrap(), before);

    let mut valid = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: "Completed.".to_owned(),
    }));
    valid.continuation = Some(choice_continuation_request());
    let terminal = store.terminal(&submitted.binding, valid).unwrap();
    assert_eq!(
        terminal.continuation.state,
        ResultContinuationState::Pending
    );

    let direct_ref = store
        .mint_from_execute(read_only_input(InitialResultState::Resolved(
            ResultResolution {
                code: "completed".to_owned(),
                summary: "Already completed.".to_owned(),
            },
        )))
        .unwrap();
    let direct = present(&store, direct_ref);
    assert_eq!(
        direct.continuation.state,
        ResultContinuationState::NotRequested
    );
    assert_eq!(direct.continuation.continuation_revision, 1);
    assert_eq!(
        store
            .claim_continuation(ClaimResultContinuationInput {
                binding: direct.binding,
                expected_continuation_revision: 1,
                mode: ResultContinuationClaimMode::Pending,
            })
            .unwrap_err(),
        ResultStoreError::InvalidState("continuation_not_claimable")
    );
}

#[test]
fn cancel_installs_only_cancelled_continuation_atomically() {
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(choice_input()).unwrap());
    let before_metrics = store.cleanup().unwrap();

    assert_eq!(
        store
            .cancel(CancelResultInput {
                binding: available.binding.clone(),
                continuation: Some(execution_continuation_request()),
            })
            .unwrap_err(),
        ResultStoreError::InvalidInput("cancel_continuation_context")
    );
    assert_eq!(store.current(&available.binding).unwrap(), available);

    assert_eq!(
        store
            .cancel(CancelResultInput {
                binding: available.binding.clone(),
                continuation: Some(ResultContinuationRequest {
                    requires_model_context: false,
                    message: "x".repeat(MAX_STRING_BYTES + 1),
                    context: ResultContinuationContext::Cancelled,
                }),
            })
            .unwrap_err(),
        ResultStoreError::InvalidInput("continuation_message")
    );
    assert_eq!(store.current(&available.binding).unwrap(), available);
    assert_eq!(store.cleanup().unwrap(), before_metrics);

    let request = ResultContinuationRequest {
        requires_model_context: false,
        message: "The user cancelled this card. Continue without the action.".to_owned(),
        context: ResultContinuationContext::Cancelled,
    };
    let cancelled = store
        .cancel(CancelResultInput {
            binding: available.binding,
            continuation: Some(request.clone()),
        })
        .unwrap();
    assert!(matches!(cancelled.state, ResultState::Resolved(_)));
    assert_eq!(
        cancelled.continuation.state,
        ResultContinuationState::Pending
    );
    let claimed = store
        .claim_continuation(ClaimResultContinuationInput {
            binding: cancelled.binding,
            expected_continuation_revision: cancelled.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    assert_eq!(claimed.request, request);
}
