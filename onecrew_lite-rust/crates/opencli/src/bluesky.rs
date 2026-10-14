//! Bounded public native Bluesky evidence. No handle-to-DID guesses.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlueskyPublicProfile {
    pub did: String,
    pub handle: String,
    pub display_name: String,
    pub biography: String,
    pub follower_count: Option<u64>,
    pub profile_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlueskyOriginalComment {
    pub post_uri: String,
    pub source_url: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlueskyOriginalPost {
    pub author_id: String,
    pub author_handle: String,
    pub post_uri: String,
    pub source_url: String,
    pub text: String,
    pub public_links: Vec<String>,
    pub comments: Vec<BlueskyOriginalComment>,
}

pub(crate) fn valid_did(did: &str) -> bool {
    if did.len() > 256 {
        return false;
    }
    if let Some(id) = did.strip_prefix("did:plc:") {
        return id.len() == 24
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b));
    }
    did.strip_prefix("did:web:").is_some_and(|id| {
        !id.is_empty()
            && id.split(':').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            })
    })
}

fn valid_handle(handle: &str) -> bool {
    handle.len() <= 253
        && handle.contains('.')
        && handle.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

pub(crate) fn post_target(uri: &str) -> Option<(&str, String)> {
    let (did, key) = uri
        .strip_prefix("at://")?
        .split_once("/app.bsky.feed.post/")?;
    (valid_did(did)
        && (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    .then(|| (did, format!("https://bsky.app/profile/{did}/post/{key}")))
}

pub(crate) fn uri_from_source(source: &str) -> Option<String> {
    let (did, key) = source
        .strip_prefix("https://bsky.app/profile/")?
        .split_once("/post/")?;
    let uri = format!("at://{did}/app.bsky.feed.post/{key}");
    post_target(&uri)
        .filter(|(_, url)| url == source)
        .map(|_| uri.clone())
}

fn bounded_text(text: &str, limit: usize, required: bool) -> bool {
    text.len() <= limit && !text.contains('\0') && (!required || !text.trim().is_empty())
}

fn one<T: DeserializeOwned>(output: &str, required: &[&str]) -> Result<T, OpenCliError> {
    if output.len() > 32000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    let [row] = rows.as_slice() else {
        return Err(OpenCliError::InvalidOutput);
    };
    if !row
        .as_object()
        .is_some_and(|v| required.iter().all(|key| v.contains_key(*key)))
    {
        return Err(OpenCliError::InvalidOutput);
    }
    serde_json::from_value(row.clone()).map_err(|_| OpenCliError::InvalidOutput)
}

fn parse_profile(output: &str, did: &str) -> Result<BlueskyPublicProfile, OpenCliError> {
    let row: BlueskyPublicProfile = one(output, &["follower_count"])?;
    if !valid_did(did)
        || row.did != did
        || !valid_handle(&row.handle)
        || row.profile_url != format!("https://bsky.app/profile/{did}")
        || !bounded_text(&row.display_name, 800, true)
        || !bounded_text(&row.biography, 8000, false)
        || row
            .follower_count
            .is_some_and(|n| n > 9_007_199_254_740_991)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

fn parse_post(
    output: &str,
    uri: &str,
    comments: bool,
) -> Result<BlueskyOriginalPost, OpenCliError> {
    let (did, source) = post_target(uri).ok_or(OpenCliError::InvalidOutput)?;
    let row: BlueskyOriginalPost = one(output, &[])?;
    if row.post_uri != uri
        || row.author_id != did
        || row.source_url != source
        || !valid_handle(&row.author_handle)
        || !bounded_text(&row.text, 16000, true)
        || row.public_links.len() > 10
        || row.public_links.iter().any(|link| {
            if let Some(email) = link.strip_prefix("mailto:") {
                return email.len() > 2041
                    || email.contains(['?', '#', '%', '/', '\0', ' ', '\n', '\r'])
                    || !email.contains('@');
            }
            crate::validate_public_web_url(link).is_err()
        })
        || row.comments.len() > 5
        || (!comments && !row.comments.is_empty())
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut seen = std::collections::HashSet::new();
    for comment in &row.comments {
        if !post_target(&comment.post_uri).is_some_and(|(_, source)| source == comment.source_url)
            || comment.post_uri == uri
            || !seen.insert(&comment.post_uri)
            || !bounded_text(&comment.text, 2000, true)
        {
            return Err(OpenCliError::InvalidOutput);
        }
    }
    Ok(row)
}

impl OpenCli {
    async fn bluesky_read(
        &self,
        profile: &str,
        command: &str,
        target: &str,
    ) -> Result<String, OpenCliError> {
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-bluesky").await?;
        if !self.registry.has_public_read("onecrew-bluesky", command) {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-bluesky",
                command,
                "-f",
                "json",
                "--",
                target,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-bluesky").await?;
        Ok(output)
    }

    pub async fn bluesky_public_profile(
        &self,
        profile: &str,
        did: &str,
    ) -> Result<BlueskyPublicProfile, OpenCliError> {
        if !valid_did(did) {
            return Err(OpenCliError::InvalidInput("bluesky_did"));
        }
        parse_profile(&self.bluesky_read(profile, "profile", did).await?, did)
    }

    pub async fn bluesky_original_post(
        &self,
        profile: &str,
        uri: &str,
    ) -> Result<BlueskyOriginalPost, OpenCliError> {
        if post_target(uri).is_none() {
            return Err(OpenCliError::InvalidInput("bluesky_post_uri"));
        }
        parse_post(&self.bluesky_read(profile, "post", uri).await?, uri, false)
    }

    pub async fn bluesky_post_evidence(
        &self,
        profile: &str,
        source: &str,
    ) -> Result<BlueskyOriginalPost, OpenCliError> {
        let uri =
            uri_from_source(source).ok_or(OpenCliError::InvalidInput("bluesky_post_source"))?;
        self.bluesky_original_post(profile, &uri).await
    }

    pub async fn bluesky_original_comments(
        &self,
        profile: &str,
        uri: &str,
    ) -> Result<BlueskyOriginalPost, OpenCliError> {
        if post_target(uri).is_none() {
            return Err(OpenCliError::InvalidInput("bluesky_post_uri"));
        }
        parse_post(
            &self.bluesky_read(profile, "comments", uri).await?,
            uri,
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DID: &str = "did:plc:abcdefghijklmnopqrstuvwx";
    #[test]
    fn bluesky_native_profile_keeps_contacts_unknown_count_and_rejects_mismatch() {
        let mut row = serde_json::json!({"did":DID,"handle":"author.bsky.social","display_name":"Author", "biography":"Business: studio@example.com TG: keyboard_studio","follower_count":null,"profile_url":format!("https://bsky.app/profile/{DID}")});
        let parsed = parse_profile(&serde_json::json!([row]).to_string(), DID).unwrap();
        assert_eq!(parsed.follower_count, None);
        assert!(parsed.biography.contains("studio@example.com"));
        row["did"] = serde_json::json!("did:plc:zzzzzzzzzzzzzzzzzzzzzzzz");
        assert!(parse_profile(&serde_json::json!([row]).to_string(), DID).is_err());
    }
    #[test]
    fn bluesky_native_post_comments_are_bounded_and_exactly_source_bound() {
        let uri = format!("at://{DID}/app.bsky.feed.post/first");
        let mut row = serde_json::json!({"author_id":DID,"author_handle":"author.bsky.social","post_uri":uri,"source_url":format!("https://bsky.app/profile/{DID}/post/first"),"text":"Native original text","public_links":["https://creator.example/contact"],"comments":[]});
        assert!(parse_post(&serde_json::json!([row]).to_string(), &uri, false).is_ok());
        let comment = serde_json::json!({"post_uri":format!("at://{DID}/app.bsky.feed.post/reply"),"source_url":format!("https://bsky.app/profile/{DID}/post/reply"),"text":"原始评论"});
        row["comments"] = serde_json::json!([comment]);
        assert!(parse_post(&serde_json::json!([row]).to_string(), &uri, true).is_ok());
        assert!(parse_post(&serde_json::json!([row]).to_string(), &uri, false).is_err());
        row["comments"] = serde_json::json!([comment, comment]);
        assert!(parse_post(&serde_json::json!([row]).to_string(), &uri, true).is_err());
        row["comments"] = serde_json::json!([]);
        row["author_id"] = serde_json::json!("did:plc:zzzzzzzzzzzzzzzzzzzzzzzz");
        assert!(parse_post(&serde_json::json!([row]).to_string(), &uri, true).is_err());
    }
    #[test]
    fn bluesky_did_and_native_uri_reject_redirects_and_handles_as_stable_identity() {
        for bad in [
            "author.bsky.social",
            "https://other.example/",
            "did:plc:123",
            "did:web:host/path",
        ] {
            assert!(!valid_did(bad));
        }
        for bad in [
            "https://bsky.app/profile/author",
            "at://author.bsky.social/app.bsky.feed.post/id",
            "at://did:web:host/app.bsky.feed.post/id?redirect=1",
        ] {
            assert!(post_target(bad).is_none());
        }
    }
}
