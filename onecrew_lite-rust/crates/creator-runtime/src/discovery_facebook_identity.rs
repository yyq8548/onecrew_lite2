//! Governed native identity for a saved, previously unbound Facebook profile.
use crate::{CreatorRepositoryCallError, CreatorRuntime};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryFacebookIdentityClaim {
    pub campaign_id: String,
    pub search_hit_id: String,
    pub expected_revision: u32,
    pub profile_ref: String,
    pub source_url: String,
    pub call_number: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryFacebookIdentityRead {
    pub claim: DiscoveryFacebookIdentityClaim,
    pub status: String,
    pub native_identity: Option<Value>,
    pub stop_code: Option<String>,
    pub captured_at: i64,
}
#[derive(Debug, Clone)]
pub struct FinishDiscoveryFacebookIdentityInput {
    pub claim: DiscoveryFacebookIdentityClaim,
    pub native_identity: Option<Value>,
    pub stop_code: Option<String>,
}

pub(crate) fn valid_source(source: &str) -> bool {
    let Some(handle) = source.strip_prefix("https://www.facebook.com/") else {
        return false;
    };
    let handle = handle.strip_suffix('/').unwrap_or(handle);
    let reserved = [
        "about",
        "ads",
        "business",
        "challenge",
        "checkpoint",
        "directory",
        "events",
        "friends",
        "gaming",
        "groups",
        "help",
        "home",
        "login",
        "login.php",
        "marketplace",
        "messages",
        "notifications",
        "pages",
        "photo",
        "photos",
        "privacy",
        "profile",
        "profile.php",
        "recover",
        "reel",
        "reels",
        "search",
        "settings",
        "share",
        "stories",
        "two_step_verification",
        "watch",
    ];
    (1..=100).contains(&handle.len())
        && handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.')
        && !handle.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && !reserved.contains(&handle.to_ascii_lowercase().as_str())
}

pub(crate) fn validate_result(input: &FinishDiscoveryFacebookIdentityInput) -> Result<(), String> {
    if !valid_source(&input.claim.source_url) {
        return Err("Facebook identity requires its saved profile source".into());
    }
    let valid = match (&input.native_identity, input.stop_code.as_deref()) {
        (Some(value), None) => {
            let keys = [
                "adapter_version",
                "source_url",
                "id",
                "display_name",
                "profile_url",
                "evidence_source",
            ];
            let id = value["id"].as_str().unwrap_or_default();
            value.as_object().is_some_and(|row| {
                row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key))
            }) && value.to_string().len() <= 4000
                && value["adapter_version"] == "1.0.0"
                && value["source_url"] == input.claim.source_url
                && value["evidence_source"] == "native_profile_header_response"
                && (1..=20).contains(&id.len())
                && !id.starts_with('0')
                && id.bytes().all(|b| b.is_ascii_digit())
                && value["profile_url"].as_str().is_some_and(|url| {
                    valid_source(url)
                        && url
                            .trim_end_matches('/')
                            .eq_ignore_ascii_case(input.claim.source_url.trim_end_matches('/'))
                })
                && value["display_name"].as_str().is_some_and(|name| {
                    !name.trim().is_empty()
                        && name.chars().count() <= 200
                        && !name.chars().any(char::is_control)
                })
        }
        (None, Some(code)) => matches!(
            code,
            "opencli_timeout"
                | "opencli_output_too_large"
                | "opencli_version_changed"
                | "opencli_daemon_unavailable"
                | "opencli_extension_unavailable"
                | "opencli_profile_unavailable"
                | "opencli_command_unavailable"
                | "opencli_invalid_output"
                | "opencli_invalid_input"
                | "opencli_unavailable"
                | "opencli_command_failed"
                | "opencli_risk_control"
                | "opencli_auth_required"
                | "interrupted_facebook_identity_read"
        ),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("Facebook identity result does not bind its native ID to the saved profile".into())
    }
}

impl CreatorRuntime {
    pub fn claim_discovery_facebook_identity(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryFacebookIdentityClaim, String> {
        self.repository
            .claim_discovery_facebook_identity(campaign, revision, hit)
    }
    pub async fn claim_discovery_facebook_identity_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoveryFacebookIdentityClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_facebook_identity(&campaign, revision, &hit)
        })
        .await
    }
    pub fn finish_discovery_facebook_identity(
        &self,
        input: FinishDiscoveryFacebookIdentityInput,
    ) -> Result<DiscoveryFacebookIdentityRead, String> {
        validate_result(&input)?;
        self.repository.finish_discovery_facebook_identity(input)
    }
    pub async fn finish_discovery_facebook_identity_async(
        &self,
        input: FinishDiscoveryFacebookIdentityInput,
    ) -> Result<DiscoveryFacebookIdentityRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let claim = input.claim.clone();
            match owner.finish_discovery_facebook_identity(input) {
                Ok(read) => Ok(read),
                Err(original) => owner
                    .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
                        claim,
                        native_identity: None,
                        stop_code: Some("opencli_invalid_output".into()),
                    })
                    .map_err(|_| original),
            }
        })
        .await
    }
    pub fn get_discovery_facebook_identity(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<DiscoveryFacebookIdentityRead, String> {
        self.repository
            .get_discovery_facebook_identity(campaign, hit)
    }
}
