//! Choice presentation, selection summaries, and bounded continuation text.
use super::call::{AdapterPresentationKind, bounded_text};
use crate::operation_registry::v2::contract::AskUserQuestionInput;
use onecrew_local_runtime::{
    CallDisposition, ExecutedBatch, ExecutedCallOutcome, PresentationItem, PresentationProjection,
    PresentationSection, ResultChoiceAnswer, ResultChoiceQuestion, ResultContinuationContext,
    ResultContinuationRequest, ResultResolution, ResultTerminalUpdate, TerminalResult,
};
const MAX_CHOICE_CONTINUATION_BYTES: usize = 4 * 1024;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceCopy {
    prefix: String,
    separator: String,
    suffix: String,
}

fn choice_copy(locale: onecrew_local_runtime::PresentationLocale) -> &'static ChoiceCopy {
    static EN: std::sync::OnceLock<ChoiceCopy> = std::sync::OnceLock::new();
    static ZH: std::sync::OnceLock<ChoiceCopy> = std::sync::OnceLock::new();
    let (cell, source) = match locale {
        onecrew_local_runtime::PresentationLocale::English => {
            (&EN, include_str!("locales/en-US/choice.json"))
        }
        onecrew_local_runtime::PresentationLocale::SimplifiedChinese => {
            (&ZH, include_str!("locales/zh-CN/choice.json"))
        }
    };
    cell.get_or_init(|| serde_json::from_str(source).expect("bundled choice locale is valid"))
}

pub(in crate::operation_registry::v2) const ASK_USER_QUESTION_OPERATION_ID: &str =
    "onecrew.ask_user_question";
pub(in crate::operation_registry::v2) fn choice_batch(
    call_id: &str,
    operation_id: &str,
    selected: bool,
) -> ExecutedBatch {
    ExecutedBatch {
        outcomes: vec![ExecutedCallOutcome {
            call_id: call_id.to_owned(),
            operation_id: operation_id.to_owned(),
            disposition: if selected {
                CallDisposition::Succeeded
            } else {
                CallDisposition::NotRun {
                    reason: "choice_required".to_owned(),
                }
            },
            summary: if selected {
                "The user completed the requested choice.".to_owned()
            } else {
                "The user must answer each question using its selection mode or custom text."
                    .to_owned()
            },
            resource_refs: Vec::new(),
        }],
    }
}

pub(in crate::operation_registry::v2) fn choice_projection(
    input: &AskUserQuestionInput,
) -> PresentationProjection {
    PresentationProjection {
        kind: AdapterPresentationKind::Choice.runtime(),
        title: input.title.clone(),
        summary: Some("Answer each question, then review before submitting.".to_owned()),
        sections: vec![PresentationSection::Items {
            heading: Some("Questions".to_owned()),
            items: input
                .questions
                .iter()
                .map(|question| PresentationItem {
                    title: question.header.clone(),
                    description: Some(question.prompt.clone()),
                })
                .collect(),
        }],
    }
}

pub(in crate::operation_registry::v2) fn selected_choice_update(
    call_id: &str,
    operation_id: &str,
    title: &str,
    questions: &[ResultChoiceQuestion],
    answers: &[ResultChoiceAnswer],
    locale: onecrew_local_runtime::PresentationLocale,
) -> ResultTerminalUpdate {
    let items = questions
        .iter()
        .zip(answers)
        .map(|(question, answer)| PresentationItem {
            title: choice_preview_text(&question.header, "Question"),
            description: answer_text(question, answer).map(|text| match answer {
                ResultChoiceAnswer::Custom { .. } => text.into_owned(),
                ResultChoiceAnswer::Option { .. } | ResultChoiceAnswer::Options { .. } => {
                    choice_preview_text(&text, "Selected option")
                }
            }),
        })
        .collect();
    let batch = choice_batch(call_id, operation_id, true);
    ResultTerminalUpdate {
        terminal: TerminalResult::Resolved(ResultResolution {
            code: "choice_selected".to_owned(),
            summary: "The user completed the requested choice.".to_owned(),
        }),
        batch,
        presentation: PresentationProjection {
            kind: AdapterPresentationKind::Choice.runtime(),
            title: bounded_text(title, "Choice completed"),
            summary: Some("All answers were recorded.".to_owned()),
            sections: vec![PresentationSection::Items {
                heading: Some("Submitted answers".to_owned()),
                items,
            }],
        },
        continuation: Some(choice_continuation(questions, answers, locale)),
    }
}

fn answer_text<'a>(
    question: &'a ResultChoiceQuestion,
    answer: &'a ResultChoiceAnswer,
) -> Option<std::borrow::Cow<'a, str>> {
    match answer {
        ResultChoiceAnswer::Options { option_ids, .. } => {
            let selections = option_ids
                .iter()
                .map(|id| {
                    question
                        .options
                        .iter()
                        .find(|option| &option.id == id)
                        .map(|option| format!("{} [{}]", option.label, option.id))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(std::borrow::Cow::Owned(selections.join("; ")))
        }
        ResultChoiceAnswer::Option { option_id, .. } => question
            .options
            .iter()
            .find(|option| option.id == *option_id)
            .map(|option| std::borrow::Cow::Borrowed(option.label.as_str())),
        ResultChoiceAnswer::Custom { text, .. } => Some(std::borrow::Cow::Borrowed(text)),
    }
}

fn choice_preview_text(value: &str, fallback: &str) -> String {
    let text = bounded_text(value, fallback);
    if text.len() <= 256 {
        return text;
    }
    let mut end = 253;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

pub(in crate::operation_registry::v2) fn choice_continuation(
    questions: &[ResultChoiceQuestion],
    answers: &[ResultChoiceAnswer],
    locale: onecrew_local_runtime::PresentationLocale,
) -> ResultContinuationRequest {
    let copy = choice_copy(locale);
    let mut message = copy.prefix.clone();
    // Multiple answers always require the complete typed context: labels are display
    // copy, while the selected option IDs are the authoritative answer.
    let mut requires_model_context = answers
        .iter()
        .any(|answer| matches!(answer, ResultChoiceAnswer::Options { .. }));
    for (question, answer) in questions.iter().zip(answers) {
        let Some(text) = answer_text(question, answer) else {
            requires_model_context = true;
            continue;
        };
        let header = bounded_text(&question.header, "选择");
        let value = if matches!(answer, ResultChoiceAnswer::Custom { .. }) {
            text.to_string()
        } else {
            bounded_text(&text, "已选择")
        };
        requires_model_context |= header != question.header || value != text.as_ref();
        let line = format!("\n- {}{}{}", header, copy.separator, value);
        if message.len() + line.len() + copy.suffix.len() > MAX_CHOICE_CONTINUATION_BYTES {
            requires_model_context = true;
            break;
        }
        message.push_str(&line);
    }
    message.push_str(&copy.suffix);
    ResultContinuationRequest {
        message,
        context: ResultContinuationContext::Choice {
            answers: answers.to_vec(),
        },
        requires_model_context,
    }
}
