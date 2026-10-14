//! One focal post's public text, bound to a previously verified native author.
//! URL entities are unverified locators; media flags never certify a demo.
use serde::{Deserialize, Serialize};

use crate::{
    OpenCli, OpenCliError, READ_TIMEOUT, TwitterPostIdentity, addons,
    process::run,
    twitter_post_identity::{numeric_id, valid_identity},
    validate_profile,
};

const ADDON: &str = "onecrew-twitter-post-content";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TwitterPostContent {
    pub adapter_version: String,
    pub identity: TwitterPostIdentity,
    pub text: String,
    pub text_truncated: bool,
    pub text_source: String,
    pub created_at: String,
    pub has_media: bool,
    pub quotes_another_post: bool,
    pub expanded_urls: Vec<String>,
}

impl OpenCli {
    /// Original API text and direct replies; translated DOM text is never returned.
    pub async fn twitter_language_sample(
        &self,
        profile: &str,
        handle: &str,
        post_id: &str,
        account_id: &str,
    ) -> Result<serde_json::Value, OpenCliError> {
        validate_profile(profile)?;
        if !super::twitter_profile::valid_handle(handle)
            || handle != handle.to_ascii_lowercase()
            || !numeric_id(post_id)
            || !numeric_id(account_id)
        {
            return Err(OpenCliError::InvalidInput("language_sample_target"));
        }
        self.require_version().await?;
        addons::require_ready(ADDON).await?;
        addons::require_ready("onecrew-twitter-post-identity").await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, ADDON, "read").await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                ADDON,
                "read",
                "-f",
                "json",
                "--language-sample",
                "--",
                handle,
                post_id,
                account_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        if output.len() > 64_000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        addons::require_ready(ADDON).await?;
        addons::require_ready("onecrew-twitter-post-identity").await?;
        addons::require_ready("onecrew-browser-search").await?;
        let mut rows: Vec<serde_json::Value> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        let row = rows.pop().ok_or(OpenCliError::InvalidOutput)?;
        if row["author_id"].as_str() != Some(account_id)
            || row["source_url"].as_str()
                != Some(format!("https://x.com/{handle}/status/{post_id}").as_str())
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(row)
    }

    /// Reads exactly one saved post; the caller supplies its verified native author.
    /// No automatic installation, link following, persistence, or qualification.
    pub async fn twitter_post_content(
        &self,
        profile: &str,
        handle: &str,
        post_id: &str,
        account_id: &str,
    ) -> Result<TwitterPostContent, OpenCliError> {
        if !super::twitter_profile::valid_handle(handle)
            || handle != handle.to_ascii_lowercase()
            || !numeric_id(post_id)
            || !numeric_id(account_id)
        {
            return Err(OpenCliError::InvalidInput("post_content_target"));
        }
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        // Both the new projector and its shared browser transport are byte-pinned.
        addons::require_ready(ADDON).await?;
        addons::require_ready("onecrew-twitter-post-identity").await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, ADDON, "read").await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                ADDON,
                "read",
                "-f",
                "json",
                "--",
                handle,
                post_id,
                account_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready(ADDON).await?;
        addons::require_ready("onecrew-twitter-post-identity").await?;
        addons::require_ready("onecrew-browser-search").await?;
        parse(&output, handle, post_id, account_id)
    }
}

fn parse(
    output: &str,
    handle: &str,
    post_id: &str,
    account_id: &str,
) -> Result<TwitterPostContent, OpenCliError> {
    if output.len() > 64_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<TwitterPostContent> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.pop().ok_or(OpenCliError::InvalidOutput)?;
    let mut urls = std::collections::HashSet::new();
    if row.adapter_version != "1.0.0"
        || !valid_identity(&row.identity, handle, post_id)
        || row.identity.account_id != account_id
        || !matches!(row.text_source.as_str(), "note_tweet" | "legacy_full_text")
        || row.text.trim().is_empty()
        || row.text.len() > 8_000
        || row.text.contains('\0')
        || row.created_at.trim().is_empty()
        || row.created_at.len() > 80
        || row.created_at.chars().any(char::is_control)
        || row.expanded_urls.len() > 20
        || row.expanded_urls.iter().any(|value| {
            value.len() > 2048
                || value.chars().any(|c| c.is_control() || c.is_whitespace())
                || !urls.insert(value)
                || url::Url::parse(value).map_or(true, |url| {
                    !matches!(url.scheme(), "http" | "https")
                        || !url.has_host()
                        || !url.username().is_empty()
                        || url.password().is_some()
                })
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        json!([{"adapter_version":"1.0.0", "identity": {
            "adapter_version":"1.0.0", "post_id":"1234567890123456789",
            "account_id":"987654321012345678", "handle":"creator_1",
            "profile_url":"https://x.com/creator_1",
            "source_url":"https://x.com/creator_1/status/1234567890123456789",
            "identity_kind":"native_post_author_id", "evidence_source":"x_tweet_detail"},
            "text":"Original public post", "text_truncated":false, "text_source":"legacy_full_text",
            "created_at":"Fri Sep 25 23:00:10 +0000 2026", "has_media":true,
            "quotes_another_post":false, "expanded_urls":["https://example.com/tool"]}])
    }

    fn decode(value: &Value) -> Result<TwitterPostContent, OpenCliError> {
        parse(
            &value.to_string(),
            "creator_1",
            "1234567890123456789",
            "987654321012345678",
        )
    }

    #[test]
    fn content_requires_exact_native_post_author_and_bounded_public_fields() {
        let result = decode(&fixture()).unwrap();
        assert_eq!(result.identity.account_id, "987654321012345678");
        for (field, value) in [
            ("adapter_version", json!("2.0.0")),
            ("text", json!("")),
            ("text", json!("中".repeat(2667))),
            ("text", json!("bad\0text")),
            ("text_source", json!("quote")),
            ("created_at", json!("\n")),
            ("text_truncated", Value::Null),
            ("has_media", Value::Null),
            ("expanded_urls", json!(["javascript:alert(1)"])),
            ("expanded_urls", json!(["https://user:secret@example.com/"])),
            (
                "expanded_urls",
                json!(["https://example.com/", "https://example.com/"]),
            ),
            ("qualified", json!(true)),
        ] {
            let mut row = fixture();
            row[0][field] = value;
            assert_eq!(decode(&row), Err(OpenCliError::InvalidOutput), "{field}");
        }
        for (field, value) in [
            ("account_id", "111"),
            ("post_id", "222"),
            ("handle", "other"),
            ("identity_kind", "mutable_handle"),
            ("cookie", "secret"),
        ] {
            let mut row = fixture();
            row[0]["identity"][field] = json!(value);
            assert_eq!(decode(&row), Err(OpenCliError::InvalidOutput), "{field}");
        }
        assert_eq!(decode(&json!([])), Err(OpenCliError::InvalidOutput));
        assert_eq!(
            decode(&json!([fixture()[0], fixture()[0]])),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[tokio::test]
    #[ignore = "one public focal-post content read; requires explicit profile, post and verified native author"]
    async fn observed_native_author_post_content_live() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
        let handle = std::env::var("OPENCLI_TEST_TWITTER_HANDLE").unwrap();
        let post = std::env::var("OPENCLI_TEST_TWITTER_POST").unwrap();
        let account = std::env::var("OPENCLI_TEST_TWITTER_ACCOUNT").unwrap();
        let cli = OpenCli::inspect_system().await.unwrap();
        let result = cli
            .twitter_post_content(&profile, &handle, &post, &account)
            .await
            .unwrap();
        assert_eq!(result.identity.account_id, account);
        println!(
            "native_author_post_content source={} text_bytes={} text_source={} links={} media={} truncated={}; no qualification or Campaign write",
            result.identity.source_url,
            result.text.len(),
            result.text_source,
            result.expanded_urls.len(),
            result.has_media,
            result.text_truncated
        );
    }
}
