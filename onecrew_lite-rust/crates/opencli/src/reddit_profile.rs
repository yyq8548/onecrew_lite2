use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RedditPublicProfile {
    pub adapter_version: String,
    pub id: String,
    pub username: String,
    pub biography: String,
    pub follower_count: Option<u64>,
    pub profile_url: String,
    pub evidence_source: String,
}

pub(crate) fn check_target(username: &str, author_id: &str) -> Result<(), OpenCliError> {
    if !(3..=20).contains(&username.len())
        || !username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        || !author_id.strip_prefix("t2_").is_some_and(|id| {
            (1..=15).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
    {
        return Err(OpenCliError::InvalidInput("reddit_profile_target"));
    }
    Ok(())
}

impl OpenCli {
    async fn reddit_author_read(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
        command: &str,
    ) -> Result<String, OpenCliError> {
        validate_profile(profile)?;
        check_target(username, author_id)?;
        if !matches!(command, "read" | "recent") {
            return Err(OpenCliError::InvalidInput("reddit_profile_target"));
        }
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-reddit-profile", command)
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-reddit-profile",
                command,
                "-f",
                "json",
                "--",
                username,
                author_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        if output.len() > 16000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        Ok(output)
    }

    pub async fn reddit_recent_language_urls(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
    ) -> Result<Vec<String>, OpenCliError> {
        let output = self
            .reddit_author_read(profile, username, author_id, "recent")
            .await?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Locator {
            source_url: String,
        }
        let rows: Vec<Locator> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.len() > 3 {
            return Err(OpenCliError::InvalidOutput);
        }
        let mut urls = Vec::new();
        for row in rows {
            crate::reddit_language::post_id(&row.source_url)
                .map_err(|_| OpenCliError::InvalidOutput)?;
            if urls.contains(&row.source_url) {
                return Err(OpenCliError::InvalidOutput);
            }
            urls.push(row.source_url);
        }
        Ok(urls)
    }

    pub async fn reddit_public_profile(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
    ) -> Result<RedditPublicProfile, OpenCliError> {
        let output = self
            .reddit_author_read(profile, username, author_id, "read")
            .await?;
        let mut rows: Vec<RedditPublicProfile> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        let row = rows.remove(0);
        if row.adapter_version != "1.0.0"
            || row.id != author_id
            || !row.username.eq_ignore_ascii_case(username)
            || row.profile_url != format!("https://www.reddit.com/user/{}/", row.username)
            || row.evidence_source != "native_profile_response"
            || row.biography.chars().count() > 2000
            || row.biography.contains('\0')
            || row
                .follower_count
                .is_some_and(|n| n > 9_007_199_254_740_991)
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(row)
    }
}
