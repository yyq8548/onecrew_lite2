//! Execute request/result DTOs and strict unique-call decoding.
use crate::operation_registry::{
    descriptor::{ConnectionRef, EntryId},
    execution::ExecuteFailurePresentation,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(in crate::operation_registry::v2) const MAX_BATCH_OPERATIONS: usize = 8;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolExecuteInput {
    #[serde(deserialize_with = "deserialize_execution_calls")]
    #[schemars(length(min = 1, max = 8))]
    pub(in crate::operation_registry) calls: Vec<ToolExecutionCall>,
    pub(in crate::operation_registry) presentation: PresentationRequest,
}

impl ToolExecuteInput {
    #[cfg(test)]
    pub(in crate::operation_registry::v2) fn valid(&self) -> bool {
        (1..=MAX_BATCH_OPERATIONS).contains(&self.calls.len())
            && self
                .calls
                .iter()
                .map(|call| &call.call_id)
                .collect::<BTreeSet<_>>()
                .len()
                == self.calls.len()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolExecutionCall {
    pub(in crate::operation_registry) call_id: EntryId,
    pub(in crate::operation_registry) operation_id: EntryId,
    pub(in crate::operation_registry) schema_revision: String,
    pub(in crate::operation_registry) authority: ToolExecutionAuthority,
    pub(in crate::operation_registry) connection: Option<ConnectionRef>,
    pub(in crate::operation_registry) arguments: Map<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ToolExecutionAuthority {
    Direct {},
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(in crate::operation_registry) enum PresentationRequest {
    Never,
    IfRequired,
    Requested,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolExecuteResult {
    pub(in crate::operation_registry) batch_id: String,
    #[schemars(length(min = 1, max = 8))]
    pub(in crate::operation_registry) outcomes: Vec<ToolExecutionOutcome>,
    pub(in crate::operation_registry) result_ref: Option<onecrew_local_runtime::ResultRef>,
    pub(in crate::operation_registry) presentation_failure: Option<ToolPresentationFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ToolPresentationFailure {
    ProjectionUnavailable,
    Capacity,
    StoreRejected,
    StoreUnavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ToolExecutionOutcome {
    Completed {
        call_id: EntryId,
        operation_id: EntryId,
        result: Value,
    },
    InputRequired {
        call_id: EntryId,
        operation_id: EntryId,
        input_kind: ToolInputKind,
        /// Owner-projected Creator save status facts, present only for a
        /// Creator review and never sourced from App metadata.
        #[serde(skip_serializing_if = "Option::is_none")]
        review_summary: Option<onecrew_local_runtime::CreatorSaveReviewSummary>,
    },
    Failed {
        call_id: EntryId,
        operation_id: EntryId,
        failure: ExecuteFailurePresentation,
        /// Owner-projected Creator save status facts when preparation itself
        /// reached a typed terminal failure without creating a review.
        #[serde(skip_serializing_if = "Option::is_none")]
        review_summary: Option<onecrew_local_runtime::CreatorSaveReviewSummary>,
    },
    Unknown {
        call_id: EntryId,
        operation_id: EntryId,
        failure: ExecuteFailurePresentation,
        /// Owner-projected Creator save status facts when preparation itself
        /// reached an uncertain terminal outcome.
        #[serde(skip_serializing_if = "Option::is_none")]
        review_summary: Option<onecrew_local_runtime::CreatorSaveReviewSummary>,
    },
    Skipped {
        call_id: EntryId,
        operation_id: EntryId,
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(in crate::operation_registry) enum ToolInputKind {
    Confirmation,
    Choice,
}
fn deserialize_execution_calls<'de, D>(deserializer: D) -> Result<Vec<ToolExecutionCall>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<ToolExecutionCall>::deserialize(deserializer)?;
    ((1..=MAX_BATCH_OPERATIONS).contains(&values.len())
        && values
            .iter()
            .map(|call| &call.call_id)
            .collect::<BTreeSet<_>>()
            .len()
            == values.len())
    .then_some(values)
    .ok_or_else(|| serde::de::Error::custom("calls must contain 1 to 8 unique call_id values"))
}
