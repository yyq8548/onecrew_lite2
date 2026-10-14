use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile, youtube};

impl OpenCli {
    async fn youtube_language_read(
        &self,
        profile: &str,
        channel: &str,
        video: &str,
    ) -> Result<String, OpenCliError> {
        validate_profile(profile)?;
        if channel.len() != 24
            || !channel.starts_with("UC")
            || !channel
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(OpenCliError::InvalidInput("channel"));
        }
        self.require_version().await?;
        addons::require_ready("onecrew-youtube").await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-youtube", "language")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-youtube",
                "language",
                "-f",
                "json",
                "--",
                channel,
                video,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-youtube").await?;
        addons::require_ready("onecrew-browser-search").await?;
        if output.len() > 64_000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        Ok(output)
    }

    pub async fn youtube_language_sample(
        &self,
        profile: &str,
        channel: &str,
        video: &str,
    ) -> Result<serde_json::Value, OpenCliError> {
        let video = youtube::video_id(video)?;
        let output = self.youtube_language_read(profile, channel, &video).await?;
        let mut rows: Vec<serde_json::Value> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        let row = rows.remove(0);
        if row["author_id"].as_str() != Some(channel)
            || row["source_url"] != format!("https://www.youtube.com/watch?v={video}")
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(row)
    }

    pub async fn youtube_recent_language_urls(
        &self,
        profile: &str,
        channel: &str,
    ) -> Result<Vec<String>, OpenCliError> {
        let output = self
            .youtube_language_read(profile, channel, "recent")
            .await?;
        #[derive(serde::Deserialize)]
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
            let id = youtube::video_id(&row.source_url).map_err(|_| OpenCliError::InvalidOutput)?;
            if row.source_url != format!("https://www.youtube.com/watch?v={id}")
                || urls.contains(&row.source_url)
            {
                return Err(OpenCliError::InvalidOutput);
            }
            urls.push(row.source_url);
        }
        Ok(urls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires installed exact add-on and explicitly selected connected profile"]
    async fn youtube_language_live_original_text_and_recent_locators() {
        let profile = std::env::var("ONECREW_OPENCLI_TEST_PROFILE").expect("explicit profile");
        let cli = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            cli.repair_bundled_addons(true).await.unwrap();
        }
        let channel = "UCwAnu01qlnVg1Ai2AbtTMaA";
        let sample = cli
            .youtube_language_sample(&profile, channel, "htZRCE2GgIs")
            .await
            .unwrap();
        assert_eq!(sample["author_id"], channel);
        assert!(
            sample["text"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty())
        );
        assert!(
            sample["comments"]
                .as_array()
                .is_some_and(|comments| !comments.is_empty() && comments.len() <= 20)
        );
        let urls = cli
            .youtube_recent_language_urls(&profile, channel)
            .await
            .unwrap();
        assert!(!urls.is_empty() && urls.len() <= 3);
        println!(
            "YouTube author bound; comments={}; recent={}; no Campaign write",
            sample["comments"].as_array().unwrap().len(),
            urls.len()
        );
    }
}
