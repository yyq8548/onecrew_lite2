//! Keep approved search origin separate from actually observed native identity.
use crate::{CreatorRepositoryCallError, CreatorRuntime};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests;
mod twitter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryWebSourceClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub search_hit_id: String,
    pub origin_platform: String,
    pub profile_ref: String,
    pub source_url: String,
    pub target_platform: String,
    pub video_id: String,
    pub call_number: u32,
}

#[derive(Debug, Clone)]
pub struct FinishDiscoveryWebSourceInput {
    pub claim: DiscoveryWebSourceClaim,
    pub native_material: Option<Value>,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryWebSourceRead {
    pub claim: DiscoveryWebSourceClaim,
    pub status: String,
    pub native_author_id: Option<String>,
    pub native_material: Option<Value>,
    pub stop_code: Option<String>,
    pub captured_at: i64,
    pub candidate_id: Option<String>,
    pub is_primary_source: bool,
}

/// Strict HTTPS video locations; URLs locate, but do not establish authorship.
pub(crate) fn video_id(source: &str) -> Option<String> {
    let url = url::Url::parse(source).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || !matches!(url.host_str(), Some("youtube.com" | "www.youtube.com"))
    {
        return None;
    }
    let valid = |id: &str| {
        id.len() == 11
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    };
    if url.path() == "/watch" {
        let mut ids = url.query_pairs().filter(|(key, _)| key == "v");
        let (_, id) = ids.next()?;
        return (valid(&id) && ids.next().is_none()).then(|| id.into_owned());
    }
    let parts: Vec<_> = url.path_segments()?.collect();
    match parts.as_slice() {
        // Observed Yahoo locator; native evidence still establishes authorship.
        ["watch", id] if valid(id) && url.query().is_none() => Some((*id).into()),
        ["shorts" | "live", id] if valid(id) => Some((*id).into()),
        _ => None,
    }
}

/// The legacy video_id field holds the reserved native content ID. Existing
/// YouTube receipts retain their exact wire and persisted representation.
pub(crate) fn source_target(source: &str) -> Option<(&'static str, String, u32)> {
    if let Some(id) = video_id(source) {
        return Some(("youtube", id, 1));
    }
    if let Some(id) = crate::discovery_douyin::post_id(source) {
        return Some(("douyin", id.into(), 2));
    }
    twitter::target(source).map(|(_, post)| ("twitter", post, 3))
}
pub fn web_source_twitter_target(source: &str) -> Option<(String, String)> {
    twitter::target(source)
}
pub(crate) fn native_author<'a>(platform: &str, value: &'a Value) -> Option<&'a str> {
    if platform == "douyin" {
        value.pointer("/content/author_id")?.as_str()
    } else if platform == "twitter" {
        value.pointer("/identity/account_id")?.as_str()
    } else {
        value["channelId"].as_str()
    }
}

pub(crate) fn validate_result(input: &FinishDiscoveryWebSourceInput) -> Result<(), String> {
    let claim = &input.claim;
    if !matches!(claim.origin_platform.as_str(), "google" | "yahoo")
        || source_target(&claim.source_url).is_none_or(|(platform, id, _)| {
            platform != claim.target_platform || id != claim.video_id
        })
        || claim.call_number == 0
        || claim.expected_revision == 0
    {
        return Err("Web source claim is not a supported native source".into());
    }
    let valid = match (&input.native_material, input.stop_code.as_deref()) {
        (None, Some(code)) => {
            matches!(
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
            ) || (claim.target_platform == "youtube"
                && matches!(
                    code,
                    "youtube_video_unavailable"
                        | "youtube_watch_layout_unsupported"
                        | "youtube_author_identity_invalid"
                        | "youtube_description_unavailable"
                ))
        }
        (Some(material), None) if claim.target_platform == "twitter" => {
            twitter::valid(material, &claim.source_url, &claim.video_id)
        }
        (Some(material), None) if claim.target_platform == "douyin" => {
            material.as_object().is_some_and(|object| {
                object.len() == 2
                    && object.contains_key("profile")
                    && object.contains_key("content")
            }) && material.to_string().len() <= 65536
                && crate::discovery_douyin::valid_profile(&material["profile"])
                && material["content"]["text_truncated"] == false
                && native_author("douyin", material).is_some_and(|author| {
                    material["profile"]["author_id"] == author
                        && crate::discovery_douyin::valid_content(
                            &material["content"],
                            author,
                            &claim.source_url,
                        )
                })
        }
        (Some(material), None) => {
            let author = material["channelId"].as_str().unwrap_or_default();
            let text = |key: &str, max: usize| {
                material[key]
                    .as_str()
                    .is_some_and(|s| s.len() <= max && !s.contains('\0'))
            };
            let label = |key: &str, max: usize| {
                text(key, max) && material[key].as_str().is_some_and(|s| !s.trim().is_empty())
            };
            let keys = [
                "adapterVersion",
                "videoId",
                "channelId",
                "sourceUrl",
                "title",
                "author",
                "subscribers",
                "description",
                "descriptionSource",
                "displayDescription",
                "links",
                "playabilityStatus",
            ];
            material.as_object().is_some_and(|row| {
                row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key))
            }) && material.to_string().len() <= 65536
                && material["adapterVersion"] == "1.0.0"
                && material["videoId"] == claim.video_id
                && material["sourceUrl"]
                    == format!("https://www.youtube.com/watch?v={}", claim.video_id)
                && author.len() == 24
                && author.starts_with("UC")
                && author
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                && material["descriptionSource"]
                    == "ytInitialPlayerResponse.videoDetails.shortDescription"
                && material["playabilityStatus"] == "OK"
                && label("title", 2000)
                && label("author", 800)
                && label("description", 16000)
                && text("subscribers", 200)
                && text("displayDescription", 16000)
                && material["links"].as_array().is_some_and(|links| {
                    links.len() <= 100
                        && links.iter().all(|link| {
                            link.as_object().is_some_and(|row| {
                                row.len() == 3
                                    && ["url", "label", "source"]
                                        .iter()
                                        .all(|key| row.contains_key(*key))
                            }) && link["url"]
                                .as_str()
                                .is_some_and(crate::discovery_pinterest::valid_link)
                                && link["label"]
                                    .as_str()
                                    .is_some_and(|s| s.len() <= 2048 && !s.contains('\0'))
                                && link["source"].as_str().is_some_and(|s| {
                                    matches!(
                                        s,
                                        "shortDescription"
                                            | "description.runs"
                                            | "attributedDescription.commandRuns"
                                    )
                                })
                        })
                })
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("Web source native material does not bind the reserved video".into())
    }
}

impl CreatorRuntime {
    pub fn claim_discovery_web_source(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryWebSourceClaim, String> {
        self.repository
            .claim_discovery_web_source(campaign, revision, hit)
    }
    pub async fn claim_discovery_web_source_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoveryWebSourceClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_web_source(&campaign, revision, &hit)
        })
        .await
    }
    pub fn finish_discovery_web_source(
        &self,
        input: FinishDiscoveryWebSourceInput,
    ) -> Result<DiscoveryWebSourceRead, String> {
        self.repository.finish_discovery_web_source(input)
    }
    pub async fn finish_discovery_web_source_async(
        &self,
        input: FinishDiscoveryWebSourceInput,
    ) -> Result<DiscoveryWebSourceRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let claim = input.claim.clone();
            match owner.finish_discovery_web_source(input) {
                Ok(read) => Ok(read),
                Err(original) => owner
                    .finish_discovery_web_source(FinishDiscoveryWebSourceInput {
                        claim,
                        native_material: None,
                        stop_code: Some("opencli_invalid_output".into()),
                    })
                    .map_err(|_| original),
            }
        })
        .await
    }
    pub fn get_discovery_web_source(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<DiscoveryWebSourceRead, String> {
        self.repository.get_discovery_web_source(campaign, hit)
    }
    pub async fn get_discovery_web_source_async(
        &self,
        campaign: String,
        hit: String,
    ) -> Result<DiscoveryWebSourceRead, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || repository.get_discovery_web_source(&campaign, &hit))
            .await
    }
}
