//! Public contact evidence and confirmed locator projections.
use crate::{Confidence, ContactChannel, ContactLocatorInput, CreatorId, LocatorKind, SourceKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One public contact observation classified by Creator's source-aware
/// extractor from the facts supplied by a platform workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlatformContactObservation {
    pub channel: ContactChannel,
    pub kind: LocatorKind,
    pub value: String,
    pub source: SourceKind,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscoveredContactRoute {
    pub channel: ContactChannel,
    pub kind: LocatorKind,
    pub value: String,
    pub source: SourceKind,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ContactRouteCollision {
    None,
    Possible { creator_ids: Vec<CreatorId> },
    Conflict { creator_ids: Vec<CreatorId> },
}

impl ContactRouteCollision {
    pub fn creator_ids(&self) -> &[CreatorId] {
        match self {
            Self::None => &[],
            Self::Possible { creator_ids } | Self::Conflict { creator_ids } => creator_ids,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContactRoutePreview {
    pub locator: ContactLocatorInput,
    pub observations: Vec<DiscoveredContactRoute>,
    pub collision: ContactRouteCollision,
}

/// Internal confirmation payload built by the runtime from server-held
/// canonical contact observations; never a public adapter input.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactRouteConfirmation {
    pub preview: ContactRoutePreview,
    pub confidence: Confidence,
}
/// Public evidence already obtained by the platform workflow. Creator owns
/// extraction policy; preparation never fetches a profile or a linked page.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorSaveContactEvidence {
    Observation {
        channel: ContactChannel,
        #[serde(rename = "locator_kind")]
        kind: LocatorKind,
        value: String,
        source: SourceKind,
        source_url: Option<String>,
    },
    ProfileField {
        source: SourceKind,
        /// The actual public source, when known; no profile-URL substitution.
        source_url: Option<String>,
        text: String,
    },
    ProfileLink {
        source: SourceKind,
        /// The page publishing this link, independently of its destination.
        source_url: Option<String>,
        display_text: String,
        target_url: String,
    },
    HeldHub {
        source_url: String,
        text: String,
        links: Vec<CreatorSavePublicLink>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSavePublicLink {
    pub display_text: String,
    pub target_url: String,
}
