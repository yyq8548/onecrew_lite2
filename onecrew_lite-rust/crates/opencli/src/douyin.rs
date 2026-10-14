//! Read only the matched Douyin video's caption and its public native author.
//! sec_uid binds the author; editable Douyin numbers and display names do not.
#[tokio::test]
#[ignore = "explicit saved native author, selected browser and QA addon repair"]
async fn actual_saved_author_expanded_profile() {
    let browser = std::env::var("OPENCLI_TEST_PROFILE").expect("selected browser");
    let author = std::env::var("OPENCLI_TEST_DOUYIN_AUTHOR").expect("previously observed author");
    let cli = OpenCli::inspect_system().await.unwrap();
    if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
        cli.repair_bundled_addons(true).await.unwrap();
    }
    let cli = OpenCli::inspect_system().await.unwrap();
    let started = std::time::Instant::now();
    let profile = cli.douyin_public_profile(&browser, &author).await.unwrap();
    println!(
        "Douyin saved profile: elapsed_ms={}; bio_truncated={}; bio_bytes={}",
        started.elapsed().as_millis(),
        profile.bio_truncated,
        profile.bio.len()
    );
    assert_eq!(profile.author_id, author);
    assert!(!profile.bio_truncated, "expanded biography required");
    assert!(
        profile.bio.contains('@'),
        "this saved sample has a public email"
    );
}
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DouyinPublicProfile {
    pub adapter_version: String,
    pub author_id: String,
    pub author_label: String,
    pub profile_url: String,
    pub follower_count: Option<u64>,
    pub follower_text: String,
    pub bio: String,
    pub bio_truncated: bool,
    pub public_links: Vec<String>,
    pub evidence_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DouyinContent {
    pub post_id: String,
    pub author_id: String,
    pub author_label: String,
    pub profile_url: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
    pub evidence_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DouyinComment {
    pub id: String,
    pub parent_post_id: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DouyinCommentWindow {
    pub post_id: String,
    pub author_id: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
    pub comments: Vec<DouyinComment>,
    pub evidence_source: String,
}

pub(crate) fn valid_author(id: &str) -> bool {
    id.strip_prefix("MS4wLjABAAAA").is_some_and(|body| {
        (8..=88).contains(&body.len())
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}
fn valid_video(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}
fn exact_followers(text: &str) -> Option<u64> {
    let groups: Vec<_> = text.split(',').collect();
    if groups
        .iter()
        .any(|group| group.is_empty() || !group.bytes().all(|b| b.is_ascii_digit()))
        || (groups.len() > 1
            && (groups[0].len() > 3 || groups.iter().skip(1).any(|group| group.len() != 3)))
        || (text.starts_with('0') && text != "0")
    {
        return None;
    }
    text.replace(',', "")
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= 9_007_199_254_740_991)
}
fn one<T: serde::de::DeserializeOwned>(output: &str) -> Result<T, OpenCliError> {
    if output.len() > 16_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<T> = serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(rows.remove(0))
}
fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
fn parse_profile(output: &str, author: &str) -> Result<DouyinPublicProfile, OpenCliError> {
    let row: DouyinPublicProfile = one(output)?;
    if !valid_author(author)
        || row.author_id != author
        || row.adapter_version != "1.0.0"
        || row.profile_url != format!("https://www.douyin.com/user/{author}")
        || !label(&row.author_label)
        || row.follower_text.len() > 100
        || row.follower_text.contains('\0')
        || row.follower_count != exact_followers(&row.follower_text)
        || row.bio.len() > 8000
        || row.bio.contains('\0')
        || row.public_links.len() > 10
        || row
            .public_links
            .iter()
            .any(|link| crate::validate_public_web_url(link).is_err())
        || row.evidence_source != "visible_profile_header"
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}
fn parse_content(
    output: &str,
    video: &str,
    author: Option<&str>,
) -> Result<DouyinContent, OpenCliError> {
    let row: DouyinContent = one(output)?;
    if !valid_video(video)
        || row.post_id != video
        || !valid_author(&row.author_id)
        || author.is_some_and(|id| !valid_author(id) || id != row.author_id)
        || row.source_url != format!("https://www.douyin.com/video/{video}")
        || row.profile_url != format!("https://www.douyin.com/user/{}", row.author_id)
        || !label(&row.author_label)
        || row.text.trim().is_empty()
        || row.text.len() > 12000
        || row.text.contains('\0')
        || row.evidence_source != "visible_native_video_caption"
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}
impl OpenCli {
    pub async fn douyin_original_comments(
        &self,
        profile: &str,
        video: &str,
        author: &str,
    ) -> Result<DouyinCommentWindow, OpenCliError> {
        if !valid_video(video) || !valid_author(author) {
            return Err(OpenCliError::InvalidInput("douyin_comments"));
        }
        let output = self
            .douyin_evidence(profile, "comments", video, Some(author))
            .await?;
        parse_comments(&output, video, author)
    }
    pub async fn douyin_public_profile(
        &self,
        profile: &str,
        author: &str,
    ) -> Result<DouyinPublicProfile, OpenCliError> {
        if !valid_author(author) {
            return Err(OpenCliError::InvalidInput("douyin_author"));
        }
        let output = self
            .douyin_evidence(profile, "profile", author, None)
            .await?;
        parse_profile(&output, author)
    }
    /// With no expected author, this obtains identity from the matched native
    /// video. Subsequent content reads require the saved sec_uid explicitly.
    pub async fn douyin_original_content(
        &self,
        profile: &str,
        video: &str,
        author: Option<&str>,
    ) -> Result<DouyinContent, OpenCliError> {
        if !valid_video(video) {
            return Err(OpenCliError::InvalidInput("douyin_video"));
        }
        if author.is_some_and(|id| !valid_author(id)) {
            return Err(OpenCliError::InvalidInput("douyin_author"));
        }
        let output = self
            .douyin_evidence(profile, "content", video, author)
            .await?;
        parse_content(&output, video, author)
    }
    async fn douyin_evidence(
        &self,
        profile: &str,
        kind: &str,
        id: &str,
        author: Option<&str>,
    ) -> Result<String, OpenCliError> {
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-douyin-reader", kind)
            .await?;
        let mut args = vec![
            "--profile",
            profile,
            "onecrew-douyin-reader",
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

fn parse_comments(
    output: &str,
    video: &str,
    author: &str,
) -> Result<DouyinCommentWindow, OpenCliError> {
    let row: DouyinCommentWindow = one(output)?;
    let mut seen = std::collections::HashSet::new();
    if !valid_video(video)
        || !valid_author(author)
        || row.post_id != video
        || row.author_id != author
        || row.source_url != format!("https://www.douyin.com/video/{video}")
        || row.text.trim().is_empty()
        || row.text.len() > 12000
        || row.text.contains('\0')
        || row.comments.len() > 5
        || row.evidence_source != "visible_same_video_comments"
        || row.comments.iter().any(|c| {
            !valid_video(&c.id)
                || c.parent_post_id != video
                || !seen.insert(&c.id)
                || c.text.len() > 2000
                || c.text.contains('\0')
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const AUTHOR: &str = "MS4wLjABAAAAg5NIc2YQ9j1jGu9OUd4tJQc_C9cDFfpTWiLKTwWztLg";
    const VIDEO: &str = "7688639864627214835";
    #[test]
    fn comments_require_same_video_native_ids_and_first_five_positions() {
        let row = json!({"post_id":VIDEO,"author_id":AUTHOR,"source_url":format!("https://www.douyin.com/video/{VIDEO}"),
            "text":"Original caption","text_truncated":false,"comments":[{"id":"101","parent_post_id":VIDEO,"text":""}],
            "evidence_source":"visible_same_video_comments"});
        assert!(parse_comments(&json!([row.clone()]).to_string(), VIDEO, AUTHOR).is_ok());
        for (field, bad) in [
            ("post_id", json!("99")),
            ("author_id", json!("23667677481")),
            ("session", json!("private")),
        ] {
            let mut wrong = row.clone();
            wrong[field] = bad;
            assert!(parse_comments(&json!([wrong]).to_string(), VIDEO, AUTHOR).is_err());
        }
        let mut wrong = row.clone();
        wrong["comments"][0]["parent_post_id"] = json!("99");
        assert!(parse_comments(&json!([wrong]).to_string(), VIDEO, AUTHOR).is_err());
        let mut wrong = row.clone();
        wrong["comments"] = json!(
            (0..6)
                .map(|i| json!({"id":format!("10{i}"),"parent_post_id":VIDEO,"text":"same"}))
                .collect::<Vec<_>>()
        );
        assert!(parse_comments(&json!([wrong]).to_string(), VIDEO, AUTHOR).is_err());
        let mut wrong = row;
        let duplicate = wrong["comments"][0].clone();
        wrong["comments"].as_array_mut().unwrap().push(duplicate);
        assert!(parse_comments(&json!([wrong]).to_string(), VIDEO, AUTHOR).is_err());
    }
    fn profile() -> serde_json::Value {
        json!({"adapter_version":"1.0.0","author_id":AUTHOR,"author_label":"Creator","profile_url":format!("https://www.douyin.com/user/{AUTHOR}"),
            "follower_count":4892,"follower_text":"4892","bio":"Business creator@example.com","bio_truncated":false,"public_links":["https://creator.example/contact"],"evidence_source":"visible_profile_header"})
    }
    fn content() -> serde_json::Value {
        json!({"post_id":VIDEO,"author_id":AUTHOR,"author_label":"Creator","profile_url":format!("https://www.douyin.com/user/{AUTHOR}"),
            "source_url":format!("https://www.douyin.com/video/{VIDEO}"),"text":"原始键盘实测标题","text_truncated":false,"evidence_source":"visible_native_video_caption"})
    }
    #[test]
    fn profile_requires_native_owner_exact_counts_and_public_fields() {
        let mut row = profile();
        assert_eq!(
            parse_profile(&json!([row.clone()]).to_string(), AUTHOR)
                .unwrap()
                .follower_count,
            Some(4892)
        );
        row["follower_text"] = json!("4.8万");
        assert!(parse_profile(&json!([row.clone()]).to_string(), AUTHOR).is_err());
        row["follower_count"] = serde_json::Value::Null;
        assert!(parse_profile(&json!([row.clone()]).to_string(), AUTHOR).is_ok());
        row["operator"] = json!("private");
        assert!(parse_profile(&json!([row]).to_string(), AUTHOR).is_err());
        assert!(parse_profile(&json!([profile()]).to_string(), "23667677481").is_err());
        let mut row = profile();
        row["public_links"] = json!(["https://127.0.0.1/"]);
        assert!(parse_profile(&json!([row]).to_string(), AUTHOR).is_err());
    }
    #[test]
    fn matched_video_identity_and_original_caption_are_source_bound() {
        let mut row = content();
        assert!(parse_content(&json!([row.clone()]).to_string(), VIDEO, None).is_ok());
        assert!(parse_content(&json!([row.clone()]).to_string(), VIDEO, Some(AUTHOR)).is_ok());
        assert!(
            parse_content(
                &json!([row.clone()]).to_string(),
                VIDEO,
                Some("23667677481")
            )
            .is_err()
        );
        row["source_url"] = json!(format!(
            "https://www.douyin.com/video/{VIDEO}?author_id=123"
        ));
        assert!(parse_content(&json!([row]).to_string(), VIDEO, None).is_err());
        let mut row = content();
        row["comments"] = json!([]);
        assert!(parse_content(&json!([row]).to_string(), VIDEO, None).is_err());
        assert!(parse_content(&json!([content(), content()]).to_string(), VIDEO, None).is_err());
        let mut row = content();
        row["text"] = json!("語".repeat(4100));
        assert!(parse_content(&json!([row]).to_string(), VIDEO, None).is_err());
        assert!(matches!(
            parse_content(&" ".repeat(16001), VIDEO, None),
            Err(OpenCliError::OutputTooLarge)
        ));
    }
}
