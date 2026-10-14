use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

fn video_id(source: &str) -> Result<&str, OpenCliError> {
    source
        .strip_prefix("https://www.bilibili.com/video/")
        .filter(|id| {
            id.len() == 12 && id.starts_with("BV") && id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .ok_or(OpenCliError::InvalidInput("bilibili_language_video"))
}

impl OpenCli {
    async fn bilibili_language_read(
        &self,
        profile: &str,
        mid: &str,
        video: &str,
    ) -> Result<Vec<serde_json::Value>, OpenCliError> {
        validate_profile(profile)?;
        crate::check_bilibili_profile_target(mid)?;
        self.require_version().await?;
        addons::require_ready("onecrew-bilibili-video").await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-bilibili-video", "language")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-bilibili-video",
                "language",
                "-f",
                "json",
                "--",
                mid,
                video,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-bilibili-video").await?;
        addons::require_ready("onecrew-browser-search").await?;
        if output.len() > 64_000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)
    }

    pub async fn bilibili_language_sample(
        &self,
        profile: &str,
        mid: &str,
        source: &str,
    ) -> Result<serde_json::Value, OpenCliError> {
        let id = video_id(source)?;
        let mut rows = self.bilibili_language_read(profile, mid, id).await?;
        if rows.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        let row = rows.remove(0);
        if row["author_id"].as_str() != Some(mid) || row["source_url"].as_str() != Some(source) {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(row)
    }

    pub async fn bilibili_recent_language_urls(
        &self,
        profile: &str,
        mid: &str,
    ) -> Result<Vec<String>, OpenCliError> {
        let rows = self.bilibili_language_read(profile, mid, "recent").await?;
        if rows.len() > 3 {
            return Err(OpenCliError::InvalidOutput);
        }
        let mut urls = Vec::new();
        for row in rows {
            let source = row["source_url"]
                .as_str()
                .ok_or(OpenCliError::InvalidOutput)?;
            video_id(source).map_err(|_| OpenCliError::InvalidOutput)?;
            if row.as_object().is_none_or(|v| v.len() != 1) || urls.iter().any(|v| v == source) {
                return Err(OpenCliError::InvalidOutput);
            }
            urls.push(source.to_owned());
        }
        Ok(urls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_native_video_locator_only() {
        assert!(video_id("https://www.bilibili.com/video/BV1234567890").is_ok());
        for suffix in ["/", "?x=1", "#reply1", "/../BV1234567890"] {
            assert!(
                video_id(&format!(
                    "https://www.bilibili.com/video/BV1234567890{suffix}"
                ))
                .is_err()
            );
        }
        assert!(video_id("https://evil.example/video/BV1234567890").is_err());
    }
}
