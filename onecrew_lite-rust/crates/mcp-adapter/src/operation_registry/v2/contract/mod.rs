//! Typed V2 meta-surface contracts and host-facing schema projections.
//!
//! Runtime decoding stays strict even where an MCP Apps descriptor uses a
//! flatter object shape for host interoperability.

mod ask_user_question;
mod execution;
mod presentation;
mod search_get;
mod state;

pub(super) use ask_user_question::valid_choice_questions;
pub(in crate::operation_registry) use ask_user_question::{AskUserQuestionInput, UserQuestionId};
pub(super) use execution::MAX_BATCH_OPERATIONS;
#[cfg(test)]
pub(in crate::operation_registry) use execution::ToolExecutionAuthority;
pub(in crate::operation_registry) use execution::{
    PresentationRequest, ToolExecuteInput, ToolExecuteResult, ToolExecutionCall,
    ToolExecutionOutcome, ToolInputKind, ToolPresentationFailure,
};
pub(in crate::operation_registry) use presentation::{
    ResultPresentInput, ResultPresentResult, ResultPresentStatus,
};
pub(super) use search_get::{MAX_SEARCH_GUIDES, MAX_SEARCH_OPERATIONS, MAX_WORKFLOW_HINTS};
pub(in crate::operation_registry) use search_get::{
    OperationExecutionContract, ToolGetFailure, ToolGetInput, ToolGetResult, ToolSearchInput,
    ToolSearchLimit, ToolSearchOperation, ToolSearchResult, ToolkitSearchGuide, WorkflowHint,
};
pub(super) use state::ResultStateWireSchema;
pub(in crate::operation_registry) use state::{
    ResultStateFailure, ResultStateInput, ResultStateOutput,
};

#[cfg(test)]
pub(in crate::operation_registry) const MODEL_TOOL_NAMES: [&str; 5] = [
    "onecrew_connection_list",
    "onecrew_tool_search",
    "onecrew_tool_get",
    "onecrew_tool_execute",
    "onecrew_result_present",
];

#[cfg(test)]
pub(in crate::operation_registry) const APP_ONLY_TOOL_NAME: &str = "onecrew_result_state";

#[cfg(test)]
pub(in crate::operation_registry) const SKILLS_MODEL_TOOL_NAMES: [&str; 2] =
    ["onecrew_tool_search", "onecrew_tool_get"];
