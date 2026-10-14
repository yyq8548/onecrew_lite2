use onecrew_local_runtime::ToolkitId;
use onecrew_local_runtime::{
    PendingActionView, ResultChoiceAnswer, ResultChoiceOption, ResultChoiceQuestion,
    ResultContinuationClaimMode, ResultContinuationContext, ResultContinuationState, ResultState,
    ResultViewKind,
};

use super::fixtures::{production_registry, result_owner};

#[test]
fn user_questions_use_onecrew_without_a_retired_task_toolkit() {
    use crate::operation_registry::descriptor::{EntryId, RegistryEntry, SurfaceId};
    let registry = production_registry();
    assert!(serde_json::from_value::<ToolkitId>(serde_json::json!("task")).is_err());
    for surface in [SurfaceId::Full, SurfaceId::Compact] {
        assert!(
            registry
                .entry(&EntryId::parse("task.choice.request").unwrap(), surface)
                .is_none()
        );
        assert!(
            registry
                .entry(&EntryId::parse("toolkit.task").unwrap(), surface)
                .is_none()
        );
        let Some(RegistryEntry::Operation(operation)) = registry.entry(
            &EntryId::parse("onecrew.ask_user_question").unwrap(),
            surface,
        ) else {
            panic!("OneCrew question operation must remain available");
        };
        assert_eq!(operation.metadata.toolkit, ToolkitId::OneCrew);
    }
}

#[test]
fn custom_continuation_preserves_text_and_requires_context_when_message_is_shortened() {
    let questions = (0..3)
        .map(|index| ResultChoiceQuestion {
            selection_mode: Default::default(),
            id: format!("q{index}"),
            header: format!("Question {index}"),
            prompt: "Choose or explain".into(),
            options: vec![ResultChoiceOption {
                id: "yes".into(),
                label: "Yes".into(),
                description: None,
            }],
        })
        .collect::<Vec<_>>();
    let short = vec![ResultChoiceAnswer::Custom {
        question_id: "q0".into(),
        text: "My own answer\n第二行".into(),
    }];
    let request = super::super::projection::choice_continuation(
        &questions[..1],
        &short,
        onecrew_local_runtime::PresentationLocale::English,
    );
    assert!(request.message.contains("My own answer\n第二行"));
    assert!(!request.requires_model_context);
    let long = (0..3)
        .map(|index| ResultChoiceAnswer::Custom {
            question_id: format!("q{index}"),
            text: "a".repeat(2048),
        })
        .collect::<Vec<_>>();
    let request = super::super::projection::choice_continuation(
        &questions,
        &long,
        onecrew_local_runtime::PresentationLocale::English,
    );
    assert!(request.requires_model_context);
    assert!(request.message.len() <= 4096);
    assert_eq!(
        request.context,
        ResultContinuationContext::Choice { answers: long }
    );
}
use crate::operation_registry::v2::{
    contract::{
        AskUserQuestionInput, PresentationRequest, ResultPresentInput, ResultStateFailure,
        ResultStateInput, ResultStateOutput, ToolExecuteInput, ToolExecutionAuthority,
        ToolExecutionCall, ToolExecutionOutcome, ToolInputKind, UserQuestionId,
    },
    state::{mint_choice_result_v2, result_present_v2},
};

#[test]
fn choice_continuation_copy_is_bounded_and_uses_only_authoritative_labels() {
    let questions = (0..32)
        .map(|index| ResultChoiceQuestion {
            selection_mode: Default::default(),
            id: format!("question_id_{index}"),
            header: format!("Header {index} {}", "H".repeat(8_000)),
            prompt: "Prompt".to_owned(),
            options: vec![ResultChoiceOption {
                id: format!("option_id_{index}"),
                label: format!("Label {index} {}", "L".repeat(8_000)),
                description: None,
            }],
        })
        .collect::<Vec<_>>();
    let answers = (0..32)
        .map(|index| ResultChoiceAnswer::Option {
            question_id: format!("question_id_{index}"),
            option_id: format!("option_id_{index}"),
        })
        .collect::<Vec<_>>();
    let continuation = super::super::projection::choice_continuation(
        &questions,
        &answers,
        onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
    );
    assert!(continuation.requires_model_context);
    let message = continuation.message;
    assert!(message.len() <= 4 * 1024);
    assert!(message.starts_with("我已完成 OneCrew 选择：\n- Header 0"));
    assert!(message.ends_with("请按这些已确认的选择继续当前任务；不要重复提交选择。"));
    assert!(!message.contains("question_id_"));
    assert!(!message.contains("option_id_"));
    assert!(!message.contains('{'));
}

fn choice_request() -> AskUserQuestionInput {
    AskUserQuestionInput {
        interaction_id: UserQuestionId::parse("choice.multi-question").unwrap(),
        title: "Choose contact policy".to_owned(),
        questions: vec![
            ResultChoiceQuestion {
                selection_mode: Default::default(),
                id: "route".to_owned(),
                header: "Contact route".to_owned(),
                prompt: "Which route should be saved?".to_owned(),
                options: vec![
                    ResultChoiceOption {
                        id: "route_email".to_owned(),
                        label: "Email".to_owned(),
                        description: Some("Save a verified public email route.".to_owned()),
                    },
                    ResultChoiceOption {
                        id: "route_youtube".to_owned(),
                        label: "YouTube".to_owned(),
                        description: None,
                    },
                ],
            },
            ResultChoiceQuestion {
                selection_mode: Default::default(),
                id: "notify".to_owned(),
                header: "Notification".to_owned(),
                prompt: "Notify the Agent after selection?".to_owned(),
                options: vec![
                    ResultChoiceOption {
                        id: "notify_yes".to_owned(),
                        label: "Yes".to_owned(),
                        description: None,
                    },
                    ResultChoiceOption {
                        id: "notify_no".to_owned(),
                        label: "No".to_owned(),
                        description: None,
                    },
                ],
            },
        ],
    }
}

#[test]
fn historical_choice_resume_keeps_previous_question_and_option_bounds() {
    use crate::operation_registry::v2::resume::{
        ChoiceResumeEnvelope, decode_choice_resume_envelope, encode_choice_resume_envelope,
    };
    for (question_count, option_count) in [(32, 1), (1, 32)] {
        let mut request = choice_request();
        let mut question = request.questions.remove(0);
        question.options = (0..option_count)
            .map(|index| ResultChoiceOption {
                id: format!("o{index}"),
                label: "Historical option".into(),
                description: None,
            })
            .collect();
        request.questions = (0..question_count)
            .map(|index| {
                let mut question = question.clone();
                question.id = format!("q{index}");
                question
            })
            .collect();
        assert!(!request.valid());
        let payload = encode_choice_resume_envelope(
            super::fixtures::id("historical-choice"),
            super::fixtures::id("onecrew.ask_user_question"),
            request.interaction_id,
            request.questions.clone(),
        )
        .unwrap();
        let ChoiceResumeEnvelope::V2 { questions, .. } =
            decode_choice_resume_envelope(payload.into_generic_bytes().unwrap()).unwrap();
        assert_eq!(questions, request.questions);
    }
}

#[tokio::test]
async fn registered_choice_operation_mints_without_dispatching_an_owner() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let operation_id =
        crate::operation_registry::descriptor::EntryId::parse("onecrew.ask_user_question").unwrap();
    let operation = match registry
        .entry(
            &operation_id,
            crate::operation_registry::descriptor::SurfaceId::Full,
        )
        .expect("registered choice operation")
    {
        crate::operation_registry::descriptor::RegistryEntry::Operation(operation) => operation,
        _ => panic!("choice must be an operation"),
    };
    assert_eq!(
        operation.view_kind,
        crate::operation_registry::descriptor::OperationViewKind::Choice
    );
    assert_eq!(
        operation.metadata.toolkit,
        onecrew_local_runtime::ToolkitId::OneCrew
    );
    assert!(!operation.effect.may_have_effect());

    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            crate::operation_registry::descriptor::SurfaceId::Full,
            "choice-register-test",
            &ToolExecuteInput {
                calls: vec![ToolExecutionCall {
                    call_id: crate::operation_registry::descriptor::EntryId::parse("choice.call")
                        .unwrap(),
                    operation_id,
                    schema_revision: operation.input.revision().to_owned(),
                    authority: ToolExecutionAuthority::Direct {},
                    connection: None,
                    arguments: serde_json::to_value(choice_request())
                        .unwrap()
                        .as_object()
                        .unwrap()
                        .clone(),
                }],
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        result.outcomes.as_slice(),
        [ToolExecutionOutcome::InputRequired {
            input_kind: ToolInputKind::Choice,
            ..
        }]
    ));
    let presented = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref: result.result_ref.expect("choice result ref"),
            locale: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(presented.snapshot.binding.view.kind, ResultViewKind::Choice);
    assert_eq!(
        presented.snapshot.batch.outcomes[0].operation_id,
        "onecrew.ask_user_question"
    );
}

fn ordered_answers() -> Vec<ResultChoiceAnswer> {
    vec![
        ResultChoiceAnswer::Option {
            question_id: "route".to_owned(),
            option_id: "route_email".to_owned(),
        },
        ResultChoiceAnswer::Option {
            question_id: "notify".to_owned(),
            option_id: "notify_yes".to_owned(),
        },
    ]
}

fn mint_choice(
    platform: &onecrew_local_runtime::LocalPlatform,
    origin: &onecrew_local_runtime::ResultOrigin,
) -> super::super::state::MintedChoiceResultV2 {
    mint_choice_result_v2(
        platform,
        origin,
        crate::operation_registry::descriptor::EntryId::parse("choice.call").unwrap(),
        crate::operation_registry::descriptor::EntryId::parse("onecrew.ask_user_question").unwrap(),
        choice_request(),
    )
    .unwrap()
}

#[tokio::test]
async fn multi_question_select_normalizes_answers_and_installs_choice_continuation() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let minted = mint_choice(&platform, &origin);
    let presented = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref: minted.result_ref,
            locale: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(presented.snapshot.binding.view.kind, ResultViewKind::Choice);
    assert_eq!(presented.snapshot.presentation.kind.as_str(), "choice");
    assert!(matches!(presented.snapshot.state, ResultState::Available));
    assert!(matches!(
        &presented.snapshot.pending_action,
        Some(PendingActionView::Choice { questions }) if questions.len() == 2
    ));
    let public_snapshot = serde_json::to_string(&presented.snapshot).unwrap();
    assert!(!public_snapshot.contains("owner_payload"));
    assert!(!public_snapshot.contains("\"version\":\"v1\""));

    let reversed_answers = ordered_answers().into_iter().rev().collect();
    let selected = registry
        .result_state_v2(
            &platform,
            crate::operation_registry::descriptor::SurfaceId::Full,
            ResultStateInput::Select {
                binding: presented.snapshot.binding,
                answers: reversed_answers,
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = selected else {
        panic!("selection must return snapshot")
    };
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));
    assert!(snapshot.pending_action.is_none());
    assert_eq!(
        snapshot.continuation.state,
        ResultContinuationState::Pending
    );
    let claimed = registry
        .result_state_v2(
            &platform,
            crate::operation_registry::descriptor::SurfaceId::Full,
            ResultStateInput::ClaimContinuation {
                input: onecrew_local_runtime::ClaimResultContinuationInput {
                    binding: snapshot.binding,
                    expected_continuation_revision: snapshot.continuation.continuation_revision,
                    mode: ResultContinuationClaimMode::Pending,
                },
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::ClaimedContinuation { claimed } = claimed else {
        panic!("claim output")
    };
    let ResultContinuationContext::Choice { answers } = claimed.request.context else {
        panic!("choice continuation")
    };
    assert_eq!(answers, ordered_answers());
    assert_eq!(
        claimed.request.message,
        "I have completed the OneCrew choice:\n- Contact route: Email\n- Notification: Yes\nContinue the current task using these answers; do not submit the choice again."
    );
    assert!(!claimed.request.message.contains("route_email"));
    assert!(!claimed.request.message.contains("notify_yes"));
    assert!(!claimed.request.message.contains('{'));
    assert!(claimed.request.message.len() <= 4 * 1024);
}

#[tokio::test]
async fn invalid_choice_answers_do_not_consume_payload_and_valid_select_replays_closed() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let minted = mint_choice(&platform, &origin);
    let available = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref: minted.result_ref,
            locale: Default::default(),
        },
    )
    .unwrap()
    .snapshot;

    let invalid_answers = [
        vec![ordered_answers()[0].clone()],
        vec![
            ordered_answers()[0].clone(),
            ordered_answers()[1].clone(),
            ResultChoiceAnswer::Option {
                question_id: "extra".to_owned(),
                option_id: "extra".to_owned(),
            },
        ],
        vec![
            ResultChoiceAnswer::Option {
                question_id: "route".to_owned(),
                option_id: "forged".to_owned(),
            },
            ordered_answers()[1].clone(),
        ],
    ];
    for answers in invalid_answers {
        assert_eq!(
            registry
                .result_state_v2(
                    &platform,
                    crate::operation_registry::descriptor::SurfaceId::Full,
                    ResultStateInput::Select {
                        binding: available.binding.clone(),
                        answers,
                    },
                )
                .await
                .unwrap_err(),
            ResultStateFailure::InvalidInput
        );
        let current = platform
            .read_current_result(&available.binding)
            .expect("invalid answers keep choice available");
        assert!(matches!(current.state, ResultState::Available));
        assert_eq!(current.binding, available.binding);
    }

    let selected = registry
        .result_state_v2(
            &platform,
            crate::operation_registry::descriptor::SurfaceId::Full,
            ResultStateInput::Select {
                binding: available.binding,
                answers: ordered_answers(),
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = selected else {
        panic!("selection snapshot")
    };
    assert_eq!(
        registry
            .result_state_v2(
                &platform,
                crate::operation_registry::descriptor::SurfaceId::Full,
                ResultStateInput::Select {
                    binding: snapshot.binding,
                    answers: ordered_answers(),
                },
            )
            .await
            .unwrap_err(),
        ResultStateFailure::Replay
    );
}
