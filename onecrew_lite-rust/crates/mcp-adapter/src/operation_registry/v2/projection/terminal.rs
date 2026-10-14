//! Confirmation and terminal presentations preserve effect-state precedence.
use super::{
    batch::{batch_presentation_kind, execution_continuation, outcome_summary},
    call::ProjectionCall,
    exact::{
        ExactOmissions, MAX_PROJECTED_FIELDS, MAX_PROJECTION_BYTES, MAX_PROJECTION_SECTIONS,
        ProjectionFailure, ensure_projection_bounds, exact_argument_projection,
        exact_output_projection_with_limit, output_list_projection_with_limits,
    },
};
use crate::operation_registry::{
    execution::ExecuteEffectState, v2::contract::ToolExecutionOutcome,
};
use onecrew_local_runtime::{
    ExecutedBatch, InitialResultState, NoticeTone, PresentationItem, PresentationLocale,
    PresentationProjection, PresentationSection, ResultContinuationContext,
    ResultContinuationRequest, ResultFailure, ResultPresentationKind, ResultResolution,
    ResultTerminalUpdate, ResultUnknown, TerminalResult,
};
use serde_json::Value;

/// The fixed card chrome this projection owns. Submitted values, owner output,
/// provider evidence and identifiers are never translated; only this copy is.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TerminalCopy {
    pub(super) confirmation_fields_heading: String,
    pub(super) confirm_label: String,
    pub(super) operations_heading: String,
    pub(super) execution_result_title: String,
    pub(super) execution_result_unavailable_title: String,
    pub(super) presentation_failed_heading: String,
    pub(super) presentation_failed_summary: String,
    pub(super) presentation_unknown_summary: String,
    pub(super) list_truncated: String,
    pub(super) fields_truncated: String,
    pub(super) fields_withheld: String,
    pub(super) effect_uncertain_call: String,
    pub(super) stopped_effect_uncertain: String,
    pub(super) stopped_outcomes_retained: String,
    pub(super) stopped_before_owner: String,
    pub(super) batch_completed: String,
    pub(super) call_effect_unverified: String,
    pub(super) call_applied_unconfirmed: String,
    pub(super) call_did_not_complete: String,
    pub(super) call_completed: String,
    pub(super) call_requires_confirmation: String,
    pub(super) call_was_not_run: String,
}

pub(super) fn terminal_copy(locale: PresentationLocale) -> &'static TerminalCopy {
    static EN: std::sync::OnceLock<TerminalCopy> = std::sync::OnceLock::new();
    static ZH: std::sync::OnceLock<TerminalCopy> = std::sync::OnceLock::new();
    let (cell, source) = match locale {
        PresentationLocale::English => (&EN, include_str!("locales/en-US/terminal.json")),
        PresentationLocale::SimplifiedChinese => (&ZH, include_str!("locales/zh-CN/terminal.json")),
    };
    cell.get_or_init(|| serde_json::from_str(source).expect("bundled terminal locale is valid"))
}

/// The confirm action's visible label. The App renders it verbatim, so it
/// follows the same card locale as the projection it belongs to.
pub(in crate::operation_registry::v2) fn confirm_action_label(
    locale: PresentationLocale,
) -> String {
    terminal_copy(locale).confirm_label.clone()
}

pub(in crate::operation_registry::v2) fn confirmation_projection(
    call: &ProjectionCall,
    calls: &[ProjectionCall],
    locale: PresentationLocale,
) -> Result<PresentationProjection, ProjectionFailure> {
    let fields = exact_argument_projection(call)?;
    // The approval prompt belongs to the pending action, which the App already
    // renders in its own box. Repeating it here as a notice showed the user the
    // same sentence twice on one card, in two different styles.
    let projection = PresentationProjection {
        kind: batch_presentation_kind(calls).runtime(),
        title: call.title.clone(),
        summary: Some(call.summary.clone()),
        sections: vec![PresentationSection::Fields {
            heading: Some(terminal_copy(locale).confirmation_fields_heading.clone()),
            fields: fields.into_fields(),
        }],
    };
    ensure_projection_bounds(&projection)?;
    Ok(projection)
}

pub(in crate::operation_registry::v2) fn terminal_projection(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
) -> Result<(PresentationProjection, InitialResultState), ProjectionFailure> {
    terminal_projection_with_budget(
        outcomes,
        calls,
        locale,
        MAX_PROJECTION_BYTES,
        MAX_PROJECTION_SECTIONS,
    )
}

pub(in crate::operation_registry::v2) fn terminal_projection_with_budget(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
    max_bytes: usize,
    max_sections: usize,
) -> Result<(PresentationProjection, InitialResultState), ProjectionFailure> {
    let copy = terminal_copy(locale);
    let initial_state = terminal_state(outcomes, calls, locale);
    let summary = match &initial_state {
        InitialResultState::Resolved(resolution) => resolution.summary.clone(),
        InitialResultState::Failed(failure) => failure.summary.clone(),
        InitialResultState::Unknown(unknown) => unknown.summary.clone(),
        InitialResultState::Available => unreachable!("terminal projection is not available"),
    };
    let items = outcomes
        .iter()
        .zip(calls)
        .map(|(outcome, call)| PresentationItem {
            title: call.title.clone(),
            description: Some(outcome_summary(outcome, call, locale)),
        })
        .collect();
    let operation_section = PresentationSection::Items {
        heading: Some(copy.operations_heading.clone()),
        items,
    };
    // A batch shares one card budget. Projecting each read against a full
    // section budget and concatenating the results lets eight individually
    // valid pages overflow the store's one-card limit. The bounded projector
    // allocates the same count/section/byte policy across all completed reads,
    // then shortens only display material while keeping every call represented.
    let sections = bounded_batch_sections(
        outcomes,
        calls,
        locale,
        operation_section,
        &summary,
        max_bytes.min(MAX_PROJECTION_BYTES),
        max_sections.min(MAX_PROJECTION_SECTIONS),
    )?;
    // No notice repeats the summary below. The App already states it as the
    // card's outcome, so adding it here printed the same sentence twice on one
    // card — once under the heading and again as a coloured line.
    let projection = PresentationProjection {
        kind: batch_presentation_kind(calls).runtime(),
        title: copy.execution_result_title.clone(),
        summary: Some(summary.clone()),
        sections,
    };
    ensure_projection_bounds(&projection)?;
    Ok((projection, initial_state))
}

/// One completed output as card sections: a page of records reads row by row,
/// and every other output keeps its exact field table.
///
/// Flattening a provider page leaf by leaf produced labels shaped like
/// `Page · Items · [5] · Media · [1] · Width`. That is the right shape for one
/// record and the wrong shape for twenty-two of them, and no field budget
/// makes it readable — twenty-two search hits are twenty-two rows, not six
/// hundred paths.
fn output_sections(
    result: &Value,
    call: &ProjectionCall,
    locale: PresentationLocale,
    budget: ProjectionBudget,
) -> Result<Vec<PresentationSection>, ProjectionFailure> {
    let copy = terminal_copy(locale);
    if let Some(list) = output_list_projection_with_limits(
        result,
        budget.max_items,
        budget.max_fields,
        budget.max_detail_bytes,
    )? {
        let shown = list.items.len();
        let total = shown + list.omitted;
        let lead_shown = list.lead.len();
        let omissions = list.omissions;
        let mut sections = Vec::new();
        // An effective query or a next cursor is how someone continues the
        // page they are reading, so the rows do not swallow it.
        if lead_shown > 0 {
            sections.push(PresentationSection::Fields {
                heading: Some(call.title.clone()),
                fields: list.lead,
            });
        }
        sections.push(PresentationSection::Items {
            heading: Some(call.title.clone()),
            items: list.items,
        });
        if list.omitted > 0 {
            sections.push(truncation_notice(&copy.list_truncated, shown, total));
        }
        sections.extend(omission_notices(copy, lead_shown, omissions));
        return Ok(sections);
    }
    let (fields, omissions) =
        exact_output_projection_with_limit(result, budget.max_fields)?.into_parts();
    let shown = fields.len();
    let mut sections = vec![PresentationSection::Fields {
        heading: Some(call.title.clone()),
        fields,
    }];
    sections.extend(omission_notices(copy, shown, omissions));
    Ok(sections)
}

#[derive(Clone, Copy)]
struct ProjectionBudget {
    max_items: usize,
    max_fields: usize,
    max_detail_bytes: usize,
}

impl ProjectionBudget {
    fn for_read_count(read_count: usize) -> Self {
        let read_count = read_count.max(1);
        Self {
            max_items: (MAX_PROJECTED_FIELDS / read_count).max(1),
            max_fields: (MAX_PROJECTED_FIELDS / read_count).max(1),
            max_detail_bytes: 1024,
        }
    }

    fn shrink(self) -> Option<Self> {
        if self.max_items > 1 {
            return Some(Self {
                max_items: (self.max_items / 2).max(1),
                ..self
            });
        }
        if self.max_detail_bytes > 128 {
            return Some(Self {
                max_detail_bytes: (self.max_detail_bytes / 2).max(128),
                ..self
            });
        }
        if self.max_fields > 1 {
            return Some(Self {
                max_fields: (self.max_fields / 2).max(1),
                ..self
            });
        }
        None
    }
}

/// Project all completed read outputs under one batch-wide budget. Effectful
/// outputs remain operation bookkeeping and are intentionally not displayed.
fn bounded_batch_sections(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
    operation_section: PresentationSection,
    summary: &str,
    max_bytes: usize,
    max_sections: usize,
) -> Result<Vec<PresentationSection>, ProjectionFailure> {
    let read_count = outcomes
        .iter()
        .zip(calls)
        .filter(|(outcome, call)| {
            !call.may_have_effect && matches!(outcome, ToolExecutionOutcome::Completed { .. })
        })
        .count();
    let compact_notices = read_count > 1;
    let mut budget = ProjectionBudget::for_read_count(read_count);
    loop {
        let mut sections = vec![operation_section.clone()];
        for (outcome, call) in outcomes.iter().zip(calls) {
            let ToolExecutionOutcome::Completed { result, .. } = outcome else {
                continue;
            };
            let projected = output_sections(result, call, locale, budget)?;
            if call.may_have_effect {
                continue;
            }
            if compact_notices {
                sections.extend(compact_notice_sections(projected));
            } else {
                sections.extend(projected);
            }
        }
        if sections.len() <= max_sections
            && serialized_projection_fits(&sections, calls, locale, summary, max_bytes)?
        {
            return Ok(sections);
        }
        if let Some(smaller) = budget.shrink() {
            budget = smaller;
        } else {
            // 安全校验已经完整通过；仅在纯展示空间不足时降为逐调用摘要。
            let summary_sections =
                summary_only_sections(outcomes, calls, locale, operation_section.clone())?;
            if summary_sections.len() <= max_sections
                && serialized_projection_fits(&summary_sections, calls, locale, summary, max_bytes)?
            {
                return Ok(summary_sections);
            }
            return Err(ProjectionFailure::Bounds);
        }
    }
}

fn summary_only_sections(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
    operations: PresentationSection,
) -> Result<Vec<PresentationSection>, ProjectionFailure> {
    let copy = terminal_copy(locale);
    let mut sections = vec![operations];
    for (outcome, call) in outcomes.iter().zip(calls) {
        let ToolExecutionOutcome::Completed { result, .. } = outcome else {
            continue;
        };
        if call.may_have_effect {
            continue;
        }
        let notices = if let Some(list) = output_list_projection_with_limits(result, 1, 1, 128)? {
            let mut notices = vec![truncation_notice(
                &copy.list_truncated,
                0,
                list.items.len() + list.omitted,
            )];
            notices.extend(omission_notices(
                copy,
                0,
                ExactOmissions {
                    truncated: list.lead.len() + list.omissions.truncated,
                    withheld: list.omissions.withheld,
                },
            ));
            notices
        } else {
            let (fields, omissions) = exact_output_projection_with_limit(result, 1)?.into_parts();
            omission_notices(
                copy,
                0,
                ExactOmissions {
                    truncated: fields.len() + omissions.truncated,
                    withheld: omissions.withheld,
                },
            )
        };
        let body = notices
            .into_iter()
            .filter_map(|notice| match notice {
                PresentationSection::Notice { body, .. } => Some(body),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(PresentationSection::Notice {
            heading: Some(call.title.clone()),
            body,
            tone: NoticeTone::Neutral,
        });
    }
    Ok(sections)
}

/// Multiple per-call omission notices are still one display fact for a call.
/// Keeping one notice per call leaves the shared section budget available for
/// the rows and their readable headings.
fn compact_notice_sections(sections: Vec<PresentationSection>) -> Vec<PresentationSection> {
    let mut retained = Vec::with_capacity(sections.len());
    let mut notices = Vec::new();
    for section in sections {
        match section {
            PresentationSection::Notice { body, .. } => notices.push(body),
            section => retained.push(section),
        }
    }
    if !notices.is_empty() {
        retained.push(PresentationSection::Notice {
            heading: None,
            body: notices.join("\n"),
            tone: NoticeTone::Neutral,
        });
    }
    retained
}

fn serialized_projection_fits(
    sections: &[PresentationSection],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
    summary: &str,
    max_bytes: usize,
) -> Result<bool, ProjectionFailure> {
    let projection = PresentationProjection {
        kind: batch_presentation_kind(calls).runtime(),
        title: terminal_copy(locale).execution_result_title.clone(),
        summary: Some(summary.to_owned()),
        sections: sections.to_vec(),
    };
    let bytes = serde_json::to_vec(&projection).map_err(|_| ProjectionFailure::Encoding)?;
    Ok(bytes.len() <= max_bytes)
}

/// What a fields section left out, each reason stated in its own words.
///
/// The two counts stay apart deliberately. Both are display bounds, but a card
/// that adds them together tells a capacity story about a value that was left
/// out for its name, and a reader given no reason supplies one — which is how
/// an uncounted omission became an invented "capacity limit" in front of a
/// user. Saying which bound applied is what keeps the card honest.
fn omission_notices(
    copy: &TerminalCopy,
    shown: usize,
    omissions: ExactOmissions,
) -> Vec<PresentationSection> {
    let mut notices = Vec::new();
    if omissions.truncated > 0 {
        notices.push(truncation_notice(
            &copy.fields_truncated,
            shown,
            shown + omissions.truncated,
        ));
    }
    if omissions.withheld > 0 {
        notices.push(PresentationSection::Notice {
            heading: None,
            body: copy
                .fields_withheld
                .replace("{count}", &omissions.withheld.to_string()),
            tone: NoticeTone::Neutral,
        });
    }
    notices
}

/// What the card left out, in the card's own language. The complete result
/// went back with the call, so this states a display bound, not a loss.
fn truncation_notice(template: &str, shown: usize, total: usize) -> PresentationSection {
    PresentationSection::Notice {
        heading: None,
        body: template
            .replace("{shown}", &shown.to_string())
            .replace("{total}", &total.to_string()),
        tone: NoticeTone::Neutral,
    }
}

pub(in crate::operation_registry::v2) fn resumed_terminal_projection(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    kind: ResultPresentationKind,
    locale: PresentationLocale,
) -> Result<(PresentationProjection, TerminalResult), ProjectionFailure> {
    let (mut presentation, state) = terminal_projection(outcomes, calls, locale)?;
    presentation.kind = kind;
    let terminal = match state {
        InitialResultState::Resolved(resolution) => TerminalResult::Resolved(resolution),
        InitialResultState::Failed(failure) => TerminalResult::Failed(failure),
        InitialResultState::Unknown(unknown) => TerminalResult::Unknown(unknown),
        InitialResultState::Available => unreachable!("resumed execution is terminal"),
    };
    Ok((presentation, terminal))
}
pub(in crate::operation_registry::v2) fn cancelled_continuation(
    locale: PresentationLocale,
) -> ResultContinuationRequest {
    ResultContinuationRequest {
        requires_model_context: false,
        message: super::continuation_copy::continuation_copy(locale)
            .cancelled
            .clone(),
        context: ResultContinuationContext::Cancelled,
    }
}
pub(in crate::operation_registry::v2) fn stranded_terminal_update(
    mut batch: ExecutedBatch,
    kind: ResultPresentationKind,
    code: &str,
    effect_unknown: bool,
    locale: PresentationLocale,
) -> ResultTerminalUpdate {
    let copy = terminal_copy(locale);
    if effect_unknown
        && let Some(pending) = batch.outcomes.iter_mut().find(|outcome| {
            matches!(
                outcome.disposition,
                onecrew_local_runtime::CallDisposition::NotRun { .. }
            )
        })
    {
        pending.disposition = onecrew_local_runtime::CallDisposition::Unknown {
            code: code.to_owned(),
        };
        pending.summary = copy.effect_uncertain_call.clone();
    }
    let unresolved_effect = effect_unknown
        || batch.outcomes.iter().any(|outcome| {
            matches!(
                outcome.disposition,
                onecrew_local_runtime::CallDisposition::Unknown { .. }
            )
        });
    let (terminal, tone, summary) = if unresolved_effect {
        let summary = copy.stopped_effect_uncertain.clone();
        (
            TerminalResult::Unknown(ResultUnknown {
                code: code.to_owned(),
                summary: summary.clone(),
            }),
            NoticeTone::Warning,
            summary,
        )
    } else {
        let summary = if batch.outcomes.iter().any(|outcome| {
            !matches!(
                outcome.disposition,
                onecrew_local_runtime::CallDisposition::NotRun { .. }
            )
        }) {
            copy.stopped_outcomes_retained.clone()
        } else {
            copy.stopped_before_owner.clone()
        };
        (
            TerminalResult::Failed(ResultFailure {
                code: code.to_owned(),
                summary: summary.clone(),
                retryable: false,
            }),
            NoticeTone::Critical,
            summary,
        )
    };
    ResultTerminalUpdate {
        terminal,
        batch: batch.clone(),
        presentation: PresentationProjection {
            kind,
            title: copy.execution_result_title.clone(),
            summary: Some(summary.clone()),
            sections: vec![PresentationSection::Notice {
                heading: None,
                body: summary,
                tone,
            }],
        },
        continuation: Some(execution_continuation(&batch, locale)),
    }
}

pub(in crate::operation_registry::v2) fn presentation_failure_terminal_update(
    batch: ExecutedBatch,
    kind: ResultPresentationKind,
    locale: PresentationLocale,
) -> ResultTerminalUpdate {
    let (presentation, terminal) = presentation_failure_frame(&batch, kind, locale);
    ResultTerminalUpdate {
        terminal,
        continuation: Some(execution_continuation(&batch, locale)),
        batch,
        presentation,
    }
}

/// The same card, for a mint whose result could not be projected.
///
/// The owners have already run by then, so the caller gets a card that says
/// the result cannot be shown completely — not a null reference and nothing
/// to look at.
pub(in crate::operation_registry::v2) fn presentation_failure_projection(
    batch: &ExecutedBatch,
    calls: &[ProjectionCall],
    locale: PresentationLocale,
) -> (PresentationProjection, InitialResultState) {
    let (presentation, terminal) =
        presentation_failure_frame(batch, batch_presentation_kind(calls).runtime(), locale);
    let state = match terminal {
        TerminalResult::Resolved(resolution) => InitialResultState::Resolved(resolution),
        TerminalResult::Failed(failure) => InitialResultState::Failed(failure),
        TerminalResult::Unknown(unknown) => InitialResultState::Unknown(unknown),
    };
    (presentation, state)
}

fn presentation_failure_frame(
    batch: &ExecutedBatch,
    kind: ResultPresentationKind,
    locale: PresentationLocale,
) -> (PresentationProjection, TerminalResult) {
    let copy = terminal_copy(locale);
    let unknown = batch.outcomes.iter().any(|outcome| {
        matches!(
            outcome.disposition,
            onecrew_local_runtime::CallDisposition::Unknown { .. }
        )
    });
    let summary = if unknown {
        copy.presentation_unknown_summary.clone()
    } else {
        copy.presentation_failed_summary.clone()
    };
    let terminal = if unknown {
        TerminalResult::Unknown(ResultUnknown {
            code: "presentation_unavailable".into(),
            summary: summary.clone(),
        })
    } else {
        TerminalResult::Failed(ResultFailure {
            code: "presentation_unavailable".into(),
            summary: summary.clone(),
            retryable: false,
        })
    };
    (
        PresentationProjection {
            kind,
            title: copy.execution_result_unavailable_title.clone(),
            summary: Some(summary.clone()),
            sections: vec![PresentationSection::Notice {
                heading: Some(copy.presentation_failed_heading.clone()),
                body: summary,
                tone: NoticeTone::Critical,
            }],
        },
        terminal,
    )
}
fn terminal_state(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
) -> InitialResultState {
    let copy = terminal_copy(locale);
    for (outcome, call) in outcomes.iter().zip(calls) {
        if let ToolExecutionOutcome::Unknown { failure, .. } = outcome {
            // Both states tell the reader not to run the call again, but for
            // opposite reasons: one effect is proven to have happened, the
            // other cannot be confirmed either way. Saying so precisely is
            // what keeps someone from sending the same email twice.
            let summary = if failure.effect == ExecuteEffectState::Applied {
                &copy.call_applied_unconfirmed
            } else {
                &copy.call_effect_unverified
            };
            return InitialResultState::Unknown(ResultUnknown {
                code: failure.code.as_str().to_owned(),
                summary: summary.replace("{call}", &call.title),
            });
        }
    }
    for (outcome, call) in outcomes.iter().zip(calls) {
        if let ToolExecutionOutcome::Failed { failure, .. } = outcome {
            return InitialResultState::Failed(ResultFailure {
                code: failure.code.as_str().to_owned(),
                summary: copy.call_did_not_complete.replace("{call}", &call.title),
                retryable: failure.retry_safe,
            });
        }
    }
    InitialResultState::Resolved(ResultResolution {
        code: "completed".to_owned(),
        summary: copy.batch_completed.clone(),
    })
}
