use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ContactChannel, CreatorId, CreatorImportOperationId, LocatorKind, SourceKind};

pub const CREATOR_BUNDLE_FORMAT: &str = "reacher.creator_bundle";
pub const CREATOR_BUNDLE_FORMAT_VERSION: u16 = 4;
pub const CREATOR_BUNDLE_MAX_CREATORS: usize = 10_000;
pub const CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR: usize = 64;
pub const CREATOR_BUNDLE_MAX_BYTES: usize = 32 * 1024 * 1024;
pub const CREATOR_BUNDLE_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const CREATOR_BUNDLE_XLSX_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorTransferFileFormat {
    Xlsx,
}

impl CreatorTransferFileFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
        }
    }
}

macro_rules! transfer_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
            JsonSchema,
        )]
        pub enum $name {
            $(#[serde(rename = $value)] $variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }
    };
}

transfer_enum!(CreatorTransferPlatform {
    X => "x",
    Instagram => "instagram",
    Telegram => "telegram",
    YouTube => "youtube",
    TikTok => "tiktok",
});

transfer_enum!(CreatorTransferStage {
    Prospect => "prospect",
    Engaged => "engaged",
    Collaborating => "collaborating",
    Archived => "archived",
});

transfer_enum!(CreatorTransferTargetKind {
    User => "user",
    Bot => "bot",
    Channel => "channel",
    Supergroup => "supergroup",
    Group => "group",
    Creator => "creator",
    Business => "business",
});

transfer_enum!(CreatorTransferLinkState {
    Claimed => "claimed",
    Confirmed => "confirmed",
    Rejected => "rejected",
});

transfer_enum!(CreatorTransferConfidence {
    Exact => "exact",
    Asserted => "asserted",
    Possible => "possible",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorBundleProfile {
    PortablePublic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorBundleSnapshotKind {
    Full,
}

/// Strict current-state Creator-list backup. Group, lifecycle/detail,
/// communication, session, audit, and secret domains never enter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorBundle {
    pub format: String,
    pub format_version: u16,
    pub profile: CreatorBundleProfile,
    pub snapshot_kind: CreatorBundleSnapshotKind,
    pub creator_count: u32,
    /// SHA-256 over the language-neutral canonical digest array documented by
    /// the Creator transfer plan. The public JSON object and this field are not
    /// themselves hashed.
    pub content_sha256: String,
    pub creators: Vec<CreatorTransferCreator>,
}

/// Local-only presentation values used by the human workbook. They are never
/// part of canonical JSON, semantic identity, import writes, or sync wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatorWorkbookDisplay {
    pub last_touch_at: Option<u64>,
    pub updated_at: u64,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatorWorkbookExport {
    pub bundle: CreatorBundle,
    pub display_by_record_key: BTreeMap<String, CreatorWorkbookDisplay>,
}

/// Completed XLSX artifact. This process-local value is not a persisted model.
#[derive(Debug)]
pub struct CreatorWorkbookXlsx {
    pub bytes: Vec<u8>,
    pub creator_count: u32,
    pub content_sha256: String,
    pub artifact_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorTransferCreator {
    /// SHA-256 over the timestamp-free canonical facts for this Creator.
    pub semantic_sha256: String,
    /// Bundle-local selector derived from the complete public page projection;
    /// never a source/receiver UUID or a cross-device entity identity.
    pub record_key: String,
    pub display_name: String,
    pub tags: Vec<String>,
    /// Public import source details, never the user's private Creator notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[schemars(range(max = 100))]
    pub score: Option<u8>,
    pub follower_counts: BTreeMap<CreatorTransferPlatform, u64>,
    pub pinned: bool,
    pub relationship: CreatorTransferRelationship,
    pub accounts: Vec<CreatorTransferAccount>,
    pub contact_routes: Vec<CreatorTransferContactRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorTransferRelationship {
    pub stage: CreatorTransferStage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorTransferAccount {
    /// Stable first-capture ordering used by Creator list projections.
    pub capture_order: u32,
    pub platform: CreatorTransferPlatform,
    pub target_kind: CreatorTransferTargetKind,
    pub external_id: String,
    pub link_state: CreatorTransferLinkState,
    pub confidence: CreatorTransferConfidence,
    pub source_kind: SourceKind,
    pub latest_profile: Option<CreatorTransferProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorTransferProfile {
    #[schemars(required)]
    pub handle: Option<String>,
    #[schemars(required)]
    pub display_name: Option<String>,
    #[schemars(required)]
    pub profile_url: Option<String>,
    #[schemars(required)]
    pub avatar_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct RequiredNullable<T>(Option<T>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreatorTransferProfileWire {
    handle: RequiredNullable<String>,
    display_name: RequiredNullable<String>,
    profile_url: RequiredNullable<String>,
    avatar_url: RequiredNullable<String>,
}

impl<'de> Deserialize<'de> for CreatorTransferProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = CreatorTransferProfileWire::deserialize(deserializer)?;
        Ok(Self {
            handle: wire.handle.0,
            display_name: wire.display_name.0,
            profile_url: wire.profile_url.0,
            avatar_url: wire.avatar_url.0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorTransferContactRoute {
    /// Stable first-save ordering used by Creator list channel projections.
    pub save_order: u32,
    pub channel: ContactChannel,
    pub locator_kind: LocatorKind,
    pub normalized_value: String,
    pub display_value: String,
    pub confidence: CreatorTransferConfidence,
    pub source_kinds: Vec<SourceKind>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorImportImpact {
    pub creator_inputs: u64,
    pub creators_created: u64,
    pub creators_merged_by_public_identity: u64,
    pub creator_scores_filled: u64,
    pub follower_counts_filled: u64,
    pub platform_accounts_created: u64,
    pub claimed_links_created: u64,
    pub confirmed_links_created: u64,
    pub rejected_links_created: u64,
    pub links_promoted_to_confirmed: u64,
    pub profile_observations_created: u64,
    pub contact_routes_created: u64,
    pub creators_unchanged: u64,
    pub creator_tags_filled: u64,
}

impl CreatorImportImpact {
    pub fn changes_creator_graph(&self) -> bool {
        [
            self.creators_created,
            self.creator_scores_filled,
            self.creator_tags_filled,
            self.follower_counts_filled,
            self.platform_accounts_created,
            self.claimed_links_created,
            self.confirmed_links_created,
            self.rejected_links_created,
            self.links_promoted_to_confirmed,
            self.profile_observations_created,
            self.contact_routes_created,
        ]
        .into_iter()
        .any(|count| count > 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorImportConflictKind {
    CreatorTagsConflict,
    IncomingAccountClaimedByMultipleCreators,
    AccountOwnedByAnotherCreator,
    PossibleHandleMatch,
    RejectedLinkWouldBeRevived,
    IncomingRejectedLinkConflictsWithActiveLocalLink,
    ContactRouteOwnedByAnotherCreator,
    PossibleContactMatch,
    IncomingCreatorResolvesToMultipleLocalCreators,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorImportAccountKey {
    pub platform: CreatorTransferPlatform,
    pub target_kind: CreatorTransferTargetKind,
    pub external_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorImportConflict {
    pub kind: CreatorImportConflictKind,
    pub incoming_creator_key: String,
    pub account: Option<CreatorImportAccountKey>,
    pub local_creator_ids: Vec<CreatorId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorImportReceipt {
    pub content_sha256: String,
    pub format_version: u16,
    pub creator_count: u32,
    pub impact: CreatorImportImpact,
    pub imported_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrepareCreatorImportResult {
    AlreadyImported {
        receipt: CreatorImportReceipt,
    },
    Blocked {
        content_sha256: String,
        conflict_count: u64,
        conflicts: Vec<CreatorImportConflict>,
    },
    ConfirmationRequired {
        operation_id: CreatorImportOperationId,
        content_sha256: String,
        impact: CreatorImportImpact,
        expires_at: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmCreatorImportInput {
    pub operation_id: CreatorImportOperationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfirmCreatorImportResult {
    Imported {
        operation_id: CreatorImportOperationId,
        receipt: CreatorImportReceipt,
    },
    Unchanged {
        operation_id: CreatorImportOperationId,
        receipt: CreatorImportReceipt,
    },
    Stale {
        operation_id: CreatorImportOperationId,
        next_action: String,
    },
    Expired {
        operation_id: CreatorImportOperationId,
        next_action: String,
    },
    Cancelled {
        operation_id: CreatorImportOperationId,
    },
}
