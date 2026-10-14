use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, process::run, validate_profile, youtube};

const ENTRY: &[u8] =
    include_bytes!("../../../integrations/opencli/onecrew-youtube/video-evidence.js");
const CORE: &[u8] = include_bytes!("../../../integrations/opencli/onecrew-youtube/evidence.mjs");

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct YoutubeVideoEvidence {
    pub adapter_version: String,
    pub video_id: String,
    pub channel_id: String,
    pub source_url: String,
    pub title: String,
    pub author: String,
    pub subscribers: String,
    pub description: String,
    pub description_source: String,
    pub display_description: String,
    pub links: Vec<YoutubeDescriptionLink>,
    pub playability_status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct YoutubeDescriptionLink {
    pub url: String,
    pub label: String,
    pub source: String,
}

impl OpenCli {
    /// Preflight the exact evidence capability before a Campaign reserves budget.
    /// This performs local checks only and does not open a provider page.
    pub async fn check_youtube_evidence_profile(&self, profile: &str) -> Result<(), OpenCliError> {
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        require_installed_adapter().await?;
        self.require_browser_read(profile, "onecrew-youtube", "video-evidence")
            .await
    }

    /// Read original author description through the separately named OneCrew
    /// adapter. It must be explicitly installed with these exact shipped bytes.
    /// No fallback to formatted official metadata and no installation side effect.
    pub async fn youtube_video_evidence(
        &self,
        profile: &str,
        video: &str,
    ) -> Result<YoutubeVideoEvidence, OpenCliError> {
        validate_profile(profile)?;
        let video_id = youtube::video_id(video)?;
        self.check_youtube_evidence_profile(profile).await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-youtube",
                "video-evidence",
                "-f",
                "json",
                "--",
                &video_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        // Detect a local adapter replacement during the read before accepting evidence.
        require_installed_adapter().await?;
        parse(&output, &video_id)
    }
}

async fn require_installed_adapter() -> Result<(), OpenCliError> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or(OpenCliError::ReadCommandUnavailable)?;
    let directory = PathBuf::from(home).join(".opencli/clis/onecrew-youtube");
    for (name, expected) in [("video-evidence.js", ENTRY), ("evidence.mjs", CORE)] {
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

fn parse(json: &str, video_id: &str) -> Result<YoutubeVideoEvidence, OpenCliError> {
    if json.len() > 512_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut values: Vec<YoutubeVideoEvidence> =
        serde_json::from_str(json).map_err(|_| OpenCliError::InvalidOutput)?;
    if values.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let value = values.remove(0);
    if value.adapter_version != "1.0.0"
        || value.video_id != video_id
        || value.source_url != format!("https://www.youtube.com/watch?v={video_id}")
        || value.channel_id.len() != 24
        || !value.channel_id.starts_with("UC")
        || !value
            .channel_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        || value.playability_status != "OK"
        || value.description_source != "ytInitialPlayerResponse.videoDetails.shortDescription"
        || value.description.trim().is_empty()
        || value.title.trim().is_empty()
        || value.author.trim().is_empty()
        || [
            (&value.title, 2000),
            (&value.author, 800),
            (&value.subscribers, 200),
            (&value.description, 16000),
            (&value.display_description, 16000),
        ]
        .iter()
        .any(|(text, max)| text.len() > *max || text.contains('\0'))
        || value.links.len() > 100
        || value.links.iter().any(|link| {
            crate::validate_public_web_url(&link.url).is_err()
                || link.label.len() > 2048
                || link.label.contains('\0')
                || !matches!(
                    link.source.as_str(),
                    "shortDescription" | "description.runs" | "attributedDescription.commandRuns"
                )
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> serde_json::Value {
        serde_json::json!([{
            "adapterVersion": "1.0.0", "videoId": "abcdefghijk",
            "channelId": "UCaaaaaaaaaaaaaaaaaaaaaa",
            "sourceUrl": "https://www.youtube.com/watch?v=abcdefghijk",
            "title": "Tool walkthrough", "author": "Creator", "subscribers": "12K subscribers",
            "description": "I remove pauses with https://www.descript.com/",
            "descriptionSource": "ytInitialPlayerResponse.videoDetails.shortDescription",
            "displayDescription": "https://www.descript.com/…", "playabilityStatus": "OK",
            "links": [{"url": "https://www.descript.com/", "label": "Descript", "source": "shortDescription"}]
        }])
    }

    #[test]
    fn source_evidence_preserves_raw_description_and_rejects_identity_drift() {
        let fixture = evidence();
        let read = parse(&fixture.to_string(), "abcdefghijk").unwrap();
        assert!(read.description.ends_with("https://www.descript.com/"));
        assert!(read.display_description.ends_with('…'));
        for (field, replacement) in [
            ("videoId", "different12"),
            ("channelId", "@not-stable"),
            ("adapterVersion", "2.0.0"),
            ("sourceUrl", "https://example.com/"),
            ("playabilityStatus", "LOGIN_REQUIRED"),
            ("descriptionSource", "recommendations"),
        ] {
            let mut invalid = fixture.clone();
            invalid[0][field] = replacement.into();
            assert_eq!(
                parse(&invalid.to_string(), "abcdefghijk"),
                Err(OpenCliError::InvalidOutput)
            );
        }
    }

    #[test]
    fn source_evidence_rejects_unbounded_and_unsafe_links() {
        for link in [
            "https://127.0.0.1/",
            "https://example.com/...",
            "https://user:pass@example.com/",
        ] {
            let mut invalid = evidence();
            invalid[0]["links"][0]["url"] = link.into();
            assert!(parse(&invalid.to_string(), "abcdefghijk").is_err());
        }
        let mut invalid = evidence();
        invalid[0]["description"] = "字".repeat(5334).into();
        assert!(parse(&invalid.to_string(), "abcdefghijk").is_err());
    }

    #[tokio::test]
    #[ignore = "requires explicit OpenCLI profile and known hidden-email channel; one native read, no verification"]
    async fn hidden_email_entry_matches_connected_channel() {
        let profile = std::env::var("ONECREW_OPENCLI_TEST_PROFILE").expect("explicit profile");
        let channel = std::env::var("ONECREW_OPENCLI_TEST_CHANNEL").expect("explicit channel");
        let cli = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            cli.repair_bundled_addons(true).await.unwrap();
        }
        let value = cli
            .youtube_channel_contact(&profile, &channel)
            .await
            .unwrap();
        println!(
            "channel={} about_data_available={} hidden_email_available={} description_bytes={}",
            value.channel_id,
            value.about_data_available,
            value.hidden_email_available,
            value.description.len()
        );
        assert!(
            value.about_data_available,
            "native About panel was not observed"
        );
        assert!(
            value.hidden_email_available,
            "native hidden-email entry was not observed"
        );
    }

    #[tokio::test]
    #[ignore = "requires explicitly installed OneCrew adapter and connected dedicated OpenCLI profile"]
    async fn source_evidence_contract_matches_connected_extension() {
        let profile = std::env::var("ONECREW_OPENCLI_TEST_PROFILE").expect("explicit profile");
        let cli = OpenCli::inspect_system().await.unwrap();
        let value = cli
            .youtube_video_evidence(&profile, "htZRCE2GgIs")
            .await
            .unwrap();
        assert_eq!(value.channel_id, "UCwAnu01qlnVg1Ai2AbtTMaA");
        assert!(!value.links.is_empty());
        assert!(
            value
                .description
                .contains("https://www.jeffsu.org/10-ai-tools-drive-most-results/")
        );
        println!(
            "video={} description_bytes={} links={}",
            value.video_id,
            value.description.len(),
            value.links.len()
        );
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct YoutubeChannelContact {
    pub adapter_version: String,
    pub channel_id: String,
    pub source_url: String,
    pub title: String,
    pub description: String,
    pub hidden_email_available: bool,
    pub about_data_available: bool,
}
impl OpenCli {
    pub async fn youtube_channel_contact(
        &self,
        profile: &str,
        channel: &str,
    ) -> Result<YoutubeChannelContact, OpenCliError> {
        validate_profile(profile)?;
        youtube::validate_channel(channel)?;
        if channel.len() != 24 || !channel.starts_with("UC") {
            return Err(OpenCliError::InvalidInput("channel"));
        }
        require_channel_adapter().await?;
        self.require_browser_read(profile, "onecrew-youtube", "channel-contact")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-youtube",
                "channel-contact",
                "-f",
                "json",
                "--",
                channel,
            ],
            READ_TIMEOUT,
        )
        .await?;
        require_channel_adapter().await?;
        if output.len() > 128000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        let mut values: Vec<YoutubeChannelContact> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if values.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        let value = values.remove(0);
        if value.adapter_version != "1.0.0"
            || value.channel_id != channel
            || value.source_url != format!("https://www.youtube.com/channel/{channel}")
            || value.title.len() > 2000
            || value.description.len() > 64000
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(value)
    }
}
async fn require_channel_adapter() -> Result<(), OpenCliError> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or(OpenCliError::ReadCommandUnavailable)?;
    let directory = PathBuf::from(home).join(".opencli/clis/onecrew-youtube");
    for (name, expected) in [
        (
            "channel.mjs",
            include_bytes!("../../../integrations/opencli/onecrew-youtube/channel.mjs").as_slice(),
        ),
        (
            "channel-contact.js",
            include_bytes!("../../../integrations/opencli/onecrew-youtube/channel-contact.js")
                .as_slice(),
        ),
    ] {
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
