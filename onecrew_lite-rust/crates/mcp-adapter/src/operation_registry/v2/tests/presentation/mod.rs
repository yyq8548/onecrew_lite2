//! Result projection completeness and presentation request policy.

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use onecrew_local_runtime::{ResultState, ResultStoreError, ResultViewKind};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::json;

use super::fixtures::{
    TestInput, TestOutput, TestOwnerError, call, metadata, policy, production_registry,
    result_owner, revision,
};

use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, ConnectionRequirement, EffectClass, OperationViewKind, SecretClass,
        SurfaceId,
    },
    execution::{ClassifiedOwnerFailure, PublicFailureCode},
    registration::{OperationPolicy, RegistryBuilder},
    v2::{
        contract::{
            PresentationRequest, ToolExecuteInput, ToolExecutionOutcome, ToolPresentationFailure,
        },
        execute::ToolExecuteOrchestrationFailure,
        resume::{ResumeEnvelope, decode_resume_envelope},
    },
};

mod projection;
mod requests;
