//! Explicit navigation only; no credentials or login-state assertions.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, process::run};

fn website(platform: &str) -> Option<&'static str> {
    Some(match platform {
        "youtube" => "https://www.youtube.com/",
        "tiktok" => "https://www.tiktok.com/",
        "instagram" => "https://www.instagram.com/",
        "twitter" => "https://x.com/",
        "linkedin" | "linkedin-salesnav" => "https://www.linkedin.com/",
        "linkedin-learning" => "https://www.linkedin.com/learning/",
        "facebook" => "https://www.facebook.com/",
        "pinterest" | "pinterest-pins" | "pinterest-boards" => "https://www.pinterest.com/",
        "reddit" | "reddit-subreddit" => "https://www.reddit.com/",
        "bilibili" => "https://www.bilibili.com/",
        "weixin" => "https://weixin.sogou.com/",
        "douyin" => "https://www.douyin.com/",
        "weibo" => "https://weibo.com/",
        "medium" => "https://medium.com/",
        "substack" => "https://substack.com/",
        "tieba" => "https://tieba.baidu.com/",
        "jike" => "https://web.okjike.com/",
        "linux-do" => "https://linux.do/",
        "nowcoder" => "https://www.nowcoder.com/",
        "hupu" => "https://www.hupu.com/",
        "weread" => "https://weread.qq.com/",
        "google" | "google-images" => "https://www.google.com/",
        "duckduckgo" => "https://duckduckgo.com/",
        "yahoo" => "https://www.yahoo.com/",
        "douban" | "douban-books" | "douban-music" => "https://www.douban.com/",
        "dribbble-shots" | "dribbble-designers" => "https://dribbble.com/",
        "v2ex" | "v2ex-node" => "https://www.v2ex.com/",
        "juejin" => "https://juejin.cn/",
        _ => return None,
    })
}

impl OpenCli {
    /// Open a new tab in the exact connected identity. Never fall back to a
    /// default browser/profile, and never return page content or credentials.
    pub async fn open_creator_login_page(
        &self,
        profile: &str,
        platform: &str,
    ) -> Result<(), OpenCliError> {
        let url = website(platform).ok_or(OpenCliError::InvalidInput("platform"))?;
        self.check_browser_profile(profile).await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "browser",
                "onecrew-login",
                "tab",
                "new",
                url,
            ],
            READ_TIMEOUT,
        )
        .await?;
        let receipt: serde_json::Value =
            serde_json::from_str(&output).map_err(|_| OpenCliError::InvalidOutput)?;
        if receipt["url"].as_str() != Some(url)
            || receipt["page"].as_str().is_none_or(str::is_empty)
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_accepts_only_known_platforms() {
        assert_eq!(website("twitter"), Some("https://x.com/"));
        assert_eq!(website("https://evil.example"), None);
        assert_eq!(website("--profile"), None);
        let registry = crate::registry::Registry::parse(r#"[{"command":"instagram/search","site":"instagram","name":"search","access":"read","browser":true}]"#).unwrap();
        for entry in crate::creator_search::ecommerce_capabilities(&registry) {
            if entry.browser_required {
                assert!(website(&entry.platform).is_some(), "{}", entry.platform);
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires Node.js; fake bridge only"]
    async fn navigation_keeps_exact_profile_and_new_tab() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("navigation.cjs");
        std::fs::write(&script, r#"
const a = process.argv.slice(2).join(' ');
if(a === '--version') console.log('1.8.8');
else if(a === 'list -f json') console.log(JSON.stringify([{command:'instagram/search',site:'instagram',name:'search',access:'read',browser:true}]));
else if(a === 'daemon status') console.log('Daemon: running\nExtension: connected\n');
else if(a === 'profile list') console.log('Connected Browser Bridge profiles\n\n  selected — connected v1.0.24\n');
else if(a === '--profile selected browser onecrew-login tab new https://www.instagram.com/') console.log(JSON.stringify({page:'tab:42',url:'https://www.instagram.com/'}));
else process.exit(90);
"#).unwrap();
        let cli = OpenCli::inspect_with_script("node".into(), Some(script))
            .await
            .unwrap();
        assert_eq!(
            cli.open_creator_login_page("selected", "instagram").await,
            Ok(())
        );
        assert_eq!(
            cli.open_creator_login_page("other", "instagram").await,
            Err(OpenCliError::ProfileUnavailable)
        );
        assert_eq!(
            cli.open_creator_login_page("selected", "https://evil.example")
                .await,
            Err(OpenCliError::InvalidInput("platform"))
        );
    }
}
