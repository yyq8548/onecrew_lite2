use crate::{NativeCommentSource, OpenCli, OpenCliError, check_tiktok_profile_target};
use serde_json::{Value, json};

fn excerpt(value: &Value, max: usize) -> Result<String, OpenCliError> {
    let text = value.as_str().ok_or(OpenCliError::InvalidOutput)?;
    if text.contains('\0') {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(text[..end].to_owned())
}

fn project(
    rows: &[Value],
    source: &str,
    username: &str,
    author_id: &str,
) -> Result<Value, OpenCliError> {
    let post = rows.first().ok_or(OpenCliError::InvalidOutput)?;
    if rows.len() > 21
        || post["type"] != "POST"
        || post["source_url"] != source
        || post["native_author_id"] != author_id
        || post["author"] != username
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let text = excerpt(&post["text"], 8000)?;
    let mut comments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &rows[1..] {
        if row["type"] != "COMMENT" || row["source_url"] != source {
            return Err(OpenCliError::InvalidOutput);
        }
        let comment = excerpt(&row["text"], 2000)?;
        if comment.trim().is_empty() {
            return Err(OpenCliError::InvalidOutput);
        }
        if seen.insert(comment.clone()) {
            comments.push(json!({"source_url":source,"text":comment}));
        }
    }
    if text.trim().is_empty() && comments.is_empty() {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(json!({"author_id":author_id,"source_url":source,"text":text,"comments":comments}))
}

impl OpenCli {
    pub async fn tiktok_recent_language_urls(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
    ) -> Result<Vec<String>, OpenCliError> {
        check_tiktok_profile_target(username, author_id)?;
        crate::validate_profile(profile)?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        crate::addons::require_ready("onecrew-tiktok-profile").await?;
        self.require_browser_read(profile, "onecrew-comments-tiktok", "recent")
            .await?;
        let output = crate::process::run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-comments-tiktok",
                "recent",
                "--author-id",
                author_id,
                "-f",
                "json",
                "--",
                username,
            ],
            crate::READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        crate::addons::require_ready("onecrew-tiktok-profile").await?;
        if output.len() > 8192 {
            return Err(OpenCliError::OutputTooLarge);
        }
        let rows: Vec<Value> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.is_empty() || rows.len() > 3 {
            return Err(OpenCliError::InvalidOutput);
        }
        let prefix = format!("https://www.tiktok.com/@{username}/video/");
        let mut urls = Vec::new();
        for row in rows {
            let source = row["source_url"]
                .as_str()
                .ok_or(OpenCliError::InvalidOutput)?;
            if row.as_object().is_none_or(|r| r.len() != 1)
                || !source.strip_prefix(&prefix).is_some_and(|id| {
                    !id.is_empty()
                        && !id.starts_with('0')
                        && id.len() <= 25
                        && id.bytes().all(|b| b.is_ascii_digit())
                })
                || urls.iter().any(|u| u == source)
            {
                return Err(OpenCliError::InvalidOutput);
            }
            urls.push(source.to_owned());
        }
        Ok(urls)
    }
    /// Read one exact post's language evidence. Only the post author is identity-bound.
    pub async fn tiktok_language_sample(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
        source: &str,
    ) -> Result<Value, OpenCliError> {
        check_tiktok_profile_target(username, author_id)?;
        let prefix = format!("https://www.tiktok.com/@{username}/video/");
        if !source.strip_prefix(&prefix).is_some_and(|id| {
            !id.is_empty()
                && !id.starts_with('0')
                && id.len() <= 25
                && id.bytes().all(|b| b.is_ascii_digit())
        }) {
            return Err(OpenCliError::InvalidInput("tiktok_language_source"));
        }
        let rows = self
            .native_comments(profile, NativeCommentSource::TiktokVideo(source), 5)
            .await?;
        project(&rows, source, username, author_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "https://www.tiktok.com/@creator/video/12345";
    fn rows() -> Vec<Value> {
        vec![
            json!({"type":"POST","source_url":SOURCE,"native_author_id":"42","author":"creator","text":"原始内容"}),
            json!({"type":"COMMENT","source_url":SOURCE,"text":"中文评论"}),
        ]
    }
    #[test]
    fn language_material_needs_no_commenter_identity() {
        let value = project(&rows(), SOURCE, "creator", "42").unwrap();
        assert_eq!(
            value["comments"][0],
            json!({"source_url":SOURCE,"text":"中文评论"})
        );
        assert!(project(&rows(), SOURCE, "other", "42").is_err());
        assert!(project(&rows(), SOURCE, "creator", "43").is_err());
        let mut wrong = rows();
        wrong[1]["source_url"] = json!("other");
        assert!(project(&wrong, SOURCE, "creator", "42").is_err());
    }
    #[test]
    fn byte_bounds_preserve_unicode_and_do_not_repeat_truncated_text() {
        let mut rows = rows();
        rows[1]["text"] = json!("中".repeat(1000));
        rows.push(rows[1].clone());
        let value = project(&rows, SOURCE, "creator", "42").unwrap();
        assert_eq!(value["comments"].as_array().unwrap().len(), 1);
        assert_eq!(value["comments"][0]["text"].as_str().unwrap().len(), 1998);
    }
}
