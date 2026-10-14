//! Original Zhihu evidence, bound to observed native identities and exact sources.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ZhihuOriginalComment {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ZhihuPublicProfile {
    pub author_id: String,
    pub url_token: String,
    pub author_label: String,
    pub author_url: String,
    pub follower_count: Option<u64>,
    pub bio: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ZhihuOriginalAnswer {
    pub author_id: String,
    pub url_token: String,
    pub author_label: String,
    pub author_url: String,
    pub answer_id: String,
    pub question_id: String,
    pub source_url: String,
    pub text: String,
    pub public_links: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ZhihuOriginalArticle {
    pub author_id: String,
    pub url_token: String,
    pub author_label: String,
    pub author_url: String,
    pub article_id: String,
    pub source_url: String,
    pub title: String,
    pub text: String,
    pub public_links: Vec<String>,
}

fn parse_article(
    output: &str,
    id: &str,
    source: &str,
) -> Result<ZhihuOriginalArticle, OpenCliError> {
    let row: ZhihuOriginalArticle =
        serde_json::from_value(one_value(output)?).map_err(|_| OpenCliError::InvalidOutput)?;
    if row.author_id != id
        || row.source_url != source
        || !author_matches(
            &row.author_id,
            &row.url_token,
            &row.author_label,
            &row.author_url,
        )
        || !valid_number(&row.article_id)
        || row.source_url != format!("https://zhuanlan.zhihu.com/p/{}", row.article_id)
        || row.title.len() > 2000
        || row.title.contains('\0')
        || row.text.len() > 16000
        || row.text.contains('\0')
        || row.public_links.len() > 10
        || row.public_links.iter().any(|link| !valid_link(link))
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn valid_token(token: &str) -> bool {
    (1..=128).contains(&token.len())
        && token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}
fn valid_number(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|c| c.is_ascii_digit())
}
fn author_matches(id: &str, token: &str, label: &str, source: &str) -> bool {
    valid_id(id)
        && valid_token(token)
        && !label.trim().is_empty()
        && label.len() <= 512
        && !label.chars().any(char::is_control)
        && source == format!("https://www.zhihu.com/people/{token}")
}
fn one_value(output: &str) -> Result<serde_json::Value, OpenCliError> {
    if output.len() > 60000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(rows.into_iter().next().expect("one row"))
}
fn parse_profile(output: &str, id: &str, token: &str) -> Result<ZhihuPublicProfile, OpenCliError> {
    let value = one_value(output)?;
    if value.get("follower_count").is_none() {
        return Err(OpenCliError::InvalidOutput);
    }
    let row: ZhihuPublicProfile =
        serde_json::from_value(value).map_err(|_| OpenCliError::InvalidOutput)?;
    if row.author_id != id
        || row.url_token != token
        || !author_matches(
            &row.author_id,
            &row.url_token,
            &row.author_label,
            &row.author_url,
        )
        || row.bio.len() > 8000
        || row.bio.contains('\0')
        || row
            .follower_count
            .is_some_and(|count| count > 9_007_199_254_740_991)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}
fn parse_answer(output: &str, id: &str, source: &str) -> Result<ZhihuOriginalAnswer, OpenCliError> {
    let row: ZhihuOriginalAnswer =
        serde_json::from_value(one_value(output)?).map_err(|_| OpenCliError::InvalidOutput)?;
    if row.author_id != id
        || row.source_url != source
        || !author_matches(
            &row.author_id,
            &row.url_token,
            &row.author_label,
            &row.author_url,
        )
        || !valid_number(&row.answer_id)
        || !valid_number(&row.question_id)
        || row.source_url
            != format!(
                "https://www.zhihu.com/question/{}/answer/{}",
                row.question_id, row.answer_id
            )
        || row.text.len() > 16000
        || row.text.contains('\0')
        || row.public_links.len() > 10
        || row.public_links.iter().any(|link| !valid_link(link))
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}
fn valid_link(link: &str) -> bool {
    if link.len() > 2048 || link.chars().any(char::is_control) {
        return false;
    }
    url::Url::parse(link).is_ok_and(|url| {
        url.username().is_empty()
            && url.password().is_none()
            && (url.scheme() == "mailto" || (url.scheme() == "https" && url.host_str().is_some()))
    })
}
pub(crate) fn parse_search(
    row: &serde_json::Value,
) -> Result<crate::CreatorSearchHit, OpenCliError> {
    let field = |key| {
        row.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    };
    let id = field("author_id");
    let token = field("url_token");
    let answer = field("answer_id");
    let question = field("question_id");
    let article = field("article_id");
    let source_matches = if !article.is_empty() {
        answer.is_empty()
            && question.is_empty()
            && valid_number(article)
            && field("source_url") == format!("https://zhuanlan.zhihu.com/p/{article}")
    } else {
        valid_number(answer)
            && valid_number(question)
            && field("source_url")
                == format!("https://www.zhihu.com/question/{question}/answer/{answer}")
    };
    if !author_matches(id, token, field("author_label"), field("author_url"))
        || !source_matches
        || field("title").len() > 2000
        || field("title").contains('\0')
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(crate::CreatorSearchHit {
        source_url: field("source_url").into(),
        title: field("title").into(),
        author_label: field("author_label").into(),
        author_id: Some(id.into()),
        author_url: Some(field("author_url").into()),
    })
}
impl OpenCli {
    pub async fn zhihu_original_comments(
        &self,
        profile: &str,
        source: &str,
    ) -> Result<Vec<ZhihuOriginalComment>, OpenCliError> {
        let (kind, id) = if let Some(id) = source.strip_prefix("https://zhuanlan.zhihu.com/p/") {
            ("article-comments", id)
        } else {
            let (question, id) = source
                .strip_prefix("https://www.zhihu.com/question/")
                .and_then(|s| s.split_once("/answer/"))
                .ok_or(OpenCliError::InvalidInput("zhihu_comments"))?;
            if !valid_number(question) {
                return Err(OpenCliError::InvalidInput("zhihu_comments"));
            }
            ("answer-comments", id)
        };
        if !valid_number(id) {
            return Err(OpenCliError::InvalidInput("zhihu_comments"));
        }
        let output = self.zhihu_read(profile, kind, id).await?;
        if output.len() > 16000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        let rows: Vec<ZhihuOriginalComment> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        let mut seen = std::collections::HashSet::new();
        if rows.len() > 5
            || rows.iter().any(|row| {
                !valid_number(&row.id)
                    || !seen.insert(&row.id)
                    || row.text.len() > 2000
                    || row.text.contains('\0')
            })
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(rows)
    }
    pub async fn zhihu_original_article(
        &self,
        profile: &str,
        author_id: &str,
        source: &str,
    ) -> Result<ZhihuOriginalArticle, OpenCliError> {
        let article = source
            .strip_prefix("https://zhuanlan.zhihu.com/p/")
            .ok_or(OpenCliError::InvalidInput("zhihu_article"))?;
        if !valid_id(author_id) || !valid_number(article) {
            return Err(OpenCliError::InvalidInput("zhihu_article"));
        }
        parse_article(
            &self.zhihu_read(profile, "article", article).await?,
            author_id,
            source,
        )
    }
    async fn zhihu_read(
        &self,
        profile: &str,
        kind: &str,
        target: &str,
    ) -> Result<String, OpenCliError> {
        if crate::creator_search::RETIRED_CREATOR_SOURCES.contains(&"zhihu") {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-zhihu-reader", kind)
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-zhihu-reader",
                kind,
                "-f",
                "json",
                "--",
                target,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        Ok(output)
    }
    pub async fn zhihu_public_profile(
        &self,
        profile: &str,
        author_id: &str,
        url_token: &str,
    ) -> Result<ZhihuPublicProfile, OpenCliError> {
        if !valid_id(author_id) || !valid_token(url_token) {
            return Err(OpenCliError::InvalidInput("zhihu_author"));
        }
        parse_profile(
            &self.zhihu_read(profile, "profile", url_token).await?,
            author_id,
            url_token,
        )
    }
    pub async fn zhihu_original_answer(
        &self,
        profile: &str,
        author_id: &str,
        source: &str,
    ) -> Result<ZhihuOriginalAnswer, OpenCliError> {
        let path = source
            .strip_prefix("https://www.zhihu.com/question/")
            .ok_or(OpenCliError::InvalidInput("zhihu_answer"))?;
        let (question, answer) = path
            .split_once("/answer/")
            .ok_or(OpenCliError::InvalidInput("zhihu_answer"))?;
        if !valid_id(author_id) || !valid_number(question) || !valid_number(answer) {
            return Err(OpenCliError::InvalidInput("zhihu_answer"));
        }
        parse_answer(
            &self.zhihu_read(profile, "answer", answer).await?,
            author_id,
            source,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn article_identity_cannot_be_replaced_by_answer_or_another_author() {
        let mut row = author();
        row["article_id"] = "789".into();
        row["title"] = "Review".into();
        row["text"] = "contact@example.com".into();
        row["public_links"] = json!([]);
        let source = "https://zhuanlan.zhihu.com/p/789";
        row["source_url"] = source.into();
        assert!(parse_article(&json!([row]).to_string(), &"a".repeat(32), source).is_ok());
        assert!(parse_article(&json!([row]).to_string(), &"b".repeat(32), source).is_err());
        assert!(parse_search(&row).is_ok());
        row["answer_id"] = "123".into();
        assert!(parse_search(&row).is_err());
    }
    fn author() -> serde_json::Value {
        json!({"author_id":"a".repeat(32),"url_token":"creator-123","author_label":"Creator","author_url":"https://www.zhihu.com/people/creator-123"})
    }
    #[test]
    fn profile_requires_observed_identity_and_explicit_count() {
        let mut row = author();
        row["bio"] = "contact@example.com".into();
        row["follower_count"] = serde_json::Value::Null;
        assert_eq!(
            parse_profile(&json!([row]).to_string(), &"a".repeat(32), "creator-123")
                .unwrap()
                .follower_count,
            None
        );
        assert!(parse_profile(&json!([row]).to_string(), &"b".repeat(32), "creator-123").is_err());
        row.as_object_mut().unwrap().remove("follower_count");
        assert!(parse_profile(&json!([row]).to_string(), &"a".repeat(32), "creator-123").is_err());
    }
    #[test]
    fn answer_rejects_wrong_owner_source_and_extra_fields() {
        let mut row = author();
        row["answer_id"] = "123".into();
        row["question_id"] = "456".into();
        let source = "https://www.zhihu.com/question/456/answer/123";
        row["source_url"] = source.into();
        row["text"] = "原创正文".into();
        row["public_links"] = json!(["mailto:contact@example.com"]);
        assert!(parse_answer(&json!([row]).to_string(), &"a".repeat(32), source).is_ok());
        assert!(parse_answer(&json!([row]).to_string(), &"b".repeat(32), source).is_err());
        row["cookie"] = "forbidden".into();
        assert!(parse_answer(&json!([row]).to_string(), &"a".repeat(32), source).is_err());
    }
    #[test]
    fn search_binds_answer_to_native_author_not_label() {
        let mut row = author();
        row["answer_id"] = "123".into();
        row["question_id"] = "456".into();
        row["source_url"] = "https://www.zhihu.com/question/456/answer/123".into();
        row["title"] = "键盘".into();
        assert_eq!(parse_search(&row).unwrap().author_id, Some("a".repeat(32)));
        row["author_id"] = "Creator".into();
        assert!(parse_search(&row).is_err());
    }
}
