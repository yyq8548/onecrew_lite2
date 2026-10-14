use super::*;
use crate::operation_registry::descriptor::OperationRegistration;
use onecrew_local_runtime::ToolkitId;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields are consumed by Deserialize and JsonSchema in registry contract tests"
)]
pub(super) struct StrictInput {
    name: String,
    action: TestAction,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "payloads are consumed by Deserialize and JsonSchema in registry contract tests"
)]
enum TestAction {
    Read(ReadAction),
    Write(WriteAction),
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadAction {}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "the field is consumed by Deserialize and JsonSchema in registry contract tests"
)]
struct WriteAction {
    count: u8,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct StrictOutput {
    pub(super) accepted: bool,
}

pub(super) fn metadata(
    id: &str,
    aliases: &[&str],
    surfaces: &[SurfaceId],
) -> OperationRegistration {
    OperationRegistration {
        id: crate::operation_registry::catalog_test_support::operation_id(id),
        domain_id: "test".to_owned(),
        aliases: aliases.iter().map(|value| (*value).to_owned()).collect(),
        surfaces: SurfaceSet::new(surfaces.iter().copied()).expect("surface set is non-empty"),
    }
}

pub(super) fn policy(approval: ApprovalClass, effect: EffectClass) -> OperationPolicy {
    if approval == ApprovalClass::None && !effect.may_have_effect() {
        return OperationPolicy::read_only(
            effect,
            ConnectionRequirement::None,
            crate::operation_registry::execution::ExecutionDeclaration::Sequential,
        );
    }
    OperationPolicy {
        effect,
        secret: SecretClass::None,
        approval,
        connection: ConnectionRequirement::None,
        sensitive_input_paths: Vec::new(),
        view_kind: crate::operation_registry::descriptor::OperationViewKind::SimpleOperation,
        approval_request: None,
        safety: OperationSafety {
            confirmation: ConfirmationSafety {
                mode: if approval == ApprovalClass::AttendedPrompt {
                    ConfirmationMode::AttendedPrompt
                } else {
                    ConfirmationMode::None
                },
                parameter_pointers: Vec::new(),
            },
            idempotency: IdempotencySafety {
                mode: IdempotencyMode::None,
                identity_pointers: Vec::new(),
            },
            readback: ReadbackSafety {
                mode: ReadbackMode::None,
                operation_ids: Vec::new(),
            },
            audit: AuditMode::None,
            warnings_present: false,
            recovery_present: false,
            warning_output_pointers: Vec::new(),
            recovery_output_pointers: Vec::new(),
        },
        execution: crate::operation_registry::execution::ExecutionDeclaration::Sequential,
    }
}

pub(super) fn playbook_entry(
    id: &str,
    surfaces: &[SurfaceId],
    operation_id: &str,
) -> PlaybookEntry {
    PlaybookEntry {
        metadata: EntryMetadata {
            id: EntryId::parse(id).unwrap(),
            title: "Test playbook".to_owned(),
            summary: "A bounded test playbook.".to_owned(),
            toolkit: ToolkitId::OneCrew,
            domain_id: "test".to_owned(),
            aliases: Vec::new(),
            surfaces: SurfaceSet::new(surfaces.iter().copied()).unwrap(),
        },
        use_case: "Exercise a registered operation safely.".to_owned(),
        toolkit_ids: vec![ToolkitId::OneCrew],
        primary_operation_ids: vec![EntryId::parse(operation_id).unwrap()],
        supporting_operation_ids: Vec::new(),
        recommended_steps: vec![
            PlaybookStep {
                instruction: "Inspect the input.".to_owned(),
                operation_ids: Vec::new(),
            },
            PlaybookStep {
                instruction: "Execute the operation.".to_owned(),
                operation_ids: vec![EntryId::parse(operation_id).unwrap()],
            },
        ],
        known_pitfalls: vec!["Do not guess identifiers.".to_owned()],
        difficulty: PlaybookDifficulty::Basic,
    }
}

#[derive(Clone, Copy)]
pub(super) struct TestOwnerError;

pub(super) fn classify_test_error(_error: TestOwnerError) -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse("test_rejected").unwrap(),
        false,
        None,
    )
}

pub(super) fn add_operation(
    builder: &mut RegistryBuilder,
    id: &str,
    aliases: &[&str],
    surfaces: &[SurfaceId],
) -> Result<(), RegistryBuildError> {
    builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
        metadata(id, aliases, surfaces),
        policy(ApprovalClass::None, EffectClass::ReadLocal),
        |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
        classify_test_error,
    )
}

pub(super) fn run<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

pub(super) fn production_owners(
    platform: Arc<onecrew_local_runtime::LocalPlatform>,
) -> crate::operation_registry::RegistryOwners {
    let operator_accounts = Arc::new(
        crate::accounts::PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
    );
    crate::operation_registry::RegistryOwners {
        creators: platform.creators().unwrap(),
        platform,
        operator_accounts,
    }
}

pub(super) fn registry_operation<'a>(
    registry: &'a crate::operation_registry::OperationRegistry,
    id: &str,
) -> &'a crate::operation_registry::descriptor::OperationEntry {
    let id = EntryId::parse(id).unwrap();
    match registry.entry(&id, SurfaceId::Full).unwrap() {
        crate::operation_registry::descriptor::RegistryEntry::Operation(operation) => operation,
        crate::operation_registry::descriptor::RegistryEntry::Playbook(_)
        | crate::operation_registry::descriptor::RegistryEntry::ToolkitGuide(_) => {
            panic!("{id:?} is not executable")
        }
    }
}

/// Every registered operation whose binding accepts a search-plan scope, read
/// from the registry itself so a new plan-governed operation joins each guard
/// that iterates this set.
pub(super) fn plan_governed_operations(
    registry: &crate::operation_registry::OperationRegistry,
) -> Vec<(
    EntryId,
    &crate::operation_registry::descriptor::OperationEntry,
)> {
    registry
        .operation_ids_and_surfaces()
        .into_iter()
        .filter_map(|(id, surfaces)| {
            let surface = *surfaces.first().expect("an operation has a surface");
            let Some(crate::operation_registry::descriptor::RegistryEntry::Operation(operation)) =
                registry.entry(&id, surface)
            else {
                panic!("{id} must be executable on {surface:?}");
            };
            operation
                .binding
                .supports_search_plan()
                .then_some((id, operation.as_ref()))
        })
        .collect()
}

pub(super) fn execute_registered(
    registry: &crate::operation_registry::OperationRegistry,
    id: &str,
    connection: &ConnectionRef,
    arguments: Value,
) -> Result<Value, crate::operation_registry::execution::ExecuteFailure> {
    execute_current_operation(
        registry_operation(registry, id),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: format!("request-{id}"),
            connection: Some(connection),
            confirmed: false,
        },
        arguments,
    )
}

pub(super) fn execute_local(
    registry: &crate::operation_registry::OperationRegistry,
    id: &str,
    arguments: Value,
) -> Result<Value, crate::operation_registry::execution::ExecuteFailure> {
    execute_current_operation(
        registry_operation(registry, id),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: format!("request-{id}"),
            connection: None,
            confirmed: false,
        },
        arguments,
    )
}

pub(super) fn execute_current_operation(
    operation: &crate::operation_registry::descriptor::OperationEntry,
    context: ExecutionContext<'_>,
    arguments: Value,
) -> Result<Value, crate::operation_registry::execution::ExecuteFailure> {
    let revision = operation.input.revision().to_owned();
    let prepared = prepare_invocation(operation, context, &revision, arguments)?;
    run(dispatch_prepared(prepared))
}
