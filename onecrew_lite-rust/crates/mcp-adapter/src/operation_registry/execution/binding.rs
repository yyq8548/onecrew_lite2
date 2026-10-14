//! Typed decoding and erased owner dispatch share one binding boundary.
use super::failure::ClassifiedOwnerFailure;
use crate::operation_registry::descriptor::ConnectionRef;
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{future::Future, pin::Pin, sync::Arc};

/// The closed execution contract attached to every registered binding.
///
/// `IndependentRead` is intentionally a capability declaration rather than a
/// property inferred from an operation name or one effect label. Registration
/// validates it against the complete operation policy before the binding can
/// enter a sealed registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionDeclaration {
    Sequential,
    IndependentRead,
}

/// The binding declares whether dispatch yields ordinary final data or an
/// immutable review whose eventual completion uses the registered output schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BindingOutputClass {
    Payload,
    OwnerReview,
}

#[derive(Debug)]
pub(crate) enum BindingOutput {
    Payload(Value),
    OwnerReview(Box<onecrew_local_runtime::PreparedOwnerReview>),
    /// A typed owner completed without requiring user action. The owner keeps
    /// the terminal projection and continuation facts inside this value; the
    /// v2 execute path decides whether to mint the optional terminal result.
    OwnerNoop(Box<onecrew_local_runtime::PreparedOwnerNoop>),
}

type BindingFuture =
    Pin<Box<dyn Future<Output = Result<BindingOutput, BindingDispatchFailure>> + Send + 'static>>;
type BindingPrepare = Arc<
    dyn Fn(Value, &BindingContext) -> Result<Box<dyn PreparedBinding>, BindingPrepareFailure>
        + Send
        + Sync,
>;

pub(crate) struct ErasedBinding {
    prepare: BindingPrepare,
    supports_search_plan: bool,
    output_class: BindingOutputClass,
    execution: ExecutionDeclaration,
}

#[derive(Clone, Copy)]
pub(crate) enum SearchScopePurpose {
    InitialPlan,
    Execute,
}
pub(crate) type SearchScopeCompiler<I> =
    fn(&I, SearchScopePurpose) -> Result<serde_json::Map<String, Value>, ()>;

pub(super) trait PreparedBinding: Send {
    fn dispatch(self: Box<Self>, context: BindingContext) -> BindingFuture;
    fn search_arguments(
        &self,
        purpose: SearchScopePurpose,
    ) -> Result<Option<serde_json::Map<String, Value>>, BindingPrepareFailure>;
}

struct TypedPreparedBinding<I, F, C, O> {
    input: I,
    binding: Arc<F>,
    classify: Arc<C>,
    search_scope: Option<SearchScopeCompiler<I>>,
    encode_output: fn(O) -> Result<BindingOutput, BindingDispatchFailure>,
}

impl<I, O, E, F, Fut, C> PreparedBinding for TypedPreparedBinding<I, F, C, O>
where
    I: Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
    F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O, E>> + Send + 'static,
    C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
{
    fn dispatch(self: Box<Self>, context: BindingContext) -> BindingFuture {
        let Self {
            input,
            binding,
            classify,
            search_scope: _,
            encode_output,
        } = *self;
        Box::pin(async move {
            let output = binding(context, input)
                .await
                .map_err(|error| BindingDispatchFailure::OwnerRejected(classify(error)))?;
            encode_output(output)
        })
    }
    fn search_arguments(
        &self,
        purpose: SearchScopePurpose,
    ) -> Result<Option<serde_json::Map<String, Value>>, BindingPrepareFailure> {
        self.search_scope
            .map(|compile| compile(&self.input, purpose))
            .transpose()
            .map_err(|_| BindingPrepareFailure::TypedDecode)
    }
}

pub(crate) fn typed_binding<I, O, E, F, Fut, C, V>(
    binding: F,
    classify: C,
    validate: V,
    search_scope: Option<SearchScopeCompiler<I>>,
    execution: ExecutionDeclaration,
) -> ErasedBinding
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + Send + 'static,
    E: Send + 'static,
    F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O, E>> + Send + 'static,
    C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
    V: Fn(&BindingContext, &I) -> Result<(), ()> + Send + Sync + 'static,
{
    make_binding(
        binding,
        classify,
        validate,
        search_scope,
        BindingOutputClass::Payload,
        execution,
        |output| {
            serde_json::to_value(output)
                .map(BindingOutput::Payload)
                .map_err(|_| BindingDispatchFailure::OutputSerialization)
        },
    )
}

pub(crate) fn typed_owner_review_binding<I, E, F, Fut, C>(
    binding: F,
    classify: C,
    execution: ExecutionDeclaration,
) -> ErasedBinding
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    E: Send + 'static,
    F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<onecrew_local_runtime::PreparedOwnerReview, E>> + Send + 'static,
    C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
{
    make_binding(
        binding,
        classify,
        |_, _: &I| Ok(()),
        None,
        BindingOutputClass::OwnerReview,
        execution,
        |review| {
            if review.is_noop() {
                let noop = review
                    .into_noop()
                    .map_err(|_| BindingDispatchFailure::OwnerNoopInvalid)?;
                Ok(BindingOutput::OwnerNoop(Box::new(noop)))
            } else {
                Ok(BindingOutput::OwnerReview(Box::new(review)))
            }
        },
    )
}

fn make_binding<I, O, E, F, Fut, C, V>(
    binding: F,
    classify: C,
    validate: V,
    search_scope: Option<SearchScopeCompiler<I>>,
    output_class: BindingOutputClass,
    execution: ExecutionDeclaration,
    encode_output: fn(O) -> Result<BindingOutput, BindingDispatchFailure>,
) -> ErasedBinding
where
    I: DeserializeOwned + Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
    F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O, E>> + Send + 'static,
    C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
    V: Fn(&BindingContext, &I) -> Result<(), ()> + Send + Sync + 'static,
{
    let binding = Arc::new(binding);
    let classify = Arc::new(classify);
    ErasedBinding {
        supports_search_plan: search_scope.is_some(),
        output_class,
        execution,
        prepare: Arc::new(move |arguments, context| {
            let input = serde_json::from_value::<I>(arguments)
                .map_err(|_| BindingPrepareFailure::TypedDecode)?;
            validate(context, &input).map_err(|_| BindingPrepareFailure::TypedDecode)?;
            Ok(Box::new(TypedPreparedBinding {
                input,
                binding: Arc::clone(&binding),
                classify: Arc::clone(&classify),
                search_scope,
                encode_output,
            }))
        }),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BindingContext {
    pub(crate) request_id: String,
    pub(crate) connection: Option<ConnectionRef>,
    pub(crate) search_authorization: Option<onecrew_local_runtime::OperationSearchAuthorization>,
    /// Server-owned result scope, attached by the result orchestration path.
    /// This context never comes from an operation's JSON arguments.
    pub(crate) result_context: Option<BindingResultContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BindingResultContext {
    pub(crate) operation_id: String,
    pub(crate) call_digest: Option<onecrew_local_runtime::OwnerCallDigest>,
    pub(crate) origin: onecrew_local_runtime::ResultOrigin,
    pub(crate) call_id: String,
}

impl BindingContext {
    /// Only V2's exact validated call path supplies the context and full call
    /// digest. A raw operation request cannot turn its arguments into a review.
    pub(crate) fn owner_review_context(
        self,
    ) -> Result<onecrew_local_runtime::OwnerReviewContext, onecrew_local_runtime::ResultStoreError>
    {
        let result =
            self.result_context
                .ok_or(onecrew_local_runtime::ResultStoreError::InvalidInput(
                    "owner_review_context_required",
                ))?;
        if result.call_digest.is_none() {
            return Err(onecrew_local_runtime::ResultStoreError::InvalidInput(
                "owner_review_call_digest_required",
            ));
        }
        onecrew_local_runtime::OwnerReviewContext::new(
            result.origin,
            result.call_id,
            result.operation_id,
            result.call_digest,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BindingPrepareFailure {
    TypedDecode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BindingDispatchFailure {
    OwnerRejected(ClassifiedOwnerFailure),
    OutputSerialization,
    OwnerNoopInvalid,
}

impl ErasedBinding {
    pub(crate) fn output_class(&self) -> BindingOutputClass {
        self.output_class
    }
    pub(crate) fn supports_search_plan(&self) -> bool {
        self.supports_search_plan
    }

    pub(crate) fn execution(&self) -> ExecutionDeclaration {
        self.execution
    }
    pub(super) fn prepare(
        &self,
        arguments: Value,
        context: &BindingContext,
    ) -> Result<Box<dyn PreparedBinding>, BindingPrepareFailure> {
        (self.prepare)(arguments, context)
    }
}
