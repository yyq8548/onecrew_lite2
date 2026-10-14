//! Accumulates unique registrations and validates every entry before sealing.
use super::{
    error::RegistryBuildError,
    policy::OperationPolicy,
    registry::OperationRegistry,
    validation::{
        validate_operation_safety, validate_playbook, validate_readback_references,
        validate_toolkit_guide,
    },
};
use crate::operation_registry::{
    descriptor::{
        ApprovalClass, EffectClass, EntryId, EntryMetadata, OperationEntry, OperationRegistration,
        OperationViewKind, PlaybookEntry, RegistryEntry, SecretClass, ToolkitGuideEntry,
    },
    execution::{self, BindingContext, ClassifiedOwnerFailure, ErasedBinding},
    schema::CompiledSchema,
};
use onecrew_local_runtime::{CapabilityCatalog, CatalogSurface};
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
};

pub(crate) struct RegistryBuilder {
    catalog: Arc<CapabilityCatalog>,
    entries: BTreeMap<EntryId, RegistryEntry>,
    aliases: BTreeMap<String, EntryId>,
}

impl RegistryBuilder {
    pub(crate) fn new(catalog: Arc<CapabilityCatalog>) -> Self {
        Self {
            catalog,
            entries: BTreeMap::new(),
            aliases: BTreeMap::new(),
        }
    }

    pub(crate) fn catalog(&self) -> &CapabilityCatalog {
        &self.catalog
    }

    fn project_operation(
        &self,
        registration: OperationRegistration,
    ) -> Result<EntryMetadata, RegistryBuildError> {
        let id = EntryId::parse(registration.id.as_str())
            .map_err(|_| RegistryBuildError::InvalidCatalogReference)?;
        let summary = self
            .catalog
            .operation(id.as_str())
            .ok_or_else(|| RegistryBuildError::UnknownCatalogOperation(id.clone()))?;
        if !summary.surfaces.contains(&CatalogSurface::McpBinding) {
            return Err(RegistryBuildError::OperationNotMcpSupported(id));
        }
        Ok(EntryMetadata {
            id,
            title: summary.title.to_owned(),
            summary: summary.summary.to_owned(),
            toolkit: summary.toolkit_id,
            domain_id: registration.domain_id,
            aliases: registration.aliases,
            surfaces: registration.surfaces,
        })
    }

    pub(crate) fn register_operation<I, O, E, F, Fut, C>(
        &mut self,
        metadata: OperationRegistration,
        policy: OperationPolicy,
        binding: F,
        classify: C,
    ) -> Result<(), RegistryBuildError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + Send + 'static,
        E: Send + 'static,
        F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, E>> + Send + 'static,
        C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
    {
        self.register_validated_operation(metadata, policy, binding, classify, |_, _: &I| Ok(()))
    }

    /// Operation-owned pure typed validation runs after exact schema decoding,
    /// before either plan minting or provider dispatch.
    pub(crate) fn register_validated_operation<I, O, E, F, Fut, C, V>(
        &mut self,
        metadata: OperationRegistration,
        policy: OperationPolicy,
        binding: F,
        classify: C,
        validate: V,
    ) -> Result<(), RegistryBuildError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + Send + 'static,
        E: Send + 'static,
        F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, E>> + Send + 'static,
        C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
        V: Fn(&BindingContext, &I) -> Result<(), ()> + Send + Sync + 'static,
    {
        self.register_operation_contract(metadata, policy, binding, classify, validate, None)
    }

    /// An open-world read that a confirmed plan scope may authorize. `compile`
    /// states which arguments the approval token binds; the owner's page is
    /// returned as the owner built it, and nothing is recorded under the plan.
    pub(crate) fn register_search_operation<I, O, E, F, Fut, C, V>(
        &mut self,
        metadata: OperationRegistration,
        policy: OperationPolicy,
        binding: F,
        classify: C,
        validate: V,
        compile: execution::SearchScopeCompiler<I>,
    ) -> Result<(), RegistryBuildError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + Send + 'static,
        E: Send + 'static,
        F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, E>> + Send + 'static,
        C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
        V: Fn(&BindingContext, &I) -> Result<(), ()> + Send + Sync + 'static,
    {
        // A confirmed search plan authorizes reads only; no plan scope can
        // stand in for a write confirmation.
        if !matches!(
            policy.effect,
            EffectClass::ReadOpenWorld | EffectClass::SyncOpenWorld
        ) {
            return Err(RegistryBuildError::SearchOperationNotReadOnly(
                EntryId::parse(metadata.id.as_str()).expect("catalog ID"),
            ));
        }
        self.register_operation_contract(
            metadata,
            policy,
            binding,
            classify,
            validate,
            Some(compile),
        )
    }

    fn register_operation_contract<I, O, E, F, Fut, C, V>(
        &mut self,
        metadata: OperationRegistration,
        policy: OperationPolicy,
        binding: F,
        classify: C,
        validate: V,
        compile: Option<execution::SearchScopeCompiler<I>>,
    ) -> Result<(), RegistryBuildError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + Send + 'static,
        E: Send + 'static,
        F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, E>> + Send + 'static,
        C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
        V: Fn(&BindingContext, &I) -> Result<(), ()> + Send + Sync + 'static,
    {
        let input = CompiledSchema::for_input::<I>().map_err(RegistryBuildError::Schema)?;
        let output = CompiledSchema::for_output::<O>().map_err(RegistryBuildError::Schema)?;
        let binding =
            execution::typed_binding(binding, classify, validate, compile, policy.execution);
        self.register_bound_operation(metadata, policy, input, output, binding)
    }

    /// Register the eventual completion contract of an immutable owner review.
    /// The preparation binding itself returns sealed authority, never a wire payload.
    pub(crate) fn register_owner_review_operation<I, O, E, F, Fut, C>(
        &mut self,
        metadata: OperationRegistration,
        policy: OperationPolicy,
        binding: F,
        classify: C,
    ) -> Result<(), RegistryBuildError>
    where
        I: DeserializeOwned + JsonSchema + Send + 'static,
        O: Serialize + JsonSchema + Send + 'static,
        E: Send + 'static,
        F: Fn(BindingContext, I) -> Fut + Send + Sync + 'static,
        Fut:
            Future<Output = Result<onecrew_local_runtime::PreparedOwnerReview, E>> + Send + 'static,
        C: Fn(E) -> ClassifiedOwnerFailure + Send + Sync + 'static,
    {
        let input = CompiledSchema::for_input::<I>().map_err(RegistryBuildError::Schema)?;
        let output = CompiledSchema::for_output::<O>().map_err(RegistryBuildError::Schema)?;
        let binding = execution::typed_owner_review_binding(binding, classify, policy.execution);
        self.register_bound_operation(metadata, policy, input, output, binding)
    }

    fn register_bound_operation(
        &mut self,
        metadata: OperationRegistration,
        policy: OperationPolicy,
        input: CompiledSchema,
        output: CompiledSchema,
        binding: ErasedBinding,
    ) -> Result<(), RegistryBuildError> {
        let metadata = self.project_operation(metadata)?;
        let unique_paths = policy.sensitive_input_paths.iter().collect::<BTreeSet<_>>();
        let secret_paths_valid = match policy.secret {
            crate::operation_registry::descriptor::SecretClass::None => {
                policy.sensitive_input_paths.is_empty()
            }
            crate::operation_registry::descriptor::SecretClass::Transient
            | crate::operation_registry::descriptor::SecretClass::VaultBound => {
                !policy.sensitive_input_paths.is_empty()
                    && unique_paths.len() == policy.sensitive_input_paths.len()
                    && policy
                        .sensitive_input_paths
                        .iter()
                        .all(|path| path.len() <= 256 && input.validates_instance_pointer(path))
            }
        };
        if !secret_paths_valid {
            return Err(RegistryBuildError::InvalidSecretPolicy);
        }
        if policy.secret != SecretClass::None && policy.view_kind != OperationViewKind::ReadOnly {
            return Err(RegistryBuildError::InvalidViewKind);
        }
        match (policy.approval, policy.approval_request.is_some()) {
            (ApprovalClass::AttendedPrompt, false)
            | (ApprovalClass::None | ApprovalClass::OwnerGuarded, true) => {
                return Err(RegistryBuildError::InvalidApprovalPolicy);
            }
            _ => {}
        }
        validate_operation_safety(&policy, &input, &output)?;
        if policy.execution != binding.execution()
            || (matches!(
                policy.execution,
                execution::ExecutionDeclaration::IndependentRead
            ) && (!matches!(
                policy.effect,
                crate::operation_registry::descriptor::EffectClass::ReadLocal
                    | crate::operation_registry::descriptor::EffectClass::ReadOpenWorld
            ) || policy.secret
                != crate::operation_registry::descriptor::SecretClass::None
                || policy.approval != crate::operation_registry::descriptor::ApprovalClass::None
                || policy.approval_request.is_some()
                || !matches!(
                    policy.view_kind,
                    OperationViewKind::SimpleOperation | OperationViewKind::ReadOnly
                )
                || binding.output_class() != execution::BindingOutputClass::Payload))
        {
            return Err(RegistryBuildError::InvalidExecutionDeclaration);
        }
        self.insert_operation_parts(metadata, policy, Some(input), Some(output), Some(binding))
    }

    fn insert_operation_parts(
        &mut self,
        metadata: EntryMetadata,
        policy: OperationPolicy,
        input: Option<CompiledSchema>,
        output: Option<CompiledSchema>,
        binding: Option<ErasedBinding>,
    ) -> Result<(), RegistryBuildError> {
        let entry = OperationEntry {
            metadata,
            effect: policy.effect,
            secret: policy.secret,
            approval: policy.approval,
            connection: policy.connection,
            sensitive_input_paths: policy.sensitive_input_paths,
            view_kind: policy.view_kind,
            input: input.ok_or(RegistryBuildError::MissingInputSchema)?,
            output: output.ok_or(RegistryBuildError::MissingOutputSchema)?,
            binding: binding.ok_or(RegistryBuildError::MissingBinding)?,
            approval_request: policy.approval_request,
            safety: policy.safety,
        };
        self.insert(RegistryEntry::Operation(Box::new(entry)))
    }

    #[cfg(test)]
    pub(in crate::operation_registry) fn register_incomplete_for_test(
        &mut self,
        metadata: OperationRegistration,
        missing_input: bool,
        missing_output: bool,
        missing_binding: bool,
    ) -> Result<(), RegistryBuildError> {
        let binding = execution::typed_binding(
            |_context, _input: StrictTestInput| async {
                Ok::<_, StrictTestError>(StrictTestOutput {})
            },
            |_error| strict_test_failure(),
            |_, _| Ok(()),
            None,
            execution::ExecutionDeclaration::Sequential,
        );
        let metadata = self.project_operation(metadata)?;
        self.insert_operation_parts(
            metadata,
            OperationPolicy::read_only(
                crate::operation_registry::descriptor::EffectClass::ReadLocal,
                crate::operation_registry::descriptor::ConnectionRequirement::None,
                execution::ExecutionDeclaration::Sequential,
            ),
            (!missing_input)
                .then(CompiledSchema::for_input::<StrictTestInput>)
                .transpose()
                .map_err(RegistryBuildError::Schema)?,
            (!missing_output)
                .then(CompiledSchema::for_output::<StrictTestOutput>)
                .transpose()
                .map_err(RegistryBuildError::Schema)?,
            (!missing_binding).then_some(binding),
        )
    }

    pub(crate) fn register_playbook(
        &mut self,
        entry: PlaybookEntry,
    ) -> Result<(), RegistryBuildError> {
        self.insert(RegistryEntry::Playbook(entry))
    }

    pub(crate) fn register_toolkit_guide(
        &mut self,
        mut entry: ToolkitGuideEntry,
    ) -> Result<(), RegistryBuildError> {
        entry.metadata.surfaces =
            crate::operation_registry::descriptor::SurfaceSet::new(self.entries.values().flat_map(
                |registered| match registered {
                    RegistryEntry::Operation(operation)
                        if operation.metadata.toolkit == entry.metadata.toolkit =>
                    {
                        operation.metadata.surfaces.iter().collect::<Vec<_>>()
                    }
                    _ => Vec::new(),
                },
            ))
            .map_err(|_| {
                RegistryBuildError::ToolkitGuideWithoutOperations(entry.metadata.id.clone())
            })?;
        self.insert(RegistryEntry::ToolkitGuide(entry))
    }

    fn insert(&mut self, entry: RegistryEntry) -> Result<(), RegistryBuildError> {
        let id = entry.metadata().id.clone();
        if matches!(entry, RegistryEntry::Operation(_))
            && id.as_str().split('.').next() != Some(entry.metadata().toolkit.as_str())
        {
            return Err(RegistryBuildError::OperationToolkitMismatch(id));
        }
        if self.entries.contains_key(&id) {
            return Err(RegistryBuildError::DuplicateId(id));
        }
        let normalized_id = normalize_alias(id.as_str());
        if self.aliases.contains_key(&normalized_id) {
            return Err(RegistryBuildError::DuplicateAlias(normalized_id));
        }
        let mut pending_aliases = BTreeMap::new();
        for alias in &entry.metadata().aliases {
            let normalized = normalize_alias(alias);
            let collides = normalized.is_empty()
                || self.aliases.contains_key(&normalized)
                || self
                    .entries
                    .keys()
                    .any(|existing| normalize_alias(existing.as_str()) == normalized)
                || pending_aliases.contains_key(&normalized);
            if collides {
                return Err(RegistryBuildError::DuplicateAlias(normalized));
            }
            pending_aliases.insert(normalized, id.clone());
        }
        self.aliases.extend(pending_aliases);
        self.entries.insert(id, entry);
        Ok(())
    }

    pub(crate) fn build(self) -> Result<OperationRegistry, RegistryBuildError> {
        for entry in self.entries.values() {
            if let RegistryEntry::Operation(operation) = entry {
                validate_readback_references(&self.entries, operation)?;
            }
            if let RegistryEntry::Playbook(playbook) = entry {
                validate_playbook(&self.entries, playbook)?;
            }
            if let RegistryEntry::ToolkitGuide(guide) = entry {
                validate_toolkit_guide(guide)?;
            }
        }
        for expected in self
            .catalog
            .operations()
            .filter(|operation| operation.surfaces.contains(&CatalogSurface::McpBinding))
        {
            let id = EntryId::parse(expected.id.as_str())
                .map_err(|_| RegistryBuildError::InvalidCatalogReference)?;
            if !matches!(self.entries.get(&id), Some(RegistryEntry::Operation(_))) {
                return Err(RegistryBuildError::MissingCatalogBinding(id));
            }
        }
        OperationRegistry::from_entries(self.entries, self.catalog)
    }
}
fn normalize_alias(value: &str) -> String {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}
#[cfg(test)]
#[derive(serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StrictTestInput {}

#[cfg(test)]
#[derive(serde::Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StrictTestOutput {}

#[cfg(test)]
struct StrictTestError;

#[cfg(test)]
fn strict_test_failure() -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(
        crate::operation_registry::execution::PublicFailureCode::parse("test_rejected").unwrap(),
        false,
        None,
    )
}
