//! Registry construction, contracts, execution, and owner-adapter invariants.

use std::{
    collections::BTreeSet,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, ConfirmationSafety, ConnectionRef,
        ConnectionRequirement, EffectClass, EntryId, EntryMetadata, IdempotencyMode,
        IdempotencySafety, InstancePointer, OperationSafety, OperatorToolkitId, PlaybookDifficulty,
        PlaybookEntry, PlaybookStep, ReadbackMode, ReadbackSafety, SearchText, SecretClass,
        ServiceToolkitId, SurfaceId, SurfaceSet,
    },
    execution::{
        ClassifiedOwnerFailure, ExecuteEffectState, ExecuteFailureKind, ExecuteFailurePhase,
        ExecutionContext, OwnerEffectState, PublicFailureCode, RecoveryDirective,
        dispatch_prepared, prepare_invocation, project_execute_failure,
    },
    registration::{OperationPolicy, ParameterPointerRole, RegistryBuildError, RegistryBuilder},
    schema::CompiledSchema,
    search::SearchQuery,
};

/// HOST-01 / HOST-03 source contract for the MCP host: an operation handler
/// body only awaits, every synchronous owner call reaches the blocking pool
/// through the one `run_blocking` wrapper, and no runtime thread ever calls
/// `block_on` or `block_in_place`.
mod blocking_dispatch_contract;
mod connection_projection;
mod discovery;
mod execution;
mod onecrew_reads;
mod platform_reads;
mod production_inventory;
mod registration;
mod safety;
mod schema_contract;
mod support;

use support::*;
