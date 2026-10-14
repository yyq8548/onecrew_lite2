use super::*;

fn multiple_input() -> ExecutedResultInput {
    let mut input = choice_input();
    let Some(PendingUserAction::Choice { questions, .. }) = &mut input.pending_action else {
        unreachable!()
    };
    questions.truncate(1);
    questions[0].selection_mode = ResultChoiceSelectionMode::Multiple;
    input
}

fn multiple(ids: &[&str]) -> Vec<ResultChoiceAnswer> {
    vec![ResultChoiceAnswer::Options {
        question_id: "route".into(),
        option_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
    }]
}

#[test]
fn multiple_selection_is_nonempty_unique_known_mode_checked_and_atomic() {
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(multiple_input()).unwrap());
    for answers in [
        multiple(&[]),
        multiple(&["email", "email"]),
        multiple(&["email", "foreign"]),
        multiple(&["email", "youtube", "third"]),
        vec![ResultChoiceAnswer::Option {
            question_id: "route".into(),
            option_id: "email".into(),
        }],
    ] {
        assert!(store.select(&available.binding, answers).is_err());
        assert_eq!(store.current(&available.binding).unwrap(), available);
    }
    let submitted = store
        .select(&available.binding, multiple(&["youtube", "email"]))
        .unwrap();
    assert_eq!(
        submitted.action,
        SubmittedActionView::Selected {
            answers: multiple(&["email", "youtube"])
        }
    );
    assert!(
        store
            .select(&available.binding, multiple(&["email"]))
            .is_err()
    );
    assert!(
        store
            .select(&submitted.binding, multiple(&["email"]))
            .is_err()
    );
}

#[test]
fn single_mode_rejects_array_and_multiple_custom_is_exclusive_alternative() {
    let store = ResultMemoryStore::new();
    let mut input = multiple_input();
    let Some(PendingUserAction::Choice { questions, .. }) = &mut input.pending_action else {
        unreachable!()
    };
    questions[0].selection_mode = ResultChoiceSelectionMode::Single;
    let available = present(&store, store.mint_from_execute(input).unwrap());
    assert!(
        store
            .select(&available.binding, multiple(&["email"]))
            .is_err()
    );
    store
        .select(
            &available.binding,
            vec![ResultChoiceAnswer::Option {
                question_id: "route".into(),
                option_id: "email".into(),
            }],
        )
        .unwrap();

    let available = present(&store, store.mint_from_execute(multiple_input()).unwrap());
    let answer = ResultChoiceAnswer::Custom {
        question_id: "route".into(),
        text: "另一个选择\nwith detail".into(),
    };
    let selected = store
        .select(&available.binding, vec![answer.clone()])
        .unwrap();
    assert_eq!(
        selected.action,
        SubmittedActionView::Selected {
            answers: vec![answer]
        }
    );
}

#[test]
fn multiple_selection_limit_and_all_selected_ids_are_preserved() {
    let store = ResultMemoryStore::new();
    let mut input = multiple_input();
    let Some(PendingUserAction::Choice { questions, .. }) = &mut input.pending_action else {
        unreachable!()
    };
    questions[0].options = (0..32)
        .map(|i| ResultChoiceOption {
            id: format!("o{i}"),
            label: format!("Option {i}"),
            description: None,
        })
        .collect();
    let available = present(&store, store.mint_from_execute(input).unwrap());
    let ids = (0..32).map(|i| format!("o{i}")).collect::<Vec<_>>();
    let oversized = ResultChoiceAnswer::Options {
        question_id: "route".into(),
        option_ids: (0..33).map(|i| format!("o{i}")).collect(),
    };
    assert!(!oversized.is_well_formed());
    assert!(store.select(&available.binding, vec![oversized]).is_err());
    let submitted = store
        .select(
            &available.binding,
            vec![ResultChoiceAnswer::Options {
                question_id: "route".into(),
                option_ids: ids.iter().rev().cloned().collect(),
            }],
        )
        .unwrap();
    assert_eq!(
        submitted.action,
        SubmittedActionView::Selected {
            answers: vec![ResultChoiceAnswer::Options {
                question_id: "route".into(),
                option_ids: ids
            }]
        }
    );
}

#[test]
fn mode_defaults_to_single_and_answer_shapes_remain_closed() {
    let question = serde_json::json!({"id":"q","header":"Question","prompt":"Pick","options":[{"id":"a","label":"A","description":null}]});
    let decoded: ResultChoiceQuestion = serde_json::from_value(question.clone()).unwrap();
    assert_eq!(decoded.selection_mode, ResultChoiceSelectionMode::Single);
    assert_eq!(
        serde_json::to_value(decoded).unwrap()["selection_mode"],
        "single"
    );
    for mode in [serde_json::Value::Null, serde_json::json!("many")] {
        let mut invalid = question.clone();
        invalid["selection_mode"] = mode;
        assert!(serde_json::from_value::<ResultChoiceQuestion>(invalid).is_err());
    }
    for answer in [
        serde_json::json!({"question_id":"q","option_id":"a","option_ids":["a"]}),
        serde_json::json!({"question_id":"q","option_ids":["a"],"text":"Other"}),
        serde_json::json!({"question_id":"q","option_ids":["a"],"extra":true}),
    ] {
        assert!(serde_json::from_value::<ResultChoiceAnswer>(answer).is_err());
    }
}
