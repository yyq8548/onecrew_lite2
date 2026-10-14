//! Native RMCP projection of the fixed V2 registry surface.
//!
//! The model sees only the five fixed meta tools. `onecrew_result_state` is a
//! separate App-only route over the same process-local result owner.

use onecrew_local_runtime::ToolkitId;
use std::sync::Arc;

use onecrew_local_runtime::SearchPlanApprovalError;
use onecrew_local_runtime::{ResultOrigin, ResultSnapshot};
use rmcp::{
    handler::server::{common::schema_for_output, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, JsonObject, MetaObject, ToolAnnotations},
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{
    failure::{
        GuidedFailure, PublicFailure, guided_failure, result_presentation_failure,
        structured_app_failure, structured_failure,
    },
    operation_registry::descriptor::{ConnectionListInput, ConnectionListResult, SurfaceId},
    server::McpAdapterServer,
};

use super::{
    contract::{
        ResultPresentInput, ResultPresentResult, ResultStateFailure, ResultStateInput,
        ResultStateWireSchema, ToolExecuteInput, ToolExecuteResult, ToolGetFailure, ToolGetInput,
        ToolGetResult, ToolSearchInput, ToolSearchResult,
    },
    execute::ToolExecuteOrchestrationFailure,
    search_plan::SearchPlanScopeInvalidReason,
    state::{PresentedResultV2, map_store_failure, result_present_v2, surface_origin_scope},
};

pub(crate) const RESULT_META_KEY: &str = "onecrew/result";

const CONNECTION_LIST_UNAVAILABLE: PublicFailure = PublicFailure {
    code: "connection_list_unavailable",
    message: "The local secret-free connection inventory is unavailable.",
    parameter_source: None,
    next_action: Some(
        "Retry onecrew_connection_list after the local OneCrew runtime is available.",
    ),
    guard: Some("Do not substitute browser state or guess a connection reference."),
};

const TOOL_GET_INVALID_BATCH: PublicFailure = PublicFailure {
    code: "invalid_batch",
    message: "onecrew_tool_get requires one to eight unique operation IDs.",
    parameter_source: None,
    next_action: Some(
        "Use operation IDs returned by onecrew_tool_search and retry one bounded batch.",
    ),
    guard: Some("Do not pass playbook, toolkit-guide, alias, or duplicate IDs."),
};

const TOOL_GET_UNAVAILABLE: PublicFailure = PublicFailure {
    code: "operation_unavailable",
    message: "At least one requested operation is unavailable on this MCP surface.",
    parameter_source: None,
    next_action: Some(
        "Call onecrew_tool_search on this surface and retry with exact returned IDs.",
    ),
    guard: Some("The batch is atomic; do not infer a partial contract response."),
};

const TOOL_EXECUTE_SEARCH_PLAN_TOO_LARGE: PublicFailure = PublicFailure {
    code: "search_plan_payload_too_large",
    message: "The search plan exceeds the 512 KiB serialized plan limit.",
    parameter_source: None,
    next_action: Some("Split the draft into smaller plans and present each for user confirmation."),
    guard: Some("No provider was called and no approval was issued."),
};

const TOOL_EXECUTE_SEARCH_PLAN_INVALID: PublicFailure = PublicFailure {
    code: "search_plan_invalid",
    message: "The search plan failed domain validation. reason and parameter_pointer identify the rejected constraint and field relative to the call's arguments.",
    parameter_source: None,
    next_action: Some(
        "Use onecrew_tool_get to read the exact selected operation contract and correct the indicated scope. Each scope binds operation_id, current schema_revision, selected tagged connection, and that operation's arguments. Do not add provider kind or fields from another operation. Prepare and present the corrected plan for confirmation; no search has run.",
    ),
    guard: Some(
        "No provider was called, no result_ref was minted, and no search approval was issued. Keep the search-plan confirmation workflow.",
    ),
};

/// The rejected scope check and its correction, stated in the failure text
/// beside the typed `reason` and `parameter_pointer` of the structured failure.
fn search_plan_scope_invalid(reason: SearchPlanScopeInvalidReason) -> PublicFailure {
    use SearchPlanScopeInvalidReason as Reason;
    const CORRECT_FROM_CONTRACT: &str = "Correct the field at parameter_pointer from the exact onecrew_tool_get contract, then prepare and present the plan again.";
    // The operation contract admits non-neutral plan fields for a search
    // call, so it cannot correct a plan scope that carries one.
    const NEUTRAL_PLAN_FIELD: &str = "Omit the field at parameter_pointer or set it to its neutral value (confirmed_plan_ref null, search_plan_policy require_confirmation), then prepare and present the plan again.";
    let (message, next_action) = match reason {
        Reason::UnknownOperation => (
            "The scope operation_id is not an operation on this surface.",
            CORRECT_FROM_CONTRACT,
        ),
        Reason::OperationNotPlanGoverned => (
            "The scope operation_id is not a plan-governed search.",
            CORRECT_FROM_CONTRACT,
        ),
        Reason::ConnectionInvalid => (
            "The scope connection is not a tagged connection the operation accepts.",
            CORRECT_FROM_CONTRACT,
        ),
        Reason::SchemaRevisionMismatch => (
            "The scope schema_revision is not the operation's current revision.",
            CORRECT_FROM_CONTRACT,
        ),
        Reason::ArgumentsInvalid => (
            "The scope arguments fail the operation's input contract.",
            CORRECT_FROM_CONTRACT,
        ),
        Reason::PlanReferenceInScope => (
            "The scope carries a confirmed_plan_ref; a plan scope omits it or holds it only as null.",
            NEUTRAL_PLAN_FIELD,
        ),
        Reason::DirectPolicyInScope => (
            "The scope selects a direct search policy; a plan scope omits search_plan_policy or holds it only as require_confirmation.",
            NEUTRAL_PLAN_FIELD,
        ),
        Reason::ContinuationInInitialPlan => (
            "The scope starts from a later page; a plan scope starts from the first page.",
            CORRECT_FROM_CONTRACT,
        ),
        Reason::DuplicateScope => (
            "The scope repeats an earlier scope's search.",
            "Remove the repeated scope, or change its arguments so it names a different search, then prepare and present the plan again.",
        ),
    };
    PublicFailure {
        code: "search_plan_scope_invalid",
        message,
        parameter_source: None,
        next_action: Some(next_action),
        guard: Some(
            "No provider was called, no result_ref was minted, and no search approval was issued.",
        ),
    }
}

const TOOL_EXECUTE_SEARCH_PLAN_REQUIRED: PublicFailure = PublicFailure {
    code: "search_plan_confirmation_required",
    message: "External searches require a user-confirmed editable search plan by default.",
    parameter_source: None,
    next_action: Some(
        "Get onecrew.search.plan.prepare, set acceptance criteria and keywords with typed search scopes, execute and present its result, then wait for user confirmation. Pass the returned confirmed_plan_ref to the search and copy the confirmed plan's exact search arguments; do not change the query, filters, count, language, or policy.",
    ),
    guard: Some(
        "No provider search ran. Never invent a plan reference or select direct mode without the user's explicit current request.",
    ),
};

/// Why the confirmed plan did not authorize a search call. Only a missing,
/// unknown, or expired plan reference is recovered by preparing a new plan;
/// the others keep the confirmed scope.
fn search_plan_approval_rejected(reason: SearchPlanApprovalError) -> PublicFailure {
    use SearchPlanApprovalError as Reason;
    let (message, next_action, guard) = match reason {
        Reason::ConfirmationRequired => {
            return PublicFailure {
                code: "search_plan_approval_rejected",
                ..TOOL_EXECUTE_SEARCH_PLAN_REQUIRED
            };
        }
        Reason::ScopeMismatch => (
            "The search call does not match a scope of the confirmed plan.",
            "Execute the confirmed scope exactly: send the operation_id, schema_revision, connection and search criteria you proposed in the plan, unchanged, plus its confirmed_plan_ref. An omitted count was frozen at 20; sending count 20 is equivalent, while a different count deviates from the confirmed scope.",
            "No provider was called. Do not change the query, filters, count, language, or policy of a confirmed scope.",
        ),
        Reason::InvalidOrExpired => (
            "The confirmed_plan_ref is unknown or its approval expired.",
            "Prepare and present a new search plan with onecrew.search.plan.prepare, then wait for the user's confirmation.",
            "No provider was called. Never invent a plan reference or select direct mode without the user's explicit current request.",
        ),
        Reason::Unavailable => (
            "The local search approval could not be checked.",
            "Stop and tell the user the search could not be authorized by the local OneCrew runtime.",
            "No provider was called. Do not retry with a direct policy or a new plan.",
        ),
    };
    PublicFailure {
        code: "search_plan_approval_rejected",
        message,
        parameter_source: None,
        next_action: Some(next_action),
        guard: Some(guard),
    }
}

const TOOL_EXECUTE_INVALID_REQUEST: PublicFailure = PublicFailure {
    code: "execute_request_invalid",
    message: "The V2 execution batch could not be safely prepared.",
    parameter_source: None,
    next_action: Some(
        "Refresh all operation contracts with onecrew_tool_get and submit one valid batch.",
    ),
    guard: Some("No owner was invoked for a prevalidation failure."),
};

const TOOL_EXECUTE_PRESENTATION_UNAVAILABLE: PublicFailure = PublicFailure {
    code: "presentation_unavailable",
    message: "This batch cannot be projected into a non-secret OneCrew result.",
    parameter_source: None,
    next_action: Some(
        "Remove presentation=requested or use only non-secret presentable operations.",
    ),
    guard: Some("Never place secret or sensitive input into an MCP App result."),
};

const RESULT_STATE_INVALID: PublicFailure = PublicFailure {
    code: "result_state_invalid",
    message: "The result binding or requested transition is not valid for the current state.",
    parameter_source: None,
    next_action: Some("Read the current result using the exact binding from the mounted App."),
    guard: Some(
        "Do not alter the result reference, origin, view identity, fingerprint, or revision.",
    ),
};

const RESULT_EXPIRED: PublicFailure = PublicFailure {
    code: "interaction_expired",
    message: "This result is no longer available in the current OneCrew process.",
    parameter_source: None,
    next_action: Some("Keep the old card read-only and ask the Agent to generate a new result."),
    guard: Some("Do not reconstruct interaction authority from transcript or browser storage."),
};

const RESULT_UNAVAILABLE: PublicFailure = PublicFailure {
    code: "result_unavailable",
    message: "The process-local OneCrew result owner is unavailable.",
    parameter_source: None,
    next_action: Some("Keep the card closed and retry only after the local runtime is healthy."),
    guard: Some("Do not blindly retry a result action or business operation."),
};

// onecrew_result_present is the model's route; the RESULT_* guidance above is
// for the mounted App's own state route.
const RESULT_PRESENT_INVALID: PublicFailure = PublicFailure {
    message: "The result_ref cannot be presented: it was not minted on this surface, was already presented, or is not a current result reference.",
    next_action: Some(
        "Present only an unpresented result_ref returned by onecrew_tool_execute on this surface.",
    ),
    guard: Some(
        "Do not alter or reconstruct a result_ref, and do not present the same reference again.",
    ),
    ..RESULT_STATE_INVALID
};

const RESULT_PRESENT_EXPIRED: PublicFailure = PublicFailure {
    next_action: Some(
        "Execute the operation again to mint a new result_ref, then present that reference.",
    ),
    guard: Some(
        "Do not repeat a write that already completed, and do not reconstruct a result_ref from the transcript.",
    ),
    ..RESULT_EXPIRED
};

const RESULT_PRESENT_UNAVAILABLE: PublicFailure = PublicFailure {
    next_action: Some(
        "Tell the user the result cannot be shown now; present the same result_ref only after the local OneCrew runtime is healthy.",
    ),
    guard: Some("Do not repeat the business operation to get a new result."),
    ..RESULT_UNAVAILABLE
};

#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
#[allow(
    dead_code,
    reason = "variants define the generated success-or-failure schema for onecrew_tool_get"
)]
enum ToolGetWireOutput {
    Success(ToolGetResult),
    Failure(GuidedFailure<ToolGetFailure>),
}

#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
#[allow(
    dead_code,
    reason = "variants define the generated success-or-failure schema for onecrew_tool_execute"
)]
enum ToolExecuteWireOutput {
    Success(ToolExecuteResult),
    Failure(GuidedFailure<ToolExecuteOrchestrationFailure>),
}

#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
#[allow(
    dead_code,
    reason = "variants define the generated success-or-failure schema for onecrew_result_present"
)]
enum ResultPresentWireOutput {
    Success(ResultPresentResult),
    Failure(GuidedFailure<ResultStateFailure>),
}

#[tool_router(router = v2_router, vis = "pub(crate)")]
impl McpAdapterServer {
    #[tool(
        description = "Search the current closed OneCrew operation registry. Returns ranked operation IDs, compact complete toolkit guides for represented or inferred toolkits, and bounded workflow hints. Use onecrew_tool_get for exact executable schemas."
    )]
    async fn onecrew_tool_search(
        &self,
        Parameters(input): Parameters<ToolSearchInput>,
    ) -> Result<CallToolResult, String> {
        structured(
            self.operation_registry
                .tool_search_v2(self.surface.registry_id(), &input),
        )
    }

    #[tool(
        description = "Get exact current execution contracts for one to eight unique operation IDs returned by onecrew_tool_search. The response preserves request order and fails atomically if any operation is unavailable. Output schemas are omitted unless include_output_schema is true; request them only when a later call or result presentation must address result fields you have not seen yet."
    )]
    async fn onecrew_tool_get(
        &self,
        Parameters(input): Parameters<ToolGetInput>,
    ) -> Result<CallToolResult, String> {
        match self
            .operation_registry
            .tool_get_v2(self.surface.registry_id(), &input)
        {
            Ok(result) => structured_tool_get(result),
            Err(failure) => Ok(structured_failure(
                failure,
                match failure {
                    ToolGetFailure::InvalidBatch => TOOL_GET_INVALID_BATCH,
                    ToolGetFailure::OperationUnavailable => TOOL_GET_UNAVAILABLE,
                },
            )),
        }
    }

    #[tool(
        description = "Execute fully specified OneCrew operations using exact contracts from onecrew_tool_get. The transport accepts at most eight calls per request; this is not a business batch-size limit. After a search plan is confirmed, execute exactly the plan's search arguments with its confirmed_plan_ref in one dependency round; never issue alternate queries, retry with changed filters, or use a direct-policy bypass. For multiple Creator candidates, use the dedicated Creator batch-save operation and its single payload instead of repeating the single-candidate save operation. Request presentation when the user should see the resulting typed card."
    )]
    async fn onecrew_tool_execute(
        &self,
        Parameters(input): Parameters<ToolExecuteInput>,
    ) -> Result<CallToolResult, String> {
        let origin = match result_origin(self.surface.registry_id()) {
            Ok(origin) => origin,
            Err(failure) => return Ok(structured_result_state_failure(failure)),
        };
        let request_id = format!("mcp-v2-{:032x}", rand::random::<u128>());
        match self
            .operation_registry
            .tool_execute_v2(
                self.platform.as_ref(),
                &origin,
                self.surface.registry_id(),
                &request_id,
                &input,
            )
            .await
        {
            Ok(result) => structured(result),
            Err(failure) => {
                let public = match failure {
                    ToolExecuteOrchestrationFailure::SearchPlanInvalid { .. } => {
                        TOOL_EXECUTE_SEARCH_PLAN_INVALID
                    }
                    ToolExecuteOrchestrationFailure::SearchPlanApprovalRejected {
                        reason, ..
                    } => search_plan_approval_rejected(reason),
                    ToolExecuteOrchestrationFailure::SearchPlanScopeInvalid { reason, .. } => {
                        search_plan_scope_invalid(reason)
                    }
                    ToolExecuteOrchestrationFailure::SearchPlanPayloadTooLarge => {
                        TOOL_EXECUTE_SEARCH_PLAN_TOO_LARGE
                    }
                    ToolExecuteOrchestrationFailure::SearchPlanConfirmationRequired { .. } => {
                        TOOL_EXECUTE_SEARCH_PLAN_REQUIRED
                    }
                    ToolExecuteOrchestrationFailure::PresentationUnavailable => {
                        TOOL_EXECUTE_PRESENTATION_UNAVAILABLE
                    }
                    ToolExecuteOrchestrationFailure::InvalidBatchRequestId
                    | ToolExecuteOrchestrationFailure::ChoiceBatchRequiresSingleCall
                    | ToolExecuteOrchestrationFailure::Prevalidation { .. } => {
                        TOOL_EXECUTE_INVALID_REQUEST
                    }
                };
                Ok(structured_failure(failure, public))
            }
        }
    }

    #[tool(
        description = "Present one result_ref minted by onecrew_tool_execute. Pass the required locale=zh-CN for Chinese conversations or locale=en-US for all other conversations. For creator save batches, present the returned batch result_ref after confirmation so the MCP App shows per-row outcomes. The model receives only a compact receipt; the mounted MCP App receives the authoritative secret-free result snapshot through result metadata. Presentation grants no new execution authority."
    )]
    async fn onecrew_result_present(
        &self,
        Parameters(input): Parameters<ResultPresentInput>,
    ) -> Result<CallToolResult, String> {
        let origin = match result_origin(self.surface.registry_id()) {
            Ok(origin) => origin,
            Err(failure) => return Ok(structured_result_present_failure(failure)),
        };
        match result_present_v2(self.platform.as_ref(), &origin, input) {
            Ok(presented) => structured_presented_result(presented),
            Err(failure) => Ok(structured_result_present_failure(failure)),
        }
    }

    #[tool(
        description = "Read or transition the authoritative process-local state of the mounted OneCrew result. This App-only route requires the exact server-issued binding and exposes no independent business operation."
    )]
    async fn onecrew_result_state(
        &self,
        Parameters(input): Parameters<ResultStateInput>,
    ) -> Result<CallToolResult, String> {
        match self
            .operation_registry
            .result_state_v2(&self.platform, self.surface.registry_id(), input)
            .await
        {
            Ok(output) => structured(output),
            Err(failure) => {
                let public = result_state_public_failure(&failure);
                Ok(structured_app_failure(failure, public))
            }
        }
    }

    #[tool(
        description = "List bounded secret-free connection references usable by operations on this fixed MCP surface, grouped by toolkit. Filters are closed and duplicate-free. Admission failures never mean an empty successful inventory."
    )]
    async fn onecrew_connection_list(
        &self,
        Parameters(mut input): Parameters<ConnectionListInput>,
    ) -> Result<CallToolResult, String> {
        let available = self
            .operation_registry
            .operation_toolkits(self.surface.registry_id());
        let is_connection_toolkit = |toolkit: &ToolkitId| {
            matches!(
                toolkit,
                ToolkitId::X
                    | ToolkitId::Instagram
                    | ToolkitId::Telegram
                    | ToolkitId::Youtube
                    | ToolkitId::Email
            )
        };
        if input.toolkits.is_empty() {
            input.toolkits = available
                .iter()
                .copied()
                .filter(is_connection_toolkit)
                .collect();
        } else {
            input
                .toolkits
                .retain(|toolkit| available.contains(toolkit) && is_connection_toolkit(toolkit));
        }
        if input.toolkits.is_empty() {
            return structured(ConnectionListResult::empty());
        }
        let platform = Arc::clone(&self.platform);
        let inventory = match crate::operation_registry::execution::run_blocking(move || {
            platform.platform_connection_list()
        })
        .await
        {
            Ok(inventory) => inventory,
            _ => return Ok(guided_failure(CONNECTION_LIST_UNAVAILABLE)),
        };
        structured(ConnectionListResult::project(inventory, &input))
    }
}

pub(crate) fn apply_v2_tool_metadata(
    router: &mut rmcp::handler::server::router::tool::ToolRouter<McpAdapterServer>,
) {
    for route in router.map.values_mut() {
        let (annotations, output_schema) = match route.attr.name.as_ref() {
            "onecrew_tool_search" => (
                hints(true, false, true, false),
                schema_for_output::<ToolSearchResult>(),
            ),
            "onecrew_tool_get" => (
                hints(true, false, true, false),
                schema_for_output::<ToolGetWireOutput>(),
            ),
            "onecrew_tool_execute" => (
                hints(false, false, false, false),
                schema_for_output::<ToolExecuteWireOutput>(),
            ),
            "onecrew_result_present" => (
                hints(false, false, true, false),
                schema_for_output::<ResultPresentWireOutput>(),
            ),
            "onecrew_result_state" => (
                hints(false, false, false, false),
                schema_for_output::<ResultStateWireSchema>(),
            ),
            "onecrew_connection_list" => (
                hints(true, false, true, false),
                schema_for_output::<ConnectionListResult>(),
            ),
            name => panic!("unexpected V2 MCP tool {name}"),
        };
        route.attr.annotations = Some(annotations);
        inline_schema_refs(Arc::make_mut(&mut route.attr.input_schema));
        let mut output_schema = output_schema;
        let output = Arc::make_mut(&mut output_schema);
        inline_schema_refs(output);
        *output = compact_output_schema(output);
        // MCP Tool.outputSchema is an object schema even when the payload is a
        // discriminated success/failure union. Some model paths accept a bare
        // anyOf, but the standard SDK used by Apps Hosts rejects tools/list
        // before proxying App calls unless the root type is explicit.
        output.insert(
            "type".to_owned(),
            serde_json::Value::String("object".to_owned()),
        );
        route.attr.output_schema = Some(output_schema);

        if route.attr.name == "onecrew_result_present" {
            let uri = crate::apps::app_resource_uri("operation-inspector");
            let meta = route.attr.meta.get_or_insert_with(MetaObject::new);
            meta.0.insert(
                "ui".into(),
                serde_json::json!({ "resourceUri": uri, "visibility": ["model", "app"] }),
            );
            meta.0.insert(
                "openai/outputTemplate".into(),
                serde_json::Value::String(uri.to_owned()),
            );
        } else if route.attr.name == "onecrew_result_state" {
            let meta = route.attr.meta.get_or_insert_with(MetaObject::new);
            meta.0
                .insert("ui".into(), serde_json::json!({ "visibility": ["app"] }));
        }
    }
}

fn result_origin(surface: SurfaceId) -> Result<ResultOrigin, ResultStateFailure> {
    ResultOrigin::new(surface_origin_scope(surface)).map_err(map_store_failure)
}

/// Contracts with a short human outline before the serialized JSON block, for
/// clients that show `content`; the model reads `structuredContent`.
pub(super) fn structured_tool_get(result: ToolGetResult) -> Result<CallToolResult, String> {
    let mut response = structured_value(result)?;
    let Some(outline) = response
        .structured_content
        .as_ref()
        .and_then(tool_get_outline)
    else {
        return Ok(result_presentation_failure());
    };
    response.content.insert(0, ContentBlock::text(outline));
    Ok(response)
}

/// One line per contract, stated only from the structured contract: ID,
/// title, connection, effect, and the short schema revision.
fn tool_get_outline(structured: &serde_json::Value) -> Option<String> {
    let contracts = structured.get("operations")?.as_array()?;
    let mut outline = format!(
        "{} operation contracts; the JSON contract follows.",
        contracts.len()
    );
    let words = |value: &serde_json::Value| value.as_str().map(|text| text.replace('_', " "));
    for contract in contracts {
        let field = |name: &str| contract.get(name).and_then(serde_json::Value::as_str);
        let connection = contract.get("connection")?;
        let connection = match connection.get("toolkit") {
            Some(toolkit) => format!("{} {}", toolkit.as_str()?, words(connection.get("kind")?)?),
            None => "no connection".to_owned(),
        };
        let revision = field("schema_revision")?
            .strip_prefix("sha256:")?
            .get(..8)?;
        outline.push_str(&format!(
            "\n- {} — {} · {connection} · {} · rev {revision}…",
            field("id")?,
            field("title")?,
            words(contract.get("effect")?)?,
        ));
    }
    Some(outline)
}

fn structured_presented_result(presented: PresentedResultV2) -> Result<CallToolResult, String> {
    let mut result = structured_value(presented.model)?;
    attach_result_snapshot(&mut result, &presented.snapshot)?;
    Ok(result)
}

fn attach_result_snapshot(
    result: &mut CallToolResult,
    snapshot: &ResultSnapshot,
) -> Result<(), String> {
    let value =
        serde_json::to_value(snapshot).map_err(|_| "result serialization failed".to_owned())?;
    let meta = result.meta.get_or_insert_with(MetaObject::new);
    meta.0.insert(RESULT_META_KEY.into(), value);
    meta.0.insert(
        "viewUUID".into(),
        serde_json::Value::String(snapshot.binding.view.identity.as_str().to_owned()),
    );
    Ok(())
}

fn structured_result_state_failure(failure: ResultStateFailure) -> CallToolResult {
    structured_failure(failure.clone(), result_state_public_failure(&failure))
}

fn structured_result_present_failure(failure: ResultStateFailure) -> CallToolResult {
    structured_failure(failure.clone(), result_present_public_failure(&failure))
}

fn result_present_public_failure(failure: &ResultStateFailure) -> PublicFailure {
    match failure {
        ResultStateFailure::Expired | ResultStateFailure::Absent => RESULT_PRESENT_EXPIRED,
        ResultStateFailure::Unavailable | ResultStateFailure::Capacity => {
            RESULT_PRESENT_UNAVAILABLE
        }
        ResultStateFailure::ViewMismatch
        | ResultStateFailure::OriginMismatch
        | ResultStateFailure::FingerprintMismatch
        | ResultStateFailure::RevisionConflict { .. }
        | ResultStateFailure::ContinuationRevisionConflict { .. }
        | ResultStateFailure::ContinuationLeaseMismatch
        | ResultStateFailure::SearchPlanPayloadTooLarge
        | ResultStateFailure::InvalidInput
        | ResultStateFailure::InvalidState
        | ResultStateFailure::Replay => RESULT_PRESENT_INVALID,
    }
}

fn result_state_public_failure(failure: &ResultStateFailure) -> PublicFailure {
    match failure {
        ResultStateFailure::SearchPlanPayloadTooLarge => PublicFailure {
            code: "search_plan_payload_too_large",
            message: "The edited search plan exceeds the 512 KiB serialized plan limit.",
            parameter_source: None,
            next_action: Some(
                "Reduce the draft or split it into smaller plans before confirmation.",
            ),
            guard: Some("The current draft and its revision remain unchanged."),
        },
        ResultStateFailure::Expired | ResultStateFailure::Absent => RESULT_EXPIRED,
        ResultStateFailure::Unavailable | ResultStateFailure::Capacity => RESULT_UNAVAILABLE,
        ResultStateFailure::ViewMismatch
        | ResultStateFailure::OriginMismatch
        | ResultStateFailure::FingerprintMismatch
        | ResultStateFailure::RevisionConflict { .. }
        | ResultStateFailure::ContinuationRevisionConflict { .. }
        | ResultStateFailure::ContinuationLeaseMismatch
        | ResultStateFailure::InvalidInput
        | ResultStateFailure::InvalidState
        | ResultStateFailure::Replay => RESULT_STATE_INVALID,
    }
}

fn structured(value: impl Serialize) -> Result<CallToolResult, String> {
    structured_value(value)
}

fn structured_value(value: impl Serialize) -> Result<CallToolResult, String> {
    Ok(match serde_json::to_value(value) {
        Ok(value) => CallToolResult::structured(value),
        Err(_) => result_presentation_failure(),
    })
}

fn hints(
    read_only: bool,
    destructive: bool,
    idempotent: bool,
    open_world: bool,
) -> ToolAnnotations {
    ToolAnnotations::from_raw(
        None,
        Some(read_only),
        Some(destructive),
        Some(idempotent),
        Some(open_world),
    )
}

fn compact_output_schema(original: &JsonObject) -> JsonObject {
    use schemars::transform::{RecursiveTransform, Transform};
    let mut schema = schemars::Schema::from(original.clone());
    RecursiveTransform(|schema: &mut schemars::Schema| {
        drop(schema.remove("description"));
    })
    .transform(&mut schema);
    schema
        .as_object()
        .expect("tool schema remains an object")
        .clone()
}

fn inline_schema_refs(schema: &mut JsonObject) {
    let definitions = match schema.remove("$defs") {
        Some(serde_json::Value::Object(definitions)) => definitions,
        Some(_) => panic!("schema $defs is not an object"),
        None => return,
    };
    for value in schema.values_mut() {
        resolve_refs(value, &definitions, &mut Vec::new());
    }
}

fn resolve_refs(
    value: &mut serde_json::Value,
    definitions: &JsonObject,
    active_references: &mut Vec<String>,
) {
    match value {
        serde_json::Value::Object(object) => {
            let reference = object.remove("$ref");
            // Ordinary JSON Schema structure is not reference nesting. Resolve the
            // original siblings once; the expanded definition is already resolved.
            for member in object.values_mut() {
                resolve_refs(member, definitions, active_references);
            }
            if let Some(reference) = reference {
                let reference = reference.as_str().expect("schema $ref is not a string");
                let key = reference
                    .strip_prefix("#/$defs/")
                    .unwrap_or_else(|| panic!("unsupported schema $ref target {reference}"));
                assert!(
                    !active_references.iter().any(|active| active == key),
                    "recursive schema $ref cannot be inlined: {reference}"
                );
                assert!(
                    active_references.len() < 32,
                    "schema $ref nesting exceeded 32 levels"
                );
                let mut resolved = definitions
                    .get(key)
                    .unwrap_or_else(|| panic!("schema $ref to unknown definition {key}"))
                    .clone();
                active_references.push(key.to_owned());
                resolve_refs(&mut resolved, definitions, active_references);
                active_references.pop();
                let serde_json::Value::Object(resolved) = resolved else {
                    panic!("schema definition is not an object")
                };
                for (name, member) in resolved {
                    object.entry(name).or_insert(member);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                resolve_refs(item, definitions, active_references);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod schema_inlining_tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn search_plan_invalid_guidance_and_actual_execute_schema_are_safe() {
        let failure = ToolExecuteOrchestrationFailure::SearchPlanInvalid {
            call_id: crate::operation_registry::descriptor::EntryId::parse("plan").unwrap(),
            operation_id: crate::operation_registry::descriptor::EntryId::parse(
                "onecrew.search.plan.prepare",
            )
            .unwrap(),
            reason: onecrew_local_runtime::SearchPlanEditorValidationReason::InvalidKeyword,
            parameter_pointer: "/keywords/1".into(),
        };
        let response = structured_failure(failure, TOOL_EXECUTE_SEARCH_PLAN_INVALID);
        assert_eq!(response.is_error, Some(true));
        let tools =
            McpAdapterServer::surface_tool_router(crate::server::McpSurface::Full).list_all();
        let execute = tools
            .iter()
            .find(|tool| tool.name == "onecrew_tool_execute")
            .unwrap();
        let schema = Value::Object((**execute.output_schema.as_ref().unwrap()).clone());
        let structured = response.structured_content.as_ref().unwrap();
        jsonschema::validator_for(&schema)
            .unwrap()
            .validate(structured)
            .unwrap();
        assert_eq!(structured["code"], "search_plan_invalid");
        // The model reads only structuredContent, so the guidance is there.
        let guidance = [
            &structured["message"],
            &structured["next_action"],
            &structured["guard"],
        ]
        .map(|field| field.as_str().unwrap())
        .join("\n");
        for expected in [
            "reason and parameter_pointer identify",
            "selected operation contract",
            "No provider was called",
            "no result_ref was minted",
            "onecrew_tool_get",
            "present the corrected plan",
        ] {
            assert!(guidance.contains(expected), "missing guidance: {expected}");
        }
        let rendered = serde_json::to_string(&response).unwrap();
        for forbidden in [
            "SENTINEL",
            "user_requested_direct",
            "presentation=never",
            "skip the plan",
        ] {
            assert!(!rendered.contains(forbidden));
        }
    }

    #[test]
    fn search_plan_scope_invalid_states_its_reason_and_matches_the_execute_schema() {
        use SearchPlanScopeInvalidReason as Reason;
        let tools =
            McpAdapterServer::surface_tool_router(crate::server::McpSurface::Full).list_all();
        let execute = tools
            .iter()
            .find(|tool| tool.name == "onecrew_tool_execute")
            .unwrap();
        let schema = Value::Object((**execute.output_schema.as_ref().unwrap()).clone());
        let validator = jsonschema::validator_for(&schema).unwrap();
        let mut messages = std::collections::BTreeSet::new();
        for reason in [
            Reason::UnknownOperation,
            Reason::OperationNotPlanGoverned,
            Reason::ConnectionInvalid,
            Reason::SchemaRevisionMismatch,
            Reason::ArgumentsInvalid,
            Reason::PlanReferenceInScope,
            Reason::DirectPolicyInScope,
            Reason::ContinuationInInitialPlan,
            Reason::DuplicateScope,
        ] {
            let public = search_plan_scope_invalid(reason);
            assert!(messages.insert(public.message), "{reason:?}");
            let next_action = public.next_action.unwrap();
            let expected_action = match reason {
                Reason::DuplicateScope => "Remove the repeated scope",
                Reason::PlanReferenceInScope | Reason::DirectPolicyInScope => {
                    "Omit the field at parameter_pointer or set it to its neutral value"
                }
                _ => "from the exact onecrew_tool_get contract",
            };
            assert!(next_action.contains(expected_action), "{reason:?}");
            assert!(
                next_action.ends_with("then prepare and present the plan again."),
                "{reason:?}"
            );
            let failure = ToolExecuteOrchestrationFailure::SearchPlanScopeInvalid {
                call_id: crate::operation_registry::descriptor::EntryId::parse("plan").unwrap(),
                operation_id: crate::operation_registry::descriptor::EntryId::parse(
                    "onecrew.search.plan.prepare",
                )
                .unwrap(),
                reason,
                parameter_pointer: "/scopes/0/arguments/search_plan_policy".into(),
            };
            let response = structured_failure(failure, public);
            assert_eq!(response.is_error, Some(true));
            let structured = response.structured_content.as_ref().unwrap();
            validator.validate(structured).unwrap();
            assert_eq!(structured["code"], "search_plan_scope_invalid");
            assert_eq!(structured["reason"], serde_json::to_value(reason).unwrap());
            // The model reads only structuredContent, so the guidance is there.
            assert_eq!(structured["message"], public.message, "{reason:?}");
            assert_eq!(structured["next_action"], next_action, "{reason:?}");
            assert!(
                structured["guard"]
                    .as_str()
                    .unwrap()
                    .contains("no result_ref was minted"),
                "{reason:?}"
            );
            let rendered = serde_json::to_string(&response).unwrap();
            for expected in ["Code: search_plan_scope_invalid", public.message] {
                assert!(rendered.contains(expected), "{reason:?}: {expected}");
            }
            assert!(!rendered.contains("user_requested_direct"), "{reason:?}");
        }
    }

    #[test]
    fn search_plan_approval_rejected_guides_by_reason_and_matches_the_execute_schema() {
        use SearchPlanApprovalError as Reason;
        let tools =
            McpAdapterServer::surface_tool_router(crate::server::McpSurface::Full).list_all();
        let execute = tools
            .iter()
            .find(|tool| tool.name == "onecrew_tool_execute")
            .unwrap();
        let schema = Value::Object((**execute.output_schema.as_ref().unwrap()).clone());
        let validator = jsonschema::validator_for(&schema).unwrap();
        for reason in Reason::ALL {
            let public = search_plan_approval_rejected(reason);
            let failure = ToolExecuteOrchestrationFailure::SearchPlanApprovalRejected {
                call_id: crate::operation_registry::descriptor::EntryId::parse("search").unwrap(),
                operation_id: crate::operation_registry::descriptor::EntryId::parse(
                    "youtube.channels.search",
                )
                .unwrap(),
                reason,
            };
            let response = structured_failure(failure, public);
            let structured = response.structured_content.as_ref().unwrap();
            validator.validate(structured).unwrap();
            assert_eq!(structured["code"], "search_plan_approval_rejected");
            assert_eq!(structured["reason"], reason.code());
            assert_eq!(structured["message"], public.message, "{reason:?}");
            let next_action = public.next_action.unwrap();
            assert_eq!(structured["next_action"], next_action, "{reason:?}");
            let guidance = serde_json::to_string(structured).unwrap();
            // The body has no parameter pointer, and only a missing or expired
            // plan is recovered by preparing a plan.
            assert!(!guidance.contains("parameter_pointer"), "{reason:?}");
            assert_eq!(
                next_action.contains("onecrew.search.plan.prepare"),
                matches!(
                    reason,
                    Reason::ConfirmationRequired | Reason::InvalidOrExpired
                ),
                "{reason:?}"
            );
            match reason {
                Reason::ScopeMismatch => {
                    assert!(next_action.starts_with("Execute the confirmed scope exactly"));
                    assert!(next_action.contains("confirmed_plan_ref"));
                    // The one field a scope fills in for the caller is the one
                    // field a caller changes without meaning to.
                    assert!(next_action.contains("omitted count was frozen at 20"));
                }
                Reason::Unavailable => assert!(next_action.starts_with("Stop")),
                Reason::ConfirmationRequired | Reason::InvalidOrExpired => {}
            }
            assert!(guidance.contains("No provider"), "{reason:?}");
            let rendered = serde_json::to_string(&response).unwrap();
            assert!(rendered.contains("Code: search_plan_approval_rejected"));
        }
    }

    #[test]
    fn model_tool_failures_carry_guidance_and_app_only_failures_stay_closed() {
        let tools =
            McpAdapterServer::surface_tool_router(crate::server::McpSurface::Full).list_all();
        let validator = |name: &str| {
            let tool = tools.iter().find(|tool| tool.name == name).unwrap();
            let schema = Value::Object((**tool.output_schema.as_ref().unwrap()).clone());
            jsonschema::validator_for(&schema).unwrap()
        };
        let tool_get = validator("onecrew_tool_get");
        for (failure, public) in [
            (ToolGetFailure::InvalidBatch, TOOL_GET_INVALID_BATCH),
            (ToolGetFailure::OperationUnavailable, TOOL_GET_UNAVAILABLE),
        ] {
            let structured = structured_failure(failure, public)
                .structured_content
                .unwrap();
            tool_get.validate(&structured).unwrap();
            assert_eq!(structured["message"], public.message);
            assert_eq!(structured["next_action"], public.next_action.unwrap());
        }
        let result_present = validator("onecrew_result_present");
        // The model cannot reach the App-only state route, so its guidance
        // names only model routes.
        for (failure, next_action) in [
            (ResultStateFailure::OriginMismatch, "onecrew_tool_execute"),
            (ResultStateFailure::Replay, "unpresented result_ref"),
            (ResultStateFailure::Expired, "Execute the operation again"),
            (ResultStateFailure::Absent, "Execute the operation again"),
            (ResultStateFailure::Unavailable, "runtime is healthy"),
        ] {
            let public = result_present_public_failure(&failure);
            let presented = structured_result_present_failure(failure.clone())
                .structured_content
                .unwrap();
            result_present.validate(&presented).unwrap();
            assert_eq!(presented["message"], public.message, "{failure:?}");
            assert_eq!(presented["next_action"], public.next_action.unwrap());
            let guidance = serde_json::to_string(&presented).unwrap();
            assert!(guidance.contains(next_action), "{failure:?}: {guidance}");
            for app_only in ["App", "card", "binding", "Agent"] {
                assert!(!guidance.contains(app_only), "{failure:?}: {app_only}");
            }
        }
        let failure = ResultStateFailure::RevisionConflict {
            current_revision: 2,
        };
        let public = result_state_public_failure(&failure);
        // The App parses a closed failure and renders its own copy.
        let app = structured_app_failure(failure, public)
            .structured_content
            .unwrap();
        assert_eq!(
            app,
            json!({ "code": "revision_conflict", "current_revision": 2 })
        );
        let result_state = validator("onecrew_result_state");
        result_state.validate(&app).unwrap();
        let presented = structured_result_present_failure(ResultStateFailure::Replay)
            .structured_content
            .unwrap();
        assert!(!result_state.is_valid(&presented));
    }

    fn has_ref(value: &Value) -> bool {
        match value {
            Value::Object(object) => object.contains_key("$ref") || object.values().any(has_ref),
            Value::Array(items) => items.iter().any(has_ref),
            _ => false,
        }
    }

    #[test]
    fn deep_structure_does_not_consume_reference_budget_and_siblings_are_preserved() {
        let mut deep = json!({"$ref":"#/$defs/Text"});
        for _ in 0..48 {
            deep = json!({"type":"object","properties":{"child":deep}});
        }
        let mut schema = json!({
            "type":"object",
            "$defs":{"Deep":deep,"Text":{"type":"string","title":"Default"}},
            "properties":{
                "first":{"$ref":"#/$defs/Deep","title":"Reviewed values"},
                "second":{"$ref":"#/$defs/Deep"},
                "label":{"$ref":"#/$defs/Text","title":"Override"}
            }
        })
        .as_object()
        .unwrap()
        .clone();
        inline_schema_refs(&mut schema);
        let schema = Value::Object(schema);
        assert!(!has_ref(&schema));
        assert!(schema.get("$defs").is_none());
        assert_eq!(schema["properties"]["first"]["title"], "Reviewed values");
        assert_eq!(schema["properties"]["label"]["title"], "Override");
        assert_eq!(schema["properties"]["label"]["type"], "string");
    }

    #[test]
    fn compact_output_schema_keeps_properties_instances_and_validation() {
        let original = json!({
            "type":"object", "description":"shared", "additionalProperties":false,
            "required":["title","description"],
            "properties":{
                "title":{"type":"string","minLength":1,"description":"shared"},
                "description":{"type":"string","enum":["title","description"],"description":"unique"}
            },
            "default":{"title":"shared","description":"shared"},
            "examples":[{"title":"shared","description":"shared"}]
        });
        let compact = Value::Object(compact_output_schema(original.as_object().unwrap()));
        assert!(compact.get("description").is_none());
        assert_eq!(compact["properties"]["description"]["type"], "string");
        assert!(compact["properties"]["title"].get("description").is_none());
        assert_eq!(compact["default"], original["default"]);
        assert_eq!(compact["examples"], original["examples"]);
        assert_eq!(compact["required"], original["required"]);
        let expected = jsonschema::validator_for(&original).unwrap();
        let actual = jsonschema::validator_for(&compact).unwrap();
        for (value, valid) in [
            (json!({"title":"a","description":"title"}), true),
            (json!({"title":"","description":"title"}), false),
            (json!({"title":"a","description":"other"}), false),
            (json!({"title":"a"}), false),
            (json!({"title":"a","description":"title","extra":1}), false),
        ] {
            assert_eq!(expected.is_valid(&value), valid);
            assert_eq!(actual.is_valid(&value), valid);
        }
    }

    fn reference_chain(count: usize) -> JsonObject {
        let mut definitions = JsonObject::new();
        for index in 0..count {
            definitions.insert(
                format!("R{index}"),
                if index + 1 == count {
                    json!({"type":"string"})
                } else {
                    json!({"$ref":format!("#/$defs/R{}",index+1)})
                },
            );
        }
        json!({"type":"object","$defs":definitions,"properties":{"value":{"$ref":"#/$defs/R0"}}})
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn thirty_two_real_reference_hops_remain_allowed() {
        let mut schema = reference_chain(32);
        inline_schema_refs(&mut schema);
        assert_eq!(schema["properties"]["value"]["type"], "string");
    }

    #[test]
    #[should_panic(expected = "schema $ref nesting exceeded 32 levels")]
    fn thirty_three_real_reference_hops_remain_rejected() {
        inline_schema_refs(&mut reference_chain(33));
    }

    #[test]
    #[should_panic(expected = "recursive schema $ref cannot be inlined")]
    fn recursive_definitions_fail_explicitly_without_repeated_expansion() {
        let mut schema = json!({"$defs":{"Node":{"type":"object","properties":{"next":{"$ref":"#/$defs/Node"}}}},"properties":{"root":{"$ref":"#/$defs/Node"}}}).as_object().unwrap().clone();
        inline_schema_refs(&mut schema);
    }

    #[test]
    fn real_creator_batch_state_schemas_inline_during_router_initialization() {
        let tools =
            McpAdapterServer::surface_tool_router(crate::server::McpSurface::Full).list_all();
        let state = tools
            .iter()
            .find(|tool| tool.name == "onecrew_result_state")
            .expect("App state tool");
        let input = Value::Object((*state.input_schema).clone());
        let output = Value::Object((**state.output_schema.as_ref().expect("state output")).clone());
        assert!(!has_ref(&input));
        assert!(!has_ref(&output));
        assert!(
            serde_json::to_string(&input)
                .unwrap()
                .contains("confirm_creator_save_batch")
        );
        assert!(
            serde_json::to_string(&output)
                .unwrap()
                .contains("creator_save_rows")
        );
    }
}
