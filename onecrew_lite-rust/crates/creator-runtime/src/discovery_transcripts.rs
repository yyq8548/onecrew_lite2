//! Public-source transcript material. Reading content never certifies a Creator.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CreatorRepositoryCallError, CreatorRuntime};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryTranscriptSegment {
    pub timestamp: String,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryTranscriptReadInput {
    pub campaign_id: String,
    pub search_hit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryTranscriptClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub profile_ref: String,
    pub search_hit_id: String,
    pub candidate_id: String,
    pub source_url: String,
    pub call_number: u32,
}

#[derive(Debug, Clone)]
pub struct FinishDiscoveryTranscriptInput {
    pub claim: DiscoveryTranscriptClaim,
    /// Empty, non-truncated content with no failure means explicitly unavailable.
    pub segments: Vec<DiscoveryTranscriptSegment>,
    pub truncated: bool,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryTranscriptRead {
    pub search_hit_id: String,
    pub candidate_id: String,
    pub source_url: String,
    pub profile_ref: String,
    pub platform_account_id: String,
    pub status: String,
    pub segments: Vec<DiscoveryTranscriptSegment>,
    pub truncated: bool,
    pub stop_code: Option<String>,
    pub call_number: u32,
    pub captured_at: i64,
}

pub(crate) fn validate_transcript_result(
    input: &FinishDiscoveryTranscriptInput,
) -> Result<(), String> {
    if input.segments.len() > 200
        || serde_json::to_vec(&input.segments)
            .map_err(|e| e.to_string())?
            .len()
            > 24_000
        || input.segments.iter().any(|line| {
            line.timestamp.is_empty()
                || line.timestamp.len() > 24
                || !line
                    .timestamp
                    .bytes()
                    .all(|c| c.is_ascii_digit() || c == b':')
                || line.speaker.chars().count() > 120
                || line.text.trim().is_empty()
                || line.text.chars().count() > 4_000
        })
        || (input.segments.is_empty() && input.truncated)
        || (input.stop_code.is_some() && (!input.segments.is_empty() || input.truncated))
        || input.stop_code.as_deref().is_some_and(|code| {
            !matches!(
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
            )
        })
    {
        return Err("Transcript result is invalid or exceeds bounded excerpt limits".into());
    }
    Ok(())
}

impl CreatorRuntime {
    pub fn claim_discovery_transcript(
        &self,
        campaign_id: &str,
        revision: u32,
        profile: &str,
        hit: &str,
    ) -> Result<DiscoveryTranscriptClaim, String> {
        self.repository
            .claim_discovery_transcript(campaign_id, revision, profile, hit)
    }

    pub async fn claim_discovery_transcript_async(
        &self,
        campaign_id: String,
        revision: u32,
        profile: String,
        hit: String,
    ) -> Result<DiscoveryTranscriptClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_transcript(&campaign_id, revision, &profile, &hit)
        })
        .await
    }

    pub fn finish_discovery_transcript(
        &self,
        input: FinishDiscoveryTranscriptInput,
    ) -> Result<DiscoveryTranscriptRead, String> {
        validate_transcript_result(&input)?;
        self.repository.finish_discovery_transcript(input)
    }

    pub async fn finish_discovery_transcript_async(
        &self,
        input: FinishDiscoveryTranscriptInput,
    ) -> Result<DiscoveryTranscriptRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let claim = input.claim.clone();
            match owner.finish_discovery_transcript(input) {
                Ok(read) => Ok(read),
                // Same reservation, no provider replay. The owner still rejects
                // stale claims and changed authorization on the failure receipt.
                Err(original) => owner
                    .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
                        claim,
                        segments: vec![],
                        truncated: false,
                        stop_code: Some("opencli_invalid_output".into()),
                    })
                    .map_err(|_| original),
            }
        })
        .await
    }

    pub fn get_discovery_transcript(
        &self,
        campaign_id: &str,
        hit: &str,
    ) -> Result<DiscoveryTranscriptRead, String> {
        self.repository.get_discovery_transcript(campaign_id, hit)
    }

    pub async fn get_discovery_transcript_async(
        &self,
        campaign_id: String,
        hit: String,
    ) -> Result<DiscoveryTranscriptRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_discovery_transcript(&campaign_id, &hit))
            .await
    }
}
