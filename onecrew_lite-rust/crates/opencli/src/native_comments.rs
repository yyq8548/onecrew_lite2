//! Read-only comment access. Native display names are not canonical author IDs;
//! these rows must not be submitted as author-bound language samples as-is.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

pub enum NativeCommentSource<'a> {
    BilibiliVideo(&'a str),
    ZhihuAnswer(&'a str),
    WeiboPost(&'a str),
    RedditPost(&'a str),
    TiktokVideo(&'a str),
    InstagramPost(&'a str),
    FacebookPost(&'a str),
    FacebookReel(&'a str),
}

impl NativeCommentSource<'_> {
    fn bounded_limit(&self, limit: u8) -> Result<u8, OpenCliError> {
        if !(1..=20).contains(&limit) {
            return Err(OpenCliError::InvalidInput("comment_limit"));
        }
        Ok(match self {
            Self::BilibiliVideo(_)
            | Self::WeiboPost(_)
            | Self::InstagramPost(_)
            | Self::FacebookPost(_)
            | Self::FacebookReel(_) => limit.min(5),
            _ => limit,
        })
    }

    fn command(&self) -> Result<(&'static str, &'static str, &str), OpenCliError> {
        let numeric =
            |id: &str| !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit());
        match self {
            Self::FacebookReel(source) => {
                let path = source
                    .strip_prefix("https://www.facebook.com/reel/")
                    .ok_or(OpenCliError::InvalidInput("native_comment_source"))?;
                let id = path.strip_suffix('/').unwrap_or(path);
                if !(1..=25).contains(&id.len())
                    || id.starts_with('0')
                    || !id.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(OpenCliError::InvalidInput("native_comment_source"));
                }
                Ok(("onecrew-comments-facebook", "comments", source))
            }
            Self::FacebookPost(source) => {
                let path = source
                    .strip_prefix("https://www.facebook.com/")
                    .ok_or(OpenCliError::InvalidInput("native_comment_source"))?;
                let (author, post) = path
                    .trim_end_matches('/')
                    .split_once("/posts/")
                    .ok_or(OpenCliError::InvalidInput("native_comment_source"))?;
                if author.is_empty()
                    || author.len() > 100
                    || !author
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.')
                    || !(post.strip_prefix("pfbid").is_some_and(|id| {
                        (10..=150).contains(&id.len())
                            && id.bytes().all(|b| b.is_ascii_alphanumeric())
                    }) || (!post.starts_with('0')
                        && (1..=25).contains(&post.len())
                        && post.bytes().all(|b| b.is_ascii_digit())))
                {
                    return Err(OpenCliError::InvalidInput("native_comment_source"));
                }
                Ok(("onecrew-comments-facebook", "comments", source))
            }
            Self::InstagramPost(source) => {
                let path = source
                    .strip_prefix("https://www.instagram.com/")
                    .and_then(|value| value.strip_suffix('/'))
                    .ok_or(OpenCliError::InvalidInput("native_comment_source"))?;
                let parts: Vec<_> = path.split('/').collect();
                let (kind, code) = match parts.as_slice() {
                    [kind, code] => (*kind, *code),
                    [name, kind, code]
                        if !name.is_empty()
                            && name.len() <= 30
                            && name
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.') =>
                    {
                        (*kind, *code)
                    }
                    _ => return Err(OpenCliError::InvalidInput("native_comment_source")),
                };
                if !matches!(kind, "p" | "reel")
                    || code.is_empty()
                    || code.len() > 32
                    || !code
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                {
                    return Err(OpenCliError::InvalidInput("native_comment_source"));
                }
                Ok(("onecrew-comments-instagram", "comments", source))
            }
            Self::TiktokVideo(source) => {
                let (handle, video) = source
                    .strip_prefix("https://www.tiktok.com/@")
                    .and_then(|path| path.split_once("/video/"))
                    .ok_or(OpenCliError::InvalidInput("native_comment_source"))?;
                if handle.is_empty()
                    || handle.len() > 24
                    || !handle
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
                    || video.is_empty()
                    || video.len() > 25
                    || video.starts_with('0')
                    || !video.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(OpenCliError::InvalidInput("native_comment_source"));
                }
                Ok(("onecrew-comments-tiktok", "comments", source))
            }
            Self::BilibiliVideo(id)
                if id.len() == 12
                    && id.starts_with("BV")
                    && id.bytes().all(|b| b.is_ascii_alphanumeric()) =>
            {
                Ok(("onecrew-comments-bilibili", "comments", id))
            }
            Self::ZhihuAnswer(id) if numeric(id) => {
                Ok(("onecrew-comments-zhihu", "answer-comments", id))
            }
            Self::WeiboPost(id) if numeric(id) => Ok(("onecrew-comments-weibo", "comments", id)),
            Self::RedditPost(id)
                if !id.is_empty()
                    && id.len() <= 15
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) =>
            {
                Ok(("onecrew-comments-reddit", "read", id))
            }
            _ => Err(OpenCliError::InvalidInput("native_comment_source")),
        }
    }
}

impl OpenCli {
    pub async fn native_comments(
        &self,
        profile: &str,
        source: NativeCommentSource<'_>,
        limit: u8,
    ) -> Result<Vec<serde_json::Value>, OpenCliError> {
        validate_profile(profile)?;
        let limit = source.bounded_limit(limit)?;
        let (site, command, id) = source.command()?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, site, command).await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                site,
                command,
                "--limit",
                &limit.to_string(),
                "-f",
                "json",
                "--",
                id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        if output.len() > 128_000 {
            return Err(OpenCliError::OutputTooLarge);
        }
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        // Reddit includes a post header and bounded truncation/context rows.
        if rows.len() > usize::from(limit) * 2 + 3 || rows.iter().any(|row| !row.is_object()) {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_screening_platforms_read_at_most_five_comments() {
        for source in [
            NativeCommentSource::BilibiliVideo("BV1234567890"),
            NativeCommentSource::WeiboPost("123456"),
            NativeCommentSource::InstagramPost("https://www.instagram.com/p/Abc/"),
            NativeCommentSource::FacebookPost("https://www.facebook.com/creator/posts/123"),
            NativeCommentSource::FacebookReel("https://www.facebook.com/reel/123"),
        ] {
            assert_eq!(source.bounded_limit(20).unwrap(), 5);
            assert_eq!(source.bounded_limit(3).unwrap(), 3);
            assert!(source.bounded_limit(0).is_err());
            assert!(source.bounded_limit(21).is_err());
        }
        assert_eq!(
            NativeCommentSource::RedditPost("abc")
                .bounded_limit(20)
                .unwrap(),
            20
        );
    }
    #[test]
    fn facebook_reel_source_preserves_observed_locator_without_url_expansion() {
        let source = "https://www.facebook.com/reel/2067046843941874";
        assert_eq!(
            NativeCommentSource::FacebookReel(source).command().unwrap(),
            ("onecrew-comments-facebook", "comments", source)
        );
        for invalid in [
            "https://www.facebook.com/reel/0",
            "https://www.facebook.com/reel/123//",
            "https://www.facebook.com/reel/123?comment_id=456",
            "https://www.facebook.com/reel/123#fragment",
            "https://www.facebook.com.evil/reel/123",
        ] {
            assert!(
                NativeCommentSource::FacebookReel(invalid)
                    .command()
                    .is_err()
            );
        }
    }
    #[test]
    fn comment_sources_cannot_select_write_commands_or_arbitrary_urls() {
        assert!(
            NativeCommentSource::FacebookPost(
                "https://www.facebook.com/creator/posts/pfbid0123456789ABC"
            )
            .command()
            .is_ok()
        );
        for source in [
            "https://www.facebook.com.evil/creator/posts/123",
            "https://www.facebook.com/creator/posts/123?redirect=evil",
            "https://www.facebook.com/creator/posts/0",
            "https://www.facebook.com/groups/123",
        ] {
            assert!(NativeCommentSource::FacebookPost(source).command().is_err());
        }
        assert!(
            NativeCommentSource::InstagramPost("https://www.instagram.com/creator/reel/Abc_123/")
                .command()
                .is_ok()
        );
        for source in [
            "https://www.instagram.com/direct/123/",
            "https://www.instagram.com/p/Abc/?x=1",
            "https://evil.example/p/Abc/",
        ] {
            assert!(
                NativeCommentSource::InstagramPost(source)
                    .command()
                    .is_err()
            );
        }
        assert!(
            NativeCommentSource::TiktokVideo("https://www.tiktok.com/@creator/video/12345")
                .command()
                .is_ok()
        );
        for source in [
            "https://evil.example/@a/video/123",
            "https://www.tiktok.com/@a/video/123?limit=100",
            "https://www.tiktok.com/@a/video/0",
        ] {
            assert!(NativeCommentSource::TiktokVideo(source).command().is_err());
        }
        assert_eq!(
            NativeCommentSource::ZhihuAnswer("123").command().unwrap().1,
            "answer-comments"
        );
        assert_eq!(
            NativeCommentSource::WeiboPost("123").command().unwrap().1,
            "comments"
        );
        assert!(NativeCommentSource::RedditPost("1abc23").command().is_ok());
        assert!(
            NativeCommentSource::RedditPost("https://evil.example")
                .command()
                .is_err()
        );
        assert!(
            NativeCommentSource::BilibiliVideo("--help")
                .command()
                .is_err()
        );
        assert!(NativeCommentSource::ZhihuAnswer("1/2").command().is_err());
    }
}
