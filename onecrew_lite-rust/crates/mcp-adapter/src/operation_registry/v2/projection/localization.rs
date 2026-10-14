//! 仅翻译 adapter 生成的固定文案；原生值、标识和业务状态保持不变。
use super::terminal::{TerminalCopy, terminal_copy};
use onecrew_local_runtime::{
    NoticeTone, PresentationItem, PresentationLocale, PresentationProjection, PresentationSection,
    ResultFailure, ResultResolution, ResultSnapshot, ResultState, ResultUnknown,
};

pub(in crate::operation_registry::v2) fn localize_adapter_presentation(
    snapshot: ResultSnapshot,
) -> ResultSnapshot {
    let source = terminal_copy(PresentationLocale::English);
    let target = terminal_copy(snapshot.locale);
    let failure = matches!(&snapshot.state,
        ResultState::Failed(value) if value.code == "presentation_unavailable")
        || matches!(&snapshot.state,
            ResultState::Unknown(value) if value.code == "presentation_unavailable");
    let adapter_terminal = snapshot.pending_action.is_none()
        && matches!(
            snapshot.presentation.kind.as_str(),
            "read_only" | "batch_result" | "record_preview" | "simple_operation" | "outreach_draft"
        );
    let unknown = matches!(snapshot.state, ResultState::Unknown(_));
    let state = localize_state(&snapshot.state, source, target, adapter_terminal);
    let projection = &snapshot.presentation;
    let presentation = PresentationProjection {
        kind: projection.kind.clone(),
        title: fixed_text(
            &projection.title,
            source,
            target,
            adapter_terminal || failure,
            unknown,
        ),
        summary: projection
            .summary
            .as_ref()
            .map(|value| fixed_text(value, source, target, adapter_terminal || failure, unknown)),
        sections: projection
            .sections
            .iter()
            .enumerate()
            .map(|(index, section)| {
                if adapter_terminal || failure || index < snapshot.completed_sections_count {
                    localize_section(section, source, target, failure, unknown)
                } else {
                    section.clone()
                }
            })
            .collect(),
    };
    ResultSnapshot {
        presentation,
        state,
        ..snapshot
    }
}

fn localize_state(
    state: &ResultState,
    source: &TerminalCopy,
    target: &TerminalCopy,
    adapter: bool,
) -> ResultState {
    match state {
        ResultState::Failed(value) if value.code == "presentation_unavailable" => {
            ResultState::Failed(ResultFailure {
                summary: target.presentation_failed_summary.clone(),
                ..value.clone()
            })
        }
        ResultState::Unknown(value) if value.code == "presentation_unavailable" => {
            ResultState::Unknown(ResultUnknown {
                summary: target.presentation_unknown_summary.clone(),
                ..value.clone()
            })
        }
        ResultState::Resolved(value)
            if adapter && value.code == "completed" && value.summary == source.batch_completed =>
        {
            ResultState::Resolved(ResultResolution {
                summary: target.batch_completed.clone(),
                ..value.clone()
            })
        }
        _ => state.clone(),
    }
}

fn fixed_text(
    text: &str,
    source: &TerminalCopy,
    target: &TerminalCopy,
    owned: bool,
    unknown: bool,
) -> String {
    if owned {
        for (from, to) in [
            (
                &source.execution_result_title,
                &target.execution_result_title,
            ),
            (
                &source.execution_result_unavailable_title,
                &target.execution_result_unavailable_title,
            ),
            (&source.batch_completed, &target.batch_completed),
            (
                &source.presentation_failed_summary,
                if unknown {
                    &target.presentation_unknown_summary
                } else {
                    &target.presentation_failed_summary
                },
            ),
            (
                &source.presentation_unknown_summary,
                &target.presentation_unknown_summary,
            ),
        ] {
            if text == from {
                return to.clone();
            }
        }
    }
    text.to_owned()
}

fn localize_section(
    section: &PresentationSection,
    source: &TerminalCopy,
    target: &TerminalCopy,
    failure: bool,
    unknown: bool,
) -> PresentationSection {
    match section {
        PresentationSection::Items { heading, items }
            if heading.as_deref() == Some(&source.operations_heading) =>
        {
            PresentationSection::Items {
                heading: Some(target.operations_heading.clone()),
                items: items
                    .iter()
                    .map(|item| PresentationItem {
                        title: item.title.clone(),
                        description: item
                            .description
                            .as_ref()
                            .map(|text| operation_text(text, &item.title, source, target)),
                    })
                    .collect(),
            }
        }
        PresentationSection::Notice {
            heading,
            body,
            tone,
        } if failure
            && heading.as_deref() == Some(&source.presentation_failed_heading)
            && (body == &source.presentation_failed_summary
                || body == &source.presentation_unknown_summary) =>
        {
            PresentationSection::Notice {
                heading: Some(target.presentation_failed_heading.clone()),
                body: if unknown {
                    target.presentation_unknown_summary.clone()
                } else {
                    target.presentation_failed_summary.clone()
                },
                tone: *tone,
            }
        }
        PresentationSection::Notice {
            heading,
            body,
            tone: NoticeTone::Neutral,
        } => PresentationSection::Notice {
            heading: heading.clone(),
            body: body
                .lines()
                .map(|line| omission_text(line, source, target))
                .collect::<Vec<_>>()
                .join("\n"),
            tone: NoticeTone::Neutral,
        },
        _ => section.clone(),
    }
}

fn operation_text(text: &str, title: &str, source: &TerminalCopy, target: &TerminalCopy) -> String {
    for (from, to) in [
        (&source.call_completed, &target.call_completed),
        (
            &source.call_requires_confirmation,
            &target.call_requires_confirmation,
        ),
        (&source.call_was_not_run, &target.call_was_not_run),
        (&source.call_did_not_complete, &target.call_did_not_complete),
        (
            &source.call_applied_unconfirmed,
            &target.call_applied_unconfirmed,
        ),
        (
            &source.call_effect_unverified,
            &target.call_effect_unverified,
        ),
    ] {
        if text == from.replace("{call}", title) {
            return to.replace("{call}", title);
        }
    }
    text.to_owned()
}

fn omission_text(text: &str, source: &TerminalCopy, target: &TerminalCopy) -> String {
    for (from, to, keys) in [
        (
            &source.list_truncated,
            &target.list_truncated,
            &["shown", "total"][..],
        ),
        (
            &source.fields_truncated,
            &target.fields_truncated,
            &["shown", "total"][..],
        ),
        (
            &source.fields_withheld,
            &target.fields_withheld,
            &["count"][..],
        ),
    ] {
        if let Some(value) = numeric_template(text, from, to, keys) {
            return value;
        }
    }
    text.to_owned()
}

/// 只接受完整匹配的内部模板与十进制计数，不猜测或翻译任意 owner 文本。
fn numeric_template(
    mut text: &str,
    mut source: &str,
    target: &str,
    keys: &[&str],
) -> Option<String> {
    let mut translated = target.to_owned();
    for key in keys {
        let placeholder = format!("{{{key}}}");
        let (prefix, rest) = source.split_once(&placeholder)?;
        text = text.strip_prefix(prefix)?;
        let length = text.bytes().take_while(u8::is_ascii_digit).count();
        let number = text.get(..length)?;
        if number.is_empty() || number.parse::<usize>().ok()?.to_string() != number {
            return None;
        }
        translated = translated.replace(&placeholder, number);
        text = &text[length..];
        source = rest;
    }
    (text == source).then_some(translated)
}
