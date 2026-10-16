use super::*;

#[test]
fn retired_confirmation_preserves_readable_history_without_consuming_authority() {
    let store = ResultMemoryStore::new();
    let mut input = confirmation_input(b"retained-peer-continuation");
    input.batch.outcomes[0].operation_id = crate::OperationId::EMAIL_MANUAL_SEND.as_str().into();
    input.batch.outcomes[0].disposition = CallDisposition::NotRun { reason: "confirmation_required".into() };
    input.presentation.kind = ResultPresentationKind::new("email_confirmation").unwrap();
    let reference = store.mint_from_execute(input).unwrap();
    let available = present(&store, reference);
    assert!(matches!(store.submit_result_review(&available.binding, crate::OwnerReviewSelection::Confirm),
        Err(ResultStoreError::InvalidState(crate::RETIRED_OPERATION_CODE))));
    let current = store.current(&available.binding).unwrap();
    assert_eq!(current, available);
}

#[test]
fn custom_choice_text_is_a_distinct_answer_and_submits_once() {
    let store = ResultMemoryStore::new();
    let reference = store.mint_from_execute(choice_input()).unwrap();
    let available = present(&store, reference);
    let mut answers = choice_answers();
    let question_id = answers[0].question_id().to_owned();
    answers[0] = ResultChoiceAnswer::Custom {
        question_id: question_id.clone(),
        text: "Custom answer\n第二行".into(),
    };
    let submitted = store.select(&available.binding, answers).unwrap();
    let SubmittedActionView::Selected { answers } = submitted.action else {
        panic!("selected answers")
    };
    let custom = answers
        .iter()
        .find(|answer| answer.question_id() == question_id)
        .unwrap();
    let wire = serde_json::to_value(custom).unwrap();
    assert_eq!(wire["text"], "Custom answer\n第二行");
    assert!(wire.get("option_id").is_none());
    assert!(store.select(&available.binding, answers).is_err());
}

#[test]
fn custom_answers_reject_ambiguous_shapes_and_invalid_text() {
    for wire in [
        serde_json::json!({"question_id":"q", "option_id":"a", "text":"custom"}),
        serde_json::json!({"question_id":"q"}),
        serde_json::json!({"question_id":"q", "text":"custom", "extra":true}),
    ] {
        assert!(serde_json::from_value::<ResultChoiceAnswer>(wire).is_err());
    }
    for text in [" ".to_owned(), "x\0y".to_owned(), "界".repeat(683)] {
        assert!(
            !ResultChoiceAnswer::Custom {
                question_id: "q".into(),
                text
            }
            .is_well_formed()
        );
    }
}

#[test]
fn explicit_card_locales_survive_reads_and_transitions_independently() {
    use crate::PresentationLocale::{English, SimplifiedChinese};
    let directory = tempfile::tempdir().unwrap();
    let preferences = std::sync::Arc::new(crate::PresentationPreferences::default());
    let platform = crate::LocalPlatform::open(directory.path())
        .unwrap()
        .with_presentation_preferences(preferences.clone());
    for locale in [SimplifiedChinese, English] {
        let reference = platform
            .mint_execution_result(confirmation_input(b"owner-only"))
            .unwrap();
        let available = platform
            .present_result_with_locale(&reference, &origin("origin-one"), locale)
            .unwrap();
        preferences.set_locale(if locale == English {
            SimplifiedChinese
        } else {
            English
        });
        for snapshot in [
            platform.read_current_result(&available.binding).unwrap(),
            platform.read_result(&available.binding).unwrap(),
        ] {
            assert_eq!(snapshot.locale, locale);
            assert_eq!(snapshot.binding, available.binding);
        }
        assert!(
            platform
                .present_result_with_locale(&reference, &origin("origin-one"), preferences.locale())
                .is_err()
        );
        assert_eq!(
            platform
                .read_current_result(&available.binding)
                .unwrap()
                .locale,
            locale
        );
        platform
            .submit_result_confirmation(&available.binding)
            .unwrap();
        assert_eq!(
            platform
                .read_current_result(&available.binding)
                .unwrap()
                .locale,
            locale
        );
    }
}

#[test]
fn oversized_combined_custom_answers_leave_the_choice_available() {
    let store = ResultMemoryStore::new();
    let mut input = choice_input();
    let Some(PendingUserAction::Choice { questions, .. }) = &mut input.pending_action else {
        panic!("choice fixture")
    };
    let template = questions[0].clone();
    *questions = (0..32)
        .map(|index| ResultChoiceQuestion {
            selection_mode: Default::default(),
            id: format!("q{index}"),
            ..template.clone()
        })
        .collect();
    let reference = store.mint_from_execute(input).unwrap();
    let available = present(&store, reference);
    let answers = (0..32)
        .map(|index| ResultChoiceAnswer::Custom {
            question_id: format!("q{index}"),
            text: "x".repeat(1024),
        })
        .collect();
    assert_eq!(
        store.select(&available.binding, answers).unwrap_err(),
        ResultStoreError::InvalidInput("choice_answers_too_large")
    );
    assert!(matches!(
        store.current(&available.binding).unwrap().state,
        ResultState::Available
    ));
}

#[test]
fn restart_has_no_result_authority() {
    let first_store = ResultMemoryStore::new();
    let result_ref = first_store
        .mint_from_execute(read_only_input(InitialResultState::Available))
        .unwrap();
    let restarted_store = ResultMemoryStore::new();

    assert_eq!(
        restarted_store
            .present(&result_ref, &origin("origin-one"))
            .unwrap_err(),
        ResultStoreError::Absent
    );
}

#[test]
fn guessed_cross_origin_cross_view_fingerprint_and_stale_bindings_are_rejected() {
    let store = ResultMemoryStore::new();
    let result_ref = store
        .mint_from_execute(confirmation_input(b"owner:confirm"))
        .unwrap();

    let guessed_ref = "rr_00000000000000000000000000000000".parse().unwrap();
    assert_eq!(
        store
            .present(&guessed_ref, &origin("origin-one"))
            .unwrap_err(),
        ResultStoreError::Absent
    );
    assert_eq!(
        store
            .present(&result_ref, &origin("origin-two"))
            .unwrap_err(),
        ResultStoreError::OriginMismatch
    );

    let available = present(&store, result_ref.clone());
    assert_eq!(
        store
            .present(&result_ref, &origin("origin-one"))
            .unwrap_err(),
        ResultStoreError::Replay("result_already_presented")
    );

    let guessed = ResultBinding {
        result_ref: guessed_ref,
        ..available.binding.clone()
    };
    assert_eq!(store.read(&guessed).unwrap_err(), ResultStoreError::Absent);

    let cross_origin = ResultBinding {
        origin: origin("origin-two"),
        ..available.binding.clone()
    };
    assert_eq!(
        store.read(&cross_origin).unwrap_err(),
        ResultStoreError::OriginMismatch
    );

    let cross_view = ResultBinding {
        view: ResultViewBinding {
            kind: ResultViewKind::Choice,
            ..available.binding.view.clone()
        },
        ..available.binding.clone()
    };
    assert_eq!(
        store.read(&cross_view).unwrap_err(),
        ResultStoreError::ViewMismatch
    );

    let wrong_fingerprint = ResultBinding {
        fingerprint: ResultFingerprint::digest(b"wrong"),
        ..available.binding.clone()
    };
    assert_eq!(
        store.read(&wrong_fingerprint).unwrap_err(),
        ResultStoreError::FingerprintMismatch
    );

    let submitted = store.submit(&available.binding).unwrap();
    assert_eq!(
        store.submit(&available.binding).unwrap_err(),
        ResultStoreError::RevisionConflict {
            current_revision: submitted.binding.revision,
        }
    );
    assert_eq!(
        store.submit(&submitted.binding).unwrap_err(),
        ResultStoreError::Replay("result_action_consumed")
    );
}

#[test]
fn initial_terminal_results_are_presentable_without_pending_actions() {
    let store = ResultMemoryStore::new();
    let states = [
        InitialResultState::Resolved(ResultResolution {
            code: "completed".to_owned(),
            summary: "Execution completed.".to_owned(),
        }),
        InitialResultState::Failed(ResultFailure {
            code: "provider_failed".to_owned(),
            summary: "The provider rejected the request.".to_owned(),
            retryable: false,
        }),
        InitialResultState::Unknown(ResultUnknown {
            code: "effect_unknown".to_owned(),
            summary: "The final effect could not be verified.".to_owned(),
        }),
    ];

    for state in states {
        let result_ref = store.mint_from_execute(read_only_input(state)).unwrap();
        let shown = present(&store, result_ref);
        assert!(shown.presented);
        assert!(shown.pending_action.is_none());
        assert!(matches!(
            shown.state,
            ResultState::Resolved(_) | ResultState::Failed(_) | ResultState::Unknown(_)
        ));
    }
}

#[test]
fn confirmation_returns_owner_payload_after_lock_scoped_cas() {
    let store = ResultMemoryStore::new();
    let result_ref = store
        .mint_from_execute(confirmation_input(b"owner:creator:save"))
        .unwrap();
    let available = present(&store, result_ref);
    assert!(!format!("{available:?}").contains("owner:creator:save"));

    let submitted = store.submit(&available.binding).unwrap();
    assert!(!format!("{submitted:?}").contains("owner:creator:save"));
    let SubmittedOwnerAction {
        binding,
        action,
        owner_payload,
    } = submitted;
    assert_eq!(
        owner_payload.into_generic_bytes().unwrap(),
        b"owner:creator:save"
    );
    assert_eq!(action, SubmittedActionView::Confirmed {});
    assert!(matches!(
        store.current(&binding).unwrap().state,
        ResultState::Submitted {
            action: SubmittedActionView::Confirmed {}
        }
    ));

    let mut update = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "created".to_owned(),
        summary: "Creator saved and read back.".to_owned(),
    }));
    update.batch.outcomes[0].summary = "Final owner batch result.".to_owned();
    update.presentation.kind = ResultPresentationKind::new("creator_result").unwrap();
    update.presentation.title = "Creator saved".to_owned();
    let final_payload_bytes = terminal_update_payload_bytes(&update).unwrap();
    let resolved = store.terminal(&binding, update).unwrap();
    assert!(matches!(resolved.state, ResultState::Resolved(_)));
    assert_eq!(
        resolved.batch.outcomes[0].summary,
        "Final owner batch result."
    );
    assert_eq!(resolved.presentation.kind.as_str(), "creator_result");
    assert_eq!(resolved.presentation.title, "Creator saved");
    assert_eq!(store.cleanup().unwrap().payload_bytes, final_payload_bytes);
}

#[test]
fn invalid_terminal_updates_preserve_submitted_state_and_projection() {
    let store = ResultMemoryStore::new();
    let available = present(
        &store,
        store
            .mint_from_execute(confirmation_input(b"owner:terminal-validation"))
            .unwrap(),
    );
    let submitted = store.submit(&available.binding).unwrap();
    let before = store.current(&submitted.binding).unwrap();

    let mut invalid_batch = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: "Completed.".to_owned(),
    }));
    invalid_batch.batch.outcomes.clear();
    assert_eq!(
        store
            .terminal(&submitted.binding, invalid_batch)
            .unwrap_err(),
        ResultStoreError::InvalidInput("batch_outcomes")
    );

    let mut oversized = terminal_update(TerminalResult::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: "Completed.".to_owned(),
    }));
    oversized.presentation.title = "x".repeat(MAX_STRING_BYTES + 1);
    assert_eq!(
        store.terminal(&submitted.binding, oversized).unwrap_err(),
        ResultStoreError::InvalidInput("projection_title")
    );
    assert_eq!(store.current(&submitted.binding).unwrap(), before);
}

#[test]
fn choice_select_and_cancel_are_distinct_single_use_transitions() {
    let store = ResultMemoryStore::new();
    let first = present(&store, store.mint_from_execute(choice_input()).unwrap());
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("owner:choice")
    );
    assert!(matches!(
        &first.pending_action,
        Some(PendingActionView::Choice { questions }) if questions.len() == 3
    ));
    let selected = store.select(&first.binding, choice_answers()).unwrap();
    assert!(!format!("{selected:?}").contains("owner:choice"));
    let SubmittedOwnerAction {
        binding,
        action,
        owner_payload,
    } = selected;
    assert_eq!(owner_payload.into_generic_bytes().unwrap(), b"owner:choice");
    assert_eq!(
        action,
        SubmittedActionView::Selected {
            answers: vec![
                ResultChoiceAnswer::Option {
                    question_id: "route".to_owned(),
                    option_id: "youtube".to_owned(),
                },
                ResultChoiceAnswer::Option {
                    question_id: "review".to_owned(),
                    option_id: "full".to_owned(),
                },
                ResultChoiceAnswer::Option {
                    question_id: "notify".to_owned(),
                    option_id: "yes".to_owned(),
                },
            ]
        }
    );
    assert_eq!(
        store.select(&binding, choice_answers()).unwrap_err(),
        ResultStoreError::Replay("result_action_consumed")
    );

    let second = present(&store, store.mint_from_execute(choice_input()).unwrap());
    let cancelled = store
        .cancel(CancelResultInput {
            binding: second.binding,
            continuation: None,
        })
        .unwrap();
    assert!(matches!(cancelled.state, ResultState::Resolved(_)));
    assert_eq!(
        store
            .cancel(CancelResultInput {
                binding: cancelled.binding,
                continuation: None,
            })
            .unwrap_err(),
        ResultStoreError::Replay("result_action_consumed")
    );
}

#[test]
fn invalid_multi_question_answers_leave_choice_available() {
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(choice_input()).unwrap());

    let mut missing = choice_answers();
    missing.pop();
    assert_eq!(
        store.select(&available.binding, missing).unwrap_err(),
        ResultStoreError::InvalidInput("choice_answer_count")
    );

    let mut extra = choice_answers();
    extra.push(ResultChoiceAnswer::Option {
        question_id: "extra".to_owned(),
        option_id: "extra".to_owned(),
    });
    assert_eq!(
        store.select(&available.binding, extra).unwrap_err(),
        ResultStoreError::InvalidInput("choice_answer_count")
    );

    let mut duplicate = choice_answers();
    let ResultChoiceAnswer::Option { question_id, .. } = &mut duplicate[0] else {
        panic!("option fixture")
    };
    *question_id = "route".to_owned();
    assert_eq!(
        store.select(&available.binding, duplicate).unwrap_err(),
        ResultStoreError::InvalidInput("duplicate_choice_answer")
    );

    let mut unknown_question = choice_answers();
    let ResultChoiceAnswer::Option { question_id, .. } = &mut unknown_question[0] else {
        panic!("option fixture")
    };
    *question_id = "unknown".to_owned();
    assert_eq!(
        store
            .select(&available.binding, unknown_question)
            .unwrap_err(),
        ResultStoreError::InvalidInput("unknown_choice_question")
    );

    let mut unknown_option = choice_answers();
    let ResultChoiceAnswer::Option { option_id, .. } = &mut unknown_option[0] else {
        panic!("option fixture")
    };
    *option_id = "unknown".to_owned();
    assert_eq!(
        store
            .select(&available.binding, unknown_option)
            .unwrap_err(),
        ResultStoreError::InvalidInput("unknown_choice_option")
    );
    assert_eq!(store.current(&available.binding).unwrap(), available);
}

#[test]
fn current_recovers_stale_views_but_mutations_require_exact_revision() {
    let store = ResultMemoryStore::new();
    let result_ref = store.mint_from_execute(choice_input()).unwrap();
    let available = store.present(&result_ref, &origin("origin-one")).unwrap();
    let zero = ResultBinding {
        revision: 0,
        ..available.binding.clone()
    };
    assert_eq!(
        store.current(&zero).unwrap_err(),
        ResultStoreError::InvalidInput("result_revision")
    );
    let future = ResultBinding {
        revision: available.binding.revision + 1,
        ..available.binding.clone()
    };
    assert_eq!(
        store.current(&future).unwrap_err(),
        ResultStoreError::RevisionConflict {
            current_revision: available.binding.revision,
        }
    );
    assert_eq!(
        store
            .present(&result_ref, &origin("origin-one"))
            .unwrap_err(),
        ResultStoreError::Replay("result_already_presented")
    );
    assert_eq!(store.read(&available.binding).unwrap(), available);
    assert_eq!(store.current(&available.binding).unwrap(), available);

    let submitted = store.select(&available.binding, choice_answers()).unwrap();
    assert!(matches!(
        store.current(&available.binding).unwrap().state,
        ResultState::Submitted { .. }
    ));
    assert_eq!(
        store
            .terminal(
                &available.binding,
                terminal_update(TerminalResult::Unknown(ResultUnknown {
                    code: "unknown".to_owned(),
                    summary: "Effect unknown.".to_owned(),
                })),
            )
            .unwrap_err(),
        ResultStoreError::RevisionConflict {
            current_revision: submitted.binding.revision,
        }
    );
    let unknown = store
        .terminal(
            &submitted.binding,
            terminal_update(TerminalResult::Unknown(ResultUnknown {
                code: "unknown".to_owned(),
                summary: "Effect unknown.".to_owned(),
            })),
        )
        .unwrap();
    assert!(matches!(unknown.state, ResultState::Unknown(_)));
}

#[test]
fn local_platform_owns_the_promoted_result_lifecycle() {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let origin = origin("application-scope");
    let mut input = confirmation_input(b"owner:promoted");
    input.origin = origin.clone();
    let result_ref = platform.mint_execution_result(input).unwrap();
    let available = platform.present_result(&result_ref, &origin).unwrap();
    let submitted = platform
        .submit_result_confirmation(&available.binding)
        .unwrap();
    let SubmittedOwnerAction {
        binding,
        owner_payload,
        ..
    } = submitted;
    assert_eq!(
        owner_payload.into_generic_bytes().unwrap(),
        b"owner:promoted"
    );
    let resolved = platform
        .transition_result_terminal(
            &binding,
            terminal_update(TerminalResult::Resolved(ResultResolution {
                code: "completed".to_owned(),
                summary: "Completed by the application owner.".to_owned(),
            })),
        )
        .unwrap();
    assert!(matches!(resolved.state, ResultState::Resolved(_)));
    assert_eq!(platform.cleanup_results().unwrap().active, 1);
}
