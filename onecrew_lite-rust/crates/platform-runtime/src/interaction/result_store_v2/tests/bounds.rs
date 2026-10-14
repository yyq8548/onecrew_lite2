use super::*;

#[test]
fn result_refs_are_strict_and_minted_only_by_execute_entry() {
    let store = ResultMemoryStore::new();
    let first = store
        .mint_from_execute(read_only_input(InitialResultState::Available))
        .unwrap();
    let second = store
        .mint_from_execute(read_only_input(InitialResultState::Available))
        .unwrap();

    assert!(first.as_str().starts_with("rr_"));
    assert_eq!(first.as_str().parse::<ResultRef>().unwrap(), first);
    assert_eq!(first.to_string().parse::<ResultRef>().unwrap(), first);
    assert_ne!(first, second);
    assert!(
        "pr_00000000000000000000000000000000"
            .parse::<ResultRef>()
            .is_err()
    );
    assert!("rr_not-a-uuid".parse::<ResultRef>().is_err());
    assert!("origin with spaces".parse::<ResultOrigin>().is_err());
    assert!("rv_not-a-uuid".parse::<ResultViewIdentity>().is_err());
    assert!("sha256:ABC".parse::<ResultFingerprint>().is_err());
    assert!(
        "kind with spaces"
            .parse::<ResultPresentationKind>()
            .is_err()
    );
    assert!(
        "x".repeat(MAX_PRESENTATION_KIND_BYTES + 1)
            .parse::<ResultPresentationKind>()
            .is_err()
    );
    assert!(
        "rcl_not-a-uuid"
            .parse::<ResultContinuationLeaseId>()
            .is_err()
    );
}

#[test]
fn wire_schemas_are_closed_and_publish_runtime_bounds() {
    let store = ResultMemoryStore::new();
    let result_ref = store
        .mint_from_execute(read_only_input(InitialResultState::Available))
        .unwrap();
    let snapshot = present(&store, result_ref);
    let mut binding_json = serde_json::to_value(&snapshot.binding).unwrap();
    binding_json
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<ResultBinding>(binding_json).is_err());
    let mut zero_revision = serde_json::to_value(&snapshot.binding).unwrap();
    zero_revision
        .as_object_mut()
        .unwrap()
        .insert("revision".to_owned(), serde_json::Value::from(0));
    assert!(serde_json::from_value::<ResultBinding>(zero_revision).is_err());
    assert!(
        serde_json::from_value::<ExecutedBatch>(serde_json::json!({
            "outcomes": [],
            "unexpected": true
        }))
        .is_err()
    );

    let schema = serde_json::to_string(&schemars::schema_for!(ResultSnapshot)).unwrap();
    assert!(schema.contains("\"additionalProperties\":false"));
    assert!(schema.contains("\"maxItems\":1000"));
    assert!(schema.contains("\"maxItems\":32"));
    assert!(schema.contains("^rv_[0-9a-f]{32}$"));
    assert!(schema.contains("^sha256:[0-9a-f]{64}$"));
    assert!(schema.contains("^[A-Za-z0-9._:-]{1,128}$"));
    let mut projection_json = serde_json::to_value(projection()).unwrap();
    projection_json.as_object_mut().unwrap().remove("kind");
    assert!(serde_json::from_value::<PresentationProjection>(projection_json).is_err());

    let update = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: "Completed.".to_owned(),
    }));
    let mut update_json = serde_json::to_value(&update).unwrap();
    update_json
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<ResultTerminalUpdate>(update_json).is_err());
    let update_schema =
        serde_json::to_string(&schemars::schema_for!(ResultTerminalUpdate)).unwrap();
    assert!(update_schema.contains("\"additionalProperties\":false"));
    assert!(update_schema.contains("\"maxItems\":8"));

    let cancel = CancelResultInput {
        binding: snapshot.binding.clone(),
        continuation: None,
    };
    let mut cancel_json = serde_json::to_value(&cancel).unwrap();
    cancel_json
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<CancelResultInput>(cancel_json).is_err());

    let claim = ClaimResultContinuationInput {
        binding: snapshot.binding,
        expected_continuation_revision: 1,
        mode: ResultContinuationClaimMode::Pending,
    };
    let mut claim_json = serde_json::to_value(&claim).unwrap();
    claim_json
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<ClaimResultContinuationInput>(claim_json).is_err());
    let mut zero_continuation_revision = serde_json::to_value(&claim).unwrap();
    zero_continuation_revision.as_object_mut().unwrap().insert(
        "expected_continuation_revision".to_owned(),
        serde_json::Value::from(0),
    );
    assert!(
        serde_json::from_value::<ClaimResultContinuationInput>(zero_continuation_revision).is_err()
    );
    let claim_schema =
        serde_json::to_string(&schemars::schema_for!(ClaimResultContinuationInput)).unwrap();
    assert!(claim_schema.contains("\"additionalProperties\":false"));
    let completion_schema =
        serde_json::to_string(&schemars::schema_for!(CompleteResultContinuationInput)).unwrap();
    assert!(completion_schema.contains("^rcl_[0-9a-f]{32}$"));
}

#[test]
fn submitted_snapshot_wire_has_one_outer_kind_and_one_nested_action_kind() {
    let confirmation_store = ResultMemoryStore::new();
    let confirmation_available = present(
        &confirmation_store,
        confirmation_store
            .mint_from_execute(confirmation_input(b"owner:submitted-wire"))
            .unwrap(),
    );
    let confirmation = confirmation_store
        .submit(&confirmation_available.binding)
        .unwrap();
    let confirmation_snapshot = confirmation_store.current(&confirmation.binding).unwrap();
    let confirmation_state = serde_json::to_value(&confirmation_snapshot.state).unwrap();
    assert_eq!(
        confirmation_state,
        serde_json::json!({
            "kind": "submitted",
            "action": { "kind": "confirmed" }
        })
    );
    let confirmation_wire = serde_json::to_string(&confirmation_snapshot.state).unwrap();
    assert_eq!(confirmation_wire.matches("\"kind\"").count(), 2);

    let choice_store = ResultMemoryStore::new();
    let choice_available = present(
        &choice_store,
        choice_store.mint_from_execute(choice_input()).unwrap(),
    );
    let selected = choice_store
        .select(&choice_available.binding, choice_answers())
        .unwrap();
    let selected_snapshot = choice_store.current(&selected.binding).unwrap();
    let selected_state = serde_json::to_value(&selected_snapshot.state).unwrap();
    assert_eq!(
        selected_state,
        serde_json::json!({
            "kind": "submitted",
            "action": {
                "kind": "selected",
                "answers": [
                    { "question_id": "route", "option_id": "youtube" },
                    { "question_id": "review", "option_id": "full" },
                    { "question_id": "notify", "option_id": "yes" }
                ]
            }
        })
    );
    let selected_wire = serde_json::to_string(&selected_snapshot.state).unwrap();
    assert_eq!(selected_wire.matches("\"kind\"").count(), 2);
}

#[test]
fn submitted_state_wire_requires_exactly_one_closed_action_field() {
    assert!(
        serde_json::from_value::<ResultState>(serde_json::json!({
            "kind": "submitted"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ResultState>(serde_json::json!({
            "kind": "submitted",
            "action": { "kind": "confirmed" },
            "unexpected": true
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ResultState>(serde_json::json!({
            "kind": "submitted",
            "action": { "kind": "confirmed", "unexpected": true }
        }))
        .is_err()
    );

    let schema = serde_json::to_string(&schemars::schema_for!(ResultState)).unwrap();
    assert!(schema.contains("\"additionalProperties\":false"));
    assert!(schema.contains("\"action\""));
    assert!(schema.contains("\"submitted\""));
}

#[test]
fn batch_projection_and_owner_payload_bounds_fail_closed() {
    let store = ResultMemoryStore::new();
    let mut empty = read_only_input(InitialResultState::Available);
    empty.batch.outcomes.clear();
    assert_eq!(
        store.mint_from_execute(empty).unwrap_err(),
        ResultStoreError::InvalidInput("batch_outcomes")
    );

    let mut overflow = read_only_input(InitialResultState::Available);
    overflow.batch.outcomes = (0..=MAX_BATCH_OUTCOMES)
        .map(|index| ExecutedCallOutcome {
            call_id: format!("call-{index}"),
            operation_id: "x.users.search".to_owned(),
            disposition: CallDisposition::Succeeded,
            summary: "ok".to_owned(),
            resource_refs: Vec::new(),
        })
        .collect();
    assert_eq!(
        store.mint_from_execute(overflow).unwrap_err(),
        ResultStoreError::InvalidInput("batch_outcomes")
    );

    let mut secret = read_only_input(InitialResultState::Available);
    secret.presentation.sections = vec![PresentationSection::Fields {
        heading: None,
        fields: vec![PresentationField {
            label: "raw_api_key".to_owned(),
            value: "must-not-appear".to_owned(),
        }],
    }];
    assert_eq!(
        store.mint_from_execute(secret).unwrap_err(),
        ResultStoreError::InvalidInput("secret_or_raw_field")
    );

    assert_eq!(
        OwnerActionPayload::new(vec![0; MAX_OWNER_ACTION_BYTES + 1]).unwrap_err(),
        ResultStoreError::InvalidInput("owner_action_payload")
    );

    let mut duplicate_question = choice_input();
    if let Some(PendingUserAction::Choice { questions, .. }) =
        &mut duplicate_question.pending_action
    {
        questions[1].id = questions[0].id.clone();
    }
    assert_eq!(
        store.mint_from_execute(duplicate_question).unwrap_err(),
        ResultStoreError::InvalidInput("duplicate_choice_question")
    );

    let mut duplicate_option = choice_input();
    if let Some(PendingUserAction::Choice { questions, .. }) = &mut duplicate_option.pending_action
    {
        questions[0].options[1].id = questions[0].options[0].id.clone();
    }
    assert_eq!(
        store.mint_from_execute(duplicate_option).unwrap_err(),
        ResultStoreError::InvalidInput("duplicate_choice_option")
    );

    let representative_variants = (
        [
            CallDisposition::Failed {
                code: "failed".to_owned(),
            },
            CallDisposition::Unknown {
                code: "unknown".to_owned(),
            },
            CallDisposition::NotRun {
                reason: "Stopped after the prior call.".to_owned(),
            },
        ],
        [
            ResultResourceKind::Conversation,
            ResultResourceKind::Connection,
            ResultResourceKind::ProviderObject,
        ],
        [
            NoticeTone::Neutral,
            NoticeTone::Success,
            NoticeTone::Warning,
            NoticeTone::Critical,
        ],
        PresentationSection::Items {
            heading: None,
            items: vec![PresentationItem {
                title: "Item".to_owned(),
                description: None,
            }],
        },
        PresentationSection::Notice {
            heading: None,
            body: "Notice".to_owned(),
            tone: NoticeTone::Neutral,
        },
        TerminalResult::Failed(ResultFailure {
            code: "failed".to_owned(),
            summary: "The owner failed.".to_owned(),
            retryable: false,
        }),
    );
    assert_eq!(representative_variants.0.len(), 3);
}
