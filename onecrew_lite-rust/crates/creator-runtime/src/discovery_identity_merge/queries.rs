//! Exact read requests shared by authenticated peer adapters.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetDiscoveryIdentityReviewInput {
    #[schemars(length(min = 1, max = 128))]
    pub campaign_id: String,
    #[schemars(length(min = 1, max = 128))]
    pub decision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetDiscoveryIdentityGroupsInput {
    #[schemars(length(min = 1, max = 128))]
    pub campaign_id: String,
    #[schemars(range(min = 1))]
    pub expected_revision: u32,
}

pub(super) fn validate_groups(campaign: &str, revision: u32) -> Result<(), String> {
    if revision == 0
        || !super::submission::text(campaign, 128)
        || campaign.chars().any(char::is_control)
    {
        return Err("identity_review_input_invalid".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_requests_reject_client_account_and_group_facts() {
        for field in [
            "operator_account_id",
            "native_author_id",
            "groups",
            "verdict",
        ] {
            let mut input = json!({"campaign_id":"campaign", "decision_id":"decision"});
            input[field] = json!("untrusted");
            assert!(serde_json::from_value::<GetDiscoveryIdentityReviewInput>(input).is_err());
            let mut input = json!({"campaign_id":"campaign", "expected_revision":1});
            input[field] = json!("untrusted");
            assert!(serde_json::from_value::<GetDiscoveryIdentityGroupsInput>(input).is_err());
        }
    }

    #[test]
    fn groups_reject_invalid_identifiers_and_revision_before_repository_work() {
        for campaign in ["", " ", "bad\n", &"x".repeat(129)] {
            assert_eq!(
                validate_groups(campaign, 1).unwrap_err(),
                "identity_review_input_invalid"
            );
        }
        assert!(validate_groups("campaign", 0).is_err());
        assert!(validate_groups("campaign", 1).is_ok());
    }
}
