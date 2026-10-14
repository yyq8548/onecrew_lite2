use super::fixtures::{call, production_registry, result_owner, revision, schema_value};
use crate::operation_registry::{
    descriptor::SurfaceId,
    v2::contract::{
        AskUserQuestionInput, PresentationRequest, ResultStateInput, ResultStateOutput,
        ToolExecuteInput,
    },
};
use onecrew_local_runtime::{
    ClaimResultContinuationInput, PendingActionView, ResultChoiceAnswer, ResultChoiceSelectionMode,
    ResultContinuationClaimMode, ResultContinuationContext, ResultState,
};
use serde_json::json;

fn request() -> AskUserQuestionInput {
    serde_json::from_value(json!({"interaction_id":"multiple-review","title":"Review selections","questions":[
        {"id":"creators","selection_mode":"multiple","header":"Creators","prompt":"Which creators?","options":[{"id":"alice","label":"Alice","description":null},{"id":"bob","label":"Bob","description":null},{"id":"carol","label":"Carol","description":null}]},
        {"id":"next","header":"Next step","prompt":"Which next step?","options":[{"id":"review","label":"Review first","description":null},{"id":"stop","label":"Stop here","description":null}]}
    ]})).unwrap()
}

fn answers(ids: &[&str]) -> Vec<ResultChoiceAnswer> {
    vec![
        ResultChoiceAnswer::Options {
            question_id: "creators".into(),
            option_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
        },
        ResultChoiceAnswer::Option {
            question_id: "next".into(),
            option_id: "review".into(),
        },
    ]
}

#[tokio::test]
async fn registered_multiple_choice_preserves_all_ids_through_selection_and_claim() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "multiple-choice-batch",
            &ToolExecuteInput {
                calls: vec![call(
                    "multiple-choice",
                    "onecrew.ask_user_question",
                    revision(&registry, "onecrew.ask_user_question"),
                    serde_json::to_value(request()).unwrap(),
                )],
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let available = platform
        .present_result(&result.result_ref.unwrap(), &origin)
        .unwrap();
    let Some(PendingActionView::Choice { questions }) = &available.pending_action else {
        panic!("choice");
    };
    assert_eq!(
        questions[0].selection_mode,
        ResultChoiceSelectionMode::Multiple
    );
    assert_eq!(
        questions[1].selection_mode,
        ResultChoiceSelectionMode::Single
    );
    for invalid in [
        answers(&[]),
        answers(&["alice", "alice"]),
        answers(&["alice", "unknown"]),
        vec![
            ResultChoiceAnswer::Option {
                question_id: "creators".into(),
                option_id: "alice".into(),
            },
            answers(&["alice"])[1].clone(),
        ],
        vec![
            answers(&["alice"])[0].clone(),
            ResultChoiceAnswer::Options {
                question_id: "next".into(),
                option_ids: vec!["review".into()],
            },
        ],
    ] {
        assert!(
            registry
                .result_state_v2(
                    &platform,
                    SurfaceId::Full,
                    ResultStateInput::Select {
                        binding: available.binding.clone(),
                        answers: invalid
                    }
                )
                .await
                .is_err()
        );
        assert_eq!(
            platform.read_current_result(&available.binding).unwrap(),
            available
        );
    }
    let ResultStateOutput::Snapshot { snapshot, .. } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Select {
                binding: available.binding.clone(),
                answers: answers(&["carol", "alice"]),
            },
        )
        .await
        .unwrap()
    else {
        panic!("selection");
    };
    assert!(
        matches!(&snapshot.state,ResultState::Resolved(value) if value.code=="choice_selected")
    );
    assert!(
        snapshot.batch.outcomes[0].resource_refs.is_empty(),
        "selection is not a Creator save"
    );
    assert!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Select {
                    binding: available.binding,
                    answers: answers(&["bob"])
                }
            )
            .await
            .is_err()
    );
    let ResultStateOutput::ClaimedContinuation { claimed } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ClaimContinuation {
                input: ClaimResultContinuationInput {
                    binding: snapshot.binding,
                    expected_continuation_revision: snapshot.continuation.continuation_revision,
                    mode: ResultContinuationClaimMode::Pending,
                },
            },
        )
        .await
        .unwrap()
    else {
        panic!("claim");
    };
    assert!(claimed.request.requires_model_context);
    assert!(claimed.request.message.contains("Alice [alice]"));
    assert!(claimed.request.message.contains("Carol [carol]"));
    assert!(!claimed.request.message.contains("Bob [bob]"));
    assert_eq!(
        claimed.request.context,
        ResultContinuationContext::Choice {
            answers: answers(&["alice", "carol"])
        }
    );
}

#[test]
fn multiple_choice_schema_and_strict_decoder_preserve_closed_answer_shapes() {
    let schema = schema_value::<ResultStateInput>();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .unwrap();
    let binding = json!({"result_ref":format!("rr_{}","1".repeat(32)),"view":{"identity":format!("rv_{}","2".repeat(32)),"kind":"choice"},"origin":"mcp:full","fingerprint":format!("sha256:{}","3".repeat(64)),"revision":2});
    let valid = json!({"action":"select","binding":binding,"answers":answers(&["alice","carol"])});
    assert!(validator.is_valid(&valid));
    assert!(serde_json::from_value::<ResultStateInput>(valid).is_ok());
    for invalid in [
        json!({"question_id":"creators","option_ids":[]}),
        json!({"question_id":"creators","option_ids":["alice","alice"]}),
        json!({"question_id":"creators","option_ids":["alice"],"option_id":"alice"}),
        json!({"question_id":"creators","option_ids":["alice"],"text":"Custom"}),
        json!({"question_id":"creators","option_ids":(0..33).map(|i|format!("o{i}")).collect::<Vec<_>>()}),
    ] {
        let value = json!({"action":"select","binding":binding,"answers":[invalid]});
        assert!(serde_json::from_value::<ResultStateInput>(value).is_err());
    }
    let choice_schema = schema_value::<AskUserQuestionInput>();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&choice_schema)
        .unwrap();
    assert!(validator.is_valid(&serde_json::to_value(request()).unwrap()));
    for mode in [json!(null), json!("many")] {
        let mut input = serde_json::to_value(request()).unwrap();
        input["questions"][0]["selection_mode"] = mode;
        assert!(!validator.is_valid(&input));
        assert!(serde_json::from_value::<AskUserQuestionInput>(input).is_err());
    }
}

#[test]
fn long_multiple_labels_cannot_replace_or_drop_complete_selected_ids() {
    let mut request = request();
    request.questions.truncate(1);
    request.questions[0].options = (0..32)
        .map(|i| onecrew_local_runtime::ResultChoiceOption {
            id: format!("creator-{i}"),
            label: "大段审核资料".repeat(300),
            description: None,
        })
        .collect();
    let answers = vec![ResultChoiceAnswer::Options {
        question_id: "creators".into(),
        option_ids: request.questions[0]
            .options
            .iter()
            .map(|option| option.id.clone())
            .collect(),
    }];
    let continuation = super::super::projection::choice_continuation(
        &request.questions,
        &answers,
        onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
    );
    assert!(continuation.requires_model_context);
    assert!(continuation.message.len() <= 4096);
    assert_eq!(
        continuation.context,
        ResultContinuationContext::Choice { answers }
    );
}
