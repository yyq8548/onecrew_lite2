use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, process::run, validate_profile};

const ENTRY: &[u8] = include_bytes!("../../../integrations/opencli/onecrew-tiktok/search.js");
const CORE: &[u8] = include_bytes!("../../../integrations/opencli/onecrew-tiktok/search.mjs");

/// Native video observations, not a Creator or an eligibility verdict.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TiktokSearchVideo {
    pub adapter_version: String,
    pub id: String,
    pub author_id: String,
    pub author: String,
    pub desc: String,
    pub url: String,
}

impl OpenCli {
    pub async fn check_tiktok_search_profile(&self, profile: &str) -> Result<(), OpenCliError> {
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        require_installed_adapter().await?;
        self.require_browser_read(profile, "onecrew-tiktok", "search")
            .await
    }

    /// One fixed-origin read through the official daemon and extension. No
    /// official-adapter fallback or retry; the installed add-on must match.
    pub async fn tiktok_search(
        &self,
        profile: &str,
        query: &str,
        limit: u8,
    ) -> Result<Vec<TiktokSearchVideo>, OpenCliError> {
        if query.trim().is_empty()
            || query.chars().count() > 160
            || query.chars().any(char::is_control)
        {
            return Err(OpenCliError::InvalidInput("query"));
        }
        Self::check_creator_search_limit("tiktok", limit)?;
        self.check_tiktok_search_profile(profile).await?;
        let limit_text = limit.to_string();
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-tiktok",
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
        require_installed_adapter().await?;
        parse(&output, limit)
    }
}

async fn require_installed_adapter() -> Result<(), OpenCliError> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or(OpenCliError::ReadCommandUnavailable)?;
    let directory = PathBuf::from(home).join(".opencli/clis/onecrew-tiktok");
    check_adapter_at(&directory).await
}

async fn check_adapter_at(directory: &Path) -> Result<(), OpenCliError> {
    for (name, expected) in [("search.js", ENTRY), ("search.mjs", CORE)] {
        let file = tokio::fs::File::open(directory.join(name))
            .await
            .map_err(|_| OpenCliError::ReadCommandUnavailable)?;
        let mut bytes = Vec::new();
        file.take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| OpenCliError::ReadCommandUnavailable)?;
        if bytes != expected {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
    }
    Ok(())
}

fn parse(output: &str, limit: u8) -> Result<Vec<TiktokSearchVideo>, OpenCliError> {
    let rows: Vec<TiktokSearchVideo> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    let mut seen = std::collections::HashSet::new();
    let id_valid = |id: &str| {
        !id.starts_with('0')
            && (1..=25).contains(&id.len())
            && id.bytes().all(|b| b.is_ascii_digit())
    };
    if rows.len() > usize::from(limit)
        || rows.iter().any(|row| {
            row.adapter_version != "1.0.0"
                || !id_valid(&row.id)
                || !id_valid(&row.author_id)
                || !(1..=24).contains(&row.author.len())
                || !row
                    .author
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
                || row.url != format!("https://www.tiktok.com/@{}/video/{}", row.author, row.id)
                || row.desc.trim().is_empty()
                || row.desc.len() > 16000
                || row
                    .desc
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                || !seen.insert(row.id.clone())
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tiktok_adapter_requires_exact_installed_files_without_overwriting_them() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            check_adapter_at(directory.path()).await,
            Err(OpenCliError::ReadCommandUnavailable)
        );
        for (name, bytes) in [("search.js", ENTRY), ("search.mjs", CORE)] {
            std::fs::write(directory.path().join(name), bytes).unwrap();
        }
        assert_eq!(check_adapter_at(directory.path()).await, Ok(()));
        let changed = [CORE, b"\n"].concat();
        std::fs::write(directory.path().join("search.mjs"), &changed).unwrap();
        assert_eq!(
            check_adapter_at(directory.path()).await,
            Err(OpenCliError::ReadCommandUnavailable)
        );
        assert_eq!(
            std::fs::read(directory.path().join("search.mjs")).unwrap(),
            changed
        );
    }

    fn fixture() -> serde_json::Value {
        serde_json::json!([{"adapter_version":"1.0.0","id":"1234567890123456789","author_id":"9876543210123456789",
            "author":"shop.creator","desc":"Ecommerce walkthrough\nOriginal content",
            "url":"https://www.tiktok.com/@shop.creator/video/1234567890123456789"}])
    }

    #[test]
    fn tiktok_native_search_keeps_ids_and_checks_source_binding() {
        let rows = parse(&fixture().to_string(), 3).unwrap();
        assert_eq!(rows[0].author_id, "9876543210123456789");
        for (field, value) in [
            ("id", "-1"),
            ("author_id", ""),
            ("author", "../other"),
            ("url", "https://evil.example/video/1234567890123456789"),
            ("adapter_version", "2.0.0"),
        ] {
            let mut invalid = fixture();
            invalid[0][field] = value.into();
            assert_eq!(
                parse(&invalid.to_string(), 3),
                Err(OpenCliError::InvalidOutput)
            );
        }
        let mut oversized = fixture();
        oversized[0]["desc"] = "字".repeat(5334).into();
        assert!(parse(&oversized.to_string(), 3).is_err());
        assert!(parse(&fixture().to_string(), 0).is_err());
        assert!(
            parse(
                &serde_json::json!([fixture()[0], fixture()[0]]).to_string(),
                3
            )
            .is_err()
        );
        assert_eq!(parse("[]", 3), Ok(vec![]));
    }

    #[tokio::test]
    #[ignore = "requires installed OneCrew TikTok adapter and OPENCLI_TEST_PROFILE"]
    async fn tiktok_search_contract_matches_connected_extension() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated profile");
        let cli = OpenCli::inspect_system().await.unwrap();
        let rows = cli.tiktok_search(&profile, "ecommerce", 3).await.unwrap();
        assert!(!rows.is_empty());
        println!(
            "tiktok source-bound videos={} first_video={} first_author_id={}",
            rows.len(),
            rows[0].id,
            rows[0].author_id
        );
    }
}
