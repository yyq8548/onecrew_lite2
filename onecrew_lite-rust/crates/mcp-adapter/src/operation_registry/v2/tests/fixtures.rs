use onecrew_local_runtime::ToolkitId;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    accounts::PlatformRuntimeOperatorAccountWorkflow,
    operation_registry::{
        RegistryOwners, build_registry,
        descriptor::{
            ApprovalClass, AuditMode, ConfirmationMode, ConfirmationSafety, ConnectionRequirement,
            EffectClass, EntryId, IdempotencyMode, IdempotencySafety, OperationRegistration,
            OperationSafety, ReadbackMode, ReadbackSafety, RegistryEntry, SecretClass, SurfaceId,
            SurfaceSet,
        },
        execution::ExecutionDeclaration,
        registration::{OperationPolicy, OperationRegistry},
        v2::contract::{
            ToolExecutionAuthority, ToolExecutionCall, ToolSearchInput, ToolSearchLimit,
        },
    },
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TestInput {
    pub(super) value: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TestOutput {
    pub(super) value: String,
}

#[derive(Clone, Copy)]
pub(super) struct TestOwnerError;

pub(super) fn metadata(id: &str) -> OperationRegistration {
    OperationRegistration {
        id: crate::operation_registry::catalog_test_support::operation_id(id),
        domain_id: "test".to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new([SurfaceId::Full]).expect("non-empty surfaces"),
    }
}

pub(super) fn policy(approval: ApprovalClass, effect: EffectClass) -> OperationPolicy {
    if approval == ApprovalClass::None && !effect.may_have_effect() {
        return OperationPolicy::read_only(
            effect,
            ConnectionRequirement::None,
            ExecutionDeclaration::Sequential,
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
        execution: ExecutionDeclaration::Sequential,
    }
}

pub(super) fn revision(registry: &OperationRegistry, operation_id: &str) -> String {
    let id = EntryId::parse(operation_id).unwrap();
    match registry.entry(&id, SurfaceId::Full).unwrap() {
        RegistryEntry::Operation(operation) => operation.input.revision().to_owned(),
        RegistryEntry::Playbook(_) | RegistryEntry::ToolkitGuide(_) => {
            panic!("test entry must be executable")
        }
    }
}

fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().expect("test arguments object").clone()
}

pub(super) fn call(
    call_id: &str,
    operation_id: &str,
    schema_revision: String,
    arguments: Value,
) -> ToolExecutionCall {
    ToolExecutionCall {
        call_id: EntryId::parse(call_id).unwrap(),
        operation_id: EntryId::parse(operation_id).unwrap(),
        schema_revision,
        authority: ToolExecutionAuthority::Direct {},
        connection: None,
        arguments: self::arguments(arguments),
    }
}

pub(super) fn id(value: &str) -> EntryId {
    EntryId::parse(value).expect("test ID")
}

pub(super) fn schema_value<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).expect("serialize schema")
}

pub(super) fn production_registry() -> OperationRegistry {
    let directory = tempfile::tempdir().expect("temporary registry directory");
    let platform = Arc::new(
        onecrew_local_runtime::LocalPlatform::open(directory.path()).expect("open local platform"),
    );
    let operator_accounts = Arc::new(PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(
        &platform,
    )));
    build_registry(
        RegistryOwners {
            creators: platform.creators().expect("OneCrew owner"),
            platform,
            operator_accounts,
        },
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .expect("build production registry")
}

pub(super) fn result_owner() -> (
    tempfile::TempDir,
    Arc<onecrew_local_runtime::LocalPlatform>,
    onecrew_local_runtime::ResultOrigin,
) {
    let directory = tempfile::tempdir().expect("temporary result owner directory");
    let platform =
        onecrew_local_runtime::LocalPlatform::open(directory.path()).expect("open local platform");
    let origin =
        onecrew_local_runtime::ResultOrigin::new("mcp:full").expect("strict result origin");
    (directory, Arc::new(platform), origin)
}

pub(super) fn search_input(query: &str, toolkits: Vec<ToolkitId>) -> ToolSearchInput {
    ToolSearchInput {
        query: crate::operation_registry::descriptor::SearchText::parse(query)
            .expect("normalized search text"),
        toolkits,
        effects: Vec::new(),
        limit: Some(ToolSearchLimit(8)),
    }
}
