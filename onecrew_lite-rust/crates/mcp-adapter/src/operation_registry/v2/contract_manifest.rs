use onecrew_local_runtime::{
    ClaimResultContinuationInput, CompleteResultContinuationInput, RequestResultEditInput,
    ResultBinding, ResultChoiceAnswer,
};
use onecrew_schema_manifest::{
    SchemaDirection, SchemaManifest, SchemaManifestError, contract_schema_root,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::contract::{ResultPresentResult, ResultStateFailure};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AdapterPresentationKindContract {
    CreatorSaveBatchConfirmation,
    CreatorSaveBatchOutcome,
    CreatorBatchConfirmation,
    CreatorBatchOutcome,
    OutreachConfirmation,
    OutreachOutcome,
    EmailConfirmation,
    EmailOutcome,
    SearchPlanEditor,
    OutreachDraft,
    RecordPreview,
    SimpleOperation,
    ReadOnly,
    Choice,
    BatchResult,
}

/// Every presentation kind this App can render, mirroring
/// `AdapterPresentationKindContract` variant for variant.
///
/// The mirror is not free-standing: `mcp_app_contract_manifest_is_exact_and_exportable`
/// compares this list against the closed enum the exported schema actually
/// carries, so a variant added to one side and not the other fails there.
const APP_PRESENTATION_KINDS: &[&str] = &[
    "creator_save_batch_confirmation",
    "creator_save_batch_outcome",
    "creator_batch_confirmation",
    "creator_batch_outcome",
    "outreach_confirmation",
    "outreach_outcome",
    "email_confirmation",
    "email_outcome",
    "search_plan_editor",
    "outreach_draft",
    "record_preview",
    "simple_operation",
    "read_only",
    "choice",
    "batch_result",
];

/// The closed vocabulary of presentation kinds this App can render.
///
/// Exported so a crate that *depends on* this adapter can prove the kinds it
/// mints are renderable. The check belongs on the depending side: a crate here
/// reaching back into its dependents' source trees would make the
/// depended-upon crate track layouts it does not own, and would pass silently
/// the moment one of those layouts moved.
///
/// The export is a list of wire strings rather than the contract enum on
/// purpose. Membership is the only question a caller below the App may ask;
/// dispatching on a presentation kind is the App's own job, and a caller that
/// could match the enum exhaustively would be writing a second renderer. The
/// order is the contract's declaration order and carries no meaning.
pub fn app_presentation_kinds() -> &'static [&'static str] {
    APP_PRESENTATION_KINDS
}

/// Reuse runtime data and every runtime bound; the peer narrows only the
/// presentation-kind vocabulary supported by this App.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct PresentationContract(onecrew_local_runtime::PresentationProjection);

impl JsonSchema for PresentationContract {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PresentationContract".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = onecrew_local_runtime::PresentationProjection::json_schema(generator);
        let kind = generator.subschema_for::<AdapterPresentationKindContract>();
        schema
            .as_object_mut()
            .expect("runtime projection object")
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .expect("runtime projection properties")
            .insert(
                "kind".into(),
                serde_json::to_value(kind).expect("presentation kind schema"),
            );
        schema
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct ResultSnapshotContract(onecrew_local_runtime::ResultSnapshot);

impl JsonSchema for ResultSnapshotContract {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultSnapshotContract".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = onecrew_local_runtime::ResultSnapshot::json_schema(generator);
        let presentation = generator.subschema_for::<PresentationContract>();
        schema
            .as_object_mut()
            .expect("runtime snapshot object")
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .expect("runtime snapshot properties")
            .insert(
                "presentation".into(),
                serde_json::to_value(presentation).expect("presentation schema"),
            );
        schema
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "variants are consumed by JsonSchema for the exported result-state contract"
)]
enum ResultStateOutputContract {
    Snapshot {
        snapshot: ResultSnapshotContract,
        #[serde(skip_serializing_if = "Option::is_none")]
        dm_failure: Option<Box<onecrew_local_runtime::DmFailure>>,
    },
    ClaimedContinuation {
        claimed: onecrew_local_runtime::ClaimedResultContinuation,
    },
    CompletedContinuation {
        completed: onecrew_local_runtime::CompletedResultContinuation,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum ExactResultStateInput {
    ConfirmCreatorSaveBatch {
        binding: ResultBinding,
        #[schemars(length(min = 1, max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
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
        #[schemars(length(min = 1, max = 32))]
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

pub fn result_contract_schema_manifest() -> Result<SchemaManifest, SchemaManifestError> {
    SchemaManifest::new(
        "mcp-app",
        vec![
            contract_schema_root::<onecrew_local_runtime::ClaimedResultContinuation>(
                "ClaimedResultContinuation",
                SchemaDirection::Serialize,
            )?,
            contract_schema_root::<onecrew_local_runtime::CompletedResultContinuation>(
                "CompletedResultContinuation",
                SchemaDirection::Serialize,
            )?,
            contract_schema_root::<onecrew_local_runtime::DmFailure>(
                "DmFailure",
                SchemaDirection::Serialize,
            )?,
            contract_schema_root::<ExactResultStateInput>(
                "ExactResultStateInput",
                SchemaDirection::Deserialize,
            )?,
            contract_schema_root::<ResultBinding>("ResultBinding", SchemaDirection::Serialize)?,
            contract_schema_root::<ResultPresentResult>(
                "ResultPresentResult",
                SchemaDirection::Serialize,
            )?,
            contract_schema_root::<ResultSnapshotContract>(
                "ResultSnapshot",
                SchemaDirection::Serialize,
            )?,
            contract_schema_root::<ResultStateFailure>(
                "ResultStateFailure",
                SchemaDirection::Serialize,
            )?,
            contract_schema_root::<ResultStateOutputContract>(
                "ResultStateOutput",
                SchemaDirection::Serialize,
            )?,
        ],
    )
}

#[cfg(test)]
mod tests {
    use onecrew_local_runtime::ResultSnapshot;
    use serde_json::json;

    use super::*;

    #[test]
    fn runtime_snapshot_and_contract_projection_share_the_wire() {
        let wire = serde_json::to_value(snapshot_fixture()).expect("runtime snapshot wire");
        let projected: ResultSnapshotContract =
            serde_json::from_value(wire.clone()).expect("contract accepts runtime wire");
        assert_eq!(
            serde_json::to_value(projected).expect("contract wire"),
            wire
        );
    }

    fn snapshot_fixture() -> ResultSnapshot {
        serde_json::from_value(json!({
            "locale":"en-US",
            "binding":{
                "result_ref":format!("rr_{}", "1".repeat(32)),
                "view":{"identity":format!("rv_{}", "2".repeat(32)),"kind":"confirmation"},
                "origin":"mcp:full",
                "fingerprint":format!("sha256:{}", "3".repeat(64)),
                "revision":2
            },
            "presented":true,
            "completed_sections_count":0,
            "batch":{"outcomes":[{
                "call_id":"call.test",
                "operation_id":"onecrew.test",
                "disposition":{"kind":"not_run","reason":"confirmation_required"},
                "summary":"Test result.",
                "resource_refs":[]
            }]},
            "presentation":{"kind":"simple_operation","title":"Test","summary":null,"sections":[]},
            "pending_action":{"kind":"confirm","prompt":"Confirm?","label":"Confirm"},
            "state":{"kind":"available"},
            "continuation":{
                "state":"not_requested",
                "continuation_revision":1,
                "dispatch_started_at_ms":null,
                "dispatch_not_after_ms":null
            }
        }))
        .expect("runtime snapshot fixture")
    }
}
