//! Durable public-profile observations, not qualification decisions.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CreatorRepositoryCallError, CreatorRuntime};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryProfileReadInput {
    pub campaign_id: String,
    pub search_hit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryProfileClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub search_hit_id: String,
    pub platform: String,
    pub profile_ref: String,
    pub author_id: String,
    pub author_url: String,
    pub source_url: String,
    pub call_number: u32,
}

#[derive(Debug, Clone)]
pub struct FinishDiscoveryProfileInput {
    pub claim: DiscoveryProfileClaim,
    /// Bounded native public-profile model, never a page/bootstrap or credential.
    pub native_profile: Option<Value>,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryProfileRead {
    pub claim: DiscoveryProfileClaim,
    pub status: String,
    pub native_profile: Option<Value>,
    pub stop_code: Option<String>,
    pub captured_at: i64,
}

pub(crate) fn validate_profile_result(input: &FinishDiscoveryProfileInput) -> Result<(), String> {
    let valid = match (&input.native_profile, input.stop_code.as_deref()) {
        (Some(value), None) => {
            let id_key = match input.claim.platform.as_str() {
                "bilibili" => "mid",
                "tiktok" | "reddit" | "facebook" | "instagram" | "weibo" => "id",
                "twitter" => "handle",
                "linkedin" => "author_handle",
                "bluesky" => "did",
                "zhihu" | "hackernews" | "pinterest" | "pixiv" | "douyin" => "author_id",
                _ => return Err("Unsupported profile evidence platform".into()),
            };
            let count_valid = if input.claim.platform == "twitter" {
                value.get("identity_kind").and_then(Value::as_str) == Some("native_id")
                    && value
                        .get("account_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| {
                            !id.is_empty()
                                && id.len() <= 20
                                && !id.starts_with('0')
                                && id.bytes().all(|byte| byte.is_ascii_digit())
                        })
                    && value.get("follower_count").is_some_and(|count| {
                        count.is_null()
                            || count
                                .as_u64()
                                .is_some_and(|count| count <= 9_007_199_254_740_991)
                    })
            } else if input.claim.platform == "reddit" {
                valid_reddit_profile(value)
            } else if input.claim.platform == "facebook" {
                valid_facebook_profile(value)
            } else if input.claim.platform == "instagram" {
                valid_instagram_profile(value)
            } else if input.claim.platform == "zhihu" {
                crate::discovery_zhihu::valid_profile(value)
            } else if input.claim.platform == "hackernews" {
                crate::discovery_hackernews::valid_profile(value)
            } else if input.claim.platform == "pixiv" {
                crate::discovery_pixiv::valid_profile(value)
            } else if input.claim.platform == "douyin" {
                crate::discovery_douyin::valid_profile(value)
            } else if input.claim.platform == "linkedin" {
                crate::discovery_linkedin::valid_profile(value)
            } else if input.claim.platform == "pinterest" {
                crate::discovery_pinterest::valid_profile(value)
            } else if input.claim.platform == "bluesky" {
                crate::discovery_bluesky::valid_profile(value)
            } else if input.claim.platform == "weibo" {
                valid_weibo_profile(value)
            } else {
                value
                    .get("follower_count")
                    .and_then(Value::as_u64)
                    .is_some()
            };
            value.is_object()
                && serde_json::to_vec(value).map_err(|e| e.to_string())?.len() <= 16_000
                && value.get(id_key).and_then(Value::as_str) == Some(input.claim.author_id.as_str())
                && value
                    .get(if input.claim.platform == "zhihu" {
                        "author_url"
                    } else {
                        "profile_url"
                    })
                    .and_then(Value::as_str)
                    == Some(input.claim.author_url.as_str())
                && count_valid
                && (input.claim.platform != "facebook"
                    || value["adapter_version"] != "2.0.0"
                    || value["observed_profile_url"]
                        .as_str()
                        .is_some_and(|source| {
                            source
                                .eq_ignore_ascii_case(input.claim.source_url.trim_end_matches('/'))
                        }))
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
        Err("Profile result is invalid or does not match its observed author".into())
    }
}

fn valid_weibo_profile(value: &Value) -> bool {
    let id = value["id"].as_str().unwrap_or_default();
    let keys = [
        "id",
        "name",
        "follower_count",
        "profile_url",
        "bio",
        "self_declared_location",
    ];
    value
        .as_object()
        .is_some_and(|v| v.len() == keys.len() && keys.iter().all(|key| v.contains_key(*key)))
        && (1..=20).contains(&id.len())
        && !id.starts_with('0')
        && id.bytes().all(|b| b.is_ascii_digit())
        && value["profile_url"] == format!("https://weibo.com/u/{id}")
        && value["name"].as_str().is_some_and(|s| {
            !s.trim().is_empty() && s.chars().count() <= 200 && !s.chars().any(char::is_control)
        })
        && value["bio"]
            .as_str()
            .is_some_and(|s| s.chars().count() <= 2000 && !s.contains('\0'))
        && value["self_declared_location"]
            .as_str()
            .is_some_and(|s| s.chars().count() <= 200 && !s.contains('\0'))
        && (value["follower_count"].is_null()
            || value["follower_count"]
                .as_u64()
                .is_some_and(|n| n <= 9_007_199_254_740_991))
}

fn valid_instagram_profile(value: &Value) -> bool {
    let Some(username) = value["username"].as_str() else {
        return false;
    };
    let Some(id) = value["id"].as_str() else {
        return false;
    };
    let expected = [
        "adapter_version",
        "id",
        "username",
        "display_name",
        "biography",
        "follower_count",
        "private_account",
        "verified",
        "profile_url",
        "evidence_source",
    ];
    value.as_object().is_some_and(|row| {
        expected.iter().all(|key| row.contains_key(*key))
            && row.keys().all(|key| {
                expected.contains(&key.as_str())
                    || matches!(key.as_str(), "public_email" | "contact_links" | "web_text")
            })
    }) && value["adapter_version"] == "1.0.0"
        && (value["evidence_source"] == "native_profile_response"
            || (value["evidence_source"] == "public_profile_dom_search_bound"
                && value["web_text"].as_str().is_some_and(|s| s.chars().count() <= 4000 && !s.contains('\0'))))
        && (1..=25).contains(&id.len())
        && !id.starts_with('0')
        && id.bytes().all(|b| b.is_ascii_digit())
        && (1..=30).contains(&username.len())
        && username == username.to_ascii_lowercase()
        && username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
        && value["profile_url"].as_str()
            == Some(format!("https://www.instagram.com/{username}/").as_str())
        && value["display_name"]
            .as_str()
            .is_some_and(|s| s.chars().count() <= 200 && !s.chars().any(char::is_control))
        && value["biography"]
            .as_str()
            .is_some_and(|s| s.chars().count() <= 2000 && !s.contains('\0'))
        && value.get("follower_count").is_some_and(|count| {
            count.is_null() || count.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
        })
        && value.get("public_email").is_none_or(|email| {
            email.is_null()
                || email.as_str().is_some_and(|s| {
                    s.len() <= 320 && s.contains('@') && !s.chars().any(char::is_control)
                })
        })
        && value.get("contact_links").is_none_or(|links| {
            links.as_array().is_some_and(|links| {
                links.len() <= 10
                    && links.iter().all(|link| {
                        link.as_str().is_some_and(|s| {
                            s.len() <= 2048
                                && !s.chars().any(char::is_control)
                                && url::Url::parse(s).is_ok_and(|url| {
                                    matches!(url.scheme(), "http" | "https")
                                        && url.host_str().is_some()
                                        && url.username().is_empty()
                                        && url.password().is_none()
                                })
                        })
                    })
            })
        })
        && (value["private_account"].is_boolean() || (value["evidence_source"] == "public_profile_dom_search_bound" && value["private_account"].is_null()))
        && (value["verified"].is_boolean() || (value["evidence_source"] == "public_profile_dom_search_bound" && value["verified"].is_null()))
}

fn valid_facebook_profile(value: &Value) -> bool {
    let Some(id) = value.get("id").and_then(Value::as_str) else {
        return false;
    };
    value.as_object().is_some_and(|row| {
        let required=["adapter_version","id","display_name","follower_count","follower_text","profile_url","evidence_source"];
        required.iter().all(|key| row.contains_key(*key))
            && row.keys().all(|key| required.contains(&key.as_str()) || matches!(key.as_str(),"contact_links"|"observed_profile_url"))
    })
        && match (value["adapter_version"].as_str(),value["observed_profile_url"].as_str()) {
            (Some("1.0.0"),None)=>value.get("observed_profile_url").is_none() && value["evidence_source"]=="visible_numeric_profile_header"
                && value.get("contact_links").is_none_or(|links|links.as_array().is_some_and(|links|links.len()<=1&&links.iter().all(|link|link.as_str()==Some(format!("https://www.facebook.com/profile.php?id={id}&sk=about_contact_and_basic_info").as_str())))),
            (Some("2.0.0"),Some(base))=>crate::discovery_facebook_identity::valid_source(base) && !base.ends_with('/') && value["evidence_source"]=="native_profile_header_response"
                && value["contact_links"].as_array().is_some_and(|links|links.len()<=2&&links.iter().all(|link|link.as_str().is_some_and(|url|[format!("{base}/directory_contact_info"),format!("{base}/directory_links")].contains(&url.to_owned())))
                    && links.iter().filter_map(Value::as_str).collect::<std::collections::HashSet<_>>().len()==links.len()),
            _=>false,
        }
        && (1..=20).contains(&id.len())
        && !id.starts_with('0')
        && id.bytes().all(|b| b.is_ascii_digit())
        && value.get("profile_url").and_then(Value::as_str)
            == Some(format!("https://www.facebook.com/profile.php?id={id}").as_str())
        && value
            .get("display_name")
            .and_then(Value::as_str)
            .is_some_and(|name| {
                !name.trim().is_empty()
                    && name.chars().count() <= 200
                    && !name.chars().any(char::is_control)
            })
        && value
            .get("follower_text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.chars().count() <= 100 && !text.chars().any(char::is_control))
        && value.get("follower_count").is_some_and(|count| {
            count.is_null()
                || count.as_u64().is_some_and(|n| {
                    let text = value
                        .get("follower_text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_ascii_lowercase();
                    let number = text
                        .strip_suffix(" followers")
                        .or_else(|| text.strip_suffix(" follower"))
                        .unwrap_or("");
                    let groups: Vec<_> = number.split(',').collect();
                    n <= 9_007_199_254_740_991
                        && !number.is_empty()
                        && (!number.starts_with('0') || number == "0")
                        && groups.iter().all(|group| {
                            !group.is_empty() && group.bytes().all(|b| b.is_ascii_digit())
                        })
                        && (groups.len() == 1
                            || ((1..=3).contains(&groups[0].len())
                                && groups[1..].iter().all(|group| group.len() == 3)))
                        && number.replace(',', "").parse::<u64>() == Ok(n)
                })
        })
}

fn valid_reddit_profile(value: &Value) -> bool {
    let Some(username) = value.get("username").and_then(Value::as_str) else {
        return false;
    };
    value.as_object().is_some_and(|row| row.len() == 7)
        && value.get("adapter_version").and_then(Value::as_str) == Some("1.0.0")
        && value.get("evidence_source").and_then(Value::as_str) == Some("native_profile_response")
        && value
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| id.strip_prefix("t2_"))
            .is_some_and(|id| {
                (1..=15).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            })
        && (3..=20).contains(&username.len())
        && username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && value.get("profile_url").and_then(Value::as_str)
            == Some(format!("https://www.reddit.com/user/{username}/").as_str())
        && value
            .get("biography")
            .and_then(Value::as_str)
            .is_some_and(|s| s.chars().count() <= 2000 && !s.contains('\0'))
        && value
            .get("follower_count")
            .is_some_and(|n| n.is_null() || n.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991))
}

impl CreatorRuntime {
    pub fn claim_discovery_profile(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryProfileClaim, String> {
        self.repository
            .claim_discovery_profile(campaign, revision, hit)
    }

    pub async fn claim_discovery_profile_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoveryProfileClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_profile(&campaign, revision, &hit)
        })
        .await
    }

    pub fn finish_discovery_profile(
        &self,
        input: FinishDiscoveryProfileInput,
    ) -> Result<DiscoveryProfileRead, String> {
        validate_profile_result(&input)?;
        self.repository.finish_discovery_profile(input)
    }

    pub async fn finish_discovery_profile_async(
        &self,
        input: FinishDiscoveryProfileInput,
    ) -> Result<DiscoveryProfileRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let claim = input.claim.clone();
            match owner.finish_discovery_profile(input) {
                Ok(read) => Ok(read),
                // Same reservation, no provider replay. The owner still rejects
                // stale claims and changed authorization on the failure receipt.
                Err(original) => {
                    let prior =
                        owner.get_discovery_profile(&claim.campaign_id, &claim.search_hit_id)?;
                    if prior.claim != claim || prior.status != "reading" {
                        return Err(original);
                    }
                    owner
                        .finish_discovery_profile(FinishDiscoveryProfileInput {
                            claim,
                            native_profile: None,
                            stop_code: Some("opencli_invalid_output".into()),
                        })
                        .map_err(|_| original)
                }
            }
        })
        .await
    }

    pub fn get_discovery_profile(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<DiscoveryProfileRead, String> {
        self.repository.get_discovery_profile(campaign, hit)
    }

    pub async fn get_discovery_profile_async(
        &self,
        campaign: String,
        hit: String,
    ) -> Result<DiscoveryProfileRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_discovery_profile(&campaign, &hit))
            .await
    }
}

#[cfg(test)]
mod instagram_public_contact_tests {
    use super::*;
    #[test]
    fn instagram_optional_contacts_are_safe_and_old_receipts_remain_valid() {
        let original = serde_json::json!({"adapter_version":"1.0.0","id":"12345","username":"creator","display_name":"Creator","biography":"Original bio","follower_count":null,"private_account":false,"verified":false,"profile_url":"https://www.instagram.com/creator/","evidence_source":"native_profile_response"});
        assert!(valid_instagram_profile(&original));
        let mut with_contacts = original.clone();
        with_contacts["public_email"] = serde_json::json!("business@example.com");
        with_contacts["contact_links"] = serde_json::json!(["https://creator.example/contact"]);
        assert!(valid_instagram_profile(&with_contacts));
        for link in [
            "javascript:alert(1)",
            "https://user:password@creator.example/",
            "https://creator.example/\n",
        ] {
            let mut unsafe_profile = with_contacts.clone();
            unsafe_profile["contact_links"] = serde_json::json!([link]);
            assert!(!valid_instagram_profile(&unsafe_profile));
        }
        let mut wrong_author = with_contacts;
        wrong_author["username"] = serde_json::json!("other");
        assert!(!valid_instagram_profile(&wrong_author));
    }
}

#[cfg(test)]
mod facebook_contact_navigation_tests {
    use super::*;
    #[test]
    fn facebook_only_same_author_contact_tab_is_valid() {
        let mut profile = serde_json::json!({"adapter_version":"1.0.0","id":"12345","display_name":"Creator","follower_count":null,"follower_text":"2K followers","profile_url":"https://www.facebook.com/profile.php?id=12345","evidence_source":"visible_numeric_profile_header"});
        assert!(valid_facebook_profile(&profile));
        profile["contact_links"] = serde_json::json!([
            "https://www.facebook.com/profile.php?id=12345&sk=about_contact_and_basic_info"
        ]);
        assert!(valid_facebook_profile(&profile));
        for link in [
            "https://www.facebook.com/profile.php?id=999&sk=about_contact_and_basic_info",
            "https://www.facebook.com/profile.php?id=12345&sk=timeline",
            "https://evil.example/contact",
        ] {
            profile["contact_links"] = serde_json::json!([link]);
            assert!(!valid_facebook_profile(&profile));
        }
    }
}
