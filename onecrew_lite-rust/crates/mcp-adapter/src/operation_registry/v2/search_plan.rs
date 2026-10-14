use super::{
    contract::{ResultStateFailure, ToolExecutionCall},
    state::SubmittedTerminalGuard,
};
use crate::operation_registry::{
    descriptor::{ConnectionRef, EntryId, RegistryEntry, SurfaceId},
    execution::{
        ExecuteFailureKind, ExecutionContext, PreparedInvocation, SearchScopePurpose,
        prepare_invocation,
    },
    registration::OperationRegistry,
};
use onecrew_local_runtime::{
    CallDisposition, ExecutedBatch, ExecutedCallOutcome, ExecutedResultInput, InitialResultState,
    LocalPlatform, OwnerActionPayload, PendingActionView, PendingUserAction,
    ResultContinuationContext, ResultContinuationRequest, ResultOrigin, ResultRef,
    ResultResolution, ResultSnapshot, ResultStoreError, ResultTerminalUpdate, ResultViewKind,
    SearchPlanEditorDraft, TerminalResult, search_plan_projection,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) const OPERATION_ID: &str = "onecrew.search.plan.prepare";

/// Why a plan scope was rejected; `parameter_pointer` names the rejected field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(in crate::operation_registry) enum SearchPlanScopeInvalidReason {
    UnknownOperation,
    OperationNotPlanGoverned,
    ConnectionInvalid,
    SchemaRevisionMismatch,
    ArgumentsInvalid,
    PlanReferenceInScope,
    DirectPolicyInScope,
    ContinuationInInitialPlan,
    DuplicateScope,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PrepareSearchPlanError {
    DraftInvalid(onecrew_local_runtime::SearchPlanEditorValidationError),
    Store(ResultStoreError),
    /// `pointer` is relative to scope `index` and holds only field names.
    ScopeInvalid {
        index: usize,
        reason: SearchPlanScopeInvalidReason,
        pointer: &'static str,
    },
}

impl From<ResultStoreError> for PrepareSearchPlanError {
    fn from(error: ResultStoreError) -> Self {
        Self::Store(error)
    }
}

pub(super) fn prepare_search_plan(
    registry: &OperationRegistry,
    surface: SurfaceId,
    origin: &ResultOrigin,
    call: &ToolExecutionCall,
    minter: &(dyn Fn(ExecutedResultInput) -> Result<ResultRef, ResultStoreError> + Sync),
) -> Result<ResultRef, PrepareSearchPlanError> {
    let draft: SearchPlanEditorDraft =
        serde_json::from_value(serde_json::Value::Object(call.arguments.clone()))
            .map_err(|_| ResultStoreError::InvalidInput("search_plan"))?;
    // Domain rejection precedes payload storage, presentation, and reference minting.
    // Store InvalidInput errors must never be reinterpreted as keyword failures.
    draft
        .validate_typed()
        .map_err(PrepareSearchPlanError::DraftInvalid)?;
    validate_scopes(registry, surface, &draft)?;
    let payload = OwnerActionPayload::from_search_plan(&draft)?;
    minter(ExecutedResultInput {
        origin: origin.clone(),
        view_kind: ResultViewKind::Confirmation,
        batch: ExecutedBatch {
            outcomes: vec![ExecutedCallOutcome {
                call_id: call.call_id.as_str().to_owned(),
                operation_id: OPERATION_ID.to_owned(),
                disposition: CallDisposition::NotRun {
                    reason: "search_plan_confirmation_required".to_owned(),
                },
                summary: "Search plan awaits user review. No search has run.".to_owned(),
                resource_refs: vec![],
            }],
        },
        presentation: search_plan_projection(&draft)?,
        pending_action: Some(PendingUserAction::SearchPlan { draft, payload }),
        initial_state: InitialResultState::Available,
    })
    .map_err(PrepareSearchPlanError::Store)
}

pub(super) fn confirm_search_plan(
    registry: &OperationRegistry,
    surface: SurfaceId,
    platform: &LocalPlatform,
    current: &ResultSnapshot,
    payload: Vec<u8>,
    mut guard: SubmittedTerminalGuard<'_>,
) -> Result<ResultSnapshot, ResultStateFailure> {
    let Ok(draft) = serde_json::from_slice::<SearchPlanEditorDraft>(&payload) else {
        return guard.close_known("search_plan_payload_invalid");
    };
    if !matches!(&current.pending_action, Some(PendingActionView::SearchPlan { draft: visible, .. }) if visible == &draft)
        || !matches!(current.batch.outcomes.as_slice(), [outcome] if outcome.operation_id == OPERATION_ID)
    {
        return guard.close_known("search_plan_snapshot_mismatch");
    }
    let Ok(compiled_scopes) = validate_scopes(registry, surface, &draft) else {
        return guard.close_known("search_plan_scope_invalid");
    };
    guard.mark_dispatch_may_have_started();
    let confirmed = match platform.confirm_search_plan_editor(draft, compiled_scopes) {
        Ok(confirmed) => confirmed,
        Err(_) => return guard.close_known("search_plan_confirmation_failed"),
    };
    let summary = confirmation_summary(&confirmed.confirmed_plan_ref);
    let mut batch = current.batch.clone();
    batch.outcomes[0].disposition = CallDisposition::Succeeded;
    batch.outcomes[0].summary = summary.clone();
    let mut presentation = current.presentation.clone();
    presentation.summary = Some(summary.clone());
    let update = ResultTerminalUpdate {
        terminal: TerminalResult::Resolved(ResultResolution {
            code: "search_plan_confirmed".to_owned(),
            summary,
        }),
        batch,
        presentation,
        // The visible message is short and localized; the context carries only
        // what the model lacks: the reference and the criteria the user may
        // have edited. The model prepared the scopes, and the card cannot edit
        // them; the authority rejects any deviation.
        continuation: Some(ResultContinuationRequest {
            message: super::projection::continuation_copy(current.locale)
                .search_plan_confirmed
                .clone(),
            requires_model_context: true,
            context: ResultContinuationContext::SearchPlanConfirmed {
                confirmed_plan_ref: confirmed.confirmed_plan_ref.clone(),
                description: confirmed.plan.description,
                keywords: confirmed.plan.keywords,
            },
        }),
    };
    match guard.commit(update) {
        Ok(snapshot) => Ok(snapshot),
        Err(commit_failure) => {
            // An undeliverable approval must not remain usable. The commit
            // failure stays the reported error; a cancellation failure means
            // the approval store itself is unavailable, which every later
            // authorize call also reports. The failure is emitted through the
            // `log` facade (visible only when the host installs a logger)
            // instead of being bound to `_`.
            if let Err(cancellation_failure) =
                platform.cancel_search_plan(&confirmed.confirmed_plan_ref)
            {
                log::warn!(
                    "search plan confirmation commit failed ({commit_failure:?}) and the approval could not be cancelled: {cancellation_failure}"
                );
            }
            Err(commit_failure)
        }
    }
}

/// The confirmed plan's batch-outcome and result summary. No card renders it;
/// the model receives the localized continuation instead.
pub(super) fn confirmation_summary(confirmed_plan_ref: &str) -> String {
    format!(
        "Search plan confirmed: {confirmed_plan_ref}. Use this reference for the approved searches and evaluate results against the final acceptance criteria. {}",
        plan_ttl_statement()
    )
}

/// The plan's lifetime, stated once in the prepare contract and once at
/// confirmation. A confirmed plan sets criteria and parameters only, so the
/// TTL is the one bound OneCrew places on it; the number comes from the
/// authority's constant.
pub(in crate::operation_registry) fn plan_ttl_statement() -> String {
    format!(
        "The confirmed_plan_ref expires {minutes} minutes after confirmation; until then approved searches page freely with next_cursor, and changed criteria need a new confirmed plan.",
        minutes = onecrew_local_runtime::SEARCH_PLAN_TTL_SECONDS / 60,
    )
}

pub(super) fn requires_plan(
    registry: &OperationRegistry,
    surface: SurfaceId,
    operation: &str,
) -> bool {
    let Ok(id) = EntryId::parse(operation) else {
        return false;
    };
    matches!(registry.entry(&id, surface), Some(RegistryEntry::Operation(entry)) if entry.binding.supports_search_plan())
}

fn validate_scopes(
    registry: &OperationRegistry,
    surface: SurfaceId,
    draft: &SearchPlanEditorDraft,
) -> Result<Vec<String>, PrepareSearchPlanError> {
    use SearchPlanScopeInvalidReason as Reason;
    let mut compiled = Vec::with_capacity(draft.scopes.len());
    for (index, scope) in draft.scopes.iter().enumerate() {
        let invalid = |reason, pointer| PrepareSearchPlanError::ScopeInvalid {
            index,
            reason,
            pointer,
        };
        let id = EntryId::parse(&scope.operation_id)
            .map_err(|_| invalid(Reason::UnknownOperation, "/operation_id"))?;
        let Some(RegistryEntry::Operation(operation)) = registry.entry(&id, surface) else {
            return Err(invalid(Reason::UnknownOperation, "/operation_id"));
        };
        if !operation.binding.supports_search_plan() {
            return Err(invalid(Reason::OperationNotPlanGoverned, "/operation_id"));
        }
        // A scope holds the search call's arguments, whose plan fields may be
        // present only with their neutral values; the compiled token excludes
        // both fields, so prepare, confirm, and execute tokens stay equal.
        if scope
            .arguments
            .get("confirmed_plan_ref")
            .is_some_and(|reference| !reference.is_null())
        {
            return Err(invalid(
                Reason::PlanReferenceInScope,
                "/arguments/confirmed_plan_ref",
            ));
        }
        match scope
            .arguments
            .get("search_plan_policy")
            .map(onecrew_local_runtime::SearchPlanPolicy::deserialize)
        {
            None | Some(Ok(onecrew_local_runtime::SearchPlanPolicy::RequireConfirmation)) => {}
            Some(Ok(onecrew_local_runtime::SearchPlanPolicy::UserRequestedDirect)) => {
                return Err(invalid(
                    Reason::DirectPolicyInScope,
                    "/arguments/search_plan_policy",
                ));
            }
            Some(Err(_)) => {
                return Err(invalid(
                    Reason::ArgumentsInvalid,
                    "/arguments/search_plan_policy",
                ));
            }
        }
        let connection: ConnectionRef = serde_json::to_value(&scope.connection)
            .and_then(serde_json::from_value)
            .map_err(|_| invalid(Reason::ConnectionInvalid, "/connection"))?;
        // Drop the prepared binding: plan validation must never dispatch it.
        let prepared = prepare_invocation(
            operation,
            ExecutionContext {
                surface,
                request_id: "search-plan-validation".into(),
                connection: Some(&connection),
                confirmed: false,
            },
            &scope.schema_revision,
            serde_json::Value::Object(scope.arguments.clone()),
        )
        .map_err(|failure| {
            let (reason, pointer) = prepare_rejection(&failure.kind);
            invalid(reason, pointer)
        })?;
        let token = match prepared.search_scope_token(SearchScopePurpose::InitialPlan) {
            Ok(Some(token)) => token,
            Ok(None) => return Err(invalid(Reason::OperationNotPlanGoverned, "/operation_id")),
            // Typed preparation accepted the arguments, so a scope that
            // compiles for a later page but not for a plan's first page
            // starts from a continuation.
            Err(_) => {
                let reason = if matches!(
                    prepared.search_scope_token(SearchScopePurpose::Execute),
                    Ok(Some(_))
                ) {
                    Reason::ContinuationInInitialPlan
                } else {
                    Reason::ArgumentsInvalid
                };
                return Err(invalid(reason, "/arguments"));
            }
        };
        // A scope is its compiled token, so a repeat names no new search.
        if compiled.contains(&token) {
            return Err(invalid(Reason::DuplicateScope, ""));
        }
        compiled.push(token);
    }
    Ok(compiled)
}

/// The scope check and field behind a typed-preparation rejection. Only the
/// pre-invocation kinds occur here; preparation never approves, dispatches,
/// or projects output, so the remaining kinds share the arguments reason.
fn prepare_rejection(kind: &ExecuteFailureKind) -> (SearchPlanScopeInvalidReason, &'static str) {
    use SearchPlanScopeInvalidReason as Reason;
    match kind {
        ExecuteFailureKind::UnknownOperation | ExecuteFailureKind::SurfaceDenied => {
            (Reason::UnknownOperation, "/operation_id")
        }
        ExecuteFailureKind::SchemaRevisionMismatch => {
            (Reason::SchemaRevisionMismatch, "/schema_revision")
        }
        ExecuteFailureKind::ConnectionMismatch => (Reason::ConnectionInvalid, "/connection"),
        ExecuteFailureKind::InputSchemaRejected
        | ExecuteFailureKind::TypedDecodeRejected
        | ExecuteFailureKind::Approval(_)
        | ExecuteFailureKind::OwnerRejected { .. }
        | ExecuteFailureKind::OutputSerializationFailed
        | ExecuteFailureKind::OutputContractFailed => (Reason::ArgumentsInvalid, "/arguments"),
    }
}

/// The model-facing policy field of a call. Absent or undecodable values
/// resolve to the default (confirmation required); the typed invocation, not
/// this lookup, decides whether the call itself is acceptable.
pub(super) fn call_search_policy(
    call: &ToolExecutionCall,
) -> onecrew_local_runtime::SearchPlanPolicy {
    call.arguments
        .get("search_plan_policy")
        .cloned()
        .map(serde_json::from_value::<onecrew_local_runtime::SearchPlanPolicy>)
        .transpose()
        .unwrap_or_default()
        .unwrap_or_default()
}

/// The only place an `OperationSearchAuthorization` is obtained for a call:
/// the reference comes from the call arguments, the scope token is compiled
/// from the already-validated typed invocation, and the OneCrew authority
/// decides. No JSON path decodes into an authorization. Minting only
/// validates the approval and records nothing, so a prevalidated call that is
/// later skipped or stopped leaves the plan as it was.
pub(super) fn authorize_call(
    platform: &LocalPlatform,
    call: &ToolExecutionCall,
    invocation: &PreparedInvocation<'_>,
) -> Result<
    onecrew_local_runtime::OperationSearchAuthorization,
    onecrew_local_runtime::SearchPlanApprovalError,
> {
    use onecrew_local_runtime::SearchPlanApprovalError as E;
    let reference = call
        .arguments
        .get("confirmed_plan_ref")
        .and_then(serde_json::Value::as_str);
    let token = invocation
        .search_scope_token(SearchScopePurpose::Execute)
        .map_err(|_| E::ScopeMismatch)?
        .ok_or(E::ScopeMismatch)?;
    platform.authorize_search_scope(reference, &token)
}
