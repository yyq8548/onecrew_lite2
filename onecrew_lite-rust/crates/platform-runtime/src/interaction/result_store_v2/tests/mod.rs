use std::time::{Duration, Instant};

use super::memory::{ResultMemoryStore, StoreLimits};
use super::validation::{result_payload_bytes, terminal_update_payload_bytes};
use super::*;

mod bounds;
mod capacity;
mod choice_multiple;
mod continuation;
pub(super) mod creator_save_batch;
mod edit;
mod search_plan;
mod state;

fn origin(scope_id: &str) -> ResultOrigin {
    ResultOrigin::new(scope_id).unwrap()
}

fn batch() -> ExecutedBatch {
    ExecutedBatch {
        outcomes: vec![ExecutedCallOutcome {
            call_id: "call-1".to_owned(),
            operation_id: "x.users.search".to_owned(),
            disposition: CallDisposition::Succeeded,
            summary: "Found bounded creator candidates.".to_owned(),
            resource_refs: vec![ResultResourceRef {
                kind: ResultResourceKind::Creator,
                id: "candidate-1".to_owned(),
                label: Some("Creator candidate".to_owned()),
            }],
        }],
    }
}

fn projection() -> PresentationProjection {
    PresentationProjection {
        kind: ResultPresentationKind::new("creator_discovery").unwrap(),
        title: "Creator discovery".to_owned(),
        summary: Some("Review the confirmed discovery result.".to_owned()),
        sections: vec![PresentationSection::Fields {
            heading: Some("Search".to_owned()),
            fields: vec![PresentationField {
                label: "Keywords".to_owned(),
                value: "beauty makeup".to_owned(),
            }],
        }],
    }
}

fn read_only_input(state: InitialResultState) -> ExecutedResultInput {
    ExecutedResultInput {
        origin: origin("origin-one"),
        view_kind: ResultViewKind::ReadOnly,
        batch: batch(),
        presentation: projection(),
        pending_action: None,
        initial_state: state,
    }
}

fn confirmation_input(payload: &'static [u8]) -> ExecutedResultInput {
    ExecutedResultInput {
        origin: origin("origin-one"),
        view_kind: ResultViewKind::Confirmation,
        batch: batch(),
        presentation: projection(),
        pending_action: Some(PendingUserAction::Confirm {
            prompt: "Save the creator?".to_owned(),
            label: "Save creator".to_owned(),
            payload: OwnerActionPayload::new(payload).unwrap(),
        }),
        initial_state: InitialResultState::Available,
    }
}

fn choice_input() -> ExecutedResultInput {
    ExecutedResultInput {
        origin: origin("origin-one"),
        view_kind: ResultViewKind::Choice,
        batch: batch(),
        presentation: projection(),
        pending_action: Some(PendingUserAction::Choice {
            questions: vec![
                ResultChoiceQuestion {
                    selection_mode: Default::default(),
                    id: "route".to_owned(),
                    header: "Contact route".to_owned(),
                    prompt: "Which route should be used?".to_owned(),
                    options: vec![
                        ResultChoiceOption {
                            id: "email".to_owned(),
                            label: "Email".to_owned(),
                            description: Some("Use the verified email route.".to_owned()),
                        },
                        ResultChoiceOption {
                            id: "youtube".to_owned(),
                            label: "YouTube".to_owned(),
                            description: None,
                        },
                    ],
                },
                ResultChoiceQuestion {
                    selection_mode: Default::default(),
                    id: "review".to_owned(),
                    header: "Review depth".to_owned(),
                    prompt: "How much should the Agent review?".to_owned(),
                    options: vec![
                        ResultChoiceOption {
                            id: "summary".to_owned(),
                            label: "Summary".to_owned(),
                            description: None,
                        },
                        ResultChoiceOption {
                            id: "full".to_owned(),
                            label: "Full review".to_owned(),
                            description: None,
                        },
                    ],
                },
                ResultChoiceQuestion {
                    selection_mode: Default::default(),
                    id: "notify".to_owned(),
                    header: "Notification".to_owned(),
                    prompt: "Notify when complete?".to_owned(),
                    options: vec![
                        ResultChoiceOption {
                            id: "yes".to_owned(),
                            label: "Yes".to_owned(),
                            description: None,
                        },
                        ResultChoiceOption {
                            id: "no".to_owned(),
                            label: "No".to_owned(),
                            description: None,
                        },
                    ],
                },
            ],
            payload: OwnerActionPayload::new(b"owner:choice".to_vec()).unwrap(),
        }),
        initial_state: InitialResultState::Available,
    }
}

fn choice_answers() -> Vec<ResultChoiceAnswer> {
    vec![
        ResultChoiceAnswer::Option {
            question_id: "notify".to_owned(),
            option_id: "yes".to_owned(),
        },
        ResultChoiceAnswer::Option {
            question_id: "route".to_owned(),
            option_id: "youtube".to_owned(),
        },
        ResultChoiceAnswer::Option {
            question_id: "review".to_owned(),
            option_id: "full".to_owned(),
        },
    ]
}

fn ordered_choice_answers() -> Vec<ResultChoiceAnswer> {
    vec![
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

fn execution_continuation_request() -> ResultContinuationRequest {
    ResultContinuationRequest {
        requires_model_context: false,
        message: "The confirmed operation finished. Continue the current task.".to_owned(),
        context: ResultContinuationContext::Execution {
            outcomes: vec![ResultContinuationOutcomeSummary {
                call_id: "call-1".to_owned(),
                operation_id: "x.users.search".to_owned(),
                disposition: CallDisposition::Succeeded,
                summary: "Found bounded creator candidates.".to_owned(),
                resource_refs: vec![],
            }],
        },
    }
}

fn choice_continuation_request() -> ResultContinuationRequest {
    ResultContinuationRequest {
        requires_model_context: false,
        message: "The user completed the choice card. Continue with these answers.".to_owned(),
        context: ResultContinuationContext::Choice {
            answers: ordered_choice_answers(),
        },
    }
}

fn present(store: &ResultMemoryStore, result_ref: ResultRef) -> ResultSnapshot {
    store.present(&result_ref, &origin("origin-one")).unwrap()
}

fn terminal_update(terminal: TerminalResult) -> ResultTerminalUpdate {
    ResultTerminalUpdate {
        terminal,
        batch: batch(),
        presentation: projection(),
        continuation: None,
    }
}
