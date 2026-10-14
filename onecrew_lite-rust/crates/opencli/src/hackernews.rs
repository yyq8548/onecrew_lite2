//! Native Hacker News identity and evidence; no external article attribution.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const MAX_NATIVE_ID: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HackerNewsPublicProfile {
    pub author_id: String,
    pub username: String,
    pub biography_html: String,
    pub biography: String,
    pub profile_url: String,
    pub created: u64,
    pub karma: u64,
    pub follower_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HackerNewsOriginalComment {
    pub comment_id: String,
    pub parent_item_id: u64,
    pub source_url: String,
    pub text_html: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HackerNewsOriginalItem {
    pub item_id: u64,
    pub author_id: String,
    pub source_url: String,
    pub title: String,
    pub text_html: String,
    pub text: String,
    pub external_url: Option<String>,
    pub comment_ids: Vec<u64>,
    pub comments: Vec<HackerNewsOriginalComment>,
}

pub(crate) fn valid_username(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn text(value: &str, limit: usize) -> bool {
    value.len() <= limit && !value.contains('\0')
}

fn valid_external_observation(value: &str) -> bool {
    // Old HN submissions may point to HTTP. Preserve that observed value;
    // validation here does not fetch it or grant contact-page navigation.
    if let Some(rest) = value.strip_prefix("http://") {
        crate::validate_public_web_url(&format!("https://{rest}")).is_ok()
    } else {
        crate::validate_public_web_url(value).is_ok()
    }
}

fn one<T: DeserializeOwned>(output: &str, required: &[&str]) -> Result<T, OpenCliError> {
    if output.len() > 160_000 {
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

fn parse_profile(output: &str, username: &str) -> Result<HackerNewsPublicProfile, OpenCliError> {
    let row: HackerNewsPublicProfile = one(output, &["follower_count"])?;
    if !valid_username(username)
        || row.author_id != username
        || row.username != username
        || row.profile_url != format!("https://news.ycombinator.com/user?id={username}")
        || row.follower_count.is_some()
        || row.created > MAX_NATIVE_ID
        || row.karma > MAX_NATIVE_ID
        || !text(&row.biography_html, 16000)
        || !text(&row.biography, 8000)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

fn parse_item(
    output: &str,
    id: u64,
    comments: bool,
) -> Result<HackerNewsOriginalItem, OpenCliError> {
    let row: HackerNewsOriginalItem = one(output, &["external_url"])?;
    if id == 0
        || id > MAX_NATIVE_ID
        || row.item_id != id
        || !valid_username(&row.author_id)
        || row.source_url != format!("https://news.ycombinator.com/item?id={id}")
        || !text(&row.title, 2000)
        || row.title.trim().is_empty()
        || !text(&row.text_html, 32000)
        || !text(&row.text, 16000)
        || row
            .external_url
            .as_ref()
            .is_some_and(|url| !valid_external_observation(url))
        || row.comment_ids.len() > 5
        || row
            .comment_ids
            .iter()
            .any(|child| *child == 0 || *child > MAX_NATIVE_ID || *child == id)
        || row
            .comment_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != row.comment_ids.len()
        || (!comments && !row.comments.is_empty())
        || (comments && row.comments.len() != row.comment_ids.len())
    {
        return Err(OpenCliError::InvalidOutput);
    }
    for (index, comment) in row.comments.iter().enumerate() {
        let child = row.comment_ids[index];
        if comment.comment_id != child.to_string()
            || comment.parent_item_id != id
            || comment.source_url != format!("https://news.ycombinator.com/item?id={child}")
            || !text(&comment.text_html, 16000)
            || !text(&comment.text, 2000)
            || comment.text_html.trim().is_empty()
            || comment.text.trim().is_empty()
        {
            return Err(OpenCliError::InvalidOutput);
        }
    }
    Ok(row)
}

impl OpenCli {
    async fn hackernews_read(
        &self,
        profile: &str,
        command: &str,
        target: &str,
    ) -> Result<String, OpenCliError> {
        if crate::creator_search::RETIRED_CREATOR_SOURCES.contains(&"hackernews") {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-hackernews").await?;
        if !self.registry.has_public_read("onecrew-hackernews", command) {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-hackernews",
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
        addons::require_ready("onecrew-hackernews").await?;
        Ok(output)
    }

    pub async fn hackernews_public_profile(
        &self,
        profile: &str,
        username: &str,
    ) -> Result<HackerNewsPublicProfile, OpenCliError> {
        if !valid_username(username) {
            return Err(OpenCliError::InvalidInput("hackernews_username"));
        }
        parse_profile(
            &self.hackernews_read(profile, "profile", username).await?,
            username,
        )
    }

    pub async fn hackernews_original_item(
        &self,
        profile: &str,
        id: u64,
    ) -> Result<HackerNewsOriginalItem, OpenCliError> {
        if id == 0 || id > MAX_NATIVE_ID {
            return Err(OpenCliError::InvalidInput("hackernews_item_id"));
        }
        parse_item(
            &self
                .hackernews_read(profile, "post", &id.to_string())
                .await?,
            id,
            false,
        )
    }

    pub async fn hackernews_original_comments(
        &self,
        profile: &str,
        id: u64,
    ) -> Result<HackerNewsOriginalItem, OpenCliError> {
        if id == 0 || id > MAX_NATIVE_ID {
            return Err(OpenCliError::InvalidInput("hackernews_item_id"));
        }
        parse_item(
            &self
                .hackernews_read(profile, "comments", &id.to_string())
                .await?,
            id,
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_profile_rejects_invented_followers_or_mismatched_username() {
        let mut row = json!({"author_id":"author", "username":"author", "biography_html":"Business: studio@example.org","biography":"Business: studio@example.org", "profile_url":"https://news.ycombinator.com/user?id=author", "created":1,"karma":9000,"follower_count":null});
        assert_eq!(
            parse_profile(&json!([row.clone()]).to_string(), "author")
                .unwrap()
                .follower_count,
            None
        );
        row["follower_count"] = json!(9000);
        assert!(parse_profile(&json!([row.clone()]).to_string(), "author").is_err());
        row["follower_count"] = json!(null);
        assert!(parse_profile(&json!([row]).to_string(), "another").is_err());
    }

    #[test]
    fn comments_require_exact_first_five_ids_and_same_parent() {
        let mut row = json!({"item_id":100,"author_id":"author","source_url":"https://news.ycombinator.com/item?id=100", "title":"Native title","text_html":"Native prose","text":"Native prose", "external_url":null,"comment_ids":[101], "comments":[{"comment_id":"101","parent_item_id":100,"source_url":"https://news.ycombinator.com/item?id=101","text_html":"Original reply","text":"Original reply"}]});
        assert!(parse_item(&json!([row.clone()]).to_string(), 100, true).is_ok());
        row["comments"][0]["parent_item_id"] = json!(99);
        assert!(parse_item(&json!([row.clone()]).to_string(), 100, true).is_err());
        row["comments"][0]["parent_item_id"] = json!(100);
        row["comments"][0]["comment_id"] = json!("102");
        assert!(parse_item(&json!([row]).to_string(), 100, true).is_err());
    }
}
