use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

/// Public profile follower evidence. Display abbreviations are never the count source.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BilibiliPublicProfile {
    pub adapter_version: String,
    pub mid: String,
    pub name: String,
    pub follower_count: u64,
    pub follower_count_text: String,
    pub follower_display: String,
    pub profile_url: String,
    pub evidence_source: String,
    /// Original text from the exact MID's native header biography. Absent in
    /// historical receipts and when the header cannot be read unambiguously.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign: Option<String>,
}

pub fn check_bilibili_profile_target(mid: &str) -> Result<(), OpenCliError> {
    if !(1..=20).contains(&mid.len())
        || mid.starts_with('0')
        || !mid.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(OpenCliError::InvalidInput("bilibili_profile_target"));
    }
    Ok(())
}

impl OpenCli {
    pub async fn bilibili_public_profile(
        &self,
        profile: &str,
        mid: &str,
    ) -> Result<BilibiliPublicProfile, OpenCliError> {
        check_bilibili_profile_target(mid)?;
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        addons::require_ready("onecrew-bilibili-profile").await?;
        self.require_browser_read(profile, "onecrew-bilibili-profile", "read")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-bilibili-profile",
                "read",
                "-f",
                "json",
                "--",
                mid,
            ],
            READ_TIMEOUT,
        )
        .await?;
        addons::require_ready("onecrew-bilibili-profile").await?;
        parse(&output, mid)
    }
}

fn exact_count(text: &str) -> Option<u64> {
    if text.is_empty() || text.len() > 32 || (text.starts_with('0') && text != "0") {
        return None;
    }
    let groups = text.split(',').collect::<Vec<_>>();
    if groups
        .iter()
        .any(|group| group.is_empty() || !group.bytes().all(|b| b.is_ascii_digit()))
        || (groups.len() > 1
            && (groups[0].len() > 3 || groups[1..].iter().any(|group| group.len() != 3)))
    {
        return None;
    }
    let count = text.replace(',', "").parse::<u64>().ok()?;
    (count <= 9_007_199_254_740_991).then_some(count)
}

fn parse(output: &str, expected_mid: &str) -> Result<BilibiliPublicProfile, OpenCliError> {
    let mut rows: Vec<BilibiliPublicProfile> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.pop().ok_or(OpenCliError::InvalidOutput)?;
    let text_valid = |text: &str, max| {
        !text.trim().is_empty()
            && text.chars().count() <= max
            && !text.chars().any(char::is_control)
    };
    if row.adapter_version != "1.0.0"
        || row.evidence_source != "profile_tooltip"
        || row.mid != expected_mid
        || row.profile_url != format!("https://space.bilibili.com/{expected_mid}")
        || !text_valid(&row.name, 200)
        || !text_valid(&row.follower_display, 32)
        || exact_count(&row.follower_count_text) != Some(row.follower_count)
        || row.sign.as_ref().is_some_and(|sign| {
            sign.chars().count() > 1000
                || sign
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    const MID: &str = "8420618";
    fn fixture() -> serde_json::Value {
        serde_json::json!([{"adapter_version":"1.0.0","mid":MID,"name":"Example creator",
            "follower_count":1534708,"follower_count_text":"1,534,708","follower_display":"153.4万",
            "profile_url":"https://space.bilibili.com/8420618","evidence_source":"profile_tooltip"}])
    }

    #[test]
    fn biography_is_optional_bounded_and_preserves_public_contact_text() {
        let mut value = fixture();
        assert_eq!(parse(&value.to_string(), MID).unwrap().sign, None);
        value[0]["sign"] = serde_json::json!("Original biography\nBusiness: studio@example.org");
        assert_eq!(
            parse(&value.to_string(), MID).unwrap().sign.as_deref(),
            Some("Original biography\nBusiness: studio@example.org")
        );
        for invalid in [
            serde_json::json!("x".repeat(1001)),
            serde_json::json!("a\0b"),
            serde_json::json!(7),
        ] {
            value[0]["sign"] = invalid;
            assert!(parse(&value.to_string(), MID).is_err());
        }
    }

    #[test]
    fn exact_follower_count_preserves_source_text_and_does_not_expand_abbreviations() {
        assert_eq!(
            parse(&fixture().to_string(), MID).unwrap().follower_count,
            1534708
        );
        assert_eq!(exact_count("0"), Some(0));
        assert_eq!(exact_count("1234"), Some(1234));
        for text in [
            "",
            "01",
            "0,123",
            "12,34",
            "1234,567",
            "1.5",
            "153.4万",
            "-1",
            "9007199254740992",
        ] {
            assert_eq!(exact_count(text), None, "{text}");
        }
        for field in [
            "mid",
            "follower_count",
            "follower_count_text",
            "follower_display",
            "evidence_source",
        ] {
            let mut value = fixture();
            value[0].as_object_mut().unwrap().remove(field);
            assert!(parse(&value.to_string(), MID).is_err());
        }
        for (field, value) in [
            ("mid", "1"),
            ("profile_url", "https://evil.example/8420618"),
            ("evidence_source", "estimated"),
            ("follower_count_text", "1,534,709"),
            ("name", ""),
            ("follower_display", "bad\0value"),
            ("adapter_version", "2.0.0"),
        ] {
            let mut invalid = fixture();
            invalid[0][field] = value.into();
            assert!(parse(&invalid.to_string(), MID).is_err());
        }
        let mut invalid = fixture();
        invalid[0]["extra"] = true.into();
        assert!(parse(&invalid.to_string(), MID).is_err());
        assert!(parse("[]", MID).is_err());
        assert!(
            parse(
                &serde_json::json!([fixture()[0], fixture()[0]]).to_string(),
                MID
            )
            .is_err()
        );
    }

    #[test]
    fn profile_target_requires_an_exact_observed_mid() {
        assert_eq!(check_bilibili_profile_target(MID), Ok(()));
        for id in [
            "",
            "0",
            "01",
            "../123",
            "--help",
            "https://space.bilibili.com/123",
        ] {
            assert!(check_bilibili_profile_target(id).is_err());
        }
    }

    #[tokio::test]
    #[ignore = "two public reads: Bilibili search then exact profile; requires OPENCLI_TEST_PROFILE and installed adapters"]
    async fn bilibili_profile_contract_matches_search_author() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated profile");
        let cli = OpenCli::inspect_system().await.unwrap();
        let started = std::time::Instant::now();
        let videos = cli.bilibili_search(&profile, "跨境电商", 1).await.unwrap();
        println!(
            "Bilibili search PASS; elapsed_ms={}; rows={}",
            started.elapsed().as_millis(),
            videos.len()
        );
        let video = videos.first().expect("actual search result");
        let started = std::time::Instant::now();
        let author = cli
            .bilibili_public_profile(&profile, &video.author_id)
            .await
            .unwrap();
        assert_eq!(author.mid, video.author_id);
        assert_eq!(author.profile_url, video.author_url);
        println!(
            "Bilibili native profile PASS; elapsed_ms={}; source={} followers={} tooltip={} display={}; no contact or recommendation asserted",
            started.elapsed().as_millis(),
            author.profile_url,
            author.follower_count,
            author.follower_count_text,
            author.follower_display
        );
    }
}
