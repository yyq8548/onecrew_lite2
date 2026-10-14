//! Submitted terminalization, confirmation dispatch, and continuation invariants.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use onecrew_local_runtime::{
    ClaimResultContinuationInput, CompleteResultContinuationInput, RequestResultEditInput,
    ResultContinuationClaimMode, ResultContinuationCompletionOutcome, ResultContinuationContext,
    ResultContinuationState, ResultFingerprint, ResultOrigin, ResultState, ResultStoreError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::fixtures::{
    TestInput, TestOutput, TestOwnerError, call, metadata, policy, result_owner, revision,
};
use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{ApprovalClass, ConnectionRequirement, EffectClass, SurfaceId},
    execution::{ClassifiedOwnerFailure, PublicFailureCode},
    registration::{OperationPolicy, OperationRegistry, RegistryBuilder},
    v2::{
        contract::{
            PresentationRequest, ResultPresentInput, ResultPresentStatus, ResultStateFailure,
            ResultStateInput, ResultStateOutput, ToolExecuteInput,
        },
        state::{SubmittedTerminalGuard, map_store_failure, result_present_v2},
    },
};

mod confirmation;
mod continuation;
mod support;
mod terminalization;

use support::*;
