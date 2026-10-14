//! Strict result-state transitions and host-compatible flattened schemas.
use super::ask_user_question::deserialize_choice_answers;
use onecrew_local_runtime::{
    ClaimResultContinuationInput, ClaimedResultContinuation, CompleteResultContinuationInput,
    CompletedResultContinuation, RequestResultEditInput, ResultBinding, ResultChoiceAnswer,
    ResultContinuationClaimMode, ResultContinuationCompletionOutcome, ResultContinuationLeaseId,
    ResultSnapshot,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ResultStateInput {
    ConfirmCreatorSaveBatch {
        binding: ResultBinding,
        selections: Vec<onecrew_local_runtime::CreatorSaveBatchSelection>,
    },
    ReviseSearchPlan {
        binding: ResultBinding,
        patch: onecrew_local_runtime::SearchPlanPatch,
    },
    ReadCurrent {
        binding: ResultBinding,
    },
    Confirm {
        binding: ResultBinding,
    },
    Select {
        binding: ResultBinding,
        #[serde(deserialize_with = "deserialize_choice_answers")]
        answers: Vec<ResultChoiceAnswer>,
    },
    Cancel {
        binding: ResultBinding,
    },
    RequestEdit {
        input: RequestResultEditInput,
    },
    ClaimContinuation {
        input: ClaimResultContinuationInput,
    },
    CompleteContinuation {
        input: CompleteResultContinuationInput,
    },
}

#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields are consumed by the JsonSchema derive for the flattened result-state input contract"
)]
struct ResultStateInputSchema {
    action: ResultStateActionSchema,
    #[schemars(length(min = 1, max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
    selections: Option<Vec<onecrew_local_runtime::CreatorSaveBatchSelection>>,
    patch: Option<onecrew_local_runtime::SearchPlanPatch>,
    binding: Option<ResultBinding>,
    #[schemars(length(min = 1, max = 32))]
    answers: Option<Vec<ResultChoiceAnswer>>,
    input: Option<ResultStateNestedInputSchema>,
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "variants are consumed by the JsonSchema derive for the result-state action contract"
)]
enum ResultStateActionSchema {
    ConfirmCreatorSaveBatch,
    ReviseSearchPlan,
    ReadCurrent,
    Confirm,
    Select,
    Cancel,
    RequestEdit,
    ClaimContinuation,
    CompleteContinuation,
}

#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields are consumed by the JsonSchema derive for nested result-state input"
)]
struct ResultStateNestedInputSchema {
    binding: ResultBinding,
    #[schemars(length(min = 1, max = 2048))]
    instruction: Option<String>,
    #[schemars(range(min = 1))]
    expected_continuation_revision: Option<u64>,
    mode: Option<ResultContinuationClaimMode>,
    lease_id: Option<ResultContinuationLeaseId>,
    outcome: Option<ResultContinuationCompletionOutcome>,
}

impl JsonSchema for ResultStateInput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultStateInput".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // The descriptor advertises one closed object shape. The tagged enum
        // above remains the execution boundary and rejects action/field
        // combinations that do not belong to the selected transition.
        ResultStateInputSchema::json_schema(generator)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ResultStateOutput {
    Snapshot {
        snapshot: ResultSnapshot,
        #[serde(skip_serializing_if = "Option::is_none")]
        dm_failure: Option<Box<onecrew_local_runtime::DmFailure>>,
    },
    ClaimedContinuation {
        claimed: ClaimedResultContinuation,
    },
    CompletedContinuation {
        completed: CompletedResultContinuation,
    },
}

impl ResultStateOutput {
    pub(in crate::operation_registry::v2) fn snapshot(snapshot: ResultSnapshot) -> Self {
        Self::Snapshot {
            snapshot,
            dm_failure: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ResultStateFailure {
    Absent,
    Expired,
    ViewMismatch,
    OriginMismatch,
    FingerprintMismatch,
    RevisionConflict { current_revision: u64 },
    ContinuationRevisionConflict { current_revision: u64 },
    ContinuationLeaseMismatch,
    SearchPlanPayloadTooLarge,
    InvalidInput,
    InvalidState,
    Replay,
    Capacity,
    Unavailable,
}

#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields are consumed by the JsonSchema derive for the result-state output union"
)]
pub(in crate::operation_registry::v2) struct ResultStateWireSchema {
    kind: Option<ResultStateOutputKindSchema>,
    snapshot: Option<ResultSnapshot>,
    dm_failure: Option<Box<onecrew_local_runtime::DmFailure>>,
    claimed: Option<ClaimedResultContinuation>,
    completed: Option<CompletedResultContinuation>,
    code: Option<ResultStateFailureCodeSchema>,
    #[schemars(range(min = 1))]
    current_revision: Option<u64>,
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "variants are consumed by the JsonSchema derive for the result-state output discriminator"
)]
enum ResultStateOutputKindSchema {
    Snapshot,
    ClaimedContinuation,
    CompletedContinuation,
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "variants are consumed by the JsonSchema derive for the result-state failure contract"
)]
enum ResultStateFailureCodeSchema {
    Absent,
    Expired,
    ViewMismatch,
    OriginMismatch,
    FingerprintMismatch,
    RevisionConflict,
    ContinuationRevisionConflict,
    ContinuationLeaseMismatch,
    SearchPlanPayloadTooLarge,
    InvalidInput,
    InvalidState,
    Replay,
    Capacity,
    Unavailable,
}
