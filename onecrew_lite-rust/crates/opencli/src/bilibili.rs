use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

/// Observations from Bilibili video-search cards, not qualified Creator records.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BilibiliSearchVideo {
    pub adapter_version: String,
    pub bvid: String,
    pub author_id: String,
    pub author: String,
    pub title: String,
    pub url: String,
    pub author_url: String,
}

impl OpenCli {
    pub async fn check_bilibili_search_profile(&self, profile: &str) -> Result<(), OpenCliError> {
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        addons::require_ready("onecrew-bilibili").await?;
        self.require_browser_read(profile, "onecrew-bilibili", "search")
            .await
    }

    /// One bounded public search-page read. No API fallback, pagination or retry.
    pub async fn bilibili_search(
        &self,
        profile: &str,
        query: &str,
        limit: u8,
    ) -> Result<Vec<BilibiliSearchVideo>, OpenCliError> {
        if query.trim().is_empty()
            || query.chars().count() > 160
            || query.chars().any(char::is_control)
        {
            return Err(OpenCliError::InvalidInput("query"));
        }
        Self::check_creator_search_limit("bilibili", limit)?;
        self.check_bilibili_search_profile(profile).await?;
        let limit_text = limit.to_string();
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-bilibili",
                "search",
                "--limit",
                &limit_text,
                "-f",
                "json",
                "--",
                query,
            ],
            READ_TIMEOUT,
        )
        .await?;
        addons::require_ready("onecrew-bilibili").await?;
        parse(&output, limit)
    }
}

fn parse(output: &str, limit: u8) -> Result<Vec<BilibiliSearchVideo>, OpenCliError> {
    let rows: Vec<BilibiliSearchVideo> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    let mut seen = std::collections::HashSet::new();
    let text_valid = |text: &str, maximum: usize| {
        !text.trim().is_empty()
            && text.chars().count() <= maximum
            && !text.chars().any(char::is_control)
    };
    if rows.is_empty()
        || rows.len() > usize::from(limit)
        || rows.iter().any(|row| {
            row.adapter_version != "1.0.0"
                || row.bvid.len() != 12
                || !row.bvid.starts_with("BV")
                || !row.bvid.bytes().all(|b| b.is_ascii_alphanumeric())
                || !(1..=20).contains(&row.author_id.len())
                || row.author_id.starts_with('0')
                || !row.author_id.bytes().all(|b| b.is_ascii_digit())
                || !text_valid(&row.title, 500)
                || !text_valid(&row.author, 200)
                || row.url != format!("https://www.bilibili.com/video/{}", row.bvid)
                || row.author_url != format!("https://space.bilibili.com/{}", row.author_id)
                || !seen.insert(row.bvid.clone())
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::json!([{"adapter_version":"1.0.0","bvid":"BV1xx411c7mD","author_id":"12345",
            "author":"Example creator","title":"跨境电商教程","url":"https://www.bilibili.com/video/BV1xx411c7mD",
            "author_url":"https://space.bilibili.com/12345"}])
    }

    #[test]
    fn native_cards_require_exact_video_and_author_sources() {
        assert_eq!(
            parse(&fixture().to_string(), 5).unwrap()[0].author_id,
            "12345"
        );
        for (field, value) in [
            ("bvid", "av1234"),
            ("author_id", "0"),
            ("author", "bad\nname"),
            ("title", ""),
            ("url", "https://evil.example/video/BV1xx411c7mD"),
            ("author_url", "https://space.bilibili.com/54321"),
            ("adapter_version", "2.0.0"),
        ] {
            let mut invalid = fixture();
            invalid[0][field] = value.into();
            assert_eq!(
                parse(&invalid.to_string(), 5),
                Err(OpenCliError::InvalidOutput)
            );
        }
        for invalid in ["[]", "{}", "null"] {
            assert_eq!(parse(invalid, 5), Err(OpenCliError::InvalidOutput));
        }
        assert!(parse(&fixture().to_string(), 0).is_err());
        assert!(
            parse(
                &serde_json::json!([fixture()[0], fixture()[0]]).to_string(),
                5
            )
            .is_err()
        );
        let mut invalid = fixture();
        invalid[0]["followers"] = 10000.into();
        assert!(parse(&invalid.to_string(), 5).is_err());
        invalid = fixture();
        invalid[0]["title"] = "字".repeat(501).into();
        assert!(parse(&invalid.to_string(), 5).is_err());
    }

    #[tokio::test]
    #[ignore = "requires installed OneCrew Bilibili adapter and OPENCLI_TEST_PROFILE"]
    async fn bilibili_search_contract_matches_connected_extension() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated profile");
        let cli = OpenCli::inspect_system().await.unwrap();
        let rows = cli.bilibili_search(&profile, "跨境电商", 5).await.unwrap();
        assert!(!rows.is_empty());
        let authors = rows
            .iter()
            .map(|row| &row.author_id)
            .collect::<std::collections::HashSet<_>>();
        println!(
            "bilibili source-bound videos={} unique_authors={}",
            rows.len(),
            authors.len()
        );
    }
}
