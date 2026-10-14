use crate::{
    AccountRefId, ContactChannel, ContactRouteCollision, CreatorContactRouteId, CreatorId,
    LocatorKind, SourceKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Agent-quality facts captured when a candidate is saved as a Creator.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CreatorQualityInput {
    /// Agent quality score, 0-100, judged against the user's request plus the
    /// confirmed plan description; save the score shown to the user.
    #[serde(default)]
    pub score: Option<u8>,
    /// The assessment shown with the score, in the user's language: criteria
    /// met, not met, and unverified.
    #[serde(default)]
    pub assessment: Option<String>,
    /// Follower/subscriber counts per platform observed during discovery.
    #[serde(default)]
    pub follower_counts: std::collections::BTreeMap<crate::models::Platform, u64>,
}

impl CreatorQualityInput {
    /// Shared value normalization for update and canonical save. This has no
    /// repository or application-dispatch dependencies.
    pub(crate) fn normalized(self) -> Result<Self, String> {
        if self.score.is_some_and(|score| score > 100) {
            return Err("creator score must be between 0 and 100".into());
        }
        let assessment = self
            .assessment
            .as_deref()
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    Err("creator assessment cannot be empty when provided".to_owned())
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()?;
        Ok(Self {
            score: self.score,
            assessment,
            follower_counts: self.follower_counts,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.score.is_none() && self.assessment.is_none() && self.follower_counts.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommunityAdministratorOutreachContext {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub operator_account_ref_id: AccountRefId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorCandidateSaveRouteOption {
    pub route_id: String,
    pub channel: ContactChannel,
    pub kind: LocatorKind,
    pub normalized_value: String,
    pub display_value: String,
    pub sources: Vec<SourceKind>,
    pub collision: ContactRouteCollision,
    pub required: bool,
}
