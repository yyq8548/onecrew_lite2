use super::*;

fn request_edit(
    store: &ResultMemoryStore,
    available: &ResultSnapshot,
    instruction: impl Into<String>,
) -> Result<ResultSnapshot, ResultStoreError> {
    store.request_edit(RequestResultEditInput {
        binding: available.binding.clone(),
        instruction: instruction.into(),
    })
}

fn claim_edit(store: &ResultMemoryStore, edited: &ResultSnapshot) -> ClaimedResultContinuation {
    store
        .claim_continuation(ClaimResultContinuationInput {
            binding: edited.binding.clone(),
            expected_continuation_revision: edited.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap()
}

#[test]
fn edit_request_is_one_atomic_claimable_transition_and_blocks_old_actions() {
    let store = ResultMemoryStore::new();
    let available = present(
        &store,
        store
            .mint_from_execute(confirmation_input(b"owner:must-not-run"))
            .unwrap(),
    );
    let instruction = "Use Spain and return three candidates.";
    let edited = request_edit(&store, &available, format!("  {instruction}  ")).unwrap();

    assert_eq!(edited.binding.revision, available.binding.revision + 1);
    assert!(edited.pending_action.is_none());
    assert!(matches!(
        &edited.state,
        ResultState::Resolved(ResultResolution { code, summary })
            if code == "edit_requested"
                && summary == "The user requested changes. The pending action was not executed."
    ));
    assert_eq!(edited.continuation.state, ResultContinuationState::Pending);
    assert_eq!(edited.continuation.continuation_revision, 2);

    let serialized = serde_json::to_string(&edited).unwrap();
    assert!(!serialized.contains(instruction));
    assert!(!serialized.contains("continuation_request"));
    assert_eq!(
        store.submit(&available.binding).unwrap_err(),
        ResultStoreError::RevisionConflict {
            current_revision: edited.binding.revision,
        }
    );
    assert_eq!(
        store.submit(&edited.binding).unwrap_err(),
        ResultStoreError::Replay("result_action_consumed")
    );
    assert_eq!(
        store.select(&edited.binding, choice_answers()).unwrap_err(),
        ResultStoreError::Replay("result_action_consumed")
    );

    let claimed = claim_edit(&store, &edited);
    assert_eq!(
        claimed.request.context,
        ResultContinuationContext::EditRequested
    );
    assert!(claimed.request.message.contains(instruction));
    assert!(!claimed.request.message.contains("  Use Spain"));
}

#[test]
fn invalid_or_capacity_rejected_edit_preserves_available_state_and_counters() {
    for (instruction, error) in [
        (
            "   ".to_owned(),
            ResultStoreError::InvalidInput("edit_instruction"),
        ),
        (
            "change\nplatform".to_owned(),
            ResultStoreError::InvalidInput("edit_instruction_control"),
        ),
        (
            "x".repeat(2_049),
            ResultStoreError::InvalidInput("edit_instruction"),
        ),
        (
            "更".repeat(683),
            ResultStoreError::InvalidInput("edit_instruction"),
        ),
    ] {
        let store = ResultMemoryStore::new();
        let available = present(
            &store,
            store
                .mint_from_execute(confirmation_input(b"owner:unchanged"))
                .unwrap(),
        );
        let before = store.cleanup().unwrap();
        assert_eq!(
            request_edit(&store, &available, instruction).unwrap_err(),
            error
        );
        assert_eq!(store.current(&available.binding).unwrap(), available);
        assert_eq!(store.cleanup().unwrap(), before);
    }

    let input = confirmation_input(b"owner:capacity");
    let initial_payload_bytes = result_payload_bytes(&input).unwrap();
    let store = ResultMemoryStore::with_limits(StoreLimits {
        max_results: 1,
        max_total_payload_bytes: initial_payload_bytes,
        available_ttl: AVAILABLE_TTL,
        terminal_ttl: TERMINAL_TTL,
    });
    let available = present(&store, store.mint_from_execute(input).unwrap());
    let before = store.cleanup().unwrap();
    assert_eq!(
        request_edit(&store, &available, "x".repeat(2_048)).unwrap_err(),
        ResultStoreError::Capacity("result_payload_limit")
    );
    assert_eq!(store.current(&available.binding).unwrap(), available);
    assert_eq!(store.cleanup().unwrap(), before);
}

#[test]
fn edit_continuation_delivery_clears_while_pause_and_unknown_keep_owner_semantics() {
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(choice_input()).unwrap());
    let edited = request_edit(&store, &available, "Prefer verified public email routes.").unwrap();
    let before_claim_bytes = store.cleanup().unwrap().payload_bytes;
    let claimed = claim_edit(&store, &edited);
    let paused = store
        .complete_continuation(CompleteResultContinuationInput {
            binding: claimed.binding.clone(),
            expected_continuation_revision: claimed.continuation_revision,
            lease_id: claimed.lease_id,
            outcome: ResultContinuationCompletionOutcome::Paused,
        })
        .unwrap();
    assert_eq!(paused.continuation.state, ResultContinuationState::Paused);
    assert_eq!(store.cleanup().unwrap().payload_bytes, before_claim_bytes);

    let retried = store
        .claim_continuation(ClaimResultContinuationInput {
            binding: paused.binding,
            expected_continuation_revision: paused.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::RetryPaused,
        })
        .unwrap();
    assert!(retried.request.message.contains("verified public email"));
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
    assert!(store.cleanup().unwrap().payload_bytes < before_claim_bytes);

    let second = present(&store, store.mint_from_execute(choice_input()).unwrap());
    let second = request_edit(&store, &second, "Use only platform-native profiles.").unwrap();
    let before_unknown_bytes = store.cleanup().unwrap().payload_bytes;
    let claimed = claim_edit(&store, &second);
    let unknown = store
        .complete_continuation(CompleteResultContinuationInput {
            binding: claimed.binding,
            expected_continuation_revision: claimed.continuation_revision,
            lease_id: claimed.lease_id,
            outcome: ResultContinuationCompletionOutcome::DeliveryUnknown,
        })
        .unwrap();
    assert_eq!(
        unknown.continuation.state,
        ResultContinuationState::DeliveryUnknown
    );
    assert_eq!(store.cleanup().unwrap().payload_bytes, before_unknown_bytes);
}

#[test]
fn edit_request_schema_is_closed_and_context_carries_no_instruction() {
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(choice_input()).unwrap());
    let input = RequestResultEditInput {
        binding: available.binding,
        instruction: "Use three candidates.".to_owned(),
    };
    let mut json = serde_json::to_value(&input).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<RequestResultEditInput>(json).is_err());

    let schema = serde_json::to_string(&schemars::schema_for!(RequestResultEditInput)).unwrap();
    assert!(schema.contains("\"additionalProperties\":false"));
    assert!(schema.contains("\"maxLength\":2048"));
    assert_eq!(
        serde_json::to_value(ResultContinuationContext::EditRequested).unwrap(),
        serde_json::json!({ "kind": "edit_requested" })
    );
}

#[test]
fn local_platform_exposes_the_same_edit_owner_transition() {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let origin = origin("edit-api");
    let mut input = confirmation_input(b"owner:edit-api");
    input.origin = origin.clone();
    let result_ref = platform.mint_execution_result(input).unwrap();
    let available = platform.present_result(&result_ref, &origin).unwrap();
    let edited = platform
        .request_result_edit(RequestResultEditInput {
            binding: available.binding,
            instruction: "Change the candidate count to three.".to_owned(),
        })
        .unwrap();
    assert!(matches!(
        edited.state,
        ResultState::Resolved(ResultResolution { ref code, .. }) if code == "edit_requested"
    ));
}
