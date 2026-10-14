//! 完整批次共享只读展示预算；已经审阅的历史前缀只追加、不改写。
use super::{ProjectionCall, exact::ProjectionFailure, terminal_projection_with_budget};
use crate::operation_registry::v2::contract::ToolExecutionOutcome;
use onecrew_local_runtime::{PresentationLocale, PresentationSection};

const MAX_READONLY_PREFIX_BYTES: usize = 64 * 1024;
const MAX_READONLY_PREFIX_SECTIONS: usize = 32;

/// 按原始批次的调用数分配展示份额，为尚未执行的阶段预留空间。
/// 份额约束只读展示，不限制 provider 调用或 Creator 业务数量。
/// 精确 owner 回执不占通用只读预算，原位置与内容均保持不变。
pub(in crate::operation_registry::v2) fn readonly_prefix_projection(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
    fixed_sections: &[PresentationSection],
    batch_call_count: usize,
) -> Result<Vec<PresentationSection>, ProjectionFailure> {
    if !(1..=8).contains(&batch_call_count) {
        return Err(ProjectionFailure::Bounds);
    }
    let completed = outcomes
        .iter()
        .zip(calls)
        .filter(|(outcome, _)| matches!(outcome, ToolExecutionOutcome::Completed { .. }))
        .map(|(outcome, call)| (outcome.clone(), call.clone()))
        .collect::<Vec<_>>();
    if completed.is_empty() {
        return Ok(fixed_sections.to_vec());
    }
    if completed.len() > batch_call_count {
        return Err(ProjectionFailure::Bounds);
    }
    let (outcomes, calls): (Vec<_>, Vec<_>) = completed.into_iter().unzip();
    let (projection, _) = terminal_projection_with_budget(
        &outcomes,
        &calls,
        locale,
        MAX_READONLY_PREFIX_BYTES * outcomes.len() / batch_call_count,
        MAX_READONLY_PREFIX_SECTIONS * outcomes.len() / batch_call_count,
    )?;
    let combined = fixed_sections
        .iter()
        .cloned()
        .chain(projection.sections)
        .collect::<Vec<_>>();
    let readonly = combined
        .iter()
        .filter(|section| !matches!(section, PresentationSection::CreatorSaveRows { .. }))
        .collect::<Vec<_>>();
    if readonly.len() > MAX_READONLY_PREFIX_SECTIONS
        || serde_json::to_vec(&readonly)
            .map_err(|_| ProjectionFailure::Encoding)?
            .len()
            > MAX_READONLY_PREFIX_BYTES
    {
        return Err(ProjectionFailure::Bounds);
    }
    Ok(combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_registry::{descriptor::EntryId, v2::projection::AdapterPresentationKind};

    #[test]
    fn tight_prefix_budget_keeps_a_summary_without_masking_unsafe_output() {
        let call = ProjectionCall {
            call_id: EntryId::parse("read-1").unwrap(),
            operation_id: EntryId::parse("onecrew.test.read").unwrap(),
            title: "Read evidence".into(),
            summary: "Public evidence".into(),
            arguments: serde_json::Map::new(),
            presentable: true,
            may_have_effect: false,
            kind: AdapterPresentationKind::ReadOnly,
        };
        let outcome = ToolExecutionOutcome::Completed {
            call_id: call.call_id.clone(),
            operation_id: call.operation_id.clone(),
            result: serde_json::json!({"text": "x".repeat(8192)}),
        };
        let sections = readonly_prefix_projection(
            &[outcome],
            std::slice::from_ref(&call),
            PresentationLocale::English,
            &[],
            8,
        )
        .expect("one valid large value can be omitted from a small summary");
        assert!(serde_json::to_vec(&sections).unwrap().len() <= 8192);
        assert!(sections.iter().any(|section| matches!(section, PresentationSection::Notice { body, .. } if body.contains("0 of 1"))));
        let invalid = ToolExecutionOutcome::Completed {
            call_id: call.call_id.clone(),
            operation_id: call.operation_id.clone(),
            result: serde_json::json!({"next_cursor": "one", "next-cursor": "two"}),
        };
        assert!(matches!(
            readonly_prefix_projection(&[invalid], &[call], PresentationLocale::English, &[], 8),
            Err(ProjectionFailure::LabelCollision)
        ));
    }
}
