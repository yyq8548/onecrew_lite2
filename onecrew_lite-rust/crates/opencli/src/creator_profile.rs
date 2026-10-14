//! Fixed profile reads for observed search authors; never accepts a CLI command.
use crate::{OpenCli, OpenCliError};

impl OpenCli {
    pub async fn creator_profile_evidence(
        &self,
        profile: &str,
        site: &str,
        author_id: &str,
        author_url: &str,
    ) -> Result<serde_json::Value, OpenCliError> {
        let username = check_profile_source(site, author_id, author_url)?;
        match site {
            // This input is the observed /in/ locator. The returned receipt
            // carries the separately observed native account ID for ownership.
            "linkedin" => {
                serde_json::to_value(self.linkedin_public_profile(profile, &username).await?)
                    .map_err(|_| OpenCliError::InvalidOutput)
            }
            "pixiv" => serde_json::to_value(self.pixiv_public_profile(profile, author_id).await?)
                .map_err(|_| OpenCliError::InvalidOutput),
            "pinterest" => serde_json::to_value(
                self.pinterest_public_profile(profile, author_id, &username)
                    .await?,
            )
            .map_err(|_| OpenCliError::InvalidOutput),
            "hackernews" => {
                serde_json::to_value(self.hackernews_public_profile(profile, author_id).await?)
                    .map_err(|_| OpenCliError::InvalidOutput)
            }
            "zhihu" => serde_json::to_value(
                self.zhihu_public_profile(profile, author_id, &username)
                    .await?,
            )
            .map_err(|_| OpenCliError::InvalidOutput),
            "bluesky" => {
                serde_json::to_value(self.bluesky_public_profile(profile, author_id).await?)
                    .map_err(|_| OpenCliError::InvalidOutput)
            }
            "weibo" => serde_json::to_value(self.weibo_public_profile(profile, author_id).await?)
                .map_err(|_| OpenCliError::InvalidOutput),
            "facebook" => {
                serde_json::to_value(self.facebook_public_profile(profile, author_id).await?)
                    .map_err(|_| OpenCliError::InvalidOutput)
            }
            "instagram" => {
                let result = self.instagram_public_profile(profile, &username, author_id).await?;
                if result.id != author_id {
                    return Err(OpenCliError::InvalidOutput);
                }
                serde_json::to_value(result).map_err(|_| OpenCliError::InvalidOutput)
            }
            "bilibili" => {
                serde_json::to_value(self.bilibili_public_profile(profile, author_id).await?)
                    .map_err(|_| OpenCliError::InvalidOutput)
            }
            "tiktok" => serde_json::to_value(
                self.tiktok_public_profile(profile, &username, author_id)
                    .await?,
            )
            .map_err(|_| OpenCliError::InvalidOutput),
            "twitter" => {
                serde_json::to_value(self.twitter_public_profile(profile, &username).await?)
                    .map_err(|_| OpenCliError::InvalidOutput)
            }
            "reddit" => serde_json::to_value(
                self.reddit_public_profile(profile, &username, author_id)
                    .await?,
            )
            .map_err(|_| OpenCliError::InvalidOutput),
            _ => Err(OpenCliError::ReadCommandUnavailable),
        }
    }
}

fn check_profile_source(site: &str, id: &str, source: &str) -> Result<String, OpenCliError> {
    match site {
        "linkedin" => {
            if !crate::linkedin::valid_handle(id)
                || source != format!("https://www.linkedin.com/in/{id}/")
            {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(id.into())
        }
        "pixiv" => {
            if !crate::pixiv::valid_id(id) || source != format!("https://www.pixiv.net/users/{id}")
            {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(String::new())
        }
        "pinterest" => {
            let username = source
                .strip_prefix("https://www.pinterest.com/")
                .and_then(|path| path.strip_suffix('/'))
                .ok_or(OpenCliError::InvalidInput("profile_source"))?;
            if !crate::pinterest::valid_id(id) || !crate::pinterest::valid_username(username) {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(username.into())
        }
        "zhihu" => {
            let token = source
                .strip_prefix("https://www.zhihu.com/people/")
                .ok_or(OpenCliError::InvalidInput("profile_source"))?;
            if id.len() != 32
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !(1..=128).contains(&token.len())
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(token.into())
        }
        "bluesky" => {
            if !crate::bluesky::valid_did(id) || source != format!("https://bsky.app/profile/{id}")
            {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(String::new())
        }
        "hackernews" => {
            if !crate::hackernews::valid_username(id)
                || source != format!("https://news.ycombinator.com/user?id={id}")
            {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(String::new())
        }
        "weibo" => {
            crate::weibo_profile::check_target(id, source)?;
            Ok(String::new())
        }
        "facebook" => {
            crate::facebook_profile::check_target(id)?;
            if source != format!("https://www.facebook.com/profile.php?id={id}") {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(String::new())
        }
        "instagram" => {
            let username = source
                .strip_prefix("https://www.instagram.com/")
                .and_then(|value| value.strip_suffix('/'))
                .ok_or(OpenCliError::InvalidInput("profile_source"))?;
            if !(1..=25).contains(&id.len())
                || id.starts_with('0')
                || !id.bytes().all(|b| b.is_ascii_digit())
                || !(1..=30).contains(&username.len())
                || username != username.to_ascii_lowercase()
                || !username
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
            {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(username.into())
        }
        "bilibili" => {
            crate::check_bilibili_profile_target(id)?;
            if source != format!("https://space.bilibili.com/{id}") {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(String::new())
        }
        "tiktok" => {
            let username = source
                .strip_prefix("https://www.tiktok.com/@")
                .ok_or(OpenCliError::InvalidInput("profile_source"))?;
            crate::check_tiktok_profile_target(username, id)?;
            Ok(username.into())
        }
        "twitter" => {
            if !crate::twitter_profile::valid_handle(id)
                || id != id.to_ascii_lowercase()
                || source != format!("https://x.com/{id}")
            {
                return Err(OpenCliError::InvalidInput("profile_source"));
            }
            Ok(id.into())
        }
        "reddit" => {
            let username = source
                .strip_prefix("https://www.reddit.com/user/")
                .and_then(|value| value.strip_suffix('/'))
                .ok_or(OpenCliError::InvalidInput("profile_source"))?;
            crate::reddit_profile::check_target(username, id)?;
            Ok(username.into())
        }
        _ => Err(OpenCliError::ReadCommandUnavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_source_is_exact_and_never_a_redirect_or_arbitrary_target() {
        assert!(
            check_profile_source(
                "facebook",
                "123",
                "https://www.facebook.com/profile.php?id=123"
            )
            .is_ok()
        );
        assert!(
            check_profile_source(
                "facebook",
                "123",
                "https://www.facebook.com/profile.php?id=456"
            )
            .is_err()
        );
        assert!(
            check_profile_source(
                "facebook",
                "123",
                "https://www.facebook.com/profile.php?id=123&sk=followers"
            )
            .is_err()
        );
        assert_eq!(
            check_profile_source(
                "instagram",
                "123456",
                "https://www.instagram.com/sample_user/"
            )
            .unwrap(),
            "sample_user"
        );
        assert!(check_profile_source("bilibili", "123", "https://space.bilibili.com/123").is_ok());
        assert_eq!(
            check_profile_source("tiktok", "123", "https://www.tiktok.com/@creator").unwrap(),
            "creator"
        );
        assert_eq!(
            check_profile_source("twitter", "creator", "https://x.com/creator").unwrap(),
            "creator"
        );
        assert_eq!(
            check_profile_source(
                "reddit",
                "t2_abc123",
                "https://www.reddit.com/user/Creator_1/"
            )
            .unwrap(),
            "Creator_1"
        );
        assert!(check_profile_source("pixiv", "123", "https://www.pixiv.net/users/123").is_ok());
        assert_eq!(
            check_profile_source(
                "linkedin",
                "native-author",
                "https://www.linkedin.com/in/native-author/"
            )
            .unwrap(),
            "native-author"
        );
        for (site, id, url) in [
            (
                "linkedin",
                "native-author",
                "https://www.linkedin.com/in/other/",
            ),
            (
                "linkedin",
                "native-author",
                "https://www.linkedin.com/in/native-author/?next=other",
            ),
            (
                "linkedin",
                "native-author",
                "https://www.linkedin.com/company/native-author/",
            ),
            ("linkedin", "me", "https://www.linkedin.com/in/me/"),
            ("pixiv", "123", "https://www.pixiv.net/users/456"),
            ("pixiv", "123", "https://www.pixiv.net/artworks/123"),
            ("pixiv", "0123", "https://www.pixiv.net/users/0123"),
            ("pixiv", "123", "https://www.pixiv.net/users/123?next=other"),
            (
                "instagram",
                "sample_user",
                "https://www.instagram.com/sample_user/",
            ),
            (
                "instagram",
                "0123",
                "https://www.instagram.com/sample_user/",
            ),
            ("instagram", "123", "https://www.instagram.com/Sample_User/"),
            (
                "instagram",
                "123",
                "https://www.instagram.com/sample_user/?redirect=evil",
            ),
            (
                "instagram",
                "123",
                "https://www.instagram.com/sample_user/reel/post/",
            ),
            (
                "instagram",
                "123",
                "https://www.instagram.com.evil.example/sample_user/",
            ),
            ("bilibili", "123", "https://space.bilibili.com/456"),
            (
                "bilibili",
                "123",
                "https://space.bilibili.com/123?redirect=evil",
            ),
            ("tiktok", "123", "https://www.tiktok.com/@creator/video/456"),
            (
                "tiktok",
                "123",
                "https://www.tiktok.com/@creator?token=secret",
            ),
            (
                "tiktok",
                "123",
                "https://www.tiktok.com.evil.example/@creator",
            ),
            ("twitter", "123", "https://x.com/creator"),
            ("twitter", "creator", "https://x.com/creator?redirect=evil"),
            ("reddit", "creator", "https://www.reddit.com/user/creator/"),
            (
                "reddit",
                "t2_abc",
                "https://www.reddit.com/user/creator/comments/",
            ),
            (
                "reddit",
                "t2_abc",
                "https://www.reddit.com/user/creator/?redirect=evil",
            ),
            (
                "reddit",
                "t2_abc",
                "https://www.reddit.com.evil.example/user/creator/",
            ),
            ("reddit", "t2_abc", "https://www.reddit.com/user/%63reator/"),
        ] {
            assert!(check_profile_source(site, id, url).is_err());
        }
    }
}
