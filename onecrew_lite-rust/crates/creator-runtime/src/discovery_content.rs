//! One bounded public video observation tied to a saved search hit and author.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CreatorRepositoryCallError, CreatorRuntime};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryContentReadInput {
    pub campaign_id: String,
    pub search_hit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryContentClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub search_hit_id: String,
    pub platform: String,
    pub profile_ref: String,
    pub author_id: String,
    pub source_url: String,
    pub call_number: u32,
}

impl DiscoveryContentClaim {
    /// Engine content claims are reserved only for the bound native video
    /// language supplement. Keep origin authority separate from failure scope.
    pub fn target_platform(&self) -> &str {
        if matches!(self.platform.as_str(), "google" | "yahoo") {
            "youtube"
        } else {
            &self.platform
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinishDiscoveryContentInput {
    pub claim: DiscoveryContentClaim,
    /// A bounded public provider result. It cannot certify a product demonstration.
    pub native_content: Option<Value>,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryContentRead {
    pub claim: DiscoveryContentClaim,
    pub status: String,
    pub native_content: Option<Value>,
    pub stop_code: Option<String>,
    pub captured_at: i64,
    /// Present only on a completed language-supplement response, not an initial caption receipt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_sample: Option<crate::DiscoveryLanguageSample>,
}

impl DiscoveryContentRead {
    /// The request can be a native author feed. Preserve its authorization URL
    /// in the claim and use the separately observed post URL for text citations.
    pub fn observed_source_url(&self) -> Option<&str> {
        if self.status != "completed" || self.stop_code.is_some() {
            return None;
        }
        if matches!(
            self.claim.platform.as_str(),
            "instagram" | "pinterest" | "linkedin"
        ) {
            self.native_content.as_ref()?.get("source_url")?.as_str()
        } else {
            Some(&self.claim.source_url)
        }
    }
}

pub(crate) fn validate_content_result(input: &FinishDiscoveryContentInput) -> Result<(), String> {
    let valid = match (&input.native_content, input.stop_code.as_deref()) {
        (Some(value), None) if input.claim.platform == "douyin" => {
            crate::discovery_douyin::valid_content(
                value,
                &input.claim.author_id,
                &input.claim.source_url,
            ) && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16000
        }
        (Some(value), None) if input.claim.platform == "linkedin" => {
            crate::discovery_linkedin::valid_content(
                value,
                &input.claim.author_id,
                &input.claim.source_url,
            ) && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 24000
        }
        (Some(value), None) if input.claim.platform == "pinterest" => {
            crate::discovery_pinterest::valid_content(
                value,
                &input.claim.author_id,
                &input.claim.source_url,
            ) && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 24000
        }
        (Some(value), None) if input.claim.platform == "pixiv" => {
            crate::discovery_pixiv::valid_content(
                value,
                &input.claim.author_id,
                &input.claim.source_url,
            ) && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 24000
        }
        (Some(value), None) if input.claim.platform == "hackernews" => {
            crate::discovery_hackernews::valid_content(
                value,
                &input.claim.author_id,
                &input.claim.source_url,
            ) && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 40000
        }
        (Some(value), None) if input.claim.platform == "zhihu" => {
            crate::discovery_zhihu::valid_content(
                value,
                &input.claim.author_id,
                &input.claim.source_url,
            ) && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 24000
        }
        (Some(value), None) if input.claim.platform == "bluesky" => {
            crate::discovery_bluesky::valid_content(
                value,
                &input.claim.author_id,
                &input.claim.source_url,
            ) && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16000
        }
        (Some(value), None) if input.claim.platform == "weibo" => {
            let source = input
                .claim
                .source_url
                .strip_prefix("https://weibo.com/")
                .and_then(|s| s.split_once('/'));
            let numeric = |id: &str| {
                (1..=20).contains(&id.len())
                    && !id.starts_with('0')
                    && id.bytes().all(|b| b.is_ascii_digit())
            };
            let keys = [
                "post_id",
                "author_id",
                "author",
                "source_url",
                "text",
                "text_truncated",
                "text_completeness_known",
                "is_repost",
            ];
            value.as_object().is_some_and(|v| {
                v.len() == keys.len() && keys.iter().all(|key| v.contains_key(*key))
            }) && source.is_some_and(|(uid, code)| {
                uid == input.claim.author_id
                    && numeric(uid)
                    && (1..=20).contains(&code.len())
                    && code.bytes().all(|b| b.is_ascii_alphanumeric())
            }) && value["author_id"].as_str() == Some(input.claim.author_id.as_str())
                && value["post_id"].as_str().is_some_and(numeric)
                && value["source_url"] == input.claim.source_url
                && value["author"].as_str().is_some_and(|s| {
                    !s.trim().is_empty()
                        && s.chars().count() <= 200
                        && !s.chars().any(char::is_control)
                })
                && value["text"]
                    .as_str()
                    .is_some_and(|s| s.len() <= 16000 && !s.contains('\0'))
                && value["text_truncated"].is_boolean()
                && value["text_completeness_known"].is_boolean()
                && value["is_repost"] == false
                && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16000
        }
        (Some(value), None) if input.claim.platform == "instagram" => {
            let profile = input
                .claim
                .source_url
                .strip_prefix("https://www.instagram.com/")
                .and_then(|s| s.strip_suffix('/'));
            let source = value["source_url"]
                .as_str()
                .and_then(|s| s.strip_prefix("https://www.instagram.com/"))
                .and_then(|s| s.strip_suffix('/'))
                .and_then(|s| s.split_once('/'));
            let numeric = |id: &str| {
                (1..=25).contains(&id.len())
                    && !id.starts_with('0')
                    && id.bytes().all(|b| b.is_ascii_digit())
            };
            let expected = [
                "post_id",
                "author_id",
                "source_url",
                "text",
                "text_truncated",
            ];
            value.as_object().is_some_and(|row| {
                row.len() == expected.len() && expected.iter().all(|key| row.contains_key(*key))
            }) && profile.is_some_and(|name| {
                (1..=30).contains(&name.len())
                    && name == name.to_ascii_lowercase()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
            }) && source.is_some_and(|(kind, code)| {
                matches!(kind, "p" | "reel")
                    && (1..=32).contains(&code.len())
                    && code
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            }) && numeric(&input.claim.author_id)
                && value["post_id"].as_str().is_some_and(numeric)
                && value["author_id"].as_str() == Some(input.claim.author_id.as_str())
                && value["text"]
                    .as_str()
                    .is_some_and(|s| s.len() <= 8000 && !s.contains('\0'))
                && value["text_truncated"].is_boolean()
                && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16_000
        }
        (Some(value), None) if input.claim.platform == "facebook" => {
            let numeric = |id: &str, max: usize| {
                !id.is_empty()
                    && id.len() <= max
                    && !id.starts_with('0')
                    && id.bytes().all(|b| b.is_ascii_digit())
            };
            let id = input
                .claim
                .source_url
                .strip_prefix("https://www.facebook.com/reel/");
            let expected = [
                "post_id",
                "author_id",
                "source_url",
                "text",
                "text_truncated",
            ];
            value.as_object().is_some_and(|row| {
                row.len() == expected.len() && expected.iter().all(|key| row.contains_key(*key))
            }) && id.is_some_and(|id| numeric(id, 25))
                && numeric(&input.claim.author_id, 20)
                && value["post_id"].as_str() == id
                && value["author_id"].as_str() == Some(input.claim.author_id.as_str())
                && value["source_url"].as_str() == Some(input.claim.source_url.as_str())
                && value["text"].as_str().is_some_and(|text| {
                    !text.trim().is_empty() && text.len() <= 8000 && !text.contains('\0')
                })
                && value["text_truncated"].is_boolean()
                && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16_000
        }
        (Some(value), None) if input.claim.platform == "reddit" => {
            let id = input
                .claim
                .source_url
                .strip_prefix("https://www.reddit.com/comments/")
                .and_then(|s| s.strip_suffix('/'));
            let base36 = |id: &str| {
                (1..=15).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            };
            value.as_object().is_some_and(|row| row.len() == 5)
                && id.is_some_and(base36)
                && input
                    .claim
                    .author_id
                    .strip_prefix("t2_")
                    .is_some_and(base36)
                && value.get("post_id").and_then(Value::as_str) == id
                && value.get("author_id").and_then(Value::as_str)
                    == Some(input.claim.author_id.as_str())
                && value.get("source_url").and_then(Value::as_str)
                    == Some(input.claim.source_url.as_str())
                && value
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty() && s.len() <= 8000 && !s.contains('\0'))
                && value
                    .get("text_truncated")
                    .and_then(Value::as_bool)
                    .is_some()
                && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16_000
        }
        (Some(value), None) if input.claim.platform == "twitter" => {
            serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16_000
                && validate_twitter_content(value, &input.claim)
        }
        (Some(value), None) => {
            let expected = [
                "bvid",
                "author_id",
                "author",
                "source_url",
                "title",
                "description",
                "description_truncated",
                "requires_payment",
                "pay_preview",
            ];
            input.claim.platform == "bilibili"
                && value.as_object().is_some_and(|object| {
                    (object.len() == expected.len()
                        || (object.len() == expected.len() + 1
                            && object
                                .get("payment_status_known")
                                .and_then(Value::as_bool)
                                .is_some()))
                        && expected.iter().all(|key| object.contains_key(*key))
                })
                && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16_000
                && value.get("author_id").and_then(Value::as_str)
                    == Some(input.claim.author_id.as_str())
                && value.get("source_url").and_then(Value::as_str)
                    == Some(input.claim.source_url.as_str())
                && value.get("bvid").and_then(Value::as_str)
                    == input
                        .claim
                        .source_url
                        .strip_prefix("https://www.bilibili.com/video/")
                && value
                    .get("description_truncated")
                    .and_then(Value::as_bool)
                    .is_some()
                && value
                    .get("requires_payment")
                    .and_then(Value::as_bool)
                    .is_some()
                && value.get("pay_preview").and_then(Value::as_bool).is_some()
                && value
                    .get("author")
                    .and_then(Value::as_str)
                    .is_some_and(|text| {
                        !text.trim().is_empty()
                            && text.chars().count() <= 200
                            && !text.chars().any(char::is_control)
                    })
                && value
                    .get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|text| {
                        !text.trim().is_empty()
                            && text.chars().count() <= 500
                            && !text.chars().any(char::is_control)
                    })
                && value
                    .get("description")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.len() <= 8_192)
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
        ),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("Content result does not match its saved hit and author".into())
    }
}

fn validate_twitter_content(value: &Value, claim: &DiscoveryContentClaim) -> bool {
    let keys = |value: &Value, expected: &[&str]| {
        value.as_object().is_some_and(|object| {
            object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
        })
    };
    let Ok(post) =
        crate::discovery_identity::post_id_for_source(&claim.author_id, &claim.source_url)
    else {
        return false;
    };
    let identity = &value["identity"];
    let numeric = |value: &Value| {
        value.as_str().is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 20
                && !id.starts_with('0')
                && id.bytes().all(|b| b.is_ascii_digit())
        })
    };
    let Some(urls) = value["expanded_urls"].as_array() else {
        return false;
    };
    let mut seen = std::collections::HashSet::new();
    keys(
        value,
        &[
            "adapter_version",
            "identity",
            "text",
            "text_truncated",
            "text_source",
            "created_at",
            "has_media",
            "quotes_another_post",
            "expanded_urls",
        ],
    ) && keys(
        identity,
        &[
            "adapter_version",
            "post_id",
            "account_id",
            "handle",
            "profile_url",
            "source_url",
            "identity_kind",
            "evidence_source",
        ],
    ) && value["adapter_version"] == "1.0.0"
        && identity["adapter_version"] == "1.0.0"
        && identity["identity_kind"] == "native_post_author_id"
        && identity["evidence_source"] == "x_tweet_detail"
        && identity["post_id"].as_str() == Some(post.as_str())
        && numeric(&identity["account_id"])
        && identity["handle"].as_str() == Some(claim.author_id.as_str())
        && identity["profile_url"].as_str()
            == Some(format!("https://x.com/{}", claim.author_id).as_str())
        && identity["source_url"].as_str()
            == Some(format!("https://x.com/{}/status/{post}", claim.author_id).as_str())
        && value["text"].as_str().is_some_and(|text| {
            !text.trim().is_empty() && text.len() <= 8000 && !text.contains('\0')
        })
        && matches!(
            value["text_source"].as_str(),
            Some("legacy_full_text" | "note_tweet")
        )
        && value["created_at"].as_str().is_some_and(|created| {
            !created.trim().is_empty()
                && created.len() <= 80
                && !created.chars().any(char::is_control)
        })
        && value["text_truncated"].as_bool().is_some()
        && value["has_media"].as_bool().is_some()
        && value["quotes_another_post"].as_bool().is_some()
        && urls.len() <= 20
        && urls.iter().all(|value| {
            value.as_str().is_some_and(|value| {
                value.len() <= 2048
                    && !value.chars().any(|c| c.is_control() || c.is_whitespace())
                    && seen.insert(value)
                    && url::Url::parse(value).is_ok_and(|url| {
                        matches!(url.scheme(), "http" | "https")
                            && url.has_host()
                            && url.username().is_empty()
                            && url.password().is_none()
                    })
            })
        })
}

impl CreatorRuntime {
    pub fn claim_discovery_content(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryContentClaim, String> {
        self.repository
            .claim_discovery_content(campaign, revision, hit)
    }

    pub async fn claim_discovery_content_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoveryContentClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_content(&campaign, revision, &hit)
        })
        .await
    }

    pub fn finish_discovery_content(
        &self,
        input: FinishDiscoveryContentInput,
    ) -> Result<DiscoveryContentRead, String> {
        validate_content_result(&input)?;
        self.repository.finish_discovery_content(input)
    }

    pub async fn finish_discovery_content_async(
        &self,
        input: FinishDiscoveryContentInput,
    ) -> Result<DiscoveryContentRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let claim = input.claim.clone();
            match owner.finish_discovery_content(input) {
                Ok(read) => Ok(read),
                // Same reservation, no provider replay. The owner still rejects
                // stale claims and changed authorization on the failure receipt.
                Err(original) => owner
                    .finish_discovery_content(FinishDiscoveryContentInput {
                        claim,
                        native_content: None,
                        stop_code: Some("opencli_invalid_output".into()),
                    })
                    .map_err(|_| original),
            }
        })
        .await
    }

    pub fn get_discovery_content(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<DiscoveryContentRead, String> {
        self.repository.get_discovery_content(campaign, hit)
    }

    pub async fn get_discovery_content_async(
        &self,
        campaign: String,
        hit: String,
    ) -> Result<DiscoveryContentRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_discovery_content(&campaign, &hit))
            .await
    }
}
