use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{CreatorGroupId, CreatorId, PageLimits};

pub const CREATOR_GROUP_NAME_MAX_CHARS: usize = 64;
pub const CREATOR_GROUP_BULK_MOVE_LIMIT: usize = 100;
pub const CREATOR_GROUP_PAGE_LIMITS: PageLimits = PageLimits::new(50, 100);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatorGroup {
    pub id: CreatorGroupId,
    pub name: String,
    pub revision: u64,
    pub position: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatorGroupRef {
    pub id: CreatorGroupId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatorGroupSummary {
    pub group: CreatorGroup,
    pub creator_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListCreatorGroupsInput {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatorGroupListResult {
    pub items: Vec<CreatorGroupSummary>,
    pub uncategorized_count: u64,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateCreatorGroupInput {
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateCreatorGroupInput {
    pub group_id: CreatorGroupId,
    pub expected_revision: u64,
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteCreatorGroupInput {
    pub group_id: CreatorGroupId,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteCreatorGroupResult {
    pub group_id: CreatorGroupId,
    pub uncategorized_creator_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorGroupDestination {
    Uncategorized,
    Group { group_id: CreatorGroupId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorGroupFilter {
    Uncategorized,
    Group { group_id: CreatorGroupId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetCreatorGroupInput {
    pub creator_id: CreatorId,
    pub destination: CreatorGroupDestination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BulkMoveCreatorsToGroupInput {
    #[schemars(length(min = 1, max = 100))]
    pub creator_ids: Vec<CreatorId>,
    pub destination: CreatorGroupDestination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatorGroupAssignment {
    pub creator_id: CreatorId,
    pub group: Option<CreatorGroupRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MoveCreatorsToGroupResult {
    pub assignments: Vec<CreatorGroupAssignment>,
    pub moved_count: u64,
    pub unchanged_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorGroupError {
    InvalidInput,
    DuplicateName,
    GroupNotFound,
    CreatorNotFound,
    RevisionConflict,
    Internal,
}

impl fmt::Display for CreatorGroupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "creator group input is invalid",
            Self::DuplicateName => "creator group name already exists",
            Self::GroupNotFound => "creator group not found",
            Self::CreatorNotFound => "creator not found",
            Self::RevisionConflict => "creator group revision changed",
            Self::Internal => "creator group operation failed",
        })
    }
}

impl std::error::Error for CreatorGroupError {}
