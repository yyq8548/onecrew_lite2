use onecrew_local_runtime::ToolkitId;
mod email;
mod instagram;
mod onecrew;
mod telegram;
mod user_questions;
mod x;
mod youtube;

#[derive(Clone, Copy)]
pub(super) struct OperationEvalCase {
    pub(super) query: &'static str,
    pub(super) toolkit: ToolkitId,
    pub(super) expected_id: &'static str,
}

pub(super) const OPERATION_EVAL_CASES: &[OperationEvalCase] = &aggregate_cases();

/// Preserve the original toolkit and record order for exhaustive recall evaluation.
const fn aggregate_cases() -> [OperationEvalCase; 159] {
    let sources = [
        onecrew::CASES,
        email::CASES,
        instagram::CASES,
        user_questions::CASES,
        telegram::CASES,
        x::CASES,
        youtube::CASES,
    ];
    let mut cases = [sources[0][0]; 159];
    let mut source_index = 0;
    let mut target_index = 0;
    while source_index < sources.len() {
        let source = sources[source_index];
        let mut record_index = 0;
        while record_index < source.len() {
            cases[target_index] = source[record_index];
            target_index += 1;
            record_index += 1;
        }
        source_index += 1;
    }
    assert!(target_index == cases.len());
    cases
}
