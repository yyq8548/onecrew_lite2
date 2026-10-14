//! Explicit, read-only creator lead search shapes from OpenCLI 1.8.8.
//! A hit is a lead, not a qualified creator. Missing/changed provider shapes
//! fail closed instead of fabricating account identity or follower evidence.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{OpenCliError, registry::Registry};

/// Local command and adapter availability; never proof of login or live success.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreatorSearchCapability {
    pub platform: String,
    pub command: String,
    pub browser_required: bool,
    pub registered: bool,
}

pub(super) fn ecommerce_capabilities(registry: &Registry) -> Vec<CreatorSearchCapability> {
    [
        "youtube",
        "tiktok",
        "instagram",
        "twitter",
        "linkedin",
        "facebook",
        "pinterest",
        "reddit",
        "bilibili",
        "douyin",
        "weibo",
        "google",
        "yahoo",
    ]
    .into_iter()
    .map(|site| {
        let name = command(site).expect("reviewed ecommerce search mapping");
        let command_site = command_site(site);
        let browser_required = !is_public(site);
        CreatorSearchCapability {
            platform: site.into(),
            command: format!("{command_site}/{name}"),
            browser_required,
            registered: if browser_required {
                registry.has_browser_read(command_site, name)
            } else {
                registry.has_public_read(command_site, name)
            },
        }
    })
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreatorSearchHit {
    pub source_url: String,
    pub title: String,
    pub author_label: String,
    /// Observed native identity and canonical profile source, never inferred from a name.
    pub author_id: Option<String>,
    pub author_url: Option<String>,
}

impl From<crate::BilibiliSearchVideo> for CreatorSearchHit {
    fn from(video: crate::BilibiliSearchVideo) -> Self {
        Self {
            source_url: video.url,
            title: video.title,
            author_label: video.author,
            author_id: Some(video.author_id),
            author_url: Some(video.author_url),
        }
    }
}

impl From<crate::TiktokSearchVideo> for CreatorSearchHit {
    fn from(video: crate::TiktokSearchVideo) -> Self {
        Self {
            source_url: video.url,
            // Native search item.desc preview, bound to its video and author IDs.
            // A reviewer may cite this saved text, but it does not prove video visuals or residence.
            title: video
                .desc
                .chars()
                .take(500)
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect(),
            author_url: Some(format!("https://www.tiktok.com/@{}", video.author)),
            author_label: video.author,
            author_id: Some(video.author_id),
        }
    }
}

pub const RETIRED_CREATOR_SOURCES: &[&str] = &[
    "zhihu",
    "pixiv",
    "weixin",
    "stackoverflow",
    "stackoverflow-users",
    "devto-tag",
    "hf-spaces",
    "nowcoder",
    "producthunt-posts",
    "medium",
    "medium-tag",
    "bluesky",
    "jike",
    "dribbble-designers",
    "hupu",
    "apple-podcasts",
    "pinterest-pins",
    "pinterest-boards",
    "dribbble-shots",
    "google-images",
    "reddit-subreddit",
    "linkedin-learning",
    "substack",
    "v2ex-node",
    "lesswrong-tag",
    "lobsters-tag",
    "stackoverflow-tag",
    "juejin-hot",
    "linux-do",
    "linkedin-salesnav",
    "duckduckgo",
    "archive",
    "weread",
    "douban-books",
    "douban-music",
    "hf-models",
    "hf-datasets",
    "hackernews",
];

pub(super) fn command(site: &str) -> Option<&'static str> {
    if RETIRED_CREATOR_SOURCES.contains(&site) {
        return None;
    }
    historical_command(site)
}

// Historical command shapes remain for receipt/adapter fixtures; all execution
// must pass command() availability and the public facade checks first.
fn historical_command(site: &str) -> Option<&'static str> {
    match site {
        "youtube" | "bilibili" | "twitter" | "instagram" | "tiktok" | "zhihu" | "weibo"
        | "douyin" | "facebook" | "reddit" | "bluesky" | "medium" | "substack" | "jike"
        | "weixin" | "hackernews" | "linux-do" | "nowcoder" | "tieba" => Some("search"),
        "linkedin" => Some("people-search"),
        "pinterest" => Some("search-pins"),
        "pinterest-pins" => Some("search-pins"),
        "pinterest-boards" => Some("search-boards"),
        "linkedin-salesnav" => Some("salesnav-search"),
        "hupu" | "pixiv" | "stackoverflow" | "linkedin-learning" | "archive" | "weread"
        | "google" | "duckduckgo" | "yahoo" => Some("search"),
        "dribbble-shots" => Some("shot"),
        "dribbble-designers" => Some("designer"),
        "google-images" => Some("images"),
        "stackoverflow-users" => Some("user"),
        "douban-books" | "douban-music" => Some("search"),
        "v2ex-node" => Some("node"),
        "reddit-subreddit" => Some("subreddit"),
        "devto-tag" | "lesswrong-tag" | "lobsters-tag" | "medium-tag" | "stackoverflow-tag" => {
            Some("tag")
        }
        "juejin-hot" => Some("hot"),
        "producthunt-posts" => Some("posts"),
        "hf-spaces" => Some("spaces"),
        "hf-models" => Some("models"),
        "hf-datasets" => Some("datasets"),
        _ => None,
    }
}

pub(super) fn command_site(site: &str) -> &str {
    match site {
        "twitter" => "onecrew-search-twitter",
        "instagram" => "onecrew-search-instagram",
        "reddit" => "onecrew-search-reddit",
        "linkedin" => "onecrew-search-linkedin",
        "facebook" => "onecrew-search-facebook",
        "zhihu" => "onecrew-zhihu-reader",
        "pinterest" => "onecrew-pinterest-reader",
        "douyin" => "onecrew-search-douyin",
        "weibo" => "onecrew-search-weibo",
        "bluesky" => "onecrew-bluesky",
        "tieba" => "onecrew-search-tieba",
        "linkedin-learning" => "onecrew-search-linkedin-learning",
        "tiktok" => "onecrew-tiktok",
        "bilibili" => "onecrew-bilibili",
        "substack" => "onecrew-substack",
        "nowcoder" => "onecrew-nowcoder",
        "pinterest-pins" => "pinterest",
        "pinterest-boards" => "onecrew-search-pinterest",
        "hupu" => "onecrew-search-hupu",
        "weixin" => "onecrew-search-weixin",
        "linkedin-salesnav" => "linkedin",
        "dribbble-shots" => "onecrew-search-dribbble",
        "google" => "onecrew-search-google",
        "duckduckgo" => "onecrew-search-duckduckgo",
        "dribbble-designers" => "onecrew-search-dribbble",
        "google-images" => "google",
        "stackoverflow-users" => "stackoverflow",
        "douban-books" | "douban-music" => "onecrew-search-douban",
        "v2ex-node" => "v2ex",
        "reddit-subreddit" => "reddit",
        "devto-tag" => "devto",
        "lesswrong-tag" => "lesswrong",
        "lobsters-tag" => "onecrew-public-lobsters",
        "medium-tag" => "medium",
        "stackoverflow-tag" => "stackoverflow",
        "juejin-hot" => "juejin",
        "producthunt-posts" => "onecrew-public-producthunt",
        "hf-spaces" | "hf-models" | "hf-datasets" => "hf",
        _ => site,
    }
}

pub(super) fn bundled_search_addon(site: &str) -> Option<&'static str> {
    match site {
        "twitter" | "linkedin" | "facebook" | "zhihu" | "douyin" | "weibo" | "tieba"
        | "linkedin-learning" | "dribbble-designers" | "dribbble-shots" | "google"
        | "duckduckgo" | "douban-books" | "douban-music" | "hupu" | "weixin" | "pinterest"
        | "pinterest-boards" | "reddit" | "instagram" => Some("onecrew-browser-search"),
        "tiktok" => Some("onecrew-tiktok"),
        "bilibili" => Some("onecrew-bilibili"),
        "substack" => Some("onecrew-substack"),
        "bluesky" => Some("onecrew-bluesky"),
        "nowcoder" => Some("onecrew-source-search"),
        "lobsters-tag" | "producthunt-posts" => Some("onecrew-public-search"),
        _ => None,
    }
}

pub(super) fn available_capabilities(
    capabilities: Vec<CreatorSearchCapability>,
    addons: &[crate::OpenCliAddonStatus],
) -> Vec<CreatorSearchCapability> {
    capabilities
        .into_iter()
        .filter(|source| {
            source.registered
                && bundled_search_addon(&source.platform).is_none_or(|id| {
                    addons.iter().any(|addon| {
                        addon.id == id && addon.state == crate::OpenCliAddonState::Ready
                    })
                })
        })
        .collect()
}

pub(super) fn is_public(site: &str) -> bool {
    // PUBLIC strategy alone does not mean browser-free: weixin/search uses
    // the dedicated browser even though the search page needs no login.
    matches!(
        site,
        "bluesky"
            | "substack"
            | "hackernews"
            | "sinablog"
            | "apple-podcasts"
            | "stackoverflow"
            | "archive"
            | "weread"
            | "stackoverflow-users"
            | "v2ex-node"
            | "devto-tag"
            | "lesswrong-tag"
            | "lobsters-tag"
            | "medium-tag"
            | "stackoverflow-tag"
            | "juejin-hot"
            | "producthunt-posts"
            | "hf-spaces"
            | "hf-models"
            | "hf-datasets"
    )
}

pub(super) fn ambiguous_empty_result(site: &str) -> bool {
    // These adapters can report EMPTY_RESULT after an unreadable browser page,
    // missing login, or changed DOM. A stop receipt preserves that uncertainty.
    matches!(
        site,
        "facebook"
            | "linkedin"
            | "lesswrong-tag"
            | "tieba"
            | "1point3acres"
            | "hupu"
            | "pixiv"
            | "pinterest-pins"
            | "pinterest-boards"
            | "linkedin-salesnav"
            | "linkedin-learning"
            | "google"
            | "brave"
            | "duckduckgo"
            | "yahoo"
            | "dribbble-shots"
            | "dribbble-designers"
            | "google-images"
            | "douban-books"
            | "douban-music"
    )
}

pub(super) fn arguments(profile: &str, site: &str, query: &str, limit: u8) -> Vec<String> {
    let mut args = Vec::new();
    if !is_public(site) {
        args.extend(["--profile".into(), profile.into()]);
    }
    args.extend([
        command_site(site).into(),
        historical_command(site)
            .expect("validated search site")
            .into(),
        "--limit".into(),
        limit.to_string(),
        "-f".into(),
        "json".into(),
    ]);
    if site == "google-images" {
        // Campaign needs the publishing source, not serial original-image clicks.
        args.push("--resolve=false".into());
    }
    if site == "dribbble-designers" {
        args.push(format!("--query={query}"));
    } else if site == "juejin-hot" || site == "producthunt-posts" {
        args.push(format!("--category={query}"));
    } else if matches!(site, "hf-spaces" | "hf-models" | "hf-datasets") {
        args.push(format!("--search={query}"));
    } else {
        if matches!(site, "douban-books" | "douban-music") {
            args.extend([
                "--type".into(),
                if site == "douban-books" {
                    "book"
                } else {
                    "music"
                }
                .into(),
            ]);
        }
        args.extend(["--".into(), query.into()]);
    }
    args
}

pub(super) fn parse(
    site: &str,
    output: &str,
    limit: u8,
) -> Result<Vec<CreatorSearchHit>, OpenCliError> {
    let rows: Vec<Value> = serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.is_empty() && matches!(site, "douban-books" | "douban-music" | "sinablog") {
        // This browser adapter returns [] when its result cards are absent;
        // without an empty-state marker that is not proof of zero matches.
        // Sina filters the first mixed news page, not a scoped blog result set.
        return Err(OpenCliError::InvalidOutput);
    }
    if rows.len() > usize::from(limit) || rows.len() > 50 {
        return Err(OpenCliError::InvalidOutput);
    }
    // Providers may repeat a source within a page. Collapse identical URLs
    // before the repository's unique-source check, without changing identity.
    let mut seen = std::collections::HashSet::new();
    let mut hits = Vec::new();
    for row in &rows {
        let hit = parse_row(site, row)?;
        if seen.insert(hit.source_url.clone()) {
            hits.push(hit);
        }
    }
    Ok(hits)
}

fn parse_row(site: &str, row: &Value) -> Result<CreatorSearchHit, OpenCliError> {
    if site == "pinterest" {
        return crate::pinterest::parse_search(row);
    }
    if site == "zhihu" {
        return crate::zhihu::parse_search(row);
    }
    let field = |name: &str| row.get(name).and_then(Value::as_str).unwrap_or_default();
    let (mut source_url, title, author_label) = match site {
        "youtube" => (field("url").to_owned(), field("title"), field("channel")),
        "bilibili" | "rednote" | "xiaohongshu" | "zhihu" | "weibo" | "medium" | "substack"
        | "apple-podcasts" | "tieba" => (field("url").to_owned(), field("title"), field("author")),
        "twitter" => (field("url").to_owned(), field("text"), field("author")),
        "jike" => {
            let id = field("id");
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                || field("url") != format!("https://web.okjike.com/originalPost/{id}")
            {
                return Err(OpenCliError::InvalidOutput);
            }
            // An image-only post has a valid locator but supplies no text evidence.
            (
                field("url").to_owned(),
                if field("content").trim().is_empty() {
                    "即刻帖子（无文字）"
                } else {
                    field("content")
                },
                field("author"),
            )
        }
        "weixin" => (field("url").to_owned(), field("title"), ""),
        "linux-do" => {
            let id = field("url")
                .strip_prefix("https://linux.do/t/topic/")
                .ok_or(OpenCliError::InvalidOutput)?;
            if !positive_id(id) {
                return Err(OpenCliError::InvalidOutput);
            }
            (field("url").to_owned(), field("title"), "")
        }
        "nowcoder" => {
            let id = field("id");
            let path = match field("post_type") {
                "content" if positive_id(id) => format!("discuss/{id}"),
                "moment"
                    if id.len() == 32
                        && id.bytes().all(|b| b.is_ascii_hexdigit())
                        && id == field("uuid") =>
                {
                    format!("feed/main/detail/{id}")
                }
                _ => return Err(OpenCliError::InvalidOutput),
            };
            let author = field("author_id");
            if field("url") != format!("https://www.nowcoder.com/{path}")
                || !positive_id(author)
                || field("author_url") != format!("https://www.nowcoder.com/users/{author}")
            {
                return Err(OpenCliError::InvalidOutput);
            }
            (field("url").to_owned(), field("title"), field("author"))
        }
        "sinablog" => {
            // Preserve only an actually observed HTTPS article URL. Never
            // upgrade a legacy HTTP result or accept a spoofed host substring.
            let id = field("url")
                .strip_prefix("https://blog.sina.com.cn/s/blog_")
                .and_then(|value| value.strip_suffix(".html"))
                .ok_or(OpenCliError::InvalidOutput)?;
            if id.is_empty()
                || id.len() > 128
                || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(OpenCliError::InvalidOutput);
            }
            (field("url").to_owned(), field("title"), field("author"))
        }
        "hackernews" => {
            // `url` is the submitted external article, not the submitting user's
            // own content. Bind the lead to the observed HN story ID instead.
            let id = field("id");
            if !positive_id(id)
                || id
                    .parse::<u64>()
                    .ok()
                    .is_none_or(|id| id > 9_007_199_254_740_991)
            {
                return Err(OpenCliError::InvalidOutput);
            }
            (
                format!("https://news.ycombinator.com/item?id={id}"),
                field("title"),
                field("author"),
            )
        }
        "bluesky" if field("provider") == "onecrew-bluesky-native-v1" => {
            let (did, source) = crate::bluesky::post_target(field("post_uri"))
                .ok_or(OpenCliError::InvalidOutput)?;
            if field("author_id") != did
                || field("source_url") != source
                || field("author_url") != format!("https://bsky.app/profile/{did}")
            {
                return Err(OpenCliError::InvalidOutput);
            }
            (source, field("text"), field("author"))
        }
        "bluesky" => {
            let handle = field("handle");
            // The installed search adapter returns handles, not DIDs or URLs.
            // Preserve that public locator as a lead; it is not stable identity.
            if handle.len() > 253
                || !handle.contains('.')
                || handle.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
            {
                return Err(OpenCliError::InvalidOutput);
            }
            (
                format!("https://bsky.app/profile/{handle}"),
                if field("name").is_empty() {
                    handle
                } else {
                    field("name")
                },
                handle,
            )
        }
        "instagram" => (
            field("url").to_owned(),
            if field("name").is_empty() {
                field("username")
            } else {
                field("name")
            },
            field("username"),
        ),
        "tiktok" | "douyin" => (field("url").to_owned(), field("desc"), field("author")),
        "facebook" => (field("url").to_owned(), field("title"), field("text")),
        "reddit" => (field("url").to_owned(), field("title"), field("author")),
        "linkedin" => (
            field("profile_url").to_owned(),
            field("name"),
            field("name"),
        ),
        "pinterest" => (
            field("url").to_owned(),
            if field("fullName").is_empty() {
                field("username")
            } else {
                field("fullName")
            },
            field("username"),
        ),
        "1point3acres" | "hupu" | "pixiv" | "stackoverflow" | "weread" => {
            (field("url").to_owned(), field("title"), field("author"))
        }
        "archive" => (field("url").to_owned(), field("title"), field("creator")),
        "pinterest-pins" => (
            field("url").to_owned(),
            if field("title").is_empty() {
                if field("description").is_empty() {
                    field("pinId")
                } else {
                    field("description")
                }
            } else {
                field("title")
            },
            field("pinner"),
        ),
        "pinterest-boards" => (field("url").to_owned(), field("name"), field("owner")),
        "linkedin-salesnav" => (
            field("profile_url").to_owned(),
            field("name"),
            field("title"),
        ),
        "linkedin-learning" => (field("url").to_owned(), field("title"), field("instructor")),
        "dribbble-shots" => (field("url").to_owned(), field("title"), field("designer")),
        "dribbble-designers" => (
            field("url").to_owned(),
            if field("name").is_empty() {
                field("username")
            } else {
                field("name")
            },
            field("username"),
        ),
        "stackoverflow-users" => (
            field("url").to_owned(),
            field("displayName"),
            field("displayName"),
        ),
        "google-images" => (field("sourceUrl").to_owned(), field("title"), ""),
        "douban-books" | "douban-music" => (field("url").to_owned(), field("title"), ""),
        "v2ex-node" => (field("url").to_owned(), field("title"), field("author")),
        "reddit-subreddit" => (field("url").to_owned(), field("title"), field("author")),
        "devto-tag" | "lesswrong-tag" | "lobsters-tag" | "medium-tag" | "stackoverflow-tag" => {
            (field("url").to_owned(), field("title"), field("author"))
        }
        "juejin-hot" => (field("url").to_owned(), field("title"), field("author")),
        "producthunt-posts" => (field("url").to_owned(), field("name"), field("author")),
        "hf-spaces" | "hf-models" | "hf-datasets" => {
            (field("url").to_owned(), field("id"), field("author"))
        }
        "google" | "brave" | "duckduckgo" | "yahoo" => {
            (field("url").to_owned(), field("title"), "")
        }
        _ => return Err(OpenCliError::ReadCommandUnavailable),
    };
    let url = Url::parse(&source_url).map_err(|_| OpenCliError::InvalidOutput)?;
    let expected_host = match site {
        "youtube" => &["youtube.com", "www.youtube.com"][..],
        "bilibili" => &["bilibili.com", "www.bilibili.com", "space.bilibili.com"],
        "twitter" => &["x.com", "www.x.com", "twitter.com", "www.twitter.com"],
        "instagram" => &["www.instagram.com", "instagram.com"],
        "tiktok" => &["www.tiktok.com", "tiktok.com"],
        "xiaohongshu" => &["www.xiaohongshu.com", "xiaohongshu.com"],
        "rednote" => &["www.rednote.com", "rednote.com"],
        "zhihu" => &["www.zhihu.com", "zhihu.com", "zhuanlan.zhihu.com"],
        "weibo" => &["weibo.com", "www.weibo.com", "s.weibo.com"],
        "douyin" => &["www.douyin.com", "douyin.com"],
        "facebook" => &["www.facebook.com", "facebook.com", "m.facebook.com"],
        "reddit" => &["www.reddit.com", "reddit.com"],
        "linkedin" => &["www.linkedin.com", "linkedin.com"],
        "pinterest" => &["www.pinterest.com", "pinterest.com"],
        "bluesky" => &["bsky.app"],
        "medium" => &["medium.com", "www.medium.com"],
        "substack" => &["substack.com", "www.substack.com"],
        "jike" => &["web.okjike.com"],
        "weixin" => &["weixin.sogou.com", "mp.weixin.qq.com"],
        "hackernews" => &["news.ycombinator.com"],
        "linux-do" => &["linux.do"],
        "nowcoder" => &["www.nowcoder.com"],
        "sinablog" => &["blog.sina.com.cn"],
        "apple-podcasts" => &["podcasts.apple.com"],
        "tieba" => &["tieba.baidu.com"],
        "1point3acres" => &["www.1point3acres.com"],
        "hupu" => &["bbs.hupu.com"],
        "pixiv" => &["www.pixiv.net"],
        "stackoverflow" => &["stackoverflow.com"],
        "pinterest-pins" | "pinterest-boards" => &["www.pinterest.com"],
        "linkedin-salesnav" | "linkedin-learning" => &["www.linkedin.com"],
        "archive" => &["archive.org"],
        "weread" => &["weread.qq.com"],
        "dribbble-shots" | "dribbble-designers" => &["dribbble.com", "www.dribbble.com"],
        "stackoverflow-users" => &["stackoverflow.com"],
        "douban-books" => &["book.douban.com"],
        "douban-music" => &["music.douban.com"],
        "v2ex-node" => &["www.v2ex.com"],
        "reddit-subreddit" => &["www.reddit.com"],
        "devto-tag" => &["dev.to"],
        "lesswrong-tag" => &["www.lesswrong.com"],
        "lobsters-tag" => &["lobste.rs"],
        "medium-tag" => &["medium.com", "www.medium.com"],
        "stackoverflow-tag" => &["stackoverflow.com"],
        "juejin-hot" => &["juejin.cn"],
        "producthunt-posts" => &["www.producthunt.com"],
        "hf-spaces" | "hf-models" | "hf-datasets" => &["huggingface.co"],
        "google" | "brave" | "duckduckgo" | "yahoo" | "google-images" => &[],
        _ => return Err(OpenCliError::ReadCommandUnavailable),
    };
    let title = if site == "linkedin" && !field("headline").trim().is_empty() {
        clean(&format!("{title}\n{}", field("headline")), 500)
    } else {
        clean(title, 500)
    };
    let author_label = clean(author_label, 200);
    let host = url.host_str().unwrap_or_default();
    let web_result = matches!(
        site,
        "google" | "brave" | "duckduckgo" | "yahoo" | "google-images" | "medium-tag"
    );
    let bound_substack_post = site == "substack"
        && field("provider") == "onecrew-substack-v1"
        && positive_id(field("post_id"))
        && positive_id(field("publication_id"))
        && !field("slug").is_empty()
        && field("slug")
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        && url.path() == format!("/p/{}", field("slug"))
        && url.fragment().is_none();
    let allowed_web_result_host = (web_result || bound_substack_post)
        && host.contains('.')
        && !host.ends_with(".localhost")
        && !host.ends_with(".local")
        && !host.ends_with(".internal")
        && !host.ends_with(".test")
        && host.parse::<std::net::IpAddr>().is_err();
    let allowed_host = expected_host.contains(&host)
        || (matches!(site, "medium" | "medium-tag") && host.ends_with(".medium.com"))
        || (site == "substack" && host.ends_with(".substack.com"))
        || allowed_web_result_host;
    if url.scheme() != "https"
        || !allowed_host
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || source_url.len() > 2048
        || title.is_empty()
    {
        return Err(OpenCliError::InvalidOutput);
    }
    if matches!(site, "google" | "google-images")
        && matches!(host, "google.com" | "www.google.com")
        && matches!(url.path(), "/goto" | "/aclk" | "/url")
    {
        // The observed Google adapter returned opaque ad redirects, not the
        // creator's source. Do not persist those as verified source locators.
        return Err(OpenCliError::InvalidOutput);
    }
    if site == "google-images"
        && (host == "google.com"
            || host.ends_with(".google.com")
            || host == "gstatic.com"
            || host.ends_with(".gstatic.com"))
    {
        // Image search must retain the publishing page, not a thumbnail or
        // search-result page hosted by Google.
        return Err(OpenCliError::InvalidOutput);
    }
    match site {
        "twitter" => {
            let id = field("id");
            let path = url.path();
            let observed_handle = field("author").strip_prefix('@').unwrap_or(field("author"));
            let valid_handle = !observed_handle.is_empty()
                && observed_handle.len() <= 15
                && observed_handle
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
            let source_matches = path == format!("/i/status/{id}")
                || path
                    .strip_prefix('/')
                    .and_then(|value| value.split_once("/status/"))
                    .is_some_and(|(handle, post)| {
                        handle.eq_ignore_ascii_case(observed_handle) && post == id
                    });
            if !positive_id(id)
                || !valid_handle
                || !source_matches
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "apple-podcasts" => {
            let id = row
                .get("id")
                .and_then(Value::as_u64)
                .filter(|id| *id > 0)
                .ok_or(OpenCliError::InvalidOutput)?;
            if url.path_segments().and_then(|mut parts| parts.next_back())
                != Some(format!("id{id}").as_str())
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "tieba" => {
            let id = field("id");
            if !positive_id(id) || url.path() != format!("/p/{id}") || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "1point3acres" => {
            let id = row_positive_id(row, "tid")?;
            if url.path() != format!("/bbs/thread-{id}-1-1.html") || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "hupu" => {
            let id = row_positive_id(row, "tid")?;
            if url.path() != format!("/{id}.html") || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "douyin" => {
            let id = url
                .path()
                .strip_prefix("/video/")
                .ok_or(OpenCliError::InvalidOutput)?;
            if !positive_id(id)
                || source_url != format!("https://www.douyin.com/video/{id}")
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "pixiv" => {
            let id = row_positive_id(row, "illust_id")?;
            row_positive_id(row, "user_id")?;
            if url.path() != format!("/artworks/{id}") || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "stackoverflow" => {
            let id = row_positive_id(row, "id")?;
            let mut segments = url.path_segments().ok_or(OpenCliError::InvalidOutput)?;
            if segments.next() != Some("questions")
                || segments.next() != Some(id.as_str())
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "pinterest-pins" => {
            let id = row_positive_id(row, "pinId")?;
            if url.path() != format!("/pin/{id}/") || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "pinterest-boards" => {
            row_positive_id(row, "boardId")?;
            let parts = url.path_segments().ok_or(OpenCliError::InvalidOutput)?;
            if parts.filter(|segment| !segment.is_empty()).count() != 2
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "linkedin-salesnav" => {
            let urn = field("recipient_urn")
                .strip_prefix("urn:li:fs_salesProfile:(")
                .and_then(|rest| rest.split_once(','))
                .map(|(id, _)| id)
                .ok_or(OpenCliError::InvalidOutput)?;
            if urn.is_empty() || url.path() != format!("/in/{urn}") || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "linkedin-learning" => {
            if !url.path().starts_with("/learning/")
                || url.path() == "/learning/"
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "archive" => {
            let id = field("identifier");
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
                || url.path() != format!("/details/{id}")
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "weread" => {
            let path = url.path();
            let known_book_path = ["/web/bookDetail/", "/web/reader/"]
                .into_iter()
                .any(|prefix| path.strip_prefix(prefix).is_some_and(|id| !id.is_empty()));
            if field("bookId").is_empty() || !known_book_path || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "dribbble-shots" => {
            let id = row_positive_id(row, "id")?;
            let prefix = format!("/shots/{id}");
            if !url.path().strip_prefix(&prefix).is_some_and(|rest| {
                rest.is_empty() || rest == "/" || (rest.starts_with('-') && !rest.contains('/'))
            }) || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "dribbble-designers" => {
            let username = field("username");
            if username.is_empty()
                || username.len() > 100
                || !username
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                || url.path() != format!("/{username}")
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "stackoverflow-users" => {
            let id = row_positive_id(row, "userId")?;
            let mut segments = url.path_segments().ok_or(OpenCliError::InvalidOutput)?;
            if segments.next() != Some("users")
                || segments.next() != Some(id.as_str())
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "douban-books" | "douban-music" => {
            let id = row_positive_id(row, "id")?;
            if url.path() != format!("/subject/{id}/")
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "v2ex-node" => {
            let id = url
                .path()
                .strip_prefix("/t/")
                .ok_or(OpenCliError::InvalidOutput)?;
            if !positive_id(id) || url.query().is_some() || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "reddit-subreddit" => {
            let id = field("id");
            let subreddit = field("subreddit")
                .strip_prefix("r/")
                .ok_or(OpenCliError::InvalidOutput)?;
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            if id.is_empty()
                || id.len() > 20
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                || subreddit.is_empty()
                || !subreddit
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || !matches!(segments.len(), 4 | 5)
                || segments[0] != "r"
                || segments[1] != subreddit
                || segments[2] != "comments"
                || segments[3] != id
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "devto-tag" => {
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            if segments.len() != 2 || url.query().is_some() || url.fragment().is_some() {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "lesswrong-tag" => {
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            if segments.len() != 3
                || segments[0] != "posts"
                || segments[1].is_empty()
                || !segments[1].bytes().all(|byte| byte.is_ascii_alphanumeric())
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "lobsters-tag" => {
            let id = field("id");
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            if segments.len() != 3
                || segments[0] != "s"
                || segments[1] != id
                || id.is_empty()
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "medium-tag" => {
            let article_id = url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .and_then(|segment| segment.rsplit_once('-'))
                .map(|(_, id)| id)
                .ok_or(OpenCliError::InvalidOutput)?;
            if article_id.len() != 12
                || !article_id.bytes().all(|byte| byte.is_ascii_hexdigit())
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "stackoverflow-tag" => {
            let id = row_positive_id(row, "id")?;
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            if segments.len() < 2
                || segments[0] != "questions"
                || segments[1] != id
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "juejin-hot" => {
            let id = field("article_id");
            if !positive_id(id)
                || url.path() != format!("/post/{id}")
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "producthunt-posts" => {
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            let valid_path = (segments.len() == 2 && matches!(segments[0], "posts" | "products"))
                || (segments.len() == 4 && segments[0] == "products" && segments[2] == "launches");
            if !valid_path
                || segments.iter().any(|part| {
                    part.is_empty()
                        || !part
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "hf-spaces" => {
            let id = field("id");
            let author = field("author");
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            if segments.len() != 3
                || segments[0] != "spaces"
                || segments[1] != author
                || format!("{}/{}", segments[1], segments[2]) != id
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        "hf-models" | "hf-datasets" => {
            let id = field("id");
            let author = field("author");
            let segments = url
                .path_segments()
                .ok_or(OpenCliError::InvalidOutput)?
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            let prefix = usize::from(site == "hf-datasets");
            if segments.len() != prefix + 2
                || (prefix == 1 && segments[0] != "datasets")
                || segments[prefix] != author
                || format!("{}/{}", segments[prefix], segments[prefix + 1]) != id
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(OpenCliError::InvalidOutput);
            }
        }
        _ => {}
    }
    // The observed native search URL adds this tracking flag. Strip only that
    // exact single flag, after host validation and before author localization.
    // Other query parameters, fragments and invalid native IDs remain leads.
    if site == "weibo" && url.query() == Some("refer_flag=1001030103_") && url.fragment().is_none()
    {
        let mut canonical = url.clone();
        canonical.set_query(None);
        if crate::weibo_post::target(canonical.as_str()).is_ok() {
            source_url = canonical.into();
        }
    }
    let (author_id, author_url) = if site == "twitter" {
        // OpenCLI search observes the account handle in the tweet's native
        // user object. Handles can change; this is a source locator, not a
        // permanent numeric account ID or a qualification decision.
        let handle = field("author")
            .strip_prefix('@')
            .unwrap_or(field("author"))
            .to_ascii_lowercase();
        (
            Some(handle.clone()),
            Some(format!("https://x.com/{handle}")),
        )
    } else if site == "linkedin" {
        // A native people-search card supplies a profile locator, not an
        // account ID. Profile and body receipts must establish native identity.
        match source_url
            .strip_prefix("https://www.linkedin.com/in/")
            .and_then(|path| path.strip_suffix('/'))
        {
            Some(handle) if crate::linkedin::valid_handle(handle) && handle.len() <= 100 => {
                (Some(handle.into()), Some(source_url.clone()))
            }
            _ => (None, None),
        }
    } else if site == "facebook" {
        // Only an observed exact numeric profile locator identifies this result's
        // account. Vanity names, posts, groups and other query IDs remain leads.
        match source_url.strip_prefix("https://www.facebook.com/profile.php?id=") {
            Some(id) if positive_id(id) => {
                if let Some(observed) = row.get("author_id")
                    && observed.as_str() != Some(id)
                {
                    return Err(OpenCliError::InvalidOutput);
                }
                (Some(id.into()), Some(source_url.clone()))
            }
            _ => (None, None),
        }
    } else if site == "instagram" && field("provider") == "onecrew-instagram-native-v1" {
        let id = field("author_id");
        let username = field("username");
        if !(1..=25).contains(&id.len())
            || id.starts_with('0')
            || !id.bytes().all(|byte| byte.is_ascii_digit())
            || !(1..=30).contains(&username.len())
            || username != username.to_ascii_lowercase()
            || !username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
            || source_url != format!("https://www.instagram.com/{username}/")
        {
            return Err(OpenCliError::InvalidOutput);
        }
        (Some(id.into()), Some(source_url.clone()))
    } else if site == "reddit" && field("provider") == "onecrew-reddit-native-v1" {
        let post = field("id");
        if !(1..=15).contains(&post.len())
            || !post
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            || source_url != format!("https://www.reddit.com/comments/{post}/")
        {
            return Err(OpenCliError::InvalidOutput);
        }
        let id = field("author_id");
        let author = field("author");
        if id.is_empty() && field("author_url").is_empty() {
            (None, None)
        } else {
            if !id.strip_prefix("t2_").is_some_and(|id| {
                (1..=15).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            }) || !(3..=20).contains(&author.len())
                || !author
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                || field("author_url") != format!("https://www.reddit.com/user/{author}/")
            {
                return Err(OpenCliError::InvalidOutput);
            }
            (Some(id.into()), Some(field("author_url").into()))
        }
    } else if site == "hackernews" {
        // Algolia's observed submitter username is a locator. The native item
        // and profile owners must still verify it before scoring any prose.
        let author = field("author");
        if !crate::hackernews::valid_username(author) {
            return Err(OpenCliError::InvalidOutput);
        }
        (
            Some(author.into()),
            Some(format!("https://news.ycombinator.com/user?id={author}")),
        )
    } else if site == "bluesky" && field("provider") == "onecrew-bluesky-native-v1" {
        (
            Some(field("author_id").into()),
            Some(field("author_url").into()),
        )
    } else if site == "weibo" {
        // A canonical native post locator observes its UID. Profile and post
        // reads must still verify this identity before scoring or recommendation.
        if let Ok((id, _)) = crate::weibo_post::target(&source_url) {
            (Some(id.into()), Some(format!("https://weibo.com/u/{id}")))
        } else {
            (None, None)
        }
    } else if site == "pixiv" {
        let id = row_positive_id(row, "user_id")?;
        (
            Some(id.clone()),
            Some(format!("https://www.pixiv.net/users/{id}")),
        )
    } else if site == "stackoverflow-users" {
        (
            Some(row_positive_id(row, "userId")?),
            Some(source_url.clone()),
        )
    } else {
        // Profile-only results (including Dribbble designers) remain traceable
        // through source_url. A URL without an observed ID is not an identity pair.
        (None, None)
    };
    Ok(CreatorSearchHit {
        author_id,
        author_url,
        source_url,
        title,
        author_label,
    })
}

fn row_positive_id(row: &Value, name: &str) -> Result<String, OpenCliError> {
    let value = row.get(name).ok_or(OpenCliError::InvalidOutput)?;
    let id = if let Some(value) = value.as_str() {
        value.to_owned()
    } else if let Some(value) = value.as_u64() {
        value.to_string()
    } else {
        return Err(OpenCliError::InvalidOutput);
    };
    positive_id(&id)
        .then_some(id)
        .ok_or(OpenCliError::InvalidOutput)
}

fn positive_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && !value.starts_with('0')
        && value.bytes().all(|b| b.is_ascii_digit())
}

fn clean(input: &str, maximum: usize) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(maximum)
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn removed_resource_sources_are_absent_and_cannot_execute_even_if_registered() {
        let registry = super::Registry::parse(r#"[
          {"command":"hackernews/search","site":"hackernews","name":"search","access":"read","browser":false,"strategy":"public"},
          {"command":"archive/search","site":"archive","name":"search","access":"read","browser":false,"strategy":"public"},
          {"command":"weread/search","site":"weread","name":"search","access":"read","browser":false,"strategy":"public"},
          {"command":"hf/spaces","site":"hf","name":"spaces","access":"read","browser":false,"strategy":"public"},
          {"command":"hf/models","site":"hf","name":"models","access":"read","browser":false,"strategy":"public"},
          {"command":"hf/datasets","site":"hf","name":"datasets","access":"read","browser":false,"strategy":"public"}
        ]"#).unwrap();
        let capabilities = super::ecommerce_capabilities(&registry);
        for site in [
            "archive",
            "weread",
            "douban-books",
            "douban-music",
            "hf-models",
            "hf-datasets",
            "hackernews",
        ] {
            assert!(super::RETIRED_CREATOR_SOURCES.contains(&site));
            assert!(super::command(site).is_none());
            assert!(!capabilities.iter().any(|entry| entry.platform == site));
        }
        assert!(
            capabilities
                .iter()
                .all(|entry| entry.platform != "hf-spaces")
        );
        assert!(
            capabilities
                .iter()
                .any(|entry| entry.platform == "instagram")
        );
    }
    #[test]
    fn hackernews_search_observes_submitter_without_using_external_article_as_source() {
        let row = serde_json::json!([{"id":"8863","title":"Native story","author":"submitter","url":"https://publisher.example/article"}]);
        let hit = super::parse("hackernews", &row.to_string(), 1)
            .unwrap()
            .remove(0);
        assert_eq!(hit.source_url, "https://news.ycombinator.com/item?id=8863");
        assert_eq!(hit.author_id.as_deref(), Some("submitter"));
        assert_eq!(
            hit.author_url.as_deref(),
            Some("https://news.ycombinator.com/user?id=submitter")
        );
        for author in ["display name", "../author", "", "user?token=secret"] {
            let invalid = serde_json::json!([{"id":"8863","title":"Story","author":author,"url":"https://publisher.example/article"}]);
            assert!(super::parse("hackernews", &invalid.to_string(), 1).is_err());
        }
        for id in ["08863", "0", "9007199254740992"] {
            let invalid = serde_json::json!([{"id":id,"title":"Story","author":"submitter"}]);
            assert!(super::parse("hackernews", &invalid.to_string(), 1).is_err());
        }
    }
    #[test]
    fn weibo_search_observes_uid_from_canonical_post_without_claiming_verification() {
        let row = r#"[{"title":"Keyboard review","author":"Author","url":"https://weibo.com/12345/Abc123"}]"#;
        let hits = super::parse("weibo", row, 1).unwrap();
        assert_eq!(hits[0].author_id.as_deref(), Some("12345"));
        assert_eq!(
            hits[0].author_url.as_deref(),
            Some("https://weibo.com/u/12345")
        );
        for source in [
            "https://weibo.com/u/12345",
            "https://s.weibo.com/weibo?q=keyboard",
            "https://weibo.com/0123/Abc123",
            "https://weibo.com/9007199254740992/Abc123",
            "https://weibo.com/12345/Abc123?redirect=1",
        ] {
            let row = serde_json::json!([{"title":"Keyboard","author":"Author","url":source}]);
            let hits = super::parse("weibo", &row.to_string(), 1).unwrap();
            assert_eq!(hits[0].author_id, None);
            assert_eq!(hits[0].author_url, None);
        }
    }

    #[test]
    fn weibo_observed_search_tracking_preserves_native_post_locator() {
        let observed = "https://weibo.com/12345/RlItu8e31?refer_flag=1001030103_";
        let row =
            serde_json::json!([{"title":"Original caption","author":"Author","url":observed}]);
        let hit = super::parse("weibo", &row.to_string(), 1)
            .unwrap()
            .remove(0);
        assert_eq!(hit.source_url, "https://weibo.com/12345/RlItu8e31");
        assert_eq!(hit.author_id.as_deref(), Some("12345"));
        assert_eq!(hit.author_url.as_deref(), Some("https://weibo.com/u/12345"));
        for source in [
            "https://weibo.com/12345/RlItu8e31?redirect=1",
            "https://weibo.com/12345/RlItu8e31?refer_flag=other",
            "https://weibo.com/12345/RlItu8e31?refer_flag=1001030103_&uid=99999",
            "https://weibo.com/12345/RlItu8e31?refer_flag=1001030103_&refer_flag=1001030103_",
            "https://weibo.com/12345/RlItu8e31?refer_flag=1001030103_#other",
            "https://weibo.com/012345/RlItu8e31?refer_flag=1001030103_",
            "https://weibo.com/9007199254740992/RlItu8e31?refer_flag=1001030103_",
            "https://s.weibo.com/12345/RlItu8e31?refer_flag=1001030103_",
        ] {
            let row = serde_json::json!([{"title":"Caption","author":"Author","url":source}]);
            let hit = super::parse("weibo", &row.to_string(), 1)
                .unwrap()
                .remove(0);
            assert_eq!(hit.source_url, source);
            assert!(hit.author_id.is_none());
            assert!(hit.author_url.is_none());
        }
    }

    #[test]
    fn bluesky_native_search_binds_at_uri_did_and_source_without_handle_guessing() {
        let did = "did:plc:abcdefghijklmnopqrstuvwx";
        let mut row = serde_json::json!({"provider":"onecrew-bluesky-native-v1","author_id":did,
            "author_url":format!("https://bsky.app/profile/{did}"),"post_uri":format!("at://{did}/app.bsky.feed.post/original"),
            "source_url":format!("https://bsky.app/profile/{did}/post/original"),"author":"Author","text":"Original keyboard review"});
        let hits = super::parse("bluesky", &serde_json::json!([row]).to_string(), 5).unwrap();
        assert_eq!(hits[0].author_id.as_deref(), Some(did));
        assert_eq!(hits[0].title, "Original keyboard review");
        row["author_id"] = serde_json::json!("author.bsky.social");
        assert!(super::parse("bluesky", &serde_json::json!([row]).to_string(), 5).is_err());
    }
    use super::*;
    #[test]
    fn facebook_search_preserves_only_exact_observed_numeric_profile_identity() {
        let source = "https://www.facebook.com/profile.php?id=61590270912364";
        let mut row =
            serde_json::json!({"url":source,"title":"Observed page","text":"Search preview"});
        let hit = parse_row("facebook", &row).unwrap();
        assert_eq!(hit.author_id.as_deref(), Some("61590270912364"));
        assert_eq!(hit.author_url.as_deref(), Some(source));
        assert_eq!(hit.source_url, source);
        row["author_id"] = serde_json::json!("another-account");
        assert_eq!(
            parse_row("facebook", &row),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn facebook_posts_vanity_names_and_unrelated_ids_do_not_identify_an_author() {
        for source in [
            "https://www.facebook.com/creator",
            "https://www.facebook.com/creator/posts/123",
            "https://www.facebook.com/groups/123",
            "https://www.facebook.com/story.php?story_fbid=123&id=456",
            "https://www.facebook.com/profile.php?id=123&id=456",
            "https://www.facebook.com/profile.php?id=123&redirect=other",
            "https://www.facebook.com/profile.php?id=%31%32%33",
            "https://www.facebook.com/profile.php?id=0123",
            "https://www.facebook.com/profile.php?id=123#other",
            "https://m.facebook.com/profile.php?id=123",
        ] {
            let row = serde_json::json!({"url":source,"title":"Observed result","text":"Preview"});
            let hit = parse_row("facebook", &row).unwrap();
            assert_eq!((hit.author_id, hit.author_url), (None, None), "{source}");
        }
    }

    #[test]
    fn instagram_search_keeps_observed_native_identity_only() {
        assert!(!RETIRED_CREATOR_SOURCES.contains(&"instagram"));
        assert_eq!(command("instagram"), Some("search"));
        let mut row = serde_json::json!({
            "provider":"onecrew-instagram-native-v1", "author_id":"123456789",
            "username":"sample_user", "name":"Sample",
            "url":"https://www.instagram.com/sample_user/"
        });
        let hit = parse_row("instagram", &row).unwrap();
        assert_eq!(hit.author_id.as_deref(), Some("123456789"));
        assert_eq!(
            hit.author_url.as_deref(),
            Some("https://www.instagram.com/sample_user/")
        );
        assert_eq!(command_site("instagram"), "onecrew-search-instagram");
        row["url"] = serde_json::json!("https://www.instagram.com/another_user/");
        assert_eq!(
            parse_row("instagram", &row),
            Err(OpenCliError::InvalidOutput)
        );
        row["url"] = serde_json::json!("https://www.instagram.com/sample_user/");
        row["author_id"] = Value::Null;
        assert_eq!(
            parse_row("instagram", &row),
            Err(OpenCliError::InvalidOutput)
        );
        row["provider"] = Value::Null;
        assert!(parse_row("instagram", &row).unwrap().author_id.is_none());
    }
    #[test]
    fn reddit_native_author_and_post_binding_are_preserved() {
        let mut row = serde_json::json!({
            "provider":"onecrew-reddit-native-v1", "id":"abc123",
            "title":"Shampoo discussion", "author":"sample_user",
            "url":"https://www.reddit.com/comments/abc123/",
            "author_id":"t2_xyz123", "author_url":"https://www.reddit.com/user/sample_user/"
        });
        let hit = parse_row("reddit", &row).unwrap();
        assert_eq!(hit.author_id.as_deref(), Some("t2_xyz123"));
        assert_eq!(
            hit.author_url.as_deref(),
            Some("https://www.reddit.com/user/sample_user/")
        );
        row["author_id"] = serde_json::json!("sample_user");
        assert_eq!(parse_row("reddit", &row), Err(OpenCliError::InvalidOutput));
        row["author_id"] = Value::Null;
        row["author_url"] = Value::Null;
        assert!(parse_row("reddit", &row).unwrap().author_id.is_none());
        row["id"] = serde_json::json!("different");
        assert_eq!(parse_row("reddit", &row), Err(OpenCliError::InvalidOutput));
    }
    #[test]
    fn removed_sources_have_no_search_command_or_capability() {
        let registry = Registry::parse(r#"[
            {"command":"onecrew-xiaohongshu/search","site":"onecrew-xiaohongshu","name":"search","access":"read","browser":true},
            {"command":"rednote/search","site":"rednote","name":"search","access":"read","browser":true}
        ]"#).unwrap();
        for platform in [
            "xiaohongshu",
            "rednote",
            "brave",
            "1point3acres",
            "sinablog",
            "apple-podcasts",
            "pinterest-pins",
            "pinterest-boards",
            "dribbble-shots",
            "google-images",
            "reddit-subreddit",
            "linkedin-learning",
            "substack",
            "v2ex-node",
            "lesswrong-tag",
            "lobsters-tag",
            "stackoverflow-tag",
            "juejin-hot",
        ] {
            assert_eq!(command(platform), None);
            assert_eq!(
                crate::OpenCli::check_creator_search_limit(platform, 5),
                Err(OpenCliError::ReadCommandUnavailable)
            );
            assert!(
                !ecommerce_capabilities(&registry)
                    .iter()
                    .any(|item| item.platform == platform)
            );
        }
    }
    #[test]
    fn invalid_tag_and_unscoped_blog_empty_are_not_proven_zero_results() {
        assert!(ambiguous_empty_result("lesswrong-tag"));
        assert!(ambiguous_empty_result("linkedin"));
        assert_eq!(parse("sinablog", "[]", 5), Err(OpenCliError::InvalidOutput));
    }

    #[test]
    fn source_list_removes_missing_commands_and_unverified_bundles() {
        let sources = ["google", "instagram", "substack", "nowcoder", "missing"]
            .into_iter()
            .map(|site| CreatorSearchCapability {
                platform: site.into(),
                command: format!("{}/search", command_site(site)),
                browser_required: !is_public(site),
                registered: site != "missing",
            })
            .collect();
        let addons = [
            crate::OpenCliAddonStatus {
                id: "onecrew-browser-search",
                state: crate::OpenCliAddonState::Ready,
            },
            crate::OpenCliAddonStatus {
                id: "onecrew-substack",
                state: crate::OpenCliAddonState::Conflict,
            },
            crate::OpenCliAddonStatus {
                id: "onecrew-source-search",
                state: crate::OpenCliAddonState::Missing,
            },
        ];
        let kept = available_capabilities(sources, &addons);
        assert_eq!(
            kept.iter()
                .map(|source| source.platform.as_str())
                .collect::<Vec<_>>(),
            ["google", "instagram"]
        );
        // Login-requiring Instagram remains available; a local adapter problem
        // is the exclusion criterion, not the state of the user's account.
    }

    #[test]
    fn image_search_does_not_resolve_originals_and_keeps_query_positional() {
        let args = arguments("selected", "google-images", "--profile=other", 20);
        assert!(args.iter().any(|arg| arg == "--resolve=false"));
        assert_eq!(&args[args.len() - 2..], ["--", "--profile=other"]);
    }

    #[test]
    fn bundled_substack_custom_domain_requires_bound_post_provenance() {
        let mut row = serde_json::json!({"title":"Phone stand review", "author":"Writer",
            "url":"https://review.3ook.com/p/mo-lei-tau-1", "provider":"onecrew-substack-v1",
            "post_id":"216810906", "publication_id":"28738", "slug":"mo-lei-tau-1"});
        assert!(parse("substack", &serde_json::json!([row.clone()]).to_string(), 5).is_ok());
        row["slug"] = serde_json::json!("different");
        assert_eq!(
            parse("substack", &serde_json::json!([row]).to_string(), 5),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn linkedin_people_card_locates_profile_without_claiming_native_account_identity() {
        let rows = parse("linkedin", r#"[{"name":"Native Author","headline":"Keyboard reviewer","profile_url":"https://www.linkedin.com/in/native-author/"}]"#, 1).unwrap();
        assert_eq!(rows[0].author_label, "Native Author");
        assert!(rows[0].title.contains("Keyboard reviewer"));
        assert_eq!(rows[0].author_id.as_deref(), Some("native-author"));
        assert_eq!(
            rows[0].author_url.as_deref(),
            Some(rows[0].source_url.as_str())
        );
        for url in [
            "https://www.linkedin.com/company/native-author/",
            "https://www.linkedin.com/in/me/",
            "https://www.linkedin.com/in/native-author/?next=other",
            "https://www.linkedin.com/in/native-author/#other",
        ] {
            let rows = parse("linkedin", &serde_json::json!([{"name":"Native Author","headline":"Reviewer","profile_url":url}]).to_string(), 1).unwrap();
            assert_eq!(rows[0].author_id, None);
            assert_eq!(rows[0].author_url, None);
        }
    }

    #[test]
    fn native_search_mapping_preserves_stable_author_ids_and_profile_sources() {
        let bili = CreatorSearchHit::from(crate::BilibiliSearchVideo {
            adapter_version: "1.0.0".into(),
            bvid: "BV1RYqHYnEw8".into(),
            author_id: "8420618".into(),
            author: "Same name".into(),
            title: "demo".into(),
            url: "https://www.bilibili.com/video/BV1RYqHYnEw8".into(),
            author_url: "https://space.bilibili.com/8420618".into(),
        });
        assert_eq!(bili.author_id.as_deref(), Some("8420618"));
        assert_eq!(
            bili.author_url.as_deref(),
            Some("https://space.bilibili.com/8420618")
        );
        let tiktok = CreatorSearchHit::from(crate::TiktokSearchVideo {
            adapter_version: "1.0.0".into(),
            id: "123".into(),
            author_id: "6864157692300477445".into(),
            author: "dropship".into(),
            desc: "demo".into(),
            url: "https://www.tiktok.com/@dropship/video/123".into(),
        });
        assert_eq!(tiktok.author_id.as_deref(), Some("6864157692300477445"));
        assert_eq!(
            tiktok.author_url.as_deref(),
            Some("https://www.tiktok.com/@dropship")
        );
    }

    #[test]
    fn ecommerce_inventory_requires_exact_read_command_and_transport() {
        let registry = Registry::parse(&serde_json::json!([
            {"command":"onecrew-tiktok/search","site":"onecrew-tiktok","name":"search","access":"read","browser":true},
            {"command":"onecrew-bilibili/search","site":"onecrew-bilibili","name":"search","access":"read","browser":true},
            {"command":"instagram/search","site":"instagram","name":"search","access":"write","browser":true},
            {"command":"youtube/search","site":"youtube","name":"search","access":"read","browser":false,"strategy":"public"},
            {"command":"linkedin/search","site":"linkedin","name":"search","access":"read","browser":true},
            {"command":"onecrew-substack/search","site":"onecrew-substack","name":"search","access":"read","browser":false,"strategy":"public"},
            {"command":"pinterest/search-pins","site":"pinterest","name":"search-pins","access":"read","browser":true,"strategy":"cookie"},
            {"command":"linkedin/salesnav-search","site":"linkedin","name":"salesnav-search","access":"read","browser":true,"strategy":"ui"},
            {"command":"archive/search","site":"archive","name":"search","access":"read","browser":false,"strategy":"public"},
            {"command":"v2ex/node","site":"v2ex","name":"node","access":"read","browser":false,"strategy":"public"},
            {"command":"reddit/subreddit","site":"reddit","name":"subreddit","access":"read","browser":true,"strategy":"cookie"},
            {"command":"devto/tag","site":"devto","name":"tag","access":"read","browser":false,"strategy":"public"},
            {"command":"lesswrong/tag","site":"lesswrong","name":"tag","access":"read","browser":false,"strategy":"public"},
            {"command":"onecrew-public-lobsters/tag","site":"onecrew-public-lobsters","name":"tag","access":"read","browser":false,"strategy":"public"},
            {"command":"medium/tag","site":"medium","name":"tag","access":"read","browser":false,"strategy":"public"},
            {"command":"stackoverflow/tag","site":"stackoverflow","name":"tag","access":"read","browser":false,"strategy":"public"},
            {"command":"juejin/hot","site":"juejin","name":"hot","access":"read","browser":false,"strategy":"public"},
            {"command":"onecrew-public-producthunt/posts","site":"onecrew-public-producthunt","name":"posts","access":"read","browser":false,"strategy":"public"},
            {"command":"hf/spaces","site":"hf","name":"spaces","access":"read","browser":false,"strategy":"public"},
            {"command":"hf/models","site":"hf","name":"models","access":"read","browser":false,"strategy":"public"},
            {"command":"hf/datasets","site":"hf","name":"datasets","access":"read","browser":false,"strategy":"public"}
        ]).to_string()).unwrap();
        let capabilities = ecommerce_capabilities(&registry);
        assert_eq!(capabilities.len(), 13);
        let ready = capabilities
            .iter()
            .filter(|item| item.registered)
            .map(|item| item.platform.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ready, ["tiktok", "bilibili"]);
        let official_only = Registry::parse(r#"[
            {"command":"tiktok/search","site":"tiktok","name":"search","access":"read","browser":true},
            {"command":"bilibili/search","site":"bilibili","name":"search","access":"read","browser":true}
        ]"#).unwrap();
        assert!(
            !ecommerce_capabilities(&official_only)
                .iter()
                .any(
                    |item| matches!(item.platform.as_str(), "tiktok" | "bilibili")
                        && item.registered
                )
        );
        assert!(capabilities.iter().all(|item| item.platform != "nowcoder"));
        assert!(
            capabilities
                .iter()
                .all(|item| item.platform != "apple-podcasts")
        );
        assert!(!capabilities.iter().any(|item| item.platform == "tieba"));
        assert!(capabilities.iter().any(|item| item.platform == "linkedin"
            && item.command == "onecrew-search-linkedin/people-search"));
        assert!(capabilities.iter().any(|item| item.platform == "pinterest"
            && item.command == "onecrew-pinterest-reader/search-pins"));
        assert!(capabilities.iter().all(|item| item.platform != "weixin"));
        assert!(capabilities.iter().all(|item| item.platform != "substack"));
        assert!(
            capabilities
                .iter()
                .all(|item| item.platform != "reddit-subreddit")
        );
    }

    #[test]
    fn forum_searches_bind_post_identity_and_do_not_invent_authors() {
        let linux = parse(
            "linux-do",
            r#"[{"title":"AI tools","views":90000,"url":"https://linux.do/t/topic/123"}]"#,
            5,
        )
        .unwrap();
        assert!(linux[0].author_label.is_empty());
        for (kind, id, path) in [
            ("content", "123", "discuss/123"),
            (
                "moment",
                "abcdefabcdefabcdefabcdefabcdefab",
                "feed/main/detail/abcdefabcdefabcdefabcdefabcdefab",
            ),
        ] {
            let row = serde_json::json!([{"post_type":kind,"id":id,"uuid":id,
                "title":"AI tutorial", "author":"Creator", "author_id":"456",
                "author_url":"https://www.nowcoder.com/users/456",
                "url":format!("https://www.nowcoder.com/{path}")}]);
            assert_eq!(
                parse("nowcoder", &row.to_string(), 5).unwrap()[0].author_label,
                "Creator"
            );
            let mut mismatch = row.clone();
            mismatch[0]["author_url"] = "https://www.nowcoder.com/users/789".into();
            assert!(parse("nowcoder", &mismatch.to_string(), 5).is_err());
            let mut mismatch = row;
            mismatch[0]["id"] = "different".into();
            assert!(parse("nowcoder", &mismatch.to_string(), 5).is_err());
        }
    }

    #[test]
    fn podcast_and_tieba_searches_keep_source_identity_without_claiming_creator_identity() {
        let podcast = parse(
            "apple-podcasts",
            r#"[{"id":1558493362,"title":"Ecommerce show","author":"Trevor Ginn","url":"https://podcasts.apple.com/us/podcast/ecommerce-show/id1558493362?uo=4"}]"#,
            1,
        )
        .unwrap();
        assert_eq!(podcast[0].author_label, "Trevor Ginn");
        assert_eq!(podcast[0].author_id, None);
        let post = parse(
            "tieba",
            r#"[{"id":"123456","title":"跨境电商经验","author":"Example","url":"https://tieba.baidu.com/p/123456"}]"#,
            1,
        )
        .unwrap();
        assert_eq!(post[0].author_label, "Example");
        assert_eq!(post[0].author_id, None);
        for (site, output) in [
            (
                "apple-podcasts",
                r#"[{"id":1558493362,"title":"Show","url":"https://podcasts.apple.com/us/podcast/show/id1558493363"}]"#,
            ),
            (
                "apple-podcasts",
                r#"[{"id":1558493362,"title":"Show","url":"https://podcasts.apple.com.evil.test/us/podcast/show/id1558493362"}]"#,
            ),
            (
                "tieba",
                r#"[{"id":"123456","title":"Post","url":"https://tieba.baidu.com/p/789"}]"#,
            ),
            (
                "tieba",
                r#"[{"id":"123456","title":"Post","url":"https://tieba.baidu.com.evil.test/p/123456"}]"#,
            ),
        ] {
            assert_eq!(parse(site, output, 1), Err(OpenCliError::InvalidOutput));
        }
        assert_eq!(
            arguments("selected", "tieba", "AI tools", 2)[..2],
            ["--profile", "selected"]
        );
    }

    #[test]
    fn v2ex_node_search_keeps_topic_url_and_unverified_author_label() {
        let hits = parse(
            "v2ex-node",
            r#"[{"title":"Python tutorial","author":"member1","url":"https://www.v2ex.com/t/1243855"}]"#,
            1,
        )
        .unwrap();
        assert_eq!(hits[0].author_label, "member1");
        assert_eq!(hits[0].author_id, None);
        assert_eq!(hits[0].author_url, None);
        assert_eq!(
            arguments("unused", "v2ex-node", "python", 2),
            ["v2ex", "node", "--limit", "2", "-f", "json", "--", "python"]
        );
        for url in [
            "https://www.v2ex.com/t/1243856?source=search",
            "https://www.v2ex.com/t/0",
            "https://www.v2ex.com.evil.test/t/1243855",
        ] {
            let row = serde_json::json!([{"title":"Post","author":"member1","url":url}]);
            assert_eq!(
                parse("v2ex-node", &row.to_string(), 1),
                Err(OpenCliError::InvalidOutput)
            );
        }
    }

    #[test]
    fn subreddit_search_binds_post_id_and_community_without_claiming_author_identity() {
        let hits = parse(
            "reddit-subreddit",
            r#"[{"id":"1wq79hw","title":"Python demo","subreddit":"r/Python","author":"member1","url":"https://www.reddit.com/r/Python/comments/1wq79hw/python_demo/"}]"#,
            1,
        )
        .unwrap();
        assert_eq!(hits[0].author_label, "member1");
        assert_eq!(hits[0].author_id, None);
        assert_eq!(
            arguments("selected", "reddit-subreddit", "Python", 2),
            [
                "--profile",
                "selected",
                "reddit",
                "subreddit",
                "--limit",
                "2",
                "-f",
                "json",
                "--",
                "Python"
            ]
        );
        for url in [
            "https://www.reddit.com/r/Python/comments/different/python_demo/",
            "https://www.reddit.com/r/rust/comments/1wq79hw/python_demo/",
            "https://www.reddit.com.evil.test/r/Python/comments/1wq79hw/python_demo/",
        ] {
            let row = serde_json::json!([{"id":"1wq79hw","title":"Post","subreddit":"r/Python","author":"member1","url":url}]);
            assert_eq!(
                parse("reddit-subreddit", &row.to_string(), 1),
                Err(OpenCliError::InvalidOutput)
            );
        }
    }

    #[test]
    fn tag_searches_preserve_publisher_sources_without_inventing_account_identity() {
        for (site, source) in [
            (
                "devto-tag",
                r#"[{"id":4744538,"title":"Tutorial","author":"Writer","url":"https://dev.to/writer/tutorial-abc"}]"#,
            ),
            (
                "lesswrong-tag",
                r#"[{"title":"Essay","author":"Writer","url":"https://www.lesswrong.com/posts/uMQ3cqWDPHhjtiesc/essay"}]"#,
            ),
            (
                "lobsters-tag",
                r#"[{"id":"ylw6qp","title":"Story","author":"Submitter","url":"https://lobste.rs/s/ylw6qp/story"}]"#,
            ),
            (
                "medium-tag",
                r#"[{"title":"Essay","author":"Writer","url":"https://medium.com/@writer/essay-abcdef123456?source=rss"}]"#,
            ),
            (
                "stackoverflow-tag",
                r#"[{"id":80005838,"title":"Question","author":"User","url":"https://stackoverflow.com/questions/80005838/question"}]"#,
            ),
        ] {
            let hits = parse(site, source, 1).unwrap();
            assert_eq!(hits[0].author_id, None, "{site}");
            assert_eq!(hits[0].author_url, None, "{site}");
            assert!(!hits[0].author_label.is_empty(), "{site}");
            assert_eq!(arguments("unused", site, "ai", 2)[0], command_site(site));
        }
        let wrong_host = r#"[{"title":"Essay","author":"Writer","url":"https://www.lesswrong.com.evil.test/posts/uMQ3cqWDPHhjtiesc/essay"}]"#;
        assert_eq!(
            parse("lesswrong-tag", wrong_host, 1),
            Err(OpenCliError::InvalidOutput)
        );
        let wrong_id = r#"[{"id":80005838,"title":"Question","author":"User","url":"https://stackoverflow.com/questions/80005839/question"}]"#;
        assert_eq!(
            parse("stackoverflow-tag", wrong_id, 1),
            Err(OpenCliError::InvalidOutput)
        );
        let custom_publication = r#"[{"title":"Essay","author":"Writer","url":"https://ai.plainenglish.io/essay-0cf2da9ad88f?source=rss"}]"#;
        assert!(parse("medium-tag", custom_publication, 1).is_ok());
        let private_host = r#"[{"title":"Essay","author":"Writer","url":"https://publisher.internal/essay-0cf2da9ad88f"}]"#;
        assert_eq!(
            parse("medium-tag", private_host, 1),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn category_and_demo_searches_keep_source_ids_without_qualifying_authors() {
        for (site, source) in [
            (
                "juejin-hot",
                r#"[{"article_id":"7687331138987196425","title":"AI tutorial","author":"Writer","url":"https://juejin.cn/post/7687331138987196425"}]"#,
            ),
            (
                "producthunt-posts",
                r#"[{"name":"Demo product","author":"Maker","url":"https://www.producthunt.com/products/demo-product"}]"#,
            ),
            (
                "producthunt-posts",
                r#"[{"name":"Athenic","author":"Maker","url":"https://www.producthunt.com/products/athenic-ai/launches/athenic-ai-finance"}]"#,
            ),
            (
                "hf-spaces",
                r#"[{"id":"cvachet/pdf-chatbot","author":"cvachet","url":"https://huggingface.co/spaces/cvachet/pdf-chatbot"}]"#,
            ),
            (
                "hf-models",
                r#"[{"id":"meta-llama/Llama-3.2-1B-Instruct","author":"meta-llama","url":"https://huggingface.co/meta-llama/Llama-3.2-1B-Instruct"}]"#,
            ),
            (
                "hf-datasets",
                r#"[{"id":"iamtarun/python_code_instructions_18k_alpaca","author":"iamtarun","url":"https://huggingface.co/datasets/iamtarun/python_code_instructions_18k_alpaca"}]"#,
            ),
        ] {
            let hits = parse(site, source, 1).unwrap();
            assert_eq!(hits[0].author_id, None, "{site}");
            assert_eq!(hits[0].author_url, None, "{site}");
            assert!(!hits[0].author_label.is_empty(), "{site}");
        }
        for (site, source) in [
            (
                "juejin-hot",
                r#"[{"article_id":"7687331138987196425","title":"AI tutorial","author":"Writer","url":"https://juejin.cn/post/7687331138987196426"}]"#,
            ),
            (
                "producthunt-posts",
                r#"[{"name":"Demo product","author":"Maker","url":"https://www.producthunt.com.evil.test/products/demo-product"}]"#,
            ),
            (
                "hf-spaces",
                r#"[{"id":"cvachet/pdf-chatbot","author":"other","url":"https://huggingface.co/spaces/cvachet/pdf-chatbot"}]"#,
            ),
            (
                "hf-models",
                r#"[{"id":"meta-llama/Llama","author":"other","url":"https://huggingface.co/meta-llama/Llama"}]"#,
            ),
            (
                "hf-datasets",
                r#"[{"id":"owner/dataset","author":"owner","url":"https://huggingface.co/owner/dataset"}]"#,
            ),
        ] {
            assert_eq!(parse(site, source, 1), Err(OpenCliError::InvalidOutput));
        }
        assert!(arguments("unused", "juejin-hot", "ai", 1).contains(&"--category=ai".into()));
        assert!(
            arguments("unused", "producthunt-posts", "marketing-sales", 1)
                .contains(&"--category=marketing-sales".into())
        );
        assert!(
            arguments("unused", "hf-spaces", "chatbot", 1).contains(&"--search=chatbot".into())
        );
        for site in ["hf-models", "hf-datasets"] {
            assert!(arguments("unused", site, "llama", 1).contains(&"--search=llama".into()));
        }
    }

    #[test]
    fn added_creator_sources_keep_observed_url_and_conservative_identity() {
        let rows = [
            (
                "1point3acres",
                r#"[{"tid":"123","title":"Shopify 教程","author":"Alice","url":"https://www.1point3acres.com/bbs/thread-123-1-1.html"}]"#,
            ),
            (
                "hupu",
                r#"[{"tid":"456","title":"测评","author":"Bob","url":"https://bbs.hupu.com/456.html"}]"#,
            ),
            (
                "pixiv",
                r#"[{"illust_id":"789","user_id":"23","title":"作品","author":"Artist","url":"https://www.pixiv.net/artworks/789"}]"#,
            ),
            (
                "stackoverflow",
                r#"[{"id":"42","title":"Question","author":"Developer","url":"https://stackoverflow.com/questions/42/example"}]"#,
            ),
            (
                "pinterest-pins",
                r#"[{"pinId":"123","description":"Demo","pinner":"Creator","url":"https://www.pinterest.com/pin/123/"}]"#,
            ),
            (
                "pinterest-boards",
                r#"[{"boardId":"345","name":"Ideas","owner":"Creator","url":"https://www.pinterest.com/creator/ideas/"}]"#,
            ),
            (
                "linkedin-salesnav",
                r#"[{"name":"Creator","title":"Reviewer","recipient_urn":"urn:li:fs_salesProfile:(abc,def,ghi)","profile_url":"https://www.linkedin.com/in/abc"}]"#,
            ),
            (
                "linkedin-learning",
                r#"[{"title":"Course","instructor":"Teacher","url":"https://www.linkedin.com/learning/shopify-marketing"}]"#,
            ),
            (
                "archive",
                r#"[{"identifier":"item_1","title":"Video","creator":"Maker","url":"https://archive.org/details/item_1"}]"#,
            ),
            (
                "weread",
                r#"[{"bookId":"book123","title":"Book","author":"Writer","url":"https://weread.qq.com/web/bookDetail/abc123"}]"#,
            ),
            (
                "dribbble-shots",
                r#"[{"id":"123","title":"Design","designer":"Artist","url":"https://dribbble.com/shots/123-design"}]"#,
            ),
            (
                "dribbble-designers",
                r#"[{"username":"artist","name":"Artist","url":"https://dribbble.com/artist"}]"#,
            ),
            (
                "stackoverflow-users",
                r#"[{"userId":42,"displayName":"Educator","url":"https://stackoverflow.com/users/42/educator"}]"#,
            ),
            (
                "douban-books",
                r#"[{"id":"42","title":"Book","url":"https://book.douban.com/subject/42/"}]"#,
            ),
            (
                "douban-music",
                r#"[{"id":"43","title":"Album","url":"https://music.douban.com/subject/43/"}]"#,
            ),
        ];
        for (site, source) in rows {
            let hits = parse(site, source, 1).unwrap();
            assert_eq!(hits.len(), 1, "{site}");
            assert!(hits[0].source_url.starts_with("https://"), "{site}");
            if site == "pixiv" {
                assert_eq!(hits[0].author_id.as_deref(), Some("23"));
                assert_eq!(
                    hits[0].author_url.as_deref(),
                    Some("https://www.pixiv.net/users/23")
                );
            } else if site == "stackoverflow-users" {
                assert_eq!(hits[0].author_id.as_deref(), Some("42"));
                assert_eq!(
                    hits[0].author_url.as_deref(),
                    Some(hits[0].source_url.as_str())
                );
            } else {
                assert_eq!(hits[0].author_id, None, "{site}");
                assert_eq!(hits[0].author_url, None, "{site}");
            }
        }
        for site in ["google", "brave", "duckduckgo", "yahoo"] {
            let hits = parse(
                site,
                r#"[{"title":"Creator page","url":"https://example.com/creator","snippet":"unverified"}]"#,
                1,
            )
            .unwrap();
            assert_eq!(hits[0].source_url, "https://example.com/creator");
            assert_eq!(hits[0].author_label, "");
        }
        let image = parse("google-images", r#"[{"title":"Designer artwork","sourceUrl":"https://example.com/art","imageUrl":"https://cdn.example.com/image.png"}]"#, 1).unwrap();
        assert_eq!(image[0].source_url, "https://example.com/art");
        assert_eq!(image[0].author_id, None);
        let untitled_pin = parse(
            "pinterest-pins",
            r#"[{"pinId":"123","title":"","description":"","url":"https://www.pinterest.com/pin/123/"}]"#,
            1,
        )
        .unwrap();
        assert_eq!(untitled_pin[0].title, "123");
        assert_eq!(
            arguments("selected", "pinterest-pins", "art", 2)[2..4],
            ["pinterest", "search-pins"]
        );
        assert_eq!(
            arguments("selected", "linkedin-salesnav", "art", 2)[2..4],
            ["linkedin", "salesnav-search"]
        );
        assert_eq!(
            arguments("selected", "dribbble-designers", "--profile=other", 2)
                .last()
                .map(String::as_str),
            Some("--query=--profile=other")
        );
        assert_eq!(
            arguments("selected", "stackoverflow-users", "AI", 2)[0],
            "stackoverflow"
        );
        assert_eq!(
            arguments("selected", "douban-books", "AI", 2)[2..4],
            ["onecrew-search-douban", "search"]
        );
        assert_eq!(
            arguments("selected", "douban-books", "AI", 2)[8..10],
            ["--type", "book"]
        );
        assert_eq!(
            arguments("selected", "douban-music", "AI", 2)[8..10],
            ["--type", "music"]
        );
    }

    #[test]
    fn added_creator_sources_reject_rewritten_identity_and_local_web_results() {
        for (site, source) in [
            (
                "1point3acres",
                r#"[{"tid":"123","title":"Post","url":"https://www.1point3acres.com/bbs/thread-999-1-1.html"}]"#,
            ),
            (
                "hupu",
                r#"[{"tid":"456","title":"Post","url":"https://bbs.hupu.com/789.html"}]"#,
            ),
            (
                "pixiv",
                r#"[{"illust_id":"789","user_id":"23","title":"Art","url":"https://www.pixiv.net/artworks/123"}]"#,
            ),
            (
                "stackoverflow",
                r#"[{"id":"42","title":"Question","url":"https://stackoverflow.com/questions/43/example"}]"#,
            ),
            (
                "pinterest-pins",
                r#"[{"pinId":"123","title":"Pin","url":"https://www.pinterest.com/pin/999/"}]"#,
            ),
            (
                "pinterest-boards",
                r#"[{"boardId":"345","name":"Board","url":"https://www.pinterest.com.evil.test/owner/board/"}]"#,
            ),
            (
                "linkedin-salesnav",
                r#"[{"name":"Person","recipient_urn":"urn:li:fs_salesProfile:(abc,def,ghi)","profile_url":"https://www.linkedin.com/in/other"}]"#,
            ),
            (
                "linkedin-learning",
                r#"[{"title":"Course","url":"https://www.linkedin.com/jobs/example"}]"#,
            ),
            (
                "archive",
                r#"[{"identifier":"item_1","title":"Video","url":"https://archive.org/details/item_2"}]"#,
            ),
            (
                "weread",
                r#"[{"bookId":"book123","title":"Book","url":"weread://reading?bId=book123"}]"#,
            ),
            (
                "google",
                r#"[{"title":"Page","url":"https://127.0.0.1/admin"}]"#,
            ),
            (
                "google",
                r#"[{"title":"Advertisement","url":"https://www.google.com/goto?url=opaque"}]"#,
            ),
            (
                "brave",
                r#"[{"title":"Page","url":"https://localhost.local/page"}]"#,
            ),
            (
                "duckduckgo",
                r#"[{"title":"Page","url":"http://example.com/page"}]"#,
            ),
            (
                "yahoo",
                r#"[{"title":"Page","url":"https://user@example.com/page"}]"#,
            ),
            (
                "dribbble-shots",
                r#"[{"id":"123","title":"Shot","url":"https://dribbble.com/shots/1234"}]"#,
            ),
            (
                "dribbble-designers",
                r#"[{"username":"artist","name":"Artist","url":"https://dribbble.com/other"}]"#,
            ),
            (
                "stackoverflow-users",
                r#"[{"userId":42,"displayName":"Educator","url":"https://stackoverflow.com/users/43/other"}]"#,
            ),
            (
                "google-images",
                r#"[{"title":"Photo","imageUrl":"https://example.com/photo.png","sourceUrl":""}]"#,
            ),
            (
                "google-images",
                r#"[{"title":"Photo","imageUrl":"https://example.com/photo.png","sourceUrl":"https://www.google.com/search?tbm=isch"}]"#,
            ),
            (
                "douban-books",
                r#"[{"id":"42","title":"Book","url":"https://book.douban.com/subject/43/"}]"#,
            ),
            (
                "douban-music",
                r#"[{"id":"43","title":"Album","url":"https://book.douban.com/subject/43/"}]"#,
            ),
            ("douban-books", "[]"),
            ("douban-music", "[]"),
        ] {
            assert_eq!(
                parse(site, source, 1),
                Err(OpenCliError::InvalidOutput),
                "{site}"
            );
        }
    }

    #[test]
    fn blog_and_forum_searches_reject_rewritten_or_ambiguous_sources() {
        let row = r#"[{"title":"AI tools", "author":"Writer", "url":"https://blog.sina.com.cn/s/blog_123abc01.html"}]"#;
        assert_eq!(parse("sinablog", row, 3).unwrap()[0].author_label, "Writer");
        for (site, source) in [
            ("sinablog", "http://blog.sina.com.cn/s/blog_123abc01.html"),
            (
                "sinablog",
                "https://blog.sina.com.cn.evil.test/s/blog_123abc01.html",
            ),
            ("sinablog", "https://blog.sina.com.cn/s/blog_.html"),
            ("linux-do", "https://linux.do/t/topic/123?user=other"),
            ("linux-do", "https://linux.do/t/topic/../admin"),
            ("linux-do", "https://linux.do/t/topic/0"),
        ] {
            let value = serde_json::json!([{"title":"AI tools","url":source}]);
            assert!(parse(site, &value.to_string(), 3).is_err());
        }
        assert!(is_public("sinablog"));
        assert!(!is_public("nowcoder"));
        assert!(!is_public("linux-do"));
    }

    #[test]
    fn expanded_searches_keep_native_source_and_unknown_author() {
        let image_only = parse("jike", r#"[{"id":"62bcf0bb2c56439566b9c61f","content":"","author":"作者","url":"https://web.okjike.com/originalPost/62bcf0bb2c56439566b9c61f"}]"#, 5).unwrap();
        assert_eq!(image_only[0].title, "即刻帖子（无文字）");
        let jike = parse("jike", r#"[{"id":"post123","content":"AI 工具演示","author":"作者","url":"https://web.okjike.com/originalPost/post123"}]"#, 5).unwrap();
        assert_eq!(jike[0].author_label, "作者");
        let weixin = parse("weixin", r#"[{"title":"AI 工具","summary":"Not the author","url":"https://weixin.sogou.com/link?url=observed"}]"#, 5).unwrap();
        assert_eq!(weixin[0].author_label, "");
        let hn = parse("hackernews", r#"[{"id":"123456","title":"AI tools","author":"submitter","score":90000,"url":"https://external.example/tool"}]"#, 5).unwrap();
        assert_eq!(
            hn[0].source_url,
            "https://news.ycombinator.com/item?id=123456"
        );
        assert_eq!(hn[0].author_label, "submitter");
    }

    #[tokio::test]
    #[ignore = "explicit addon repair and six bounded read-only adapter checks; requires OPENCLI_TEST_PROFILE"]
    async fn repaired_sources_match_live_contract() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile required");
        let cli = crate::OpenCli::inspect_system().await.unwrap();
        cli.repair_bundled_addons(true).await.unwrap();
        let cli = crate::OpenCli::inspect_system().await.unwrap();
        for (site, query) in [
            ("jike", "稳定币"),
            ("google", "稳定币 中文 创作者"),
            ("duckduckgo", "稳定币 中文 创作者"),
            ("dribbble-shots", "stablecoin"),
            ("linux-do", "稳定币"),
            ("linkedin-salesnav", "crypto"),
        ] {
            let result = cli.creator_search(&profile, site, query, 5).await;
            match &result {
                Ok(rows) => eprintln!("{site}: parsed {} source rows", rows.len()),
                Err(error) => eprintln!("{site}: {error}"),
            }
            if matches!(site, "jike" | "google") {
                assert!(result.is_ok(), "{site} must parse real source rows");
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires exact installed add-ons, selected profile and one explicit Google/Yahoo origin"]
    async fn engine_source_native_identity_matches_actual_search_result() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit acceptance browser");
        let origin = std::env::var("OPENCLI_TEST_ENGINE").expect("explicit engine");
        assert!(matches!(origin.as_str(), "google" | "yahoo"));
        let cli = crate::OpenCli::inspect_system().await.unwrap();
        let started = std::time::Instant::now();
        let rows = cli
            .creator_search(
                &profile,
                &origin,
                "site:youtube.com/watch keyboard review",
                5,
            )
            .await
            .expect("actual engine result");
        println!(
            "Engine source search PASS; origin={origin}; elapsed_ms={}; rows={}",
            started.elapsed().as_millis(),
            rows.len()
        );
        let hit = rows
            .iter()
            .find(|hit| {
                Url::parse(&hit.source_url).is_ok_and(|url| {
                    url.scheme() == "https"
                        && matches!(url.host_str(), Some("youtube.com" | "www.youtube.com"))
                        && url.path() == "/watch"
                        && url.fragment().is_none()
                        && crate::youtube::video_id(&hit.source_url).is_ok()
                })
            })
            .expect("observed supported native video, no rewritten search or guessed source");
        assert!(
            hit.author_id.is_none() && hit.author_url.is_none(),
            "engine snippet never certifies author identity"
        );
        let video = crate::youtube::video_id(&hit.source_url).unwrap();
        let started = std::time::Instant::now();
        let native = cli
            .youtube_video_evidence(&profile, &video)
            .await
            .expect("actual source native receipt");
        assert_eq!(native.video_id, video);
        assert_eq!(
            native.source_url,
            format!("https://www.youtube.com/watch?v={video}")
        );
        assert!(native.channel_id.starts_with("UC") && native.channel_id.len() == 24);
        assert!(!native.description.trim().is_empty());
        println!(
            "Engine source native identity PASS; origin={origin}; elapsed_ms={}; original_bytes={}; no contact or recommendation asserted",
            started.elapsed().as_millis(),
            native.description.len()
        );
    }

    #[tokio::test]
    #[ignore = "requires selected browser, exact installed add-ons and explicit Douyin or Pinterest entrance"]
    async fn native_source_matches_actual_douyin_or_pinterest_search() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit acceptance browser");
        let site = std::env::var("OPENCLI_TEST_NATIVE").expect("explicit native entrance");
        assert!(matches!(site.as_str(), "douyin" | "pinterest"));
        let cli = crate::OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            cli.repair_bundled_addons(true)
                .await
                .expect("explicit repair owner and backup");
        }
        let query = std::env::var("OPENCLI_TEST_NATIVE_QUERY").unwrap_or_else(|_| {
            if site == "douyin" {
                "穿搭"
            } else {
                "fashion"
            }
            .into()
        });
        let started = std::time::Instant::now();
        let hits = cli
            .creator_search(&profile, &site, &query, 5)
            .await
            .expect("actual native search");
        println!(
            "Native search PASS; platform={site}; elapsed_ms={}; rows={}",
            started.elapsed().as_millis(),
            hits.len()
        );
        let hit = hits
            .first()
            .expect("actual source, no invented sample or extra search");
        if site == "douyin" {
            let url = Url::parse(&hit.source_url).unwrap();
            assert_eq!(url.scheme(), "https");
            assert_eq!(url.host_str(), Some("www.douyin.com"));
            assert!(url.query().is_none() && url.fragment().is_none());
            let video = url
                .path()
                .strip_prefix("/video/")
                .expect("matched native video");
            let started = std::time::Instant::now();
            let content = cli
                .douyin_original_content(&profile, video, None)
                .await
                .expect("actual matched caption and author");
            assert_eq!(content.post_id, video);
            assert_eq!(content.source_url, hit.source_url);
            assert!(!content.text.trim().is_empty());
            println!(
                "Native original PASS; platform={site}; elapsed_ms={}; original_bytes={}",
                started.elapsed().as_millis(),
                content.text.len()
            );
            let started = std::time::Instant::now();
            let author = cli
                .douyin_public_profile(&profile, &content.author_id)
                .await
                .expect("same native author profile");
            assert_eq!(author.author_id, content.author_id);
            assert_eq!(author.profile_url, content.profile_url);
            println!(
                "Native profile PASS; platform={site}; elapsed_ms={}; exact_follower_count={}; no contact or recommendation asserted",
                started.elapsed().as_millis(),
                author.follower_count.is_some()
            );
        } else {
            let mut originals = 0;
            for (index, hit) in hits.iter().take(5).enumerate() {
                let author_id = hit.author_id.as_deref().expect("observed native user ID");
                let url =
                    Url::parse(hit.author_url.as_deref().expect("observed pinner URL")).unwrap();
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("www.pinterest.com"));
                assert!(url.query().is_none() && url.fragment().is_none());
                let username = url.path().trim_matches('/');
                let started = std::time::Instant::now();
                let author = match cli
                    .pinterest_public_profile(&profile, author_id, username)
                    .await
                {
                    Ok(author) => author,
                    Err(OpenCliError::InvalidOutput) => {
                        println!(
                            "Native profile unavailable; platform={site}; sample={}; elapsed_ms={}; source-local skip",
                            index + 1,
                            started.elapsed().as_millis()
                        );
                        continue;
                    }
                    Err(error) => panic!("native profile stops acceptance: {error}"),
                };
                assert_eq!(author.author_id, author_id);
                assert_eq!(Some(author.profile_url.as_str()), hit.author_url.as_deref());
                println!(
                    "Native profile PASS; platform={site}; elapsed_ms={}; exact_follower_count={}",
                    started.elapsed().as_millis(),
                    author.follower_count.is_some()
                );
                let started = std::time::Instant::now();
                let pin_id = hit
                    .source_url
                    .strip_prefix("https://www.pinterest.com/pin/")
                    .and_then(|path| path.strip_suffix('/'))
                    .expect("observed Pin URL");
                let content = match cli
                    .pinterest_public_pin_content(&profile, author_id, username, pin_id)
                    .await
                {
                    Ok(content) => content,
                    Err(OpenCliError::InvalidOutput) => {
                        println!(
                            "Native original unavailable; platform={site}; sample={}; elapsed_ms={}; source-local skip",
                            index + 1,
                            started.elapsed().as_millis()
                        );
                        continue;
                    }
                    Err(error) => panic!("native original stops acceptance: {error}"),
                };
                assert_eq!(content.author_id, author.author_id);
                assert_eq!(content.profile_url, author.profile_url);
                assert!(!content.text.trim().is_empty());
                println!(
                    "Native original PASS; platform={site}; elapsed_ms={}; original_bytes={}; no image authorship, contact or recommendation asserted",
                    started.elapsed().as_millis(),
                    content.text.len()
                );
                originals += 1;
                break;
            }
            assert!(
                originals > 0,
                "no usable original in this actual five-author batch; no extra search or invented sample"
            );
        }
    }

    #[tokio::test]
    #[ignore = "explicit addon repair and five bounded read-only incident checks; requires OPENCLI_TEST_PROFILE"]
    async fn incident_sources_match_live_contract() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile required");
        let cli = crate::OpenCli::inspect_system().await.unwrap();
        cli.repair_bundled_addons(true).await.unwrap();
        let cli = crate::OpenCli::inspect_system().await.unwrap();
        for (site, query) in [
            ("hupu", "手机支架"),
            ("weixin", "虚拟货币"),
            ("pinterest-boards", "cryptocurrency"),
            ("lobsters-tag", "finance"),
            ("producthunt-posts", "finance"),
        ] {
            let rows = cli.creator_search(&profile, site, query, 2).await;
            match &rows {
                Ok(rows) => eprintln!("{site}: parsed {} source rows", rows.len()),
                Err(error) => eprintln!("{site}: {error}"),
            }
            assert!(
                rows.is_ok(),
                "incident source {site} still fails its native read contract"
            );
        }
    }

    #[test]
    fn expanded_searches_reject_ambiguous_identity_and_host_spoofing() {
        for (site, output) in [
            ("hackernews", r#"[{"id":"1&evil=2","title":"A"}]"#),
            (
                "hackernews",
                r#"[{"title":"A","url":"https://external.example/"}]"#,
            ),
            (
                "jike",
                r#"[{"id":"a","content":"A","url":"https://web.okjike.com/originalPost/b"}]"#,
            ),
            (
                "weixin",
                r#"[{"title":"A","url":"https://weixin.sogou.com.evil.test/link"}]"#,
            ),
            (
                "weixin",
                r#"[{"title":"A","url":"https://user:pass@mp.weixin.qq.com/s/123"}]"#,
            ),
        ] {
            assert_eq!(parse(site, output, 5), Err(OpenCliError::InvalidOutput));
        }
    }

    #[test]
    fn public_browser_search_still_pins_profile_and_limits_match_provider() {
        assert!(!is_public("weixin"));
        for site in ["weixin", "jike"] {
            let args = arguments("dedicated", site, "--profile=other", 5);
            assert_eq!(&args[..2], &["--profile", "dedicated"]);
            assert_eq!(&args[args.len() - 2..], &["--", "--profile=other"]);
        }
        let args = arguments("never-forward", "hackernews", "AI tools", 5);
        assert!(
            !args
                .iter()
                .any(|arg| arg == "--profile" || arg == "never-forward")
        );
        assert_eq!(
            crate::OpenCli::check_creator_search_limit("weixin", 10),
            Err(OpenCliError::ReadCommandUnavailable)
        );
        assert_eq!(
            crate::OpenCli::check_creator_search_limit("weixin", 11),
            Err(OpenCliError::ReadCommandUnavailable)
        );
    }

    #[test]
    fn public_search_does_not_forward_browser_identity_or_query_as_options() {
        assert_eq!(
            arguments("private-profile", "bluesky", "--profile=other", 5),
            [
                "onecrew-bluesky",
                "search",
                "--limit",
                "5",
                "-f",
                "json",
                "--",
                "--profile=other"
            ]
        );
        assert_eq!(
            arguments("dedicated", "medium", "AI tools", 5),
            [
                "--profile",
                "dedicated",
                "medium",
                "search",
                "--limit",
                "5",
                "-f",
                "json",
                "--",
                "AI tools"
            ]
        );
        assert!(is_public("substack"));
        assert!(!is_public("unknown"));
        assert!(!is_public("bilibili"));
    }

    #[test]
    fn public_profiles_and_publication_results_remain_source_backed_leads() {
        let blue = parse(
            "bluesky",
            r#"[{"handle":"creator.bsky.social","name":"AI Creator","followers":20000}]"#,
            5,
        )
        .unwrap();
        assert_eq!(
            blue[0].source_url,
            "https://bsky.app/profile/creator.bsky.social"
        );
        assert_eq!(blue[0].author_label, "creator.bsky.social");
        let unnamed = parse("bluesky", r#"[{"handle":"creator.example"}]"#, 5).unwrap();
        assert_eq!(unnamed[0].title, "creator.example");
        for (site, source) in [
            ("substack", "https://creator.substack.com/p/ai-tools"),
            ("medium", "https://medium.com/@creator/ai-tools-123"),
        ] {
            let output =
                serde_json::json!([{"title":"AI tools", "author":"Creator", "url":source}]);
            assert_eq!(
                parse(site, &output.to_string(), 5).unwrap()[0].source_url,
                source
            );
        }
    }

    #[test]
    fn unsafe_public_locators_and_unreviewed_custom_domains_fail_closed() {
        for handle in [
            "",
            "creator",
            "../admin",
            "evil.test/path",
            "a..test",
            "-a.test",
            "a.test?x=1",
        ] {
            let output = serde_json::json!([{"handle":handle}]).to_string();
            assert_eq!(
                parse("bluesky", &output, 5),
                Err(OpenCliError::InvalidOutput)
            );
        }
        for source in [
            "https://evilsubstack.com/p/ai",
            "https://creator.substack.com.evil.test/p/ai",
            "https://user:secret@creator.substack.com/p/ai",
            "https://creator.substack.com:8443/p/ai",
            "http://creator.substack.com/p/ai",
            "https://custom.example/p/ai",
        ] {
            let output = serde_json::json!([{"title":"AI", "url":source}]).to_string();
            assert_eq!(
                parse("substack", &output, 5),
                Err(OpenCliError::InvalidOutput)
            );
        }
    }

    #[test]
    fn repeated_sources_are_collapsed_but_output_limit_is_not_bypassed() {
        let output = r#"[{"handle":"a.bsky.social"},{"handle":"a.bsky.social"}]"#;
        assert_eq!(parse("bluesky", output, 2).unwrap().len(), 1);
        assert_eq!(
            parse("bluesky", output, 1),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn maps_bilibili_and_x_without_qualification_claims() {
        let bili = parse("bilibili", r#"[{"title":"AI 演示","author":"作者","url":"https://www.bilibili.com/video/BV1234567890"}]"#, 10).unwrap();
        assert_eq!(bili[0].author_label, "作者");
        let x = parse("twitter", r#"[{"id":"123","text":"AI tool demo","author":"@Creator","url":"https://x.com/i/status/123"}]"#, 10).unwrap();
        assert_eq!(x[0].title, "AI tool demo");
        assert_eq!(x[0].author_id.as_deref(), Some("creator"));
        assert_eq!(x[0].author_url.as_deref(), Some("https://x.com/creator"));
        let named_url = parse("twitter", r#"[{"id":"123","text":"AI tool demo","author":"Creator","url":"https://x.com/creator/status/123"}]"#, 10).unwrap();
        assert_eq!(named_url[0].author_id.as_deref(), Some("creator"));
    }

    #[test]
    fn x_author_locator_requires_native_handle_and_matching_post_id() {
        for row in [
            serde_json::json!({"id":"123","text":"AI tools","author":"fake/other","url":"https://x.com/i/status/123"}),
            serde_json::json!({"id":"123","text":"AI tools","author":"creator","url":"https://x.com/i/status/456"}),
            serde_json::json!({"id":"123","text":"AI tools","author":"creator","url":"https://x.com/other/status/123"}),
            serde_json::json!({"id":"123","text":"AI tools","author":"creator","url":"https://x.com/i/status/123?redirect=evil"}),
            serde_json::json!({"id":"123","text":"AI tools","author":"creator","url":"https://x.com.evil.example/i/status/123"}),
            serde_json::json!({"id":"123","text":"AI tools","author":"","url":"https://x.com/i/status/123"}),
        ] {
            assert_eq!(
                parse("twitter", &serde_json::json!([row]).to_string(), 1),
                Err(OpenCliError::InvalidOutput)
            );
        }
    }

    #[test]
    fn rejects_untrusted_or_changed_provider_rows() {
        assert_eq!(
            parse(
                "bilibili",
                r#"[{"title":"A","url":"https://evil.example/video/1"}]"#,
                10
            ),
            Err(OpenCliError::InvalidOutput)
        );
        assert_eq!(
            parse(
                "twitter",
                r#"[{"author":"x","url":"https://x.com/x/status/1"}]"#,
                10
            ),
            Err(OpenCliError::InvalidOutput)
        );
    }
}

#[cfg(test)]
mod removed_ecommerce_sources_tests {
    #[test]
    fn removed_sources_are_hidden_and_execution_is_disabled() {
        let registry = super::Registry::parse(r#"[{"command":"youtube/search","site":"youtube","name":"search","access":"read","browser":true}]"#).unwrap();
        let capabilities = super::ecommerce_capabilities(&registry);
        for site in [
            "zhihu",
            "pixiv",
            "weixin",
            "stackoverflow",
            "stackoverflow-users",
            "devto-tag",
            "hf-spaces",
            "nowcoder",
            "producthunt-posts",
            "medium",
            "medium-tag",
            "bluesky",
            "jike",
            "dribbble-designers",
            "hupu",
        ] {
            assert!(super::RETIRED_CREATOR_SOURCES.contains(&site));
            assert!(super::command(site).is_none());
            assert!(!capabilities.iter().any(|c| c.platform == site));
        }
        for site in ["youtube", "instagram", "facebook", "tiktok"] {
            assert!(capabilities.iter().any(|c| c.platform == site));
        }
    }
}
