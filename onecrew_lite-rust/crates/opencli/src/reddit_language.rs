use crate::{NativeCommentSource, OpenCli, OpenCliError};
use serde_json::{Value, json};

fn base36(value: &str) -> bool {
    (1..=15).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

pub(crate) fn post_id(source: &str) -> Result<&str, OpenCliError> {
    source
        .strip_prefix("https://www.reddit.com/comments/")
        .and_then(|v| v.strip_suffix('/'))
        .filter(|id| base36(id))
        .ok_or(OpenCliError::InvalidInput("reddit_language_source"))
}

fn excerpt(value: &Value, limit: usize) -> Result<String, OpenCliError> {
    let value = value
        .as_str()
        .filter(|s| !s.contains('\0'))
        .ok_or(OpenCliError::InvalidOutput)?;
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    Ok(value[..end].to_owned())
}

fn project(
    rows: &[Value],
    username: &str,
    author_id: &str,
    source: &str,
) -> Result<Value, OpenCliError> {
    let id = post_id(source)?;
    let post = rows.first().ok_or(OpenCliError::InvalidOutput)?;
    if rows.len() > 21
        || post["type"] != "POST"
        || post["id"] != format!("t3_{id}")
        || post["native_author_id"] != author_id
        || post["source_url"] != source
        || !author_id.strip_prefix("t2_").is_some_and(base36)
        || !post["author"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(username))
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let text = excerpt(&post["text"], 8000)?;
    let mut comments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &rows[1..] {
        let url = row["source_url"]
            .as_str()
            .ok_or(OpenCliError::InvalidOutput)?;
        let comment_id = url
            .strip_prefix(&format!("{source}_/"))
            .and_then(|s| s.strip_suffix('/'))
            .filter(|s| base36(s))
            .ok_or(OpenCliError::InvalidOutput)?;
        let body = excerpt(&row["text"], 2000)?;
        if row["type"] != "L0"
            || row["id"] != format!("t1_{comment_id}")
            || body.trim().is_empty()
            || !seen.insert(url)
        {
            return Err(OpenCliError::InvalidOutput);
        }
        comments.push(json!({"source_url":url,"text":body}));
    }
    if text.trim().is_empty() && comments.is_empty() {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(json!({"author_id":author_id,"source_url":source,"text":text,"comments":comments}))
}

impl OpenCli {
    /// Initial content proof contains only the bound author's original post.
    /// The provider's minimal comment envelope is not promoted to author text.
    pub async fn reddit_post_content(
        &self,
        profile: &str,
        author_id: &str,
        source: &str,
    ) -> Result<Value, OpenCliError> {
        let id = post_id(source)?;
        let rows = self
            .native_comments(profile, NativeCommentSource::RedditPost(id), 1)
            .await?;
        project_content(&rows, author_id, source)
    }

    pub async fn reddit_language_sample(
        &self,
        profile: &str,
        username: &str,
        author_id: &str,
        source: &str,
    ) -> Result<Value, OpenCliError> {
        let id = post_id(source)?;
        let rows = self
            .native_comments(profile, NativeCommentSource::RedditPost(id), 5)
            .await?;
        project(&rows, username, author_id, source)
    }
}

fn project_content(rows: &[Value], author_id: &str, source: &str) -> Result<Value, OpenCliError> {
    let post = rows.first().ok_or(OpenCliError::InvalidOutput)?;
    let username = post["author"].as_str().ok_or(OpenCliError::InvalidOutput)?;
    crate::reddit_profile::check_target(username, author_id)?;
    let sample = project(&rows[..1], username, author_id, source)?;
    let truncated = post["text_truncated"]
        .as_bool()
        .ok_or(OpenCliError::InvalidOutput)?;
    Ok(
        json!({"post_id":post_id(source)?,"author_id":author_id,"source_url":source,
        "text":sample["text"],"text_truncated":truncated || post["text"].as_str().unwrap().len() > 8000}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rows() -> Vec<Value> {
        vec![
            json!({"type":"POST","id":"t3_abc","native_author_id":"t2_user","author":"Example","source_url":"https://www.reddit.com/comments/abc/","text":"Original"}),
            json!({"type":"L0","id":"t1_reply","source_url":"https://www.reddit.com/comments/abc/_/reply/","text":"日".repeat(1000),"score":20}),
        ]
    }
    #[test]
    fn original_comment_language_has_bounded_text_and_only_source() {
        let sample = project(
            &rows(),
            "example",
            "t2_user",
            "https://www.reddit.com/comments/abc/",
        )
        .unwrap();
        assert_eq!(sample["comments"][0].as_object().unwrap().len(), 2);
        assert_eq!(sample["comments"][0]["text"].as_str().unwrap().len(), 1998);
    }
    #[test]
    fn rejects_author_or_comment_post_drift() {
        let source = "https://www.reddit.com/comments/abc/";
        assert!(project(&rows(), "other", "t2_user", source).is_err());
        assert!(project(&rows(), "Example", "t2_other", source).is_err());
        let mut wrong = rows();
        wrong[1]["source_url"] = json!("https://www.reddit.com/comments/other/_/reply/");
        assert!(project(&wrong, "Example", "t2_user", source).is_err());
        for source in [
            "https://evil.test/comments/abc/",
            "https://www.reddit.com/comments/abc/?x=1",
            "https://www.reddit.com/comments/abc/../",
        ] {
            assert!(post_id(source).is_err());
        }
    }

    #[test]
    fn initial_post_content_excludes_comments_and_keeps_truncation() {
        let mut rows = rows();
        rows[0]["text_truncated"] = json!(false);
        rows[0]["text"] = json!("日".repeat(3000));
        let value =
            project_content(&rows, "t2_user", "https://www.reddit.com/comments/abc/").unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        assert_eq!(value["text_truncated"], true);
        assert!(value.get("comments").is_none());
        assert_eq!(value["text"].as_str().unwrap().len(), 7998);
        assert!(
            project_content(&rows, "t2_other", "https://www.reddit.com/comments/abc/").is_err()
        );
    }
}
