//! Resolve an unknown native author from exactly one saved Douyin video.
//! The caption and public profile are one governed, source-bound observation;
//! neither a display name nor an editable Douyin number supplies identity.
use crate::{CreatorRepositoryCallError, CreatorRuntime};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests;

/// Conservative bounds for a compact native display; never an exact count.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Deserialize))]
pub struct DiscoveryFollowerEstimate {
    pub display: String,
    pub nominal: u64,
    pub lower: u64,
    pub upper: u64,
}

pub(crate) fn follower_estimate(raw: &str) -> Option<DiscoveryFollowerEstimate> {
    let (number, multiplier) = raw
        .strip_suffix('万')
        .map(|n| (n, 10_000_u64))
        .or_else(|| raw.strip_suffix('亿').map(|n| (n, 100_000_000)))
        .or_else(|| raw.strip_suffix('千').map(|n| (n, 1_000)))?;
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty()
        || (whole.len() > 1 && whole.starts_with('0'))
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || (number.contains('.') && fraction.is_empty())
    {
        return None;
    }
    let factor = 10_u64.checked_pow(fraction.len() as u32)?;
    if factor > multiplier {
        return None;
    }
    let nominal = whole
        .parse::<u64>()
        .ok()?
        .checked_mul(multiplier)?
        .checked_add(if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u64>().ok()?.checked_mul(multiplier)? / factor
        })?;
    let resolution = multiplier / factor;
    let upper = nominal.checked_add(resolution)?;
    if upper > 9_007_199_254_740_991 {
        return None;
    }
    Some(DiscoveryFollowerEstimate {
        display: raw.into(),
        nominal,
        lower: nominal.saturating_sub(resolution),
        upper,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscoverySourceAuthorClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub search_hit_id: String,
    pub platform: String,
    pub profile_ref: String,
    pub source_url: String,
    pub post_id: String,
    pub call_number: u32,
}

#[derive(Debug, Clone)]
pub struct FinishDiscoverySourceAuthorInput {
    pub claim: DiscoverySourceAuthorClaim,
    pub native_profile: Option<Value>,
    pub native_content: Option<Value>,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoverySourceAuthorRead {
    pub claim: DiscoverySourceAuthorClaim,
    pub status: String,
    pub author_id: Option<String>,
    pub profile_url: Option<String>,
    /// An already observed author is never projected or scored a second time.
    pub duplicate_of: Option<String>,
    pub stop_code: Option<String>,
    pub captured_at: i64,
}

pub(crate) fn valid_author(id: &str) -> bool {
    id.strip_prefix("MS4wLjABAAAA").is_some_and(|body| {
        (8..=88).contains(&body.len())
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}
pub(crate) fn post_id(source: &str) -> Option<&str> {
    source
        .strip_prefix("https://www.douyin.com/video/")
        .filter(|id| {
            (1..=20).contains(&id.len())
                && !id.starts_with('0')
                && id.bytes().all(|b| b.is_ascii_digit())
        })
}
fn fields(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|row| {
        row.len() == fields.len() && fields.iter().all(|field| row.contains_key(*field))
    })
}
fn text(value: &Value, max: usize) -> bool {
    value
        .as_str()
        .is_some_and(|text| text.len() <= max && !text.contains('\0'))
}
fn label(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        !text.trim().is_empty() && text.len() <= 512 && !text.chars().any(char::is_control)
    })
}
fn exact_followers(text: &str) -> Option<u64> {
    let groups: Vec<_> = text.split(',').collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit()))
        || (groups.len() > 1
            && (groups[0].len() > 3 || groups.iter().skip(1).any(|g| g.len() != 3)))
        || (text.starts_with('0') && text != "0")
    {
        return None;
    }
    text.replace(',', "")
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= 9_007_199_254_740_991)
}
pub(crate) fn valid_profile(value: &Value) -> bool {
    let id = value["author_id"].as_str().unwrap_or_default();
    fields(
        value,
        &[
            "adapter_version",
            "author_id",
            "author_label",
            "profile_url",
            "follower_count",
            "follower_text",
            "bio",
            "bio_truncated",
            "public_links",
            "evidence_source",
        ],
    ) && valid_author(id)
        && value["adapter_version"] == "1.0.0"
        && value["profile_url"] == format!("https://www.douyin.com/user/{id}")
        && label(&value["author_label"])
        && text(&value["bio"], 8000)
        && value["bio_truncated"].is_boolean()
        && text(&value["follower_text"], 100)
        && (value["follower_count"].is_null() || value["follower_count"].as_u64().is_some())
        && value["follower_count"].as_u64()
            == value["follower_text"].as_str().and_then(exact_followers)
        && value["public_links"].as_array().is_some_and(|links| {
            links.len() <= 10
                && links.iter().all(|link| {
                    link.as_str()
                        .is_some_and(crate::discovery_pinterest::valid_link)
                })
        })
        && value["evidence_source"] == "visible_profile_header"
}
pub(crate) fn valid_content(value: &Value, author: &str, source: &str) -> bool {
    fields(
        value,
        &[
            "post_id",
            "author_id",
            "author_label",
            "profile_url",
            "source_url",
            "text",
            "text_truncated",
            "evidence_source",
        ],
    ) && valid_author(author)
        && value["author_id"].as_str() == Some(author)
        && value["post_id"].as_str() == post_id(source)
        && post_id(source).is_some()
        && value["source_url"] == source
        && value["profile_url"] == format!("https://www.douyin.com/user/{author}")
        && label(&value["author_label"])
        && text(&value["text"], 12000)
        && value["text"].as_str().is_some_and(|t| !t.trim().is_empty())
        && value["text_truncated"].is_boolean()
        && value["evidence_source"] == "visible_native_video_caption"
}

pub(crate) fn validate_result(input: &FinishDiscoverySourceAuthorInput) -> Result<(), String> {
    let claim = &input.claim;
    if claim.platform != "douyin" || post_id(&claim.source_url) != Some(claim.post_id.as_str()) {
        return Err("Source author claim is not its canonical matched video".into());
    }
    let valid = match (
        &input.native_profile,
        &input.native_content,
        input.stop_code.as_deref(),
    ) {
        (Some(profile), Some(content), None) => {
            valid_profile(profile)
                && profile["author_id"]
                    .as_str()
                    .is_some_and(|author| valid_content(content, author, &claim.source_url))
                && profile.to_string().len() <= 16000
                && content.to_string().len() <= 16000
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
        Err("Source author result does not bind its native profile and original caption".into())
    }
}

impl CreatorRuntime {
    pub fn claim_discovery_source_author(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoverySourceAuthorClaim, String> {
        self.repository
            .claim_discovery_source_author(campaign, revision, hit)
    }
    pub async fn claim_discovery_source_author_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoverySourceAuthorClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_source_author(&campaign, revision, &hit)
        })
        .await
    }
    pub fn finish_discovery_source_author(
        &self,
        input: FinishDiscoverySourceAuthorInput,
    ) -> Result<DiscoverySourceAuthorRead, String> {
        validate_result(&input)?;
        self.repository.finish_discovery_source_author(input)
    }
    pub async fn finish_discovery_source_author_async(
        &self,
        input: FinishDiscoverySourceAuthorInput,
    ) -> Result<DiscoverySourceAuthorRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let claim = input.claim.clone();
            match owner.finish_discovery_source_author(input) {
                Ok(read) => Ok(read),
                Err(original) => owner
                    .finish_discovery_source_author(FinishDiscoverySourceAuthorInput {
                        claim,
                        native_profile: None,
                        native_content: None,
                        stop_code: Some("opencli_invalid_output".into()),
                    })
                    .map_err(|_| original),
            }
        })
        .await
    }
    pub fn get_discovery_source_author(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<DiscoverySourceAuthorRead, String> {
        self.repository.get_discovery_source_author(campaign, hit)
    }
    pub async fn get_discovery_source_author_async(
        &self,
        campaign: String,
        hit: String,
    ) -> Result<DiscoverySourceAuthorRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_discovery_source_author(&campaign, &hit))
            .await
    }
}
