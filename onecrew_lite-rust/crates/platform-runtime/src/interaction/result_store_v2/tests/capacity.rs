use super::*;

#[test]
fn capacity_never_evicts_submitted_or_unknown_results() {
    let limits = StoreLimits {
        max_results: 1,
        max_total_payload_bytes: MAX_PROJECTION_BYTES * 2,
        available_ttl: Duration::from_secs(1),
        terminal_ttl: Duration::from_secs(1),
    };
    let submitted_store = ResultMemoryStore::with_limits(limits);
    let available = present(
        &submitted_store,
        submitted_store
            .mint_from_execute(confirmation_input(b"owner:confirm"))
            .unwrap(),
    );
    let submitted = submitted_store.submit(&available.binding).unwrap();
    let later = Instant::now() + Duration::from_secs(60);
    let metrics = submitted_store.cleanup_at(later).unwrap();
    assert_eq!(metrics.removed, 0);
    assert!(matches!(
        submitted_store.current(&submitted.binding).unwrap().state,
        ResultState::Submitted { .. }
    ));
    assert_eq!(
        submitted_store
            .mint_from_execute(read_only_input(InitialResultState::Available))
            .unwrap_err(),
        ResultStoreError::Capacity("result_row_limit")
    );

    let unknown_store = ResultMemoryStore::with_limits(limits);
    let unknown_ref = unknown_store
        .mint_from_execute(read_only_input(InitialResultState::Unknown(
            ResultUnknown {
                code: "unknown".to_owned(),
                summary: "Effect unknown.".to_owned(),
            },
        )))
        .unwrap();
    assert_eq!(unknown_store.cleanup_at(later).unwrap().removed, 0);
    let unknown = present(&unknown_store, unknown_ref);
    assert!(matches!(
        unknown_store.read(&unknown.binding).unwrap().state,
        ResultState::Unknown(_)
    ));
    assert_eq!(
        unknown_store
            .mint_from_execute(read_only_input(InitialResultState::Available))
            .unwrap_err(),
        ResultStoreError::Capacity("result_row_limit")
    );
}

#[test]
fn total_payload_cap_rejects_new_results_without_partial_insert() {
    let input = read_only_input(InitialResultState::Available);
    let payload_bytes = result_payload_bytes(&input).unwrap();
    let store = ResultMemoryStore::with_limits(StoreLimits {
        max_results: 2,
        max_total_payload_bytes: payload_bytes - 1,
        available_ttl: AVAILABLE_TTL,
        terminal_ttl: TERMINAL_TTL,
    });

    assert_eq!(
        store.mint_from_execute(input).unwrap_err(),
        ResultStoreError::Capacity("result_payload_limit")
    );
    let metrics = store.cleanup().unwrap();
    assert_eq!(metrics.active, 0);
    assert_eq!(metrics.payload_bytes, 0);
}

#[test]
fn expired_evictable_results_release_capacity_but_restart_remains_absent() {
    let limits = StoreLimits {
        max_results: 1,
        max_total_payload_bytes: MAX_PROJECTION_BYTES * 2,
        available_ttl: Duration::from_secs(1),
        terminal_ttl: Duration::ZERO,
    };
    let store = ResultMemoryStore::with_limits(limits);
    let expired_ref = store
        .mint_from_execute(read_only_input(InitialResultState::Available))
        .unwrap();
    let expired = present(&store, expired_ref.clone());
    let later = Instant::now() + Duration::from_secs(60);
    assert_eq!(store.cleanup_at(later).unwrap().removed, 1);
    assert_eq!(
        store.read(&expired.binding).unwrap_err(),
        ResultStoreError::Absent
    );
    let replacement_ref = store
        .mint_from_execute(read_only_input(InitialResultState::Available))
        .unwrap();
    assert_ne!(expired_ref, replacement_ref);
    assert_eq!(store.cleanup().unwrap().active, 1);
}

#[test]
fn terminal_capacity_rejection_preserves_submitted_state_and_counters() {
    let input = confirmation_input(b"owner:capacity");
    let initial_payload_bytes = result_payload_bytes(&input).unwrap();
    let store = ResultMemoryStore::with_limits(StoreLimits {
        max_results: 2,
        max_total_payload_bytes: initial_payload_bytes + 32,
        available_ttl: AVAILABLE_TTL,
        terminal_ttl: TERMINAL_TTL,
    });
    let available = present(&store, store.mint_from_execute(input).unwrap());
    let submitted = store.submit(&available.binding).unwrap();
    let before_snapshot = store.current(&submitted.binding).unwrap();
    let before_metrics = store.cleanup().unwrap();

    let mut update = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: "Completed.".to_owned(),
    }));
    update.presentation.title = "x".repeat(4 * 1024);
    assert!(terminal_update_payload_bytes(&update).unwrap() > before_metrics.payload_bytes + 32);
    assert_eq!(
        store.terminal(&submitted.binding, update).unwrap_err(),
        ResultStoreError::Capacity("result_payload_limit")
    );
    assert_eq!(store.current(&submitted.binding).unwrap(), before_snapshot);
    assert_eq!(store.cleanup().unwrap(), before_metrics);
}

#[test]
fn submitted_unknown_terminal_result_is_never_ttl_evicted() {
    let store = ResultMemoryStore::with_limits(StoreLimits {
        max_results: 2,
        max_total_payload_bytes: MAX_PROJECTION_BYTES * 2,
        available_ttl: Duration::from_secs(1),
        terminal_ttl: Duration::ZERO,
    });
    let available = present(
        &store,
        store
            .mint_from_execute(confirmation_input(b"owner:unknown"))
            .unwrap(),
    );
    let submitted = store.submit(&available.binding).unwrap();
    let unknown = store
        .terminal(
            &submitted.binding,
            terminal_update(TerminalResult::Unknown(ResultUnknown {
                code: "effect_unknown".to_owned(),
                summary: "The final effect could not be verified.".to_owned(),
            })),
        )
        .unwrap();
    let later = Instant::now() + Duration::from_secs(60);
    assert_eq!(store.cleanup_at(later).unwrap().removed, 0);
    assert!(matches!(
        store.current(&unknown.binding).unwrap().state,
        ResultState::Unknown(_)
    ));
}

#[test]
fn cancel_continuation_capacity_rejection_has_no_partial_state_or_counter_change() {
    let input = confirmation_input(b"owner:cancel-capacity");
    let initial_payload_bytes = result_payload_bytes(&input).unwrap();
    let store = ResultMemoryStore::with_limits(StoreLimits {
        max_results: 2,
        max_total_payload_bytes: initial_payload_bytes + 32,
        available_ttl: AVAILABLE_TTL,
        terminal_ttl: TERMINAL_TTL,
    });
    let available = present(&store, store.mint_from_execute(input).unwrap());
    let before_metrics = store.cleanup().unwrap();
    assert_eq!(
        store
            .cancel(CancelResultInput {
                binding: available.binding.clone(),
                continuation: Some(ResultContinuationRequest {
                    requires_model_context: false,
                    message: "x".repeat(4 * 1024),
                    context: ResultContinuationContext::Cancelled,
                }),
            })
            .unwrap_err(),
        ResultStoreError::Capacity("result_payload_limit")
    );
    assert_eq!(store.current(&available.binding).unwrap(), available);
    assert_eq!(store.cleanup().unwrap(), before_metrics);
}
