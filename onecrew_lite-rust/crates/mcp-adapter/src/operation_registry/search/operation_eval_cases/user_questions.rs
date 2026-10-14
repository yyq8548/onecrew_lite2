use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[OperationEvalCase {
    query: "Ask the user a question",
    toolkit: ToolkitId::OneCrew,
    expected_id: "onecrew.ask_user_question",
}];
