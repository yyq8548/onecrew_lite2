//! Preserves validation, typed preparation, approval, owner dispatch, and output checks in order.
use super::{
    binding::{
        BindingContext, BindingDispatchFailure, BindingOutput, BindingOutputClass,
        BindingPrepareFailure, BindingResultContext, PreparedBinding,
    },
    failure::{ExecuteFailure, ExecuteFailureKind},
};
use crate::operation_registry::{
    approval::preflight,
    descriptor::{ConnectionRef, OperationEntry, SurfaceId},
};
use serde_json::Value;

pub(crate) struct ExecutionContext<'a> {
    pub(crate) surface: SurfaceId,
    pub(crate) request_id: String,
    pub(crate) connection: Option<&'a ConnectionRef>,
    /// True only when the human confirmed this exact call through a
    /// result-store submission; a direct execute is never confirmed.
    pub(crate) confirmed: bool,
}

pub(crate) struct PreparedInvocation<'entry> {
    entry: &'entry OperationEntry,
    binding_context: BindingContext,
    binding: Box<dyn PreparedBinding>,
    confirmed: bool,
}

pub(crate) fn prepare_invocation<'entry>(
    entry: &'entry OperationEntry,
    context: ExecutionContext<'_>,
    schema_revision: &str,
    arguments: Value,
) -> Result<PreparedInvocation<'entry>, ExecuteFailure> {
    reject_retired_operation(entry)?;
    validate_invocation(
        entry,
        context.surface,
        context.connection,
        schema_revision,
        &arguments,
    )?;
    let binding_context = BindingContext {
        request_id: context.request_id,
        connection: context.connection.cloned(),
        search_authorization: None,
        result_context: None,
    };
    let binding = entry.binding.prepare(arguments, &binding_context).map_err(
        |BindingPrepareFailure::TypedDecode| {
            ExecuteFailure::before(ExecuteFailureKind::TypedDecodeRejected)
        },
    )?;
    Ok(PreparedInvocation {
        entry,
        binding_context,
        binding,
        confirmed: context.confirmed,
    })
}

impl PreparedInvocation<'_> {
    /// Bind a prepared typed call to the authenticated result origin before
    /// dispatch. Owner-minted review results retain this exact scope.
    pub(crate) fn bind_result_context(
        &mut self,
        origin: &onecrew_local_runtime::ResultOrigin,
        call_id: &str,
    ) {
        self.binding_context.result_context = Some(BindingResultContext {
            operation_id: self.entry.metadata.id.as_str().to_owned(),
            call_digest: None,
            origin: origin.clone(),
            call_id: call_id.to_owned(),
        });
    }

    pub(crate) fn bind_owner_review_context(
        &mut self,
        origin: &onecrew_local_runtime::ResultOrigin,
        call_id: &str,
        call_digest: onecrew_local_runtime::OwnerCallDigest,
    ) {
        self.bind_result_context(origin, call_id);
        self.binding_context
            .result_context
            .as_mut()
            .expect("bound above")
            .call_digest = Some(call_digest);
    }

    pub(crate) fn authorize_search(
        &mut self,
        authorization: onecrew_local_runtime::OperationSearchAuthorization,
    ) {
        self.binding_context.search_authorization = Some(authorization);
    }
    /// Compile the approval scope of this exact typed invocation: operation,
    /// input schema revision, selected connection, and the operation-owned
    /// argument normalization. `None` means the operation takes no plan.
    pub(crate) fn search_scope_token(
        &self,
        purpose: super::SearchScopePurpose,
    ) -> Result<Option<String>, ExecuteFailure> {
        let Some(arguments) = self.search_scope_arguments(purpose)? else {
            return Ok(None);
        };
        let connection = self
            .binding_context
            .connection
            .as_ref()
            .ok_or_else(|| ExecuteFailure::before(ExecuteFailureKind::ConnectionMismatch))?;
        let connection = serde_json::to_value(connection)
            .and_then(serde_json::from_value)
            .map_err(|_| ExecuteFailure::before(ExecuteFailureKind::TypedDecodeRejected))?;
        let scope = onecrew_local_runtime::SearchPlanScope {
            operation_id: self.entry.metadata.id.as_str().into(),
            schema_revision: self.entry.input.revision().into(),
            connection,
            arguments,
        };
        scope
            .approval_token()
            .map(Some)
            .map_err(|_| ExecuteFailure::before(ExecuteFailureKind::TypedDecodeRejected))
    }

    /// The operation-owned scope arguments before hashing; tests use it to
    /// prove which fields a compiler keeps out of the approval scope.
    pub(crate) fn search_scope_arguments(
        &self,
        purpose: super::SearchScopePurpose,
    ) -> Result<Option<serde_json::Map<String, Value>>, ExecuteFailure> {
        self.binding
            .search_arguments(purpose)
            .map_err(|_| ExecuteFailure::before(ExecuteFailureKind::TypedDecodeRejected))
    }
}

/// Test convenience for ordinary payload assertions; production dispatch uses
/// the declared output class through `dispatch_prepared_output` exclusively.
#[cfg(test)]
pub(crate) async fn dispatch_prepared(
    prepared: PreparedInvocation<'_>,
) -> Result<Value, ExecuteFailure> {
    if prepared.entry.binding.output_class() == BindingOutputClass::OwnerReview {
        return Err(ExecuteFailure::before(
            ExecuteFailureKind::TypedDecodeRejected,
        ));
    }
    match dispatch_prepared_output(prepared).await? {
        BindingOutput::Payload(value) => Ok(value),
        BindingOutput::OwnerNoop(noop) => Ok(noop.output().clone()),
        BindingOutput::OwnerReview(_) => unreachable!("ordinary binding class checked above"),
    }
}

pub(crate) async fn dispatch_prepared_output(
    prepared: PreparedInvocation<'_>,
) -> Result<BindingOutput, ExecuteFailure> {
    reject_retired_operation(prepared.entry)?;
    let PreparedInvocation {
        entry,
        binding_context,
        binding,
        confirmed,
    } = prepared;
    if entry.binding.output_class() == BindingOutputClass::Payload {
        preflight(entry.approval, entry.approval_request.as_ref(), confirmed)
            .map_err(ExecuteFailure::approval)?;
    }

    let output = binding
        .dispatch(binding_context)
        .await
        .map_err(|failure| match failure {
            BindingDispatchFailure::OwnerRejected(failure) => ExecuteFailure::owner(failure),
            BindingDispatchFailure::OutputSerialization => {
                ExecuteFailure::after(ExecuteFailureKind::OutputSerializationFailed, entry.effect)
            }
            BindingDispatchFailure::OwnerNoopInvalid => {
                ExecuteFailure::after(ExecuteFailureKind::OutputContractFailed, entry.effect)
            }
        })?;
    if matches!(&output, BindingOutput::Payload(value) if !entry.output.validate(value))
        || matches!(
            &output,
            BindingOutput::OwnerNoop(noop) if !entry.output.validate(noop.output())
        )
    {
        return Err(ExecuteFailure::after(
            ExecuteFailureKind::OutputContractFailed,
            entry.effect,
        ));
    }
    Ok(output)
}

fn reject_retired_operation(entry: &OperationEntry) -> Result<(), ExecuteFailure> {
    if onecrew_local_runtime::is_retired_operation(entry.metadata.id.as_str()) {
        return Err(ExecuteFailure::unknown_operation());
    }
    Ok(())
}

fn validate_invocation(
    entry: &OperationEntry,
    surface: SurfaceId,
    connection: Option<&ConnectionRef>,
    schema_revision: &str,
    arguments: &Value,
) -> Result<(), ExecuteFailure> {
    if !entry.metadata.surfaces.allows(surface) {
        return Err(ExecuteFailure::before(ExecuteFailureKind::SurfaceDenied));
    }
    if entry.input.revision() != schema_revision {
        return Err(ExecuteFailure::before(
            ExecuteFailureKind::SchemaRevisionMismatch,
        ));
    }
    if !entry.connection.accepts(connection) {
        return Err(ExecuteFailure::before(
            ExecuteFailureKind::ConnectionMismatch,
        ));
    }
    if !entry.input.validate(arguments) {
        return Err(ExecuteFailure::before(
            ExecuteFailureKind::InputSchemaRejected,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
