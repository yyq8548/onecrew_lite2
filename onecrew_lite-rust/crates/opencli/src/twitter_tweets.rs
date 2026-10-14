//! Bounded recent X posts for the exact handle observed in a Campaign search.
//! Post text and media flags are review clues, not proof of a product demo.
use serde::Serialize;
use serde_json::Value;

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, process::run, validate_profile};

const LIMIT: usize = 3;
const MAX_TEXT_BYTES: usize = 2_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TwitterRecentPost {
    pub id: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
    pub created_at: String,
    pub has_media: bool,
    pub quotes_another_post: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TwitterRecentContent {
    pub author_id: String,
    pub profile_url: String,
    pub sampled_posts: usize,
    pub excluded_retweets: usize,
    pub original_posts: Vec<TwitterRecentPost>,
    pub identity_kind: &'static str,
}

impl OpenCli {
    pub async fn twitter_recent_content(
        &self,
        profile: &str,
        expected_handle: &str,
    ) -> Result<TwitterRecentContent, OpenCliError> {
        if !super::twitter_profile::valid_handle(expected_handle)
            || expected_handle != expected_handle.to_ascii_lowercase()
        {
            return Err(OpenCliError::InvalidInput("author_handle"));
        }
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        self.require_browser_read(profile, "twitter", "tweets")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "twitter",
                "tweets",
                "-f",
                "json",
                "--limit",
                "3",
                "--",
                expected_handle,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        parse(&output, expected_handle)
    }
}

fn parse(output: &str, expected_handle: &str) -> Result<TwitterRecentContent, OpenCliError> {
    if output.len() > 160_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let rows: Vec<Value> = serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.is_empty() || rows.len() > LIMIT {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut original_posts = Vec::new();
    let mut excluded_retweets = 0;
    let mut seen = std::collections::HashSet::new();
    for row in &rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or(OpenCliError::InvalidOutput)?;
        let author = row
            .get("author")
            .and_then(Value::as_str)
            .ok_or(OpenCliError::InvalidOutput)?;
        let url = row
            .get("url")
            .and_then(Value::as_str)
            .ok_or(OpenCliError::InvalidOutput)?;
        let is_retweet = row
            .get("is_retweet")
            .and_then(Value::as_bool)
            .ok_or(OpenCliError::InvalidOutput)?;
        let has_media = row
            .get("has_media")
            .and_then(Value::as_bool)
            .ok_or(OpenCliError::InvalidOutput)?;
        let created_at = row
            .get("created_at")
            .and_then(Value::as_str)
            .ok_or(OpenCliError::InvalidOutput)?;
        let text = row
            .get("text")
            .and_then(Value::as_str)
            .ok_or(OpenCliError::InvalidOutput)?;
        if !super::twitter_profile::valid_handle(author)
            || id.is_empty()
            || id.len() > 20
            || !id.bytes().all(|b| b.is_ascii_digit())
            || !seen.insert(id)
            || !url.eq_ignore_ascii_case(&format!("https://x.com/{author}/status/{id}"))
            || created_at.len() > 80
            || created_at.chars().any(char::is_control)
            || text.len() > 40_000
            || text.contains('\0')
        {
            return Err(OpenCliError::InvalidOutput);
        }
        // OpenCLI walks nested quoted tweets too. Their canonical URLs are
        // valid locators, but they are not this author's original content.
        if !author.eq_ignore_ascii_case(expected_handle) {
            continue;
        }
        if is_retweet {
            excluded_retweets += 1;
            continue;
        }
        let mut end = text.len().min(MAX_TEXT_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        original_posts.push(TwitterRecentPost {
            id: id.into(),
            source_url: format!("https://x.com/{expected_handle}/status/{id}"),
            text: text[..end].into(),
            text_truncated: end < text.len(),
            created_at: created_at.into(),
            has_media,
            quotes_another_post: row
                .get("quoted_tweet")
                .is_some_and(|value| !value.is_null()),
        });
    }
    Ok(TwitterRecentContent {
        author_id: expected_handle.into(),
        profile_url: format!("https://x.com/{expected_handle}"),
        sampled_posts: original_posts.len() + excluded_retweets,
        excluded_retweets,
        original_posts,
        identity_kind: "mutable_handle",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        json!([
            {"id":"123456789","author":"Creator_1","url":"https://x.com/Creator_1/status/123456789",
             "is_retweet":false,"has_media":true,"quoted_tweet":null,"created_at":"Fri Sep 25 23:00:10 +0000 2026","text":"Watch this demo"},
            {"id":"123456790","author":"Creator_1","url":"https://x.com/Creator_1/status/123456790",
             "is_retweet":true,"has_media":false,"quoted_tweet":null,"created_at":"Fri Sep 25 22:00:10 +0000 2026","text":"RT @someone: demo"}
        ])
    }

    #[test]
    fn keeps_only_bounded_author_posts_and_excludes_retweets() {
        let result = parse(&fixture().to_string(), "creator_1").unwrap();
        assert_eq!(result.sampled_posts, 2);
        assert_eq!(result.excluded_retweets, 1);
        assert_eq!(result.original_posts.len(), 1);
        assert_eq!(
            result.original_posts[0].source_url,
            "https://x.com/creator_1/status/123456789"
        );
        let mut rows = fixture();
        rows[0]["text"] = json!("中".repeat(1_000));
        assert!(
            parse(&rows.to_string(), "creator_1")
                .unwrap()
                .original_posts[0]
                .text_truncated
        );
    }

    #[test]
    fn nested_foreign_posts_are_excluded_without_discarding_bound_originals() {
        let mut rows = fixture().as_array().unwrap().clone();
        rows.push(json!({"id":"987654321","author":"Other","url":"https://x.com/Other/status/987654321",
            "is_retweet":false,"has_media":false,"created_at":"Fri Sep 25 22:00:10 +0000 2026","text":"Foreign quoted text"}));
        let result = parse(&serde_json::to_string(&rows).unwrap(), "creator_1").unwrap();
        assert_eq!(result.sampled_posts, 2);
        assert_eq!(result.original_posts.len(), 1);
        assert!(
            result
                .original_posts
                .iter()
                .all(|p| p.source_url.starts_with("https://x.com/creator_1/status/"))
        );
        rows[2]["url"] = json!("https://x.com/creator_1/status/987654321");
        assert!(parse(&serde_json::to_string(&rows).unwrap(), "creator_1").is_err());
    }
    #[test]
    fn rejects_mismatched_authors_urls_duplicate_ids_and_missing_flags() {
        for (field, value) in [
            ("author", json!("another")),
            ("url", json!("https://evil.example/status/123456789")),
            ("id", json!("123456790")),
            ("is_retweet", Value::Null),
            ("has_media", Value::Null),
        ] {
            let mut rows = fixture();
            rows[0][field] = value;
            assert_eq!(
                parse(&rows.to_string(), "creator_1"),
                Err(OpenCliError::InvalidOutput)
            );
        }
        assert_eq!(parse("[]", "creator_1"), Err(OpenCliError::InvalidOutput));
    }

    #[tokio::test]
    #[ignore = "one public X timeline read; requires OPENCLI_TEST_PROFILE and OPENCLI_TEST_TWITTER_HANDLE"]
    async fn observed_handle_has_bounded_recent_posts_live() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
        let handle = std::env::var("OPENCLI_TEST_TWITTER_HANDLE")
            .unwrap()
            .to_ascii_lowercase();
        let cli = OpenCli::inspect_system().await.unwrap();
        let result = cli.twitter_recent_content(&profile, &handle).await.unwrap();
        assert_eq!(result.author_id, handle);
        assert!(result.sampled_posts <= LIMIT);
        assert_eq!(
            result.original_posts.len() + result.excluded_retweets,
            result.sampled_posts
        );
    }
}
