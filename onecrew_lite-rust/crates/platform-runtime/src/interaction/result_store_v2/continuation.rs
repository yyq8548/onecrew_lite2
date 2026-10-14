use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::{CallDisposition, ResultBinding, ResultChoiceAnswer, ResultContinuationLeaseId};

pub const RESULT_CONTINUATION_DISPATCH_LEASE_MS: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultContinuationRequest {
    #[schemars(length(min = 1, max = 8192))]
    pub message: String,
    pub context: ResultContinuationContext,
    /// True exactly when the context carries content the model needs and the
    /// concise message omits; only then is context sent, and the Host must
    /// accept it before the message.
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_model_context: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelResultInput {
    pub binding: ResultBinding,
    pub continuation: Option<ResultContinuationRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestResultEditInput {
    pub binding: ResultBinding,
    #[schemars(length(min = 1, max = 2048))]
    pub instruction: String,
}

/// Model context carries only what the model cannot otherwise know; the Host
/// may show it to the user, so it never repeats the model's own request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultContinuationContext {
    CreatorSaveBatch {
        summary: super::CreatorSaveResultSummary,
        #[schemars(length(min = 1, max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
        outcomes: Vec<ResultContinuationCreatorSaveOutcome>,
    },
    /// The model prepared the scopes itself and the card cannot change them;
    /// the user may have edited the acceptance criteria and keywords.
    SearchPlanConfirmed {
        confirmed_plan_ref: String,
        description: String,
        keywords: Vec<String>,
    },
    Execution {
        #[schemars(length(min = 1, max = 8))]
        outcomes: Vec<ResultContinuationOutcomeSummary>,
    },
    Choice {
        #[schemars(length(min = 1, max = 32))]
        answers: Vec<ResultChoiceAnswer>,
    },
    Cancelled,
    EditRequested,
}

/// One reviewed Creator row as a follow-up needs it: the saved Creator, or why
/// the row was not saved. Display fields stay on the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultContinuationCreatorSaveOutcome {
    pub row_id: String,
    pub status: super::CreatorSaveBatchRowStatus,
    pub operation: super::CreatorSaveBatchOperation,
    pub reason: Option<super::CreatorSaveBatchReason>,
    pub creator_id: Option<String>,
    pub failure_code: Option<String>,
}

impl From<&super::CreatorSaveBatchOutcome> for ResultContinuationCreatorSaveOutcome {
    fn from(row: &super::CreatorSaveBatchOutcome) -> Self {
        Self {
            row_id: row.row_id.clone(),
            status: row.status.clone(),
            operation: row.operation,
            reason: row.reason,
            creator_id: row.creator_id.clone(),
            failure_code: row.failure_code.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultContinuationOutcomeSummary {
    #[schemars(length(min = 1, max = 128))]
    pub call_id: String,
    #[schemars(length(min = 1, max = 128))]
    pub operation_id: String,
    pub disposition: CallDisposition,
    #[schemars(length(min = 1, max = 8192))]
    pub summary: String,
    #[serde(default)]
    #[schemars(length(max = 64))]
    pub resource_refs: Vec<super::ResultResourceRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultContinuationState {
    NotRequested,
    Pending,
    Dispatching,
    Delivered,
    Paused,
    DeliveryUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultContinuationSnapshot {
    pub state: ResultContinuationState,
    #[serde(deserialize_with = "deserialize_positive_revision")]
    #[schemars(range(min = 1))]
    pub continuation_revision: u64,
    pub dispatch_started_at_ms: Option<u64>,
    pub dispatch_not_after_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultContinuationClaimMode {
    Pending,
    RetryPaused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimResultContinuationInput {
    pub binding: ResultBinding,
    #[serde(deserialize_with = "deserialize_positive_revision")]
    #[schemars(range(min = 1))]
    pub expected_continuation_revision: u64,
    pub mode: ResultContinuationClaimMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimedResultContinuation {
    pub binding: ResultBinding,
    #[serde(deserialize_with = "deserialize_positive_revision")]
    #[schemars(range(min = 1))]
    pub continuation_revision: u64,
    pub lease_id: ResultContinuationLeaseId,
    pub request: ResultContinuationRequest,
    pub dispatch_started_at_ms: u64,
    pub dispatch_not_after_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultContinuationCompletionOutcome {
    Delivered,
    Paused,
    DeliveryUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteResultContinuationInput {
    pub binding: ResultBinding,
    #[serde(deserialize_with = "deserialize_positive_revision")]
    #[schemars(range(min = 1))]
    pub expected_continuation_revision: u64,
    pub lease_id: ResultContinuationLeaseId,
    pub outcome: ResultContinuationCompletionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompletedResultContinuation {
    pub binding: ResultBinding,
    pub continuation: ResultContinuationSnapshot,
}

fn deserialize_positive_revision<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let revision = u64::deserialize(deserializer)?;
    if revision == 0 {
        return Err(D::Error::custom("continuation revision must be positive"));
    }
    Ok(revision)
}
