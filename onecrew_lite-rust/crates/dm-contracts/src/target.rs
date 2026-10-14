use serde::{Deserialize, Serialize};

/// Kinds of account a direct message can be addressed to.
///
/// There is no shared table of which platform accepts which kind: every
/// provider decides that for itself, immediately before it acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmTargetKind {
    User,
    Bot,
    Channel,
    Supergroup,
    Group,
    Creator,
    Business,
}

/// The one account a direct message is addressed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmTarget {
    /// Addressable handle. Providers trim a leading `@` themselves.
    pub handle: String,
    /// Stable provider-side identifier, re-verified by the provider
    /// immediately before a write.
    pub external_id: String,
    pub kind: DmTargetKind,
}
