use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{
    AccountRefId, ContactChannel, LocatorKind, ObservationSourceId, Platform, PlatformAccountId,
    SourceKind, TargetKind,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpsertPlatformAccountInput {
    pub platform: Platform,
    pub target_kind: TargetKind,
    pub external_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlatformAccountObservationInput {
    pub platform_account_id: PlatformAccountId,
    pub source_id: ObservationSourceId,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub profile_url: Option<String>,
    #[serde(default)]
    pub public_data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObservationSourceInput {
    pub kind: SourceKind,
    pub platform: Option<Platform>,
    pub source_url: Option<String>,
    pub external_ref: Option<String>,
    pub account_ref_id: Option<AccountRefId>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContactLocatorInput {
    pub channel: ContactChannel,
    pub kind: LocatorKind,
    pub normalized_value: String,
    pub display_value: String,
}
