//! Private implementation of the approved V2 meta-tool contract.

mod batch_resume;
mod contract;
mod contract_manifest;
#[cfg(test)]
mod creator_save;
mod execute;
mod mcp;
mod projection;
mod resume;
mod search_get;
mod search_plan;
mod state;

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) use tests::search_plan::{
    RETIRED_PLAN_PROPERTIES, RETIRED_PLAN_WIRE_NAMES, RETIRED_SEARCH_OPERATIONS,
};

pub(super) use contract::{
    AskUserQuestionInput, ToolGetFailure, ToolGetInput, ToolGetResult, ToolSearchInput,
    ToolSearchResult,
};
#[cfg(test)]
pub(super) use contract::{
    PresentationRequest, ResultStateInput, ResultStateOutput, ToolExecuteInput,
    ToolExecutionAuthority, ToolExecutionCall, ToolExecutionOutcome,
};
pub use contract_manifest::{app_presentation_kinds, result_contract_schema_manifest};
#[cfg(test)]
pub(super) use execute::ToolExecuteOrchestrationFailure;
pub(crate) use mcp::apply_v2_tool_metadata;
pub(super) use search_get::{project_tool_get, project_tool_search};
