//! One observed Weibo source, preserving native post/author binding and original
//! API text. Reposted text is never projected as the source author's own writing.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WeiboPostEvidence {
    pub post_id: String,
    pub author_id: String,
    pub author: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
    /// Upstream long-text retrieval can fail while returning a usable excerpt.
    /// This must never be represented as proof that the whole post was read.
    pub text_completeness_known: bool,
    pub is_repost: bool,
}

pub(crate) fn target(source: &str) -> Result<(&str, &str), OpenCliError> {
    let (author, post) = source
        .strip_prefix("https://weibo.com/")
        .and_then(|path| path.split_once('/'))
        .ok_or(OpenCliError::InvalidInput("weibo_post_source"))?;
    // Upstream emits u.id as a JavaScript number. Refuse IDs whose exact value
    // could have been rounded, rather than trusting an approximate URL.
    if !numeric(author)
        || author
            .parse::<u64>()
            .ok()
            .is_none_or(|id| id > 9_007_199_254_740_991)
        || post.is_empty()
        || post.len() > 20
        || !post.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(OpenCliError::InvalidInput("weibo_post_source"));
    }
    Ok((author, post))
}

fn numeric(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('0')
        && id.len() <= 20
        && id.bytes().all(|b| b.is_ascii_digit())
}

impl OpenCli {
    /// At most the native post request and one long-text request. No profile
    /// lookup, comment expansion, Campaign write, or qualification happens here.
    pub async fn weibo_post_evidence(
        &self,
        profile: &str,
        source_url: &str,
    ) -> Result<WeiboPostEvidence, OpenCliError> {
        let (_, post) = target(source_url)?;
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-weibo-post", "post")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-weibo-post",
                "post",
                "-f",
                "json",
                "--",
                post,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        parse(&output, source_url)
    }
}

fn parse(output: &str, source: &str) -> Result<WeiboPostEvidence, OpenCliError> {
    if output.len() > 128_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let (author, post) = target(source)?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Field {
        field: String,
        value: String,
    }
    let rows: Vec<Field> = serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    let mut fields = BTreeMap::new();
    for row in rows {
        if ![
            "id",
            "mblogid",
            "author",
            "text",
            "created_at",
            "source",
            "reposts",
            "comments",
            "likes",
            "pic_count",
            "url",
            "retweeted_from",
            "retweeted_text",
        ]
        .contains(&row.field.as_str())
            || fields.insert(row.field, row.value).is_some()
        {
            return Err(OpenCliError::InvalidOutput);
        }
    }
    let get = |key: &str| {
        fields
            .get(key)
            .map(String::as_str)
            .ok_or(OpenCliError::InvalidOutput)
    };
    let id = get("id")?;
    let name = get("author")?;
    let text = get("text")?;
    if !numeric(id)
        || id
            .parse::<u64>()
            .ok()
            .is_none_or(|id| id > 9_007_199_254_740_991)
        || get("mblogid")? != post
        || get("url")? != source
        || name.trim().is_empty()
        || name.chars().count() > 200
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        || name.chars().any(char::is_control)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(WeiboPostEvidence {
        post_id: id.to_owned(),
        author_id: author.to_owned(),
        author: name.to_owned(),
        source_url: source.to_owned(),
        text: text.chars().take(4000).collect(),
        text_truncated: text.chars().count() > 4000,
        text_completeness_known: false,
        is_repost: fields.contains_key("retweeted_from") || fields.contains_key("retweeted_text"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "https://weibo.com/7407613416/RkNMBxdqR";

    fn fixture(extra: &[(&str, &str)]) -> String {
        let mut fields = vec![
            ("id", "5091234567891234"),
            ("mblogid", "RkNMBxdqR"),
            ("url", SOURCE),
            ("author", "作者"),
            ("text", "作者原文"),
        ];
        fields.extend_from_slice(extra);
        serde_json::to_string(
            &fields
                .into_iter()
                .map(|(field, value)| serde_json::json!({"field":field,"value":value}))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn source_requires_exact_observed_author_and_post() {
        assert_eq!(target(SOURCE).unwrap(), ("7407613416", "RkNMBxdqR"));
        for source in [
            "http://weibo.com/1/abc",
            "https://weibo.com/1/abc?x=1",
            "https://weibo.com/1/abc/",
            "https://weibo.com/9007199254740992/abc",
            "https://weibo.com/0/abc",
            "https://weibo.com/1/--help",
        ] {
            assert!(target(source).is_err());
        }
        assert!(parse(&fixture(&[]).replace("RkNMBxdqR", "other"), SOURCE).is_err());
        assert!(parse(&fixture(&[]).replace("7407613416", "123"), SOURCE).is_err());
        assert!(parse(&fixture(&[("id", "123")]), SOURCE).is_err());
        assert!(
            parse(
                &fixture(&[]).replace("5091234567891234", "9007199254740992"),
                SOURCE
            )
            .is_err()
        );
    }

    #[test]
    fn original_text_never_includes_someone_elses_repost() {
        let post = parse(&fixture(&[("retweeted_text", "其他作者正文")]), SOURCE).unwrap();
        assert_eq!(post.text, "作者原文");
        assert_eq!(post.author_id, "7407613416");
        assert!(post.is_repost);
        assert!(!post.text_completeness_known);
        assert!(!post.text_truncated);
    }

    #[test]
    fn bounded_unicode_excerpt_and_invalid_output() {
        let output = fixture(&[]).replace("作者原文", &"中".repeat(4001));
        let post = parse(&output, SOURCE).unwrap();
        assert_eq!(post.text.chars().count(), 4000);
        assert!(post.text_truncated);
        assert!(parse("[]", SOURCE).is_err());
        assert!(parse(&fixture(&[("unknown", "value")]), SOURCE).is_err());
        assert_eq!(
            parse(&" ".repeat(128001), SOURCE),
            Err(OpenCliError::OutputTooLarge)
        );
    }
}
