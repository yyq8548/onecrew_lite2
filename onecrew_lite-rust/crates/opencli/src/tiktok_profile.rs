use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

/// Public profile observations bound to the author seen in a search result.
/// A verified profile is not a customer eligibility verdict.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TiktokPublicProfile {
    pub adapter_version: String,
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub signature: String,
    pub follower_count: u64,
    pub verified: bool,
    pub private_account: bool,
    pub profile_url: String,
    pub evidence_source: String,
}

pub fn check_tiktok_profile_target(username: &str, author_id: &str) -> Result<(), OpenCliError> {
    if !(1..=24).contains(&username.len())
        || !username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
        || !(1..=25).contains(&author_id.len())
        || author_id.starts_with('0')
        || !author_id.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(OpenCliError::InvalidInput("tiktok_profile_target"));
    }
    Ok(())
}

impl OpenCli {
    /// One exact profile read, using the stable author ID obtained from search.
    /// Never falls back to a username-only response or substitutes missing counts.
    pub async fn tiktok_public_profile(
        &self,
        profile: &str,
        username: &str,
        expected_author_id: &str,
    ) -> Result<TiktokPublicProfile, OpenCliError> {
        check_tiktok_profile_target(username, expected_author_id)?;
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        addons::require_ready("onecrew-tiktok-profile").await?;
        self.require_browser_read(profile, "onecrew-tiktok-profile", "read")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-tiktok-profile",
                "read",
                "--author-id",
                expected_author_id,
                "-f",
                "json",
                "--",
                username,
            ],
            READ_TIMEOUT,
        )
        .await?;
        addons::require_ready("onecrew-tiktok-profile").await?;
        parse(&output, username, expected_author_id)
    }
}

fn parse(
    output: &str,
    username: &str,
    expected_id: &str,
) -> Result<TiktokPublicProfile, OpenCliError> {
    let mut rows: Vec<TiktokPublicProfile> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.pop().ok_or(OpenCliError::InvalidOutput)?;
    let text_valid = |text: &str, maximum| {
        text.len() <= maximum
            && !text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    };
    if row.adapter_version != "1.0.0"
        || row.evidence_source != "profile_bootstrap"
        || row.id != expected_id
        || row.username != username
        || row.profile_url != format!("https://www.tiktok.com/@{username}")
        || row.follower_count > 9_007_199_254_740_991
        || row.display_name.trim().is_empty()
        || !text_valid(&row.display_name, 1000)
        || !text_valid(&row.signature, 16000)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    const USERNAME: &str = "sample.creator";
    const AUTHOR_ID: &str = "1234567890123456789";

    fn fixture() -> serde_json::Value {
        serde_json::json!([{"adapter_version":"1.0.0","evidence_source":"profile_bootstrap",
            "id":AUTHOR_ID,"username":USERNAME,"display_name":"Example creator",
            "signature":"Shopify tutorials\nPublic content", "follower_count":12345,
            "verified":false,"private_account":false,"profile_url":"https://www.tiktok.com/@sample.creator"}])
    }

    #[test]
    fn profile_evidence_requires_stable_identity_and_real_count() {
        let parsed = parse(&fixture().to_string(), USERNAME, AUTHOR_ID).unwrap();
        assert_eq!(parsed.follower_count, 12345);
        assert_eq!(parsed.signature, "Shopify tutorials\nPublic content");
        let mut zero = fixture();
        zero[0]["follower_count"] = 0.into();
        assert_eq!(
            parse(&zero.to_string(), USERNAME, AUTHOR_ID)
                .unwrap()
                .follower_count,
            0
        );
        for field in [
            "follower_count",
            "id",
            "signature",
            "verified",
            "private_account",
            "evidence_source",
        ] {
            let mut missing = fixture();
            missing[0].as_object_mut().unwrap().remove(field);
            assert!(parse(&missing.to_string(), USERNAME, AUTHOR_ID).is_err());
        }
        for count in [
            serde_json::Value::Null,
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!("12K"),
            serde_json::json!(9_007_199_254_740_992_u64),
        ] {
            let mut invalid = fixture();
            invalid[0]["follower_count"] = count;
            assert!(parse(&invalid.to_string(), USERNAME, AUTHOR_ID).is_err());
        }
        for (field, value) in [
            ("id", "1"),
            ("username", "other"),
            ("profile_url", "https://evil.example"),
            ("evidence_source", "search_fallback"),
            ("signature", "\0"),
            ("adapter_version", "2.0.0"),
        ] {
            let mut invalid = fixture();
            invalid[0][field] = value.into();
            assert!(parse(&invalid.to_string(), USERNAME, AUTHOR_ID).is_err());
        }
        let mut invalid = fixture();
        invalid[0]["signature"] = "字".repeat(5334).into();
        assert!(parse(&invalid.to_string(), USERNAME, AUTHOR_ID).is_err());
        invalid = fixture();
        invalid[0]["unexpected"] = true.into();
        assert!(parse(&invalid.to_string(), USERNAME, AUTHOR_ID).is_err());
        assert!(parse("[]", USERNAME, AUTHOR_ID).is_err());
        assert!(
            parse(
                &serde_json::json!([fixture()[0], fixture()[0]]).to_string(),
                USERNAME,
                AUTHOR_ID
            )
            .is_err()
        );
    }

    #[test]
    fn target_validation_rejects_urls_options_and_coerced_identity() {
        assert_eq!(check_tiktok_profile_target(USERNAME, AUTHOR_ID), Ok(()));
        for (username, id) in [
            ("", AUTHOR_ID),
            ("../other", AUTHOR_ID),
            ("--help", AUTHOR_ID),
            (USERNAME, "0"),
            (USERNAME, "1/other"),
            (USERNAME, "1;exit"),
        ] {
            assert!(check_tiktok_profile_target(username, id).is_err());
        }
    }

    #[tokio::test]
    #[ignore = "two public reads: TikTok search then exact profile; requires OPENCLI_TEST_PROFILE and installed adapters"]
    async fn tiktok_profile_contract_matches_search_author() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated profile");
        let cli = OpenCli::inspect_system().await.unwrap();
        let started = std::time::Instant::now();
        let videos = cli.tiktok_search(&profile, "ecommerce", 1).await.unwrap();
        println!(
            "TikTok search PASS; elapsed_ms={}; rows={}",
            started.elapsed().as_millis(),
            videos.len()
        );
        let video = videos.first().expect("actual search result");
        let started = std::time::Instant::now();
        let author = cli
            .tiktok_public_profile(&profile, &video.author, &video.author_id)
            .await
            .unwrap();
        assert_eq!(author.id, video.author_id);
        assert_eq!(author.username, video.author);
        println!(
            "TikTok native profile PASS; elapsed_ms={}; source={} author_id={} followers={} biography_bytes={} private_account={}; no contact or recommendation asserted",
            started.elapsed().as_millis(),
            author.profile_url,
            author.id,
            author.follower_count,
            author.signature.len(),
            author.private_account
        );
    }
}
