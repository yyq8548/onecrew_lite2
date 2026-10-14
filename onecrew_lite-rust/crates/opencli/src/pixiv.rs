//! Author and artwork receipts from the selected browser's native Pixiv response.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PixivPublicProfile {
    pub adapter_version: String,
    pub author_id: String,
    pub author_label: String,
    pub profile_url: String,
    pub follower_count: Option<u64>,
    pub bio: String,
    pub public_links: Vec<String>,
    pub evidence_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PixivContent {
    pub post_id: String,
    pub author_id: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
}

pub(crate) fn valid_id(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}
fn target(id: &str) -> Result<(), OpenCliError> {
    valid_id(id)
        .then_some(())
        .ok_or(OpenCliError::InvalidInput("pixiv_id"))
}
fn one<T: serde::de::DeserializeOwned>(output: &str, maximum: usize) -> Result<T, OpenCliError> {
    if output.len() > maximum {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<T> = serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(rows.remove(0))
}
fn parse_profile(output: &str, id: &str) -> Result<PixivPublicProfile, OpenCliError> {
    let row: PixivPublicProfile = one(output, 32_000)?;
    if row.adapter_version != "1.0.0"
        || row.author_id != id
        || !valid_id(&row.author_id)
        || row.profile_url != format!("https://www.pixiv.net/users/{id}")
        || row.evidence_source != "native_ajax_response"
        || row.follower_count.is_some()
        || row.author_label.trim().is_empty()
        || row.author_label.len() > 512
        || row.author_label.chars().any(char::is_control)
        || row.bio.len() > 8000
        || row.bio.contains('\0')
        || row.public_links.len() > 10
        || row
            .public_links
            .iter()
            .any(|link| crate::validate_public_web_url(link).is_err())
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}
fn parse_content(output: &str, id: &str, author: &str) -> Result<PixivContent, OpenCliError> {
    let row: PixivContent = one(output, 24_000)?;
    if row.post_id != id
        || !valid_id(&row.post_id)
        || row.author_id != author
        || !valid_id(&row.author_id)
        || row.source_url != format!("https://www.pixiv.net/artworks/{id}")
        || row.text.trim().is_empty()
        || row.text.len() > 16_000
        || row.text.contains('\0')
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}
impl OpenCli {
    pub async fn pixiv_public_profile(
        &self,
        profile: &str,
        id: &str,
    ) -> Result<PixivPublicProfile, OpenCliError> {
        let output = self.pixiv_evidence(profile, "profile", id, None).await?;
        parse_profile(&output, id)
    }
    pub async fn pixiv_original_content(
        &self,
        profile: &str,
        id: &str,
        author: &str,
    ) -> Result<PixivContent, OpenCliError> {
        target(author)?;
        let output = self
            .pixiv_evidence(profile, "content", id, Some(author))
            .await?;
        parse_content(&output, id, author)
    }
    async fn pixiv_evidence(
        &self,
        profile: &str,
        kind: &str,
        id: &str,
        author: Option<&str>,
    ) -> Result<String, OpenCliError> {
        if crate::creator_search::RETIRED_CREATOR_SOURCES.contains(&"pixiv") {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        validate_profile(profile)?;
        target(id)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-pixiv-reader", kind)
            .await?;
        let mut args = vec![
            "--profile",
            profile,
            "onecrew-pixiv-reader",
            kind,
            "-f",
            "json",
            "--",
            id,
        ];
        if let Some(author) = author {
            args.push(author);
        }
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &args,
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn profile_unknown_followers_and_contact_sources_require_native_owner() {
        let mut row = json!({"adapter_version":"1.0.0","author_id":"123","author_label":"Artist","profile_url":"https://www.pixiv.net/users/123",
            "follower_count":null,"bio":"Art contact art@example.com","public_links":["https://artist.example/contact"],"evidence_source":"native_ajax_response"});
        assert_eq!(
            parse_profile(&json!([row.clone()]).to_string(), "123")
                .unwrap()
                .follower_count,
            None
        );
        assert!(parse_profile(&json!([row.clone()]).to_string(), "456").is_err());
        row["follower_count"] = json!(12000);
        assert!(parse_profile(&json!([row.clone()]).to_string(), "123").is_err());
        row["follower_count"] = serde_json::Value::Null;
        row["public_links"] = json!(["https://127.0.0.1/"]);
        assert!(parse_profile(&json!([row]).to_string(), "123").is_err());
    }
    #[test]
    fn original_content_requires_artwork_and_author_identity_and_bounded_text() {
        let row = json!({"post_id":"456","author_id":"123","source_url":"https://www.pixiv.net/artworks/456","text":"Original art","text_truncated":false});
        assert!(parse_content(&json!([row.clone()]).to_string(), "456", "123").is_ok());
        assert!(parse_content(&json!([row.clone()]).to_string(), "999", "123").is_err());
        assert!(parse_content(&json!([row.clone()]).to_string(), "456", "999").is_err());
        assert!(parse_content(&json!([row.clone(), row]).to_string(), "456", "123").is_err());
    }
}
