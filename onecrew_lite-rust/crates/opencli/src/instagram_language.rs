//! Bounded original captions bound to the search-observed native account.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstagramOriginalPost {
    pub post_id: String,
    pub author_id: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
}

fn valid_id(id: &str) -> bool {
    (1..=25).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}

fn parse_originals(output: &str, author: &str) -> Result<Vec<InstagramOriginalPost>, OpenCliError> {
    if output.len() > 30000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let rows: Vec<InstagramOriginalPost> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if !valid_id(author) || rows.len() > 3 {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut seen = std::collections::HashSet::new();
    let mut sources = std::collections::HashSet::new();
    for row in &rows {
        let target = row
            .source_url
            .strip_prefix("https://www.instagram.com/")
            .and_then(|s| s.strip_suffix('/'))
            .and_then(|s| s.split_once('/'));
        let source_valid = target.is_some_and(|(kind, code)| {
            matches!(kind, "p" | "reel")
                && (1..=32).contains(&code.len())
                && code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        });
        if row.author_id != author
            || !valid_id(&row.post_id)
            || !source_valid
            || row.text.len() > 8000
            || row.text.contains('\0')
            || !seen.insert(&row.post_id)
            || !sources.insert(&row.source_url)
        {
            return Err(OpenCliError::InvalidOutput);
        }
    }
    Ok(rows)
}

impl OpenCli {
    /// Comments are audience evidence only. The visible post header must match
    /// the exact username bound by the saved native profile; its caption and
    /// null numeric identity are never promoted to new author evidence.
    pub async fn instagram_comment_language_evidence(
        &self,
        profile: &str,
        username: &str,
        source: &str,
    ) -> Result<serde_json::Value, OpenCliError> {
        let rows = self
            .native_comments(
                profile,
                crate::NativeCommentSource::InstagramPost(source),
                5,
            )
            .await?;
        project_comment_evidence(&rows, username, source)
    }

    pub async fn instagram_recent_original_posts(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
    ) -> Result<Vec<InstagramOriginalPost>, OpenCliError> {
        validate_profile(profile)?;
        if !valid_id(author_id)
            || !(1..=30).contains(&username.len())
            || username != username.to_ascii_lowercase()
            || !username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
        {
            return Err(OpenCliError::InvalidInput("instagram_recent_target"));
        }
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-instagram-profile", "recent")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-instagram-profile",
                "recent",
                "-f",
                "json",
                "--",
                username,
                author_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        parse_originals(&output, author_id)
    }
}

fn project_comment_evidence(
    rows: &[serde_json::Value],
    username: &str,
    source: &str,
) -> Result<serde_json::Value, OpenCliError> {
    let code = source
        .strip_prefix("https://www.instagram.com/")
        .and_then(|s| s.strip_suffix('/'))
        .and_then(|s| s.split_once('/'))
        .filter(|(kind, code)| {
            matches!(*kind, "p" | "reel")
                && (1..=32).contains(&code.len())
                && code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
        .map(|(_, code)| code)
        .ok_or(OpenCliError::InvalidInput("instagram_language_source"))?;
    let post = rows.first().ok_or(OpenCliError::InvalidOutput)?;
    if rows.len() > 6
        || post["type"] != "POST"
        || post["source_url"] != source
        || post["id"] != code
        || post["author"] != username
        || post["author_url"] != format!("https://www.instagram.com/{username}/")
        || post["identity_source"] != "visible_profile_link"
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let prefix = format!("https://www.instagram.com/p/{code}/c/");
    let mut seen = std::collections::HashSet::new();
    let mut comments = Vec::new();
    for row in &rows[1..] {
        let url = row["source_url"]
            .as_str()
            .ok_or(OpenCliError::InvalidOutput)?;
        let id = url
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix('/'))
            .filter(|id| valid_id(id))
            .ok_or(OpenCliError::InvalidOutput)?;
        let text = row["text"].as_str().ok_or(OpenCliError::InvalidOutput)?;
        if row["type"] != "COMMENT"
            || row["id"] != id
            || row["text_truncated"] != false
            || text.trim().is_empty()
            || text.len() > 2000
            || text.contains('\0')
            || !seen.insert(url)
        {
            return Err(OpenCliError::InvalidOutput);
        }
        comments.push(serde_json::json!({"source_url":url,"text":text}));
    }
    Ok(serde_json::json!({"source_url":source,"comments":comments}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instagram_comments_require_exact_post_author_and_original_locators() {
        let source = "https://www.instagram.com/reel/Abc123/";
        let rows = vec![
            serde_json::json!({"type":"POST","id":"Abc123","source_url":source,"author":"creator","author_url":"https://www.instagram.com/creator/","identity_source":"visible_profile_link","native_author_id":null,"text":"Translated caption"}),
            serde_json::json!({"type":"COMMENT","id":"123","source_url":"https://www.instagram.com/p/Abc123/c/123/","text":"原始评论内容","text_truncated":false}),
        ];
        let evidence = project_comment_evidence(&rows, "creator", source).unwrap();
        assert!(evidence.get("author_id").is_none());
        assert!(evidence.get("text").is_none());
        assert_eq!(evidence["comments"].as_array().unwrap().len(), 1);
        for (index, key, value) in [
            (0, "author", serde_json::json!("other")),
            (
                0,
                "source_url",
                serde_json::json!("https://www.instagram.com/p/Other/"),
            ),
            (1, "text_truncated", serde_json::json!(true)),
            (
                1,
                "source_url",
                serde_json::json!("https://www.instagram.com/p/Other/c/123/"),
            ),
            (1, "id", serde_json::json!("999")),
        ] {
            let mut wrong = rows.clone();
            wrong[index][key] = value;
            assert!(project_comment_evidence(&wrong, "creator", source).is_err());
        }
        let mut duplicates = rows.clone();
        duplicates.push(rows[1].clone());
        assert!(project_comment_evidence(&duplicates, "creator", source).is_err());
        let mut six = rows.clone();
        for id in 124..129 {
            six.push(serde_json::json!({"type":"COMMENT","id":id.to_string(),"source_url":format!("https://www.instagram.com/p/Abc123/c/{id}/"),"text":"不同原始评论","text_truncated":false}));
        }
        assert!(project_comment_evidence(&six, "creator", source).is_err());
    }
    #[tokio::test]
    #[ignore = "one bounded native feed diagnosis; prints only reviewed stop markers, explicit profile required"]
    async fn native_feed_failure_has_secret_free_diagnostic() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").unwrap();
        let cli = OpenCli::inspect_system().await.unwrap();
        cli.repair_bundled_addons(true).await.unwrap();
        let cli = OpenCli::inspect_system().await.unwrap();
        let hits = cli
            .creator_search(&profile, "instagram", "shampoo", 1)
            .await
            .unwrap();
        let hit = hits.first().unwrap();
        let author = hit.author_id.as_deref().unwrap();
        let username = hit
            .source_url
            .strip_prefix("https://www.instagram.com/")
            .unwrap()
            .strip_suffix('/')
            .unwrap();
        let mut command = tokio::process::Command::new(&cli.executable);
        #[cfg(windows)]
        {
            command.creation_flags(0x0800_0000);
        }
        if let Some(script) = &cli.script {
            command.arg(script);
        }
        command.kill_on_drop(true).args([
            "--profile",
            &profile,
            "onecrew-instagram-profile",
            "recent",
            "-f",
            "json",
            "--",
            username,
            author,
        ]);
        let output = tokio::time::timeout(READ_TIMEOUT, command.output())
            .await
            .unwrap()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let marker = [
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_PATH_ROOT",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_PATH_API_V1_FEED_USER_NUMERIC_USERNAME",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_PATH_ACCOUNTS_ONETAP",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_PATH_ACCOUNTS_OTHER",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_PATH_API_V1_FEED_USER_OTHER",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_PAGE_ORIGIN",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_ORIGIN",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_SLASH",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_PATH",
            "ONECREW_INSTAGRAM_RECENT_HTTP_400",
            "ONECREW_INSTAGRAM_RECENT_HTTP_404",
            "ONECREW_INSTAGRAM_RECENT_HTTP_500",
            "ONECREW_INSTAGRAM_RECENT_HTTP_503",
            "ONECREW_INSTAGRAM_RECENT_SOURCE",
            "ONECREW_INSTAGRAM_RECENT_JSON",
            "ONECREW_INSTAGRAM_RECENT_INVALID_ENVELOPE",
            "ONECREW_INSTAGRAM_RECENT_INVALID_OWNER",
            "ONECREW_INSTAGRAM_RECENT_INVALID_POST",
            "ONECREW_INSTAGRAM_RECENT_INVALID_CAPTION",
            "ONECREW_INSTAGRAM_RECENT_READ",
            "ONECREW_INSTAGRAM_RECENT_AUTH",
            "ONECREW_INSTAGRAM_RECENT_RISK",
            "ONECREW_INSTAGRAM_RECENT_TIMEOUT",
            "TextEncoder is not defined",
        ]
        .into_iter()
        .find(|marker| stderr.contains(marker))
        .unwrap_or("unclassified_provider_failure");
        println!("Instagram feed diagnostic: {marker}");
        assert!(output.status.success(), "native feed stopped with {marker}");
        let rows = parse_originals(&String::from_utf8(output.stdout).unwrap(), author).unwrap();
        println!("Instagram native original posts={}", rows.len());
    }
    fn row() -> serde_json::Value {
        serde_json::json!({"post_id":"1234","author_id":"123","source_url":"https://www.instagram.com/reel/Code_1/","text":"Original","text_truncated":false})
    }
    #[test]
    fn same_author_original_is_bounded_and_absent_caption_stays_unknown() {
        let mut row = row();
        assert_eq!(
            parse_originals(&serde_json::json!([row]).to_string(), "123").unwrap()[0].text,
            "Original"
        );
        row["text"] = serde_json::json!("");
        assert_eq!(
            parse_originals(&serde_json::json!([row]).to_string(), "123").unwrap()[0].text,
            ""
        );
    }
    #[test]
    fn author_drift_duplicate_oversize_and_unexpected_fields_are_rejected() {
        for (field, value) in [
            ("author_id", serde_json::json!("456")),
            ("post_id", serde_json::json!("0")),
            (
                "source_url",
                serde_json::json!("https://evil.example/p/Code/"),
            ),
            (
                "source_url",
                serde_json::json!("https://www.instagram.com/p/Code/?x=1"),
            ),
            ("text", serde_json::json!("日".repeat(3000))),
            ("commenter_id", serde_json::json!("123")),
        ] {
            let mut r = row();
            r[field] = value;
            assert!(parse_originals(&serde_json::json!([r]).to_string(), "123").is_err());
        }
        assert!(parse_originals(&serde_json::json!([row(), row()]).to_string(), "123").is_err());
        assert!(
            parse_originals(
                &serde_json::json!([row(), row(), row(), row()]).to_string(),
                "123"
            )
            .is_err()
        );
    }
}
