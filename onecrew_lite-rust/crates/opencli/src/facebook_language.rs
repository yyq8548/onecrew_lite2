use crate::{NativeCommentSource, OpenCli, OpenCliError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacebookReelContent {
    pub post_id: String,
    pub author_id: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacebookLanguageComment {
    pub source_url: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacebookLanguageSample {
    pub author_id: String,
    pub source_url: String,
    pub text: String,
    pub comments: Vec<FacebookLanguageComment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecentReel {
    author_id: String,
    source_url: String,
}

fn recent_urls(output: &str, author_id: &str) -> Result<Vec<String>, OpenCliError> {
    if output.len() > 2048 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let rows: Vec<RecentReel> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if !(1..=3).contains(&rows.len()) {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut urls = Vec::new();
    for row in rows {
        if row.author_id != author_id
            || reel_id(&row.source_url).is_err()
            || urls.contains(&row.source_url)
        {
            return Err(OpenCliError::InvalidOutput);
        }
        urls.push(row.source_url);
    }
    Ok(urls)
}

#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Row {
    #[serde(rename = "POST")]
    Post {
        source_url: String,
        native_author_id: String,
        author_url: String,
        identity_source: String,
        text: String,
        text_truncated: bool,
    },
    #[serde(rename = "COMMENT")]
    Comment {
        source_url: String,
        text: String,
        text_truncated: bool,
    },
}

pub(crate) fn reel_id(source: &str) -> Result<&str, OpenCliError> {
    source
        .strip_prefix("https://www.facebook.com/reel/")
        .filter(|id| {
            (1..=25).contains(&id.len())
                && !id.starts_with('0')
                && id.bytes().all(|b| b.is_ascii_digit())
        })
        .ok_or(OpenCliError::InvalidInput("facebook_reel_source"))
}

fn excerpt(text: String, chars: usize, bytes: usize) -> Result<(String, bool), OpenCliError> {
    if text.trim().is_empty() || text.contains('\0') || text.chars().count() > chars {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((text[..end].to_owned(), end < text.len()))
}

fn project(
    rows: Vec<Value>,
    source: &str,
    author_id: &str,
) -> Result<(FacebookReelContent, FacebookLanguageSample), OpenCliError> {
    let id = reel_id(source)?;
    crate::facebook_profile::check_target(author_id)?;
    if !(1..=6).contains(&rows.len()) {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut rows = rows.into_iter().map(serde_json::from_value::<Row>);
    let Row::Post {
        source_url,
        native_author_id,
        author_url,
        identity_source,
        text,
        text_truncated,
    } = rows
        .next()
        .unwrap()
        .map_err(|_| OpenCliError::InvalidOutput)?
    else {
        return Err(OpenCliError::InvalidOutput);
    };
    if source_url != source
        || native_author_id != author_id
        || author_url != format!("https://www.facebook.com/profile.php?id={author_id}")
        || identity_source != "visible_numeric_profile_link"
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let (text, truncated) = excerpt(text, 8000, 8000)?;
    let content = FacebookReelContent {
        post_id: id.to_owned(),
        author_id: author_id.to_owned(),
        source_url: source.to_owned(),
        text: text.clone(),
        text_truncated: text_truncated || truncated,
    };
    let mut comments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let Row::Comment {
            source_url,
            text,
            text_truncated: _truncated,
        } = row.map_err(|_| OpenCliError::InvalidOutput)?
        else {
            return Err(OpenCliError::InvalidOutput);
        };
        let comment_id = source_url
            .strip_prefix(&format!("{source}?comment_id="))
            .filter(|id| {
                (1..=25).contains(&id.len())
                    && !id.starts_with('0')
                    && id.bytes().all(|b| b.is_ascii_digit())
            })
            .ok_or(OpenCliError::InvalidOutput)?;
        if !seen.insert(comment_id.to_owned()) {
            return Err(OpenCliError::InvalidOutput);
        }
        let (text, _) = excerpt(text, 2000, 2000)?;
        comments.push(FacebookLanguageComment { source_url, text });
    }
    Ok((
        content,
        FacebookLanguageSample {
            author_id: author_id.to_owned(),
            source_url: source.to_owned(),
            text,
            comments,
        },
    ))
}

impl OpenCli {
    pub async fn facebook_native_targets(
        &self,
        profile: &str,
        id: &str,
        base: &str,
    ) -> Result<Vec<String>, OpenCliError> {
        crate::facebook_profile::check_target(id)?;
        crate::facebook_identity::check_source(base)?;
        crate::validate_profile(profile)?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-facebook-profile", "native-targets")
            .await?;
        let output = crate::process::run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-facebook-profile",
                "native-targets",
                "-f",
                "json",
                "--",
                id,
                base,
            ],
            crate::READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        recent_urls(&output, id)
    }
    pub async fn facebook_native_original(
        &self,
        profile: &str,
        id: &str,
        base: &str,
        source: &str,
    ) -> Result<FacebookReelContent, OpenCliError> {
        crate::facebook_profile::check_target(id)?;
        crate::facebook_identity::check_source(base)?;
        crate::validate_profile(profile)?;
        reel_id(source)?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-facebook-profile", "native-original")
            .await?;
        let output = crate::process::run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-facebook-profile",
                "native-original",
                "-f",
                "json",
                "--",
                id,
                base,
                source,
            ],
            crate::READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        if output.len() > 80_000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        let mut rows: Vec<FacebookReelContent> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        let mut row = rows.remove(0);
        if row.author_id != id || row.source_url != source || row.post_id != reel_id(source)? {
            return Err(OpenCliError::InvalidOutput);
        }
        let (text, truncated) = excerpt(row.text, 8000, 8000)?;
        row.text = text;
        row.text_truncated |= truncated;
        Ok(row)
    }
    /// These are profile-list locators, not content-author proof. Every caption
    /// or language sample still validates the requested numeric author itself.
    pub async fn facebook_recent_language_urls(
        &self,
        profile: &str,
        author_id: &str,
    ) -> Result<Vec<String>, OpenCliError> {
        crate::facebook_profile::check_target(author_id)?;
        crate::validate_profile(profile)?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-comments-facebook", "recent")
            .await?;
        let output = crate::process::run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-comments-facebook",
                "recent",
                "-f",
                "json",
                "--",
                author_id,
            ],
            crate::READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        recent_urls(&output, author_id)
    }
    /// Only the bound owner's original caption becomes initial content proof.
    pub async fn facebook_reel_content(
        &self,
        profile: &str,
        author_id: &str,
        source: &str,
    ) -> Result<FacebookReelContent, OpenCliError> {
        reel_id(source)?;
        crate::facebook_profile::check_target(author_id)?;
        crate::validate_profile(profile)?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-comments-facebook", "content")
            .await?;
        let output = crate::process::run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-comments-facebook",
                "content",
                "-f",
                "json",
                "--",
                source,
            ],
            crate::READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        crate::addons::require_ready("onecrew-browser-search").await?;
        if output.len() > 40_000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        let rows: Vec<Value> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if rows.len() != 1 {
            return Err(OpenCliError::InvalidOutput);
        }
        project(rows, source, author_id).map(|(content, _)| content)
    }

    pub async fn facebook_language_sample(
        &self,
        profile: &str,
        author_id: &str,
        source: &str,
    ) -> Result<FacebookLanguageSample, OpenCliError> {
        reel_id(source)?;
        crate::facebook_profile::check_target(author_id)?;
        let rows = self
            .native_comments(profile, NativeCommentSource::FacebookReel(source), 5)
            .await?;
        if rows.is_empty() {
            return Err(OpenCliError::InvalidOutput);
        }
        project(rows, source, author_id).map(|(_, sample)| sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires selected connected profile; one saved Facebook original batch, no search"]
    async fn facebook_native_original_acceptance() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser profile");
        let bridge = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            bridge.repair_bundled_addons(true).await.unwrap();
        }
        let bridge = OpenCli::inspect_system().await.unwrap();
        let base = "https://www.facebook.com/LinusTech";
        let id = "100044572203205";
        let start = std::time::Instant::now();
        let sources = bridge
            .facebook_native_targets(&profile, id, base)
            .await
            .unwrap();
        println!(
            "Facebook native targets elapsed_ms={} count={}",
            start.elapsed().as_millis(),
            sources.len()
        );
        assert_eq!(
            sources.first().map(String::as_str),
            Some("https://www.facebook.com/reel/1075265928641482")
        );
        assert_eq!(
            sources.get(1).map(String::as_str),
            Some("https://www.facebook.com/reel/2542739356215319")
        );
        let start = std::time::Instant::now();
        let first = bridge
            .facebook_native_original(&profile, id, base, &sources[0])
            .await;
        println!(
            "Facebook first saved original elapsed_ms={} unavailable={}",
            start.elapsed().as_millis(),
            matches!(first, Err(OpenCliError::InvalidOutput))
        );
        assert!(
            matches!(first, Err(OpenCliError::InvalidOutput)),
            "blank original must not use translated or feed text"
        );
        let start = std::time::Instant::now();
        let second = bridge
            .facebook_native_original(&profile, id, base, &sources[1])
            .await
            .unwrap();
        println!(
            "Facebook next saved original elapsed_ms={} chars={}",
            start.elapsed().as_millis(),
            second.text.chars().count()
        );
        assert_eq!(second.author_id, id);
        assert_eq!(second.source_url, sources[1]);
        assert_eq!(second.text, "What's the Worst Tech Linus Still Uses?");
    }
    use serde_json::json;
    const SOURCE: &str = "https://www.facebook.com/reel/2067046843941874";
    const AUTHOR: &str = "61590270912364";
    fn rows() -> Vec<Value> {
        vec![
            json!({"type":"POST","source_url":SOURCE,"native_author_id":AUTHOR,
            "author_url":format!("https://www.facebook.com/profile.php?id={AUTHOR}"),
            "identity_source":"visible_numeric_profile_link","text":"Original shampoo caption","text_truncated":false}),
            json!({"type":"COMMENT","source_url":format!("{SOURCE}?comment_id=1427989936108468"),
                "text":"Is this ammonia free?","text_truncated":false}),
        ]
    }
    #[test]
    fn author_caption_and_audience_text_remain_separate() {
        let (content, sample) = project(rows(), SOURCE, AUTHOR).unwrap();
        assert_eq!(content.text, "Original shampoo caption");
        assert_eq!(content.post_id, "2067046843941874");
        assert!(
            serde_json::to_value(content)
                .unwrap()
                .get("comments")
                .is_none()
        );
        assert_eq!(
            serde_json::to_value(&sample.comments[0])
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(sample.comments[0].text, "Is this ammonia free?");
    }
    #[test]
    fn reject_drift_ambiguous_comments_and_extra_identity_metadata() {
        for field in [
            "native_author_id",
            "source_url",
            "author_url",
            "identity_source",
        ] {
            let mut rows = rows();
            rows[0][field] = json!("wrong");
            assert!(project(rows, SOURCE, AUTHOR).is_err());
        }
        for url in [
            format!("{SOURCE}?comment_id=123&reply_comment_id=4"),
            format!("{SOURCE}?comment_id=123&comment_id=4"),
            "https://www.facebook.com/reel/123?comment_id=4".into(),
        ] {
            let mut rows = rows();
            rows[1]["source_url"] = json!(url);
            assert!(project(rows, SOURCE, AUTHOR).is_err());
        }
        let mut extra = rows();
        extra[1]["commenter_id"] = json!("private-unneeded");
        assert!(project(extra, SOURCE, AUTHOR).is_err());
        let mut duplicate = rows();
        duplicate.push(duplicate[1].clone());
        assert!(project(duplicate, SOURCE, AUTHOR).is_err());
    }
    #[test]
    fn bound_unicode_and_preserve_caption_truncation() {
        let mut rows = rows();
        rows[0]["text"] = json!("日".repeat(3000));
        rows[1]["text"] = json!("日".repeat(1000));
        let (content, sample) = project(rows, SOURCE, AUTHOR).unwrap();
        assert_eq!(content.text.len(), 7998);
        assert!(content.text_truncated);
        assert_eq!(sample.comments[0].text.len(), 1998);
        let mut empty = self::rows();
        empty.truncate(1);
        let (caption, sample) = project(empty, SOURCE, AUTHOR).unwrap();
        assert_eq!(caption.text, "Original shampoo caption");
        assert!(sample.comments.is_empty());
        let mut nul = self::rows();
        nul[1]["text"] = json!("bad\0text");
        assert!(project(nul, SOURCE, AUTHOR).is_err());
    }
    #[test]
    fn audience_projection_accepts_five_comments_and_rejects_six() {
        let mut evidence = rows();
        evidence.truncate(1);
        for id in 1..=5 {
            evidence.push(
                json!({"type":"COMMENT","source_url":format!("{SOURCE}?comment_id={id}"),
                "text":format!("Original audience comment {id}"),"text_truncated":false}),
            );
        }
        assert_eq!(
            project(evidence.clone(), SOURCE, AUTHOR)
                .unwrap()
                .1
                .comments
                .len(),
            5
        );
        evidence.push(
            json!({"type":"COMMENT","source_url":format!("{SOURCE}?comment_id=6"),
            "text":"Sixth comment","text_truncated":false}),
        );
        assert!(project(evidence, SOURCE, AUTHOR).is_err());
    }
    #[test]
    fn target_is_canonical_and_never_guessed() {
        for source in [
            "https://www.facebook.com/reel/0",
            "https://www.facebook.com/reel/123/",
            "https://www.facebook.com/reel/123?redirect=evil",
            "https://www.facebook.com/creator/posts/123",
        ] {
            assert!(reel_id(source).is_err());
        }
    }

    #[test]
    fn recent_locators_are_distinct_bounded_and_not_author_proof() {
        let row = json!({"author_id":AUTHOR,"source_url":SOURCE});
        assert_eq!(
            recent_urls(&json!([row.clone()]).to_string(), AUTHOR).unwrap(),
            vec![SOURCE]
        );
        assert!(recent_urls(&json!([row.clone(), row.clone()]).to_string(), AUTHOR).is_err());
        assert!(recent_urls(&json!([row.clone()]).to_string(), "999").is_err());
        assert!(recent_urls("[]", AUTHOR).is_err());
        let mut wrong = row.clone();
        wrong["source_url"] = json!(format!("{SOURCE}?x=1"));
        assert!(recent_urls(&json!([wrong]).to_string(), AUTHOR).is_err());
        let mut extra = row;
        extra["private_metadata"] = json!("unneeded");
        assert!(recent_urls(&json!([extra]).to_string(), AUTHOR).is_err());
    }

    #[tokio::test]
    #[ignore = "one public profile-list read; requires exact installed source add-on and explicit acceptance browser profile"]
    async fn facebook_recent_contract_returns_only_observed_profile_reels() {
        let profile =
            std::env::var("OPENCLI_TEST_PROFILE").expect("explicit acceptance browser profile");
        let bridge = OpenCli::inspect_system().await.unwrap();
        let urls = bridge
            .facebook_recent_language_urls(&profile, AUTHOR)
            .await
            .unwrap();
        assert!((1..=3).contains(&urls.len()));
        assert!(urls.iter().all(|url| reel_id(url).is_ok()));
        println!(
            "Facebook observed profile Reel locators PASS; count={}",
            urls.len()
        );
    }

    #[tokio::test]
    #[ignore = "two public reads; requires exact installed source add-on and explicit acceptance browser profile"]
    async fn facebook_reel_contract_matches_observed_numeric_author() {
        let profile =
            std::env::var("OPENCLI_TEST_PROFILE").expect("explicit acceptance browser profile");
        let bridge = OpenCli::inspect_system().await.unwrap();
        let content = bridge
            .facebook_reel_content(&profile, AUTHOR, SOURCE)
            .await
            .unwrap();
        assert_eq!(content.author_id, AUTHOR);
        assert_eq!(content.source_url, SOURCE);
        assert!(!content.text.trim().is_empty());
        let sample = bridge
            .facebook_language_sample(&profile, AUTHOR, SOURCE)
            .await
            .unwrap();
        assert_eq!(sample.author_id, AUTHOR);
        assert!(!sample.comments.is_empty());
        println!(
            "Observed Facebook Reel owner/caption/comment binding PASS; comments={}",
            sample.comments.len()
        );
    }
}
