//! Retrieval recall, intent boundaries, and search response contracts.

use std::sync::Arc;

use serde::Serialize;
use serde_json::json;

use super::{SearchQuery, operation_eval_cases::OPERATION_EVAL_CASES};
use crate::{
    accounts::PlatformRuntimeOperatorAccountWorkflow,
    operation_registry::{
        OperationRegistry, RegistryOwners, build_registry,
        descriptor::{
            CatalogEntrySummary, CatalogMatchRole, EffectClass, PlaybookSummary, SearchText,
            SurfaceId,
        },
        directory::ToolkitDirectory,
        v2::ToolSearchInput,
    },
};

mod contracts;
mod intent_boundaries;
mod recall;
mod support;

use support::*;
