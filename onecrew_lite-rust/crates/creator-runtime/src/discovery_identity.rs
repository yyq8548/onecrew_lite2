//! A budgeted native post-author observation; never a qualification decision.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CreatorRepositoryCallError, CreatorRuntime};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryIdentityReadInput {
    pub campaign_id: String,
    pub search_hit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub search_hit_id: String,
    pub profile_ref: String,
    pub author_handle: String,
    pub source_url: String,
    pub post_id: String,
    pub call_number: u32,
}

#[derive(Debug, Clone)]
pub struct FinishDiscoveryIdentityInput {
    pub claim: DiscoveryIdentityClaim,
    pub account_id: Option<String>,
    pub observed_source_url: Option<String>,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityRead {
    pub claim: DiscoveryIdentityClaim,
    pub status: String,
    pub account_id: Option<String>,
    pub observed_source_url: Option<String>,
    pub stop_code: Option<String>,
    pub captured_at: i64,
}

fn numeric_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 20
        && !id.starts_with('0')
        && id.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn post_id_for_source(handle: &str, source: &str) -> Result<String, String> {
    let invalid = || "Post identity source does not match the observed author".to_owned();
    if handle.is_empty()
        || handle.len() > 15
        || !handle
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(invalid());
    }
    let url = url::Url::parse(source).map_err(|_| invalid())?;
    let parts: Vec<_> = url.path().split('/').collect();
    if url.scheme() != "https"
        || url.host_str() != Some("x.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || parts.len() != 4
        || !matches!(parts[1], "i") && parts[1] != handle
        || parts[2] != "status"
        || !numeric_id(parts[3])
        || source != format!("https://x.com/{}/status/{}", parts[1], parts[3])
    {
        return Err(invalid());
    }
    Ok(parts[3].to_owned())
}

pub(crate) fn validate_identity_result(input: &FinishDiscoveryIdentityInput) -> Result<(), String> {
    let valid = match (
        &input.account_id,
        &input.observed_source_url,
        input.stop_code.as_deref(),
    ) {
        (Some(account), Some(source), None) => {
            numeric_id(account)
                && post_id_for_source(&input.claim.author_handle, &input.claim.source_url)
                    .ok()
                    .as_deref()
                    == Some(input.claim.post_id.as_str())
                && *source
                    == format!(
                        "https://x.com/{}/status/{}",
                        input.claim.author_handle, input.claim.post_id
                    )
        }
        (None, None, Some(code)) => matches!(
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
        ),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("Native post identity does not match its claim".into())
    }
}

impl CreatorRuntime {
    pub fn claim_discovery_identity(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryIdentityClaim, String> {
        self.repository
            .claim_discovery_identity(campaign, revision, hit)
    }

    pub async fn claim_discovery_identity_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoveryIdentityClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_identity(&campaign, revision, &hit)
        })
        .await
    }

    pub fn finish_discovery_identity(
        &self,
        input: FinishDiscoveryIdentityInput,
    ) -> Result<DiscoveryIdentityRead, String> {
        validate_identity_result(&input)?;
        self.repository.finish_discovery_identity(input)
    }

    pub async fn finish_discovery_identity_async(
        &self,
        input: FinishDiscoveryIdentityInput,
    ) -> Result<DiscoveryIdentityRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let claim = input.claim.clone();
            match owner.finish_discovery_identity(input) {
                Ok(read) => Ok(read),
                // Same reservation, no provider replay. The owner still rejects
                // stale claims and changed authorization on the failure receipt.
                Err(original) => owner
                    .finish_discovery_identity(FinishDiscoveryIdentityInput {
                        claim,
                        account_id: None,
                        observed_source_url: None,
                        stop_code: Some("opencli_invalid_output".into()),
                    })
                    .map_err(|_| original),
            }
        })
        .await
    }

    pub fn get_discovery_identity(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<DiscoveryIdentityRead, String> {
        self.repository.get_discovery_identity(campaign, hit)
    }

    pub async fn get_discovery_identity_async(
        &self,
        campaign: String,
        hit: String,
    ) -> Result<DiscoveryIdentityRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_discovery_identity(&campaign, &hit))
            .await
    }
}
