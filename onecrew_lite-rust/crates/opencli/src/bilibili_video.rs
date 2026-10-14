//! One fixed, author-bound read of the official OpenCLI 1.8.8 video command.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

const FIELDS: [&str; 21] = [
    "bvid",
    "aid",
    "title",
    "author",
    "category",
    "publish_time",
    "duration",
    "view",
    "danmaku",
    "reply",
    "like",
    "coin",
    "favorite",
    "share",
    "parts",
    "thumbnail",
    "description",
    "requires_payment",
    "payment_type",
    "pay_preview",
    "redirect_url",
];
const MAX_DESCRIPTION_BYTES: usize = 8_192;

/// Public metadata tied to the exact search video and observed author.
/// Description text is a clue for review, never proof that the video was watched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BilibiliVideoEvidence {
    pub bvid: String,
    pub author_id: String,
    pub author: String,
    pub source_url: String,
    pub title: String,
    pub description: String,
    pub description_truncated: bool,
    pub requires_payment: bool,
    pub pay_preview: bool,
    /// The official view API supplies payment rights. Public-page fallback
    /// cannot verify those rights, even when no paywall marker is visible.
    #[serde(default = "payment_known_for_legacy_receipt")]
    pub payment_status_known: bool,
}

fn payment_known_for_legacy_receipt() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldValue {
    field: String,
    value: String,
}

fn check_target<'a>(source_url: &'a str, author_id: &str) -> Result<&'a str, OpenCliError> {
    crate::check_bilibili_profile_target(author_id)?;
    check_source(source_url)
}

fn check_source(source_url: &str) -> Result<&str, OpenCliError> {
    let bvid = source_url
        .strip_prefix("https://www.bilibili.com/video/")
        .ok_or(OpenCliError::InvalidInput("bilibili_video_source"))?;
    if bvid.len() != 12
        || !bvid.starts_with("BV")
        || !bvid.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(OpenCliError::InvalidInput("bilibili_video_source"));
    }
    Ok(bvid)
}

impl OpenCli {
    /// Read one saved engine video locator. The native response, never the
    /// engine title or display name, establishes the author's MID. Without a
    /// known MID the profile-bound public-page fallback is not authorized.
    pub async fn bilibili_source_video_evidence(
        &self,
        profile: &str,
        source_url: &str,
    ) -> Result<BilibiliVideoEvidence, OpenCliError> {
        let bvid = check_source(source_url)?;
        let output = self.read_bilibili_video_api(profile, bvid).await?;
        self.require_version().await?;
        parse_native(&output, bvid, None)
    }

    async fn read_bilibili_video_api(
        &self,
        profile: &str,
        bvid: &str,
    ) -> Result<String, OpenCliError> {
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        self.require_browser_read(profile, "bilibili", "video")
            .await?;
        run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "bilibili",
                "video",
                "-f",
                "json",
                "--",
                bvid,
            ],
            READ_TIMEOUT,
        )
        .await
    }

    /// The caller must supply both values from one saved search hit. No free URL,
    /// command, page number, or author name reaches the CLI.
    pub async fn bilibili_video_evidence(
        &self,
        profile: &str,
        source_url: &str,
        author_id: &str,
    ) -> Result<BilibiliVideoEvidence, OpenCliError> {
        let bvid = check_target(source_url, author_id)?;
        let api_result = self.read_bilibili_video_api(profile, bvid).await;
        let (output, payment_status_known) = match api_result {
            Ok(output) => (output, true),
            Err(original @ OpenCliError::CommandFailed(_)) => {
                // The official view API can return an HTML challenge instead of
                // JSON while the public video page remains readable. Only this
                // failure class may use our source-bound page adapter. That
                // adapter verifies the page, JSON-LD, visible title and MID.
                if addons::require_ready("onecrew-bilibili-video")
                    .await
                    .is_err()
                {
                    return Err(original);
                }
                self.require_browser_read(profile, "onecrew-bilibili-video", "read")
                    .await?;
                let output = run(
                    &self.executable,
                    self.script.as_deref(),
                    &[
                        "--profile",
                        profile,
                        "onecrew-bilibili-video",
                        "read",
                        "-f",
                        "json",
                        "--",
                        bvid,
                        author_id,
                    ],
                    READ_TIMEOUT,
                )
                .await?;
                addons::require_ready("onecrew-bilibili-video").await?;
                (output, false)
            }
            Err(error) => return Err(error),
        };
        self.require_version().await?;
        let mut evidence = parse(&output, bvid, author_id)?;
        evidence.payment_status_known = payment_status_known;
        Ok(evidence)
    }
}

fn parse(output: &str, bvid: &str, author_id: &str) -> Result<BilibiliVideoEvidence, OpenCliError> {
    parse_native(output, bvid, Some(author_id))
}

fn parse_native(
    output: &str,
    bvid: &str,
    expected_author: Option<&str>,
) -> Result<BilibiliVideoEvidence, OpenCliError> {
    if output.len() > 160_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let rows: Vec<FieldValue> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != FIELDS.len() {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut values = BTreeMap::new();
    for row in rows {
        if !FIELDS.contains(&row.field.as_str())
            || row.value.len()
                > if row.field == "description" {
                    128_000
                } else {
                    2_048
                }
            || row.value.contains('\0')
            || values.insert(row.field, row.value).is_some()
        {
            return Err(OpenCliError::InvalidOutput);
        }
    }
    let value = |name: &str| {
        values
            .get(name)
            .map(String::as_str)
            .ok_or(OpenCliError::InvalidOutput)
    };
    if value("bvid")? != bvid {
        return Err(OpenCliError::InvalidOutput);
    }
    let author = value("author")?;
    let (name, mid) = author
        .strip_suffix(')')
        .and_then(|text| text.rsplit_once(" (mid: "))
        .ok_or(OpenCliError::InvalidOutput)?;
    if crate::check_bilibili_profile_target(mid).is_err()
        || expected_author.is_some_and(|author| mid != author)
        || name.trim().is_empty()
        || name.chars().count() > 200
        || name.chars().any(char::is_control)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let title = value("title")?;
    if title.trim().is_empty() || title.chars().count() > 500 || title.chars().any(char::is_control)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let boolean = |name| match value(name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(OpenCliError::InvalidOutput),
    };
    let requires_payment = boolean("requires_payment")?;
    let pay_preview = boolean("pay_preview")?;
    let payment_type = value("payment_type")?;
    if !matches!(payment_type, "" | "vip" | "ugc_pay" | "upower")
        || requires_payment != !payment_type.is_empty()
        || !value("redirect_url")?.is_empty()
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let raw_description = value("description")?;
    let mut end = raw_description.len().min(MAX_DESCRIPTION_BYTES);
    while !raw_description.is_char_boundary(end) {
        end -= 1;
    }
    Ok(BilibiliVideoEvidence {
        bvid: bvid.into(),
        author_id: mid.into(),
        author: name.into(),
        source_url: format!("https://www.bilibili.com/video/{bvid}"),
        title: title.into(),
        description: raw_description[..end].into(),
        description_truncated: end < raw_description.len(),
        requires_payment,
        pay_preview,
        payment_status_known: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BV: &str = "BV1xx411c7mD";
    const MID: &str = "12345";

    #[tokio::test]
    #[ignore = "one saved observed Bilibili video; no search or fallback"]
    async fn saved_engine_video_resolves_native_author_without_guessing() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser profile");
        let source =
            std::env::var("OPENCLI_TEST_BILIBILI_SOURCE").expect("saved observed video URL");
        let cli = OpenCli::inspect_system().await.unwrap();
        let started = std::time::Instant::now();
        let result = cli.bilibili_source_video_evidence(&profile, &source).await;
        println!(
            "Bilibili native source elapsed_ms={}",
            started.elapsed().as_millis()
        );
        let evidence = result.unwrap();
        assert_eq!(evidence.source_url, source);
        assert!(!evidence.author_id.is_empty());
        println!(
            "Bilibili native source verified; bvid={}; author_id={}; payment_status_known={}; description_truncated={}",
            evidence.bvid,
            evidence.author_id,
            evidence.payment_status_known,
            evidence.description_truncated
        );
    }

    #[tokio::test]
    #[ignore = "requires OPENCLI_TEST_PROFILE and a connected Chrome identity"]
    async fn public_video_page_fallback_matches_the_saved_author() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser profile");
        let cli = OpenCli::inspect_system().await.unwrap();
        let evidence = cli
            .bilibili_video_evidence(
                &profile,
                "https://www.bilibili.com/video/BV1bTYv6cELe",
                "168310298",
            )
            .await
            .unwrap();
        assert_eq!(evidence.author_id, "168310298");
        assert_eq!(evidence.bvid, "BV1bTYv6cELe");
        assert!(evidence.title.contains("鞋"));
        assert!(!evidence.description.is_empty());
    }

    fn fixture() -> serde_json::Value {
        serde_json::json!([
            {"field":"bvid","value":BV}, {"field":"aid","value":"1"},
            {"field":"title","value":"跨境电商教程"},
            {"field":"author","value":"Example creator (mid: 12345)"},
            {"field":"category","value":"科技"}, {"field":"publish_time","value":"2026-09-25 10:00"},
            {"field":"duration","value":"1m0s (60s)"}, {"field":"view","value":"100"},
            {"field":"danmaku","value":"0"}, {"field":"reply","value":"0"},
            {"field":"like","value":"0"}, {"field":"coin","value":"0"},
            {"field":"favorite","value":"0"}, {"field":"share","value":"0"},
            {"field":"parts","value":"1"}, {"field":"thumbnail","value":""},
            {"field":"description","value":"本人讲解商品演示；仅文字线索"},
            {"field":"requires_payment","value":"false"},
            {"field":"payment_type","value":""}, {"field":"pay_preview","value":"false"},
            {"field":"redirect_url","value":""}
        ])
    }

    fn change(rows: &mut serde_json::Value, field: &str, value: &str) {
        rows.as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["field"] == field)
            .unwrap()["value"] = value.into();
    }

    #[test]
    fn engine_locator_gets_author_only_from_matching_native_video() {
        let evidence = parse_native(&fixture().to_string(), BV, None).unwrap();
        assert_eq!(evidence.author_id, MID);
        assert_eq!(
            evidence.source_url,
            format!("https://www.bilibili.com/video/{BV}")
        );
        assert_eq!(evidence, parse(&fixture().to_string(), BV, MID).unwrap());
        for author in [
            "Creator",
            "Creator (mid: 0)",
            "Creator (mid: 0123)",
            "Creator (mid: guessed)",
            " (mid: 12345)",
        ] {
            let mut rows = fixture();
            change(&mut rows, "author", author);
            assert!(parse_native(&rows.to_string(), BV, None).is_err());
        }
        let mut rows = fixture();
        change(&mut rows, "bvid", "BV1bTYv6cELe");
        assert!(parse_native(&rows.to_string(), BV, None).is_err());
    }

    #[test]
    fn engine_video_locator_cannot_be_an_arbitrary_command_or_page() {
        assert_eq!(
            check_source(&format!("https://www.bilibili.com/video/{BV}")).unwrap(),
            BV
        );
        for source in [
            "--help",
            "https://space.bilibili.com/12345",
            "https://www.bilibili.com.evil.test/video/BV1xx411c7mD",
            "https://www.bilibili.com/video/BV1xx411c7mD?token=private",
            "https://www.bilibili.com/video/BV1xx411c7mD#fragment",
            "https://user@www.bilibili.com/video/BV1xx411c7mD",
        ] {
            assert!(check_source(source).is_err(), "{source}");
        }
    }

    #[test]
    fn exact_video_and_native_author_are_required() {
        let evidence = parse(&fixture().to_string(), BV, MID).unwrap();
        assert_eq!(evidence.author_id, MID);
        assert!(evidence.payment_status_known);
        assert!(!evidence.description_truncated);
        assert!(!evidence.requires_payment);
        for (field, replacement) in [
            ("bvid", "BV1xx411c7mE"),
            ("author", "Example creator (mid: 54321)"),
            ("author", "Example creator"),
            ("title", ""),
            ("requires_payment", "yes"),
            ("payment_type", "other"),
            (
                "redirect_url",
                "https://www.bilibili.com/video/BV1xx411c7mE",
            ),
        ] {
            let mut rows = fixture();
            change(&mut rows, field, replacement);
            assert_eq!(
                parse(&rows.to_string(), BV, MID),
                Err(OpenCliError::InvalidOutput)
            );
        }
        for url in [
            "https://www.bilibili.com/video/BV1xx411c7mD?from=search",
            "https://www.bilibili.com.evil.example/video/BV1xx411c7mD",
            "https://www.bilibili.com/video/BV1xx411c7mE",
        ] {
            assert_ne!(check_target(url, MID), Ok(BV));
        }
    }

    #[test]
    fn output_fields_are_unique_bounded_and_truncation_is_explicit() {
        let mut rows = fixture();
        change(&mut rows, "description", &"中".repeat(3_000));
        let evidence = parse(&rows.to_string(), BV, MID).unwrap();
        assert!(evidence.description_truncated);
        assert!(evidence.description.len() <= MAX_DESCRIPTION_BYTES);
        assert!(
            evidence
                .description
                .is_char_boundary(evidence.description.len())
        );
        let mut rows = fixture();
        let duplicate = rows[0].clone();
        rows.as_array_mut().unwrap().push(duplicate);
        assert_eq!(
            parse(&rows.to_string(), BV, MID),
            Err(OpenCliError::InvalidOutput)
        );
        let mut rows = fixture();
        rows.as_array_mut().unwrap().pop();
        assert_eq!(
            parse(&rows.to_string(), BV, MID),
            Err(OpenCliError::InvalidOutput)
        );
        let mut rows = fixture();
        rows[0]["secret"] = "cookie".into();
        assert_eq!(
            parse(&rows.to_string(), BV, MID),
            Err(OpenCliError::InvalidOutput)
        );
    }
}
