//! Search/get DTOs, bounded search limits, and operation-ID decoding.
use super::execution::MAX_BATCH_OPERATIONS;
use crate::operation_registry::{
    descriptor::{
        ApprovalClass, ConnectionRequirement, EffectClass, EntryId, OperationSafety, SearchText,
        SecretClass, deserialize_effect_filters, deserialize_toolkit_filters,
    },
    directory::ToolkitOperationSummary,
};
use onecrew_local_runtime::ToolkitId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub(in crate::operation_registry::v2) const MAX_SEARCH_OPERATIONS: usize = 8;
pub(in crate::operation_registry::v2) const MAX_SEARCH_GUIDES: usize = 8;
pub(in crate::operation_registry::v2) const MAX_WORKFLOW_HINTS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolSearchInput {
    pub(in crate::operation_registry) query: SearchText,
    #[serde(default, deserialize_with = "deserialize_toolkit_filters")]
    #[schemars(length(max = 8))]
    pub(in crate::operation_registry) toolkits: Vec<ToolkitId>,
    #[serde(default, deserialize_with = "deserialize_effect_filters")]
    #[schemars(length(max = 7))]
    pub(in crate::operation_registry) effects: Vec<EffectClass>,
    pub(in crate::operation_registry) limit: Option<ToolSearchLimit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(in crate::operation_registry) struct ToolSearchLimit(pub(in crate::operation_registry::v2) u8);

impl JsonSchema for ToolSearchLimit {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ToolSearchLimit".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "integer",
            "minimum": 1,
            "maximum": 8
        })
    }
}

impl<'de> Deserialize<'de> for ToolSearchLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        (1..=8)
            .contains(&value)
            .then_some(Self(value))
            .ok_or_else(|| serde::de::Error::custom("search limit must be between 1 and 8"))
    }
}

impl ToolSearchLimit {
    pub(in crate::operation_registry::v2) fn get(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolSearchResult {
    #[schemars(length(max = 8))]
    pub(in crate::operation_registry) operations: Vec<ToolSearchOperation>,
    #[schemars(length(max = 8))]
    pub(in crate::operation_registry) toolkit_guides: Vec<ToolkitSearchGuide>,
    #[schemars(length(max = 3))]
    pub(in crate::operation_registry) workflow_hints: Vec<WorkflowHint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolSearchOperation {
    pub(in crate::operation_registry) id: EntryId,
    pub(in crate::operation_registry) toolkit: ToolkitId,
    pub(in crate::operation_registry) title: String,
    pub(in crate::operation_registry) summary: String,
    #[schemars(length(max = 7))]
    pub(in crate::operation_registry) effects: Vec<EffectClass>,
    pub(in crate::operation_registry) connection: ConnectionRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolkitSearchGuide {
    pub(in crate::operation_registry) toolkit: ToolkitId,
    pub(in crate::operation_registry) title: String,
    pub(in crate::operation_registry) summary: String,
    pub(in crate::operation_registry) purpose: String,
    pub(in crate::operation_registry) connection_model: String,
    #[schemars(length(max = 16))]
    pub(in crate::operation_registry) identifier_semantics: Vec<String>,
    #[schemars(length(max = 16))]
    pub(in crate::operation_registry) capability_categories: Vec<String>,
    #[schemars(length(max = 16))]
    pub(in crate::operation_registry) gotchas: Vec<String>,
    #[schemars(length(max = 64))]
    pub(in crate::operation_registry) operations: Vec<ToolkitOperationSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct WorkflowHint {
    pub(in crate::operation_registry) id: EntryId,
    pub(in crate::operation_registry) title: String,
    pub(in crate::operation_registry) summary: String,
    #[schemars(length(min = 1, max = 8))]
    pub(in crate::operation_registry) operation_ids: Vec<EntryId>,
    #[schemars(length(min = 1, max = 8))]
    pub(in crate::operation_registry) steps: Vec<String>,
    #[schemars(length(max = 8))]
    pub(in crate::operation_registry) pitfalls: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolGetInput {
    #[serde(deserialize_with = "deserialize_operation_ids")]
    #[schemars(length(min = 1, max = 8))]
    pub(in crate::operation_registry) operation_ids: Vec<EntryId>,
    /// Set true only when a later call or result presentation must address
    /// result fields you have not seen yet; execute outcomes are
    /// self-describing JSON.
    #[serde(default)]
    pub(in crate::operation_registry) include_output_schema: bool,
}

impl ToolGetInput {
    pub(in crate::operation_registry::v2) fn valid(&self) -> bool {
        unique_bounded(&self.operation_ids, MAX_BATCH_OPERATIONS)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ToolGetFailure {
    InvalidBatch,
    OperationUnavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ToolGetResult {
    #[schemars(length(min = 1, max = 8))]
    pub(in crate::operation_registry) operations: Vec<OperationExecutionContract>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct OperationExecutionContract {
    pub(in crate::operation_registry) id: EntryId,
    pub(in crate::operation_registry) toolkit: ToolkitId,
    pub(in crate::operation_registry) title: String,
    pub(in crate::operation_registry) summary: String,
    pub(in crate::operation_registry) schema_revision: String,
    pub(in crate::operation_registry) effect: EffectClass,
    /// Absent when the operation takes no secret input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::operation_registry) secret: Option<SecretClass>,
    pub(in crate::operation_registry) approval: ApprovalClass,
    pub(in crate::operation_registry) connection: ConnectionRequirement,
    pub(in crate::operation_registry) input_schema: Value,
    pub(in crate::operation_registry) output_schema: Option<Value>,
    /// Absent when safety is the read-only default, with attended-prompt
    /// confirmation for attended_prompt approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::operation_registry) safety: Option<OperationSafety>,
}
fn unique_bounded(values: &[EntryId], maximum: usize) -> bool {
    (1..=maximum).contains(&values.len())
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn deserialize_operation_ids<'de, D>(deserializer: D) -> Result<Vec<EntryId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<EntryId>::deserialize(deserializer)?;
    unique_bounded(&values, MAX_BATCH_OPERATIONS)
        .then_some(values)
        .ok_or_else(|| serde::de::Error::custom("operation_ids must contain 1 to 8 unique values"))
}
