use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstagramPublicProfile {
    pub adapter_version: String,
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub biography: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_email: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contact_links: Vec<String>,
    pub follower_count: Option<u64>,
    pub private_account: Option<bool>,
    pub verified: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_text: Option<String>,
    pub profile_url: String,
    pub evidence_source: String,
}

impl OpenCli {
    pub async fn instagram_public_profile(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
    ) -> Result<InstagramPublicProfile, OpenCliError> {
        validate_profile(profile)?;
        if !(1..=30).contains(&username.len())
            || !username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            return Err(OpenCliError::InvalidInput("instagram_profile_target"));
        }
        if author_id.is_empty()
            || author_id.len() > 25
            || author_id.starts_with('0')
            || !author_id.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(OpenCliError::InvalidInput("instagram_profile_identity"));
        }
        let username = username.to_ascii_lowercase();
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-instagram-profile", "read")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-instagram-profile",
                "read",
                "-f",
                "json",
                "--",
                &username,
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
        let mut rows: Vec<InstagramPublicProfile> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        let result = rows.remove(0);
        if result.adapter_version != "1.0.0"
            || result.username != username
            || result.profile_url != format!("https://www.instagram.com/{username}/")
            || result.evidence_source != "public_profile_dom_search_bound"
            || result.id != author_id
            || result
                .web_text
                .as_ref()
                .is_none_or(|text| text.chars().count() > 4000 || text.contains('\0'))
            || !(1..=25).contains(&result.id.len())
            || result.id.starts_with('0')
            || !result.id.bytes().all(|b| b.is_ascii_digit())
            || result.display_name.chars().count() > 200
            || result.biography.chars().count() > 2000
            || result.display_name.contains('\0')
            || result.biography.contains('\0')
            || result.public_email.as_ref().is_some_and(|email| {
                email.len() > 320 || email.chars().any(char::is_control) || !email.contains('@')
            })
            || result.contact_links.len() > 10
            || result.contact_links.iter().any(|link| {
                link.len() > 2048
                    || link.chars().any(char::is_control)
                    || !url::Url::parse(link).is_ok_and(|url| {
                        matches!(url.scheme(), "http" | "https")
                            && url.host_str().is_some()
                            && url.username().is_empty()
                            && url.password().is_none()
                    })
            })
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(result)
    }
}
