//! Contacts observed in author-bound source text. Models cannot invent contacts.
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::LazyLock};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryContactKind {
    Email,
    Telegram,
    HiddenEmail,
    SocialProfile,
    Whatsapp,
    Wechat,
    WechatOfficial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryContact {
    pub kind: DiscoveryContactKind,
    pub original_value: String,
    pub normalized_value: String,
    pub source_url: String,
    pub source_excerpt: String,
    pub captured_at: i64,
    pub native_author_id: String,
    /// A source observation does not establish the destination's owner or purpose.
    pub attribution: DiscoveryContactAttribution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryContactAttribution {
    AuthorPublished,
    Unconfirmed,
}

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[a-z0-9][a-z0-9.!#$%&'*+/=?^_`{|}~-]*@[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+").unwrap()
});
static TELEGRAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)https://(?:t\.me|telegram\.me)/[a-z0-9_+/-]+(?:\?[a-z0-9_=&%-]+)?").unwrap()
});
static TELEGRAM_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?im)(?:^|[\s,，;；|])(?:telegram|tg|电报)[ \t]*[:：][ \t]*@?([a-z][a-z0-9_]{4,31})",
    )
    .unwrap()
});

/// Called only with saved, author-bound observations. Comments are never contact sources.
pub(crate) fn extract(
    text: &str,
    source_url: &str,
    native_author_id: &str,
    captured_at: i64,
    attribution: DiscoveryContactAttribution,
) -> Vec<DiscoveryContact> {
    if text.len() > 20_000
        || native_author_id.is_empty()
        || native_author_id.len() > 128
        || source_url.len() > 2048
        || captured_at <= 0
    {
        return vec![];
    }
    let Ok(source) = Url::parse(source_url) else {
        return vec![];
    };
    if source.scheme() != "https"
        || source.host_str().is_none()
        || !source.username().is_empty()
        || source.password().is_some()
    {
        return vec![];
    }
    let mut seen = BTreeSet::new();
    let mut contacts = Vec::new();
    for (kind, expression) in [
        (DiscoveryContactKind::Email, &*EMAIL),
        (DiscoveryContactKind::Telegram, &*TELEGRAM),
    ] {
        for found in expression.find_iter(text) {
            // Reject fragments of a longer locator, shortened links and email-like URL paths.
            let before = text[..found.start()].chars().next_back();
            let after = text[found.end()..].chars().next();
            if before.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '/' | '_'))
                || after.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '_' | '…'))
                || text[found.end()..].starts_with("...")
            {
                continue;
            }
            let original = found.as_str();
            if original.len() > 2048 {
                continue;
            }
            let normalized = match kind {
                DiscoveryContactKind::Email => {
                    let Some((local, domain)) = original.rsplit_once('@') else {
                        continue;
                    };
                    if local.len() > 64
                        || original.len() > 254
                        || local.ends_with('.')
                        || local.contains("..")
                    {
                        continue;
                    }
                    // Preserve local-part case; domains are case-insensitive.
                    format!("{local}@{}", domain.to_ascii_lowercase())
                }
                DiscoveryContactKind::HiddenEmail
                | DiscoveryContactKind::SocialProfile
                | DiscoveryContactKind::Whatsapp
                | DiscoveryContactKind::Wechat
                | DiscoveryContactKind::WechatOfficial => {
                    unreachable!()
                }
                DiscoveryContactKind::Telegram => {
                    let Ok(url) = Url::parse(original) else {
                        continue;
                    };
                    let path = url.path();
                    let target = path.trim_start_matches('/');
                    if target.is_empty()
                        || matches!(
                            target.to_ascii_lowercase().as_str(),
                            "share" | "iv" | "login" | "proxy" | "socks"
                        )
                        || target.starts_with("share/")
                        || target.starts_with("s/")
                        || (target.contains('/') && !target.starts_with("joinchat/"))
                    {
                        continue;
                    }
                    // Retain invite/group/bot locators without claiming they are private accounts.
                    format!(
                        "https://t.me/{target}{}",
                        url.query().map(|q| format!("?{q}")).unwrap_or_default()
                    )
                }
            };
            if !seen.insert(normalized.clone()) {
                continue;
            }
            let prefix: String = text[..found.start()]
                .chars()
                .rev()
                .take(100)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let suffix: String = text[found.end()..].chars().take(100).collect();
            contacts.push(DiscoveryContact {
                kind: kind.clone(),
                original_value: original.into(),
                normalized_value: normalized,
                source_url: source_url.into(),
                source_excerpt: format!("{prefix}{original}{suffix}"),
                captured_at,
                native_author_id: native_author_id.into(),
                attribution: attribution.clone(),
            });
        }
    }
    // A username is a Telegram locator only when the author explicitly labels
    // it as Telegram. Bare @handles must never be guessed across platforms.
    for capture in TELEGRAM_LABEL.captures_iter(text) {
        let handle = capture.get(1).unwrap();
        let after = text[handle.end()..].chars().next();
        if after
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '…' | '/' | '@' | '.' | '-'))
            || text[handle.end()..].starts_with("...")
        {
            continue;
        }
        let normalized = format!("https://t.me/{}", handle.as_str());
        if !seen.insert(normalized.clone()) {
            continue;
        }
        let observed = capture.get(0).unwrap().as_str().trim();
        contacts.push(DiscoveryContact {
            kind: DiscoveryContactKind::Telegram,
            original_value: observed.into(),
            normalized_value: normalized,
            source_url: source_url.into(),
            source_excerpt: observed.into(),
            captured_at,
            native_author_id: native_author_id.into(),
            attribution: attribution.clone(),
        });
    }
    for found in SOCIAL.find_iter(text) {
        let value = found.as_str().trim_end_matches(['.', ',', ';', ')', ']']);
        if !social_profile(value)
            || platform_host(value) == platform_host(source_url)
            || crate::contact_navigation::third_party_link(value, text)
        {
            continue;
        }
        if !seen.insert(value.to_string()) {
            continue;
        }
        contacts.push(DiscoveryContact {
            kind: DiscoveryContactKind::SocialProfile,
            original_value: value.into(),
            normalized_value: value.into(),
            source_url: source_url.into(),
            source_excerpt: value.into(),
            captured_at,
            native_author_id: native_author_id.into(),
            attribution: attribution.clone(),
        });
    }
    if attribution == DiscoveryContactAttribution::AuthorPublished {
        contacts.extend(extract_messaging(
            text,
            source_url,
            native_author_id,
            captured_at,
        ));
    }
    contacts
}

fn extract_messaging(
    text: &str,
    source: &str,
    author: &str,
    captured_at: i64,
) -> Vec<DiscoveryContact> {
    static WECHAT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:微信号?|wechat|\bwx|\bvx|商务(?:合作)?(?:[ \t]*(?:wx|vx|v|微信))?|[🌍🌎🌏][\u{FE0F} \t]*[:：])[ \t]*[:：]?[ \t]*([a-z][a-z0-9_-]{5,19})(?:[ \t]*[（(]([^\r\n）)]{1,100})[）)])?").unwrap()
    });
    static OFFICIAL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?:微信公众号|公众号)[ \t]*(?:(?:叫|是|名称)[ \t]*)?(?:[:：]?[ \t]*[「『“【\"]([^」』”】\"\r\n]{1,80})[」』”】\"]|[:：][ \t]*([^\s，,；;（）()「『“【\"]{1,80}))"#,
        )
        .unwrap()
    });
    // A globe followed by a V-prefixed numeric handle is a public contact convention,
    // unlike an ordinary location such as “🌍 London”. Keep the full handle.
    static GLOBE_HANDLE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"[🌍🌎🌏][\u{FE0F} \t]*([Vv][0-9]{5,19})(?:[ \t]*[（(]([^\r\n）)]{1,100})[）)])?",
        )
        .unwrap()
    });
    static WHATSAPP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)https://wa\.me/([1-9][0-9]{6,14})\b|whatsapp[ \t]*[:：][ \t]*(\+[1-9][0-9 ()-]{6,24}[0-9])").unwrap()
    });
    let mut result: Vec<DiscoveryContact> = Vec::new();
    let mut seen = BTreeSet::new();
    for (kind, regex) in [
        (DiscoveryContactKind::Wechat, &*WECHAT),
        (DiscoveryContactKind::Wechat, &*GLOBE_HANDLE),
        (DiscoveryContactKind::WechatOfficial, &*OFFICIAL),
        (DiscoveryContactKind::Whatsapp, &*WHATSAPP),
    ] {
        for captures in regex.captures_iter(text) {
            let found = captures.get(0).unwrap();
            if kind == DiscoveryContactKind::Wechat
                && found.as_str().starts_with("商务")
                && !Url::parse(source)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
                    .is_some_and(|host| {
                        [
                            "bilibili.com",
                            "douyin.com",
                            "xiaohongshu.com",
                            "weibo.com",
                            "kuaishou.com",
                            "x.com",
                            "twitter.com",
                        ]
                        .iter()
                        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
                    })
            {
                continue;
            }
            // Never accept a prefix of an email, longer handle, or clipped text.
            if text[found.end()..].starts_with(['@', '_', '-', '…'])
                || text[found.end()..].starts_with("...")
                || text[found.end()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric())
            {
                continue;
            }
            let raw = captures
                .get(1)
                .or_else(|| captures.get(2))
                .unwrap()
                .as_str();
            if found.as_str().starts_with(['🌍', '🌎', '🌏'])
                && !raw.chars().any(|c| c.is_ascii_digit() || c == '_' || c == '-')
            {
                continue;
            }
            if kind == DiscoveryContactKind::Wechat
                && matches!(
                    raw.to_ascii_lowercase().as_str(),
                    "wechat" | "whatsapp" | "telegram" | "instagram"
                )
            {
                continue;
            }
            let normalized = if kind == DiscoveryContactKind::Whatsapp {
                let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
                if !(7..=15).contains(&digits.len()) {
                    continue;
                }
                format!("https://wa.me/{digits}")
            } else {
                raw.to_string()
            };
            if !seen.insert(format!("{kind:?}:{normalized}")) {
                if let Some(existing) = result
                    .iter_mut()
                    .find(|c| c.kind == kind && c.normalized_value == normalized)
                    && found.as_str().chars().count() > existing.source_excerpt.chars().count()
                {
                    existing.original_value = found.as_str().into();
                    existing.source_excerpt = found.as_str().into();
                }
                continue;
            }
            result.push(DiscoveryContact {
                kind: kind.clone(),
                original_value: found.as_str().into(),
                normalized_value: normalized,
                source_url: source.into(),
                source_excerpt: found.as_str().into(),
                captured_at,
                native_author_id: author.into(),
                attribution: DiscoveryContactAttribution::AuthorPublished,
            });
        }
    }
    result
}

pub(crate) fn score_passes(verdict: &str, evidence: &serde_json::Value, author: &str) -> bool {
    verdict == "qualified"
        && evidence["assessment"]["reasons"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && evidence["reviewed_match"]["verified"] == true
        && evidence["reviewed_match"]["native_author_id"].as_str() == Some(author)
        && evidence["reviewed_match"]["outcome"] == "supported"
        && evidence["reviewed_match"]["level"].as_str()
            == evidence["assessment"]["match_rating"]["level"].as_str()
        && matches!(
            evidence["assessment"]["match_rating"]["level"].as_str(),
            Some("excellent" | "good" | "medium")
        )
}

/// Released approvals are intentionally tested separately from new recommendations.
#[cfg(test)]
pub(crate) fn legacy_approval_fixture(path: &std::path::Path) {
    rusqlite::Connection::open(path.join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE discovery_recommendation_policies SET policy='legacy'",
            [],
        )
        .unwrap();
}

static SOCIAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https://[^\s<>"']+"#).unwrap());
fn platform_host(value: &str) -> Option<String> {
    Url::parse(value)
        .ok()?
        .host_str()
        .map(|host| match host.trim_start_matches("www.") {
            "twitter.com" => "x.com".into(),
            "space.bilibili.com" => "bilibili.com".into(),
            "m.facebook.com" => "facebook.com".into(),
            host => host.into(),
        })
}
/// Only account routes, not content, communities, search, shops or shortened links.
pub(crate) fn social_profile(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|h| matches!(h, "facebook.com" | "www.facebook.com"))
        && url.path() == "/profile.php"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
    {
        let pairs = url.query_pairs().collect::<Vec<_>>();
        return pairs.len() == 1
            && pairs[0].0 == "id"
            && !pairs[0].1.is_empty()
            && pairs[0].1.len() <= 20
            && !pairs[0].1.starts_with('0')
            && pairs[0].1.bytes().all(|b| b.is_ascii_digit());
    }
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || value.contains("...")
        || value.contains('…')
    {
        return false;
    }
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let handle = |s: &str| {
        !s.is_empty()
            && s.len() <= 128
            && s.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '%'))
    };
    match url.host_str().unwrap_or("").trim_start_matches("www.") {
        "instagram.com" | "x.com" | "twitter.com" => {
            parts.len() == 1
                && handle(parts[0])
                && ![
                    "home", "explore", "search", "i", "accounts", "reel", "reels", "p",
                ]
                .contains(&parts[0])
        }
        "tiktok.com" => parts.len() == 1 && parts[0].starts_with('@') && handle(&parts[0][1..]),
        "douyin.com" => {
            parts.len() == 2
                && parts[0] == "user"
                && crate::discovery_douyin::valid_author(parts[1])
        }
        "youtube.com" => {
            (parts.len() == 1 && parts[0].starts_with('@') && handle(&parts[0][1..]))
                || (parts.len() == 2
                    && parts[0] == "channel"
                    && parts[1].starts_with("UC")
                    && parts[1].len() == 24)
        }
        "space.bilibili.com" => {
            parts.len() == 1 && parts[0].bytes().all(|b| b.is_ascii_digit()) && !parts[0].is_empty()
        }
        "linkedin.com" => parts.len() == 2 && parts[0] == "in" && handle(parts[1]),
        "reddit.com" => parts.len() == 2 && matches!(parts[0], "user" | "u") && handle(parts[1]),
        "facebook.com" => {
            parts.len() == 1
                && handle(parts[0])
                && !["groups", "watch", "login", "share", "reel", "marketplace"].contains(&parts[0])
        }
        "bsky.app" => parts.len() == 2 && parts[0] == "profile" && handle(parts[1]),
        _ => false,
    }
}

/// Map locators to the candidate's native platform, independent of the page
/// where a link was found (a website link-back isn't an alternative account).
pub(crate) fn contact_platform(value: &str) -> Option<&'static str> {
    match platform_host(value)?.as_str() {
        "youtube.com" => Some("youtube"),
        "bilibili.com" => Some("bilibili"),
        "x.com" => Some("twitter"),
        "instagram.com" => Some("instagram"),
        "facebook.com" => Some("facebook"),
        "tiktok.com" => Some("tiktok"),
        "douyin.com" => Some("douyin"),
        "linkedin.com" => Some("linkedin"),
        "reddit.com" => Some("reddit"),
        "bsky.app" => Some("bluesky"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn douyin_contact_requires_the_native_author_profile_not_video_or_search() {
        let author = "MS4wLjABAAAA_native_author_123456789";
        let source = format!("https://www.douyin.com/user/{author}");
        assert!(social_profile(&source));
        assert_eq!(contact_platform(&source), Some("douyin"));
        for invalid in [
            "https://www.douyin.com/video/7688639864627214835".to_string(),
            "https://www.douyin.com/user/editable_douyin_number".to_string(),
            format!("https://www.douyin.com/user/{author}?other=1"),
            format!("https://www.douyin.com.evil.example/user/{author}"),
        ] {
            assert!(!social_profile(&invalid));
        }
    }

    #[test]
    fn official_account_named_in_biography_preserves_type_and_source() {
        for text in [
            "公众号叫「外设研究」",
            "公众号是『外设研究』",
            "微信公众号名称：【外设研究】",
            "公众号：外设研究",
        ] {
            let contacts = extract(
                text,
                "https://space.bilibili.com/123",
                "123",
                10,
                DiscoveryContactAttribution::AuthorPublished,
            );
            assert_eq!(contacts.len(), 1, "{text}");
            let contact = &contacts[0];
            assert_eq!(contact.kind, DiscoveryContactKind::WechatOfficial);
            assert_eq!(contact.normalized_value, "外设研究");
            assert_eq!(contact.source_url, "https://space.bilibili.com/123");
            assert_eq!(contact.native_author_id, "123");
            assert!(contact.source_excerpt.contains("外设研究"));
            assert!(
                extract(
                    text,
                    "https://example.com",
                    "123",
                    10,
                    DiscoveryContactAttribution::Unconfirmed
                )
                .is_empty()
            );
        }
        assert!(
            extract(
                "公众号是很方便的工具，欢迎关注",
                "https://space.bilibili.com/123",
                "123",
                10,
                DiscoveryContactAttribution::AuthorPublished
            )
            .is_empty()
        );
    }

    #[test]
    fn x_business_and_location_contact_conventions() {
        for (text, expected) in [
            ("商务合作V：chendahuang6（备注意向）", "chendahuang6"),
            ("商务合作 vx: creator_123", "creator_123"),
            ("🌍： Gk_lyy", "Gk_lyy"),
            ("🌍V510626 出海、AI账号", "V510626"),
        ] {
            let rows = extract(
                text,
                "https://x.com/creator",
                "123",
                10,
                DiscoveryContactAttribution::AuthorPublished,
            );
            assert_eq!(rows.len(), 1, "{text}");
            assert_eq!(rows[0].kind, DiscoveryContactKind::Wechat);
            assert_eq!(rows[0].normalized_value, expected);
            assert!(rows[0].source_excerpt.contains(expected));
        }
        for text in [
            "🌍 London",
            "🌍: London",
            "🌍：Beijing",
            "V for victory",
            "商务合作V：email@example.com",
            "🌍V510626toolongforwechat000000",
        ] {
            let rows = extract(
                text,
                "https://x.com/creator",
                "123",
                10,
                DiscoveryContactAttribution::AuthorPublished,
            );
            assert!(
                rows.iter().all(|r| r.kind != DiscoveryContactKind::Wechat),
                "{text}"
            );
        }
    }

    #[test]
    fn bilibili_business_shorthand_preserves_wechat_and_remarks() {
        let contacts = extract(
            "商务cyx112656 桌椅国补群553583633\nB站微博油管同名 商务wx：cherrymiaojiang （备注来历）\n商务wx：cherrymiaojiang （备注来历）",
            "https://space.bilibili.com/471303350",
            "471303350",
            10,
            DiscoveryContactAttribution::AuthorPublished,
        );
        assert_eq!(contacts.len(), 2);
        assert!(
            contacts
                .iter()
                .all(|c| c.kind == DiscoveryContactKind::Wechat)
        );
        assert!(contacts.iter().any(|c| c.normalized_value == "cyx112656"));
        assert!(
            contacts
                .iter()
                .any(|c| c.normalized_value == "cherrymiaojiang"
                    && c.source_excerpt.contains("备注来历"))
        );
    }

    #[test]
    fn messaging_routes_preserve_type_and_remarks_without_guessing_other_contacts() {
        let contacts = extract(
            "商务合作 GJYWS719\n商务合作 GJYWS719（备注品牌来意） 微信：cherrymiaojiang（备注来历） 公众号：『硬件评测』 WhatsApp: +1 202-555-0123 商务合作 QQ:123456 商务合作 email@example.com",
            "https://space.bilibili.com/451115073",
            "451115073",
            10,
            DiscoveryContactAttribution::AuthorPublished,
        );
        assert!(
            contacts
                .iter()
                .any(|c| c.kind == DiscoveryContactKind::Wechat
                    && c.normalized_value == "GJYWS719"
                    && c.source_excerpt.contains("备注品牌来意"))
        );
        assert!(
            contacts
                .iter()
                .any(|c| c.kind == DiscoveryContactKind::Wechat
                    && c.normalized_value == "cherrymiaojiang")
        );
        assert!(
            contacts
                .iter()
                .any(|c| c.kind == DiscoveryContactKind::WechatOfficial
                    && c.normalized_value == "硬件评测")
        );
        assert!(
            contacts
                .iter()
                .any(|c| c.kind == DiscoveryContactKind::Whatsapp
                    && c.normalized_value == "https://wa.me/12025550123")
        );
        assert_eq!(
            contacts
                .iter()
                .filter(|c| c.kind == DiscoveryContactKind::Wechat)
                .count(),
            2
        );
        assert!(
            extract(
                "商务合作 abcdefgh",
                "https://example.com/about",
                "author",
                10,
                DiscoveryContactAttribution::AuthorPublished
            )
            .is_empty()
        );
        assert!(
            extract(
                "微信：cherrymiaojiang",
                "https://example.com/about",
                "author",
                10,
                DiscoveryContactAttribution::Unconfirmed
            )
            .is_empty()
        );
    }

    #[test]
    fn contact_navigation_staff_accounts_do_not_enter_manual_contact_table() {
        let contacts = extract(
            "Follow The Studio:\nhttps://instagram.com/thestudio\n\nFollow the staff:\nAdam https://instagram.com/adam\nEric https://twitter.com/eric",
            "https://youtube.com/watch?v=abcdefghijk",
            "channel",
            10,
            DiscoveryContactAttribution::AuthorPublished,
        );
        assert_eq!(contacts.len(), 1);
        assert_eq!(
            contacts[0].normalized_value,
            "https://instagram.com/thestudio"
        );
        assert_eq!(contacts[0].native_author_id, "channel");
    }

    #[test]
    fn observes_contacts_with_source_and_no_inferred_private_identity() {
        let contacts = extract(
            "合作: Person@EXAMPLE.com, https://telegram.me/team_bot https://t.me/+AbCdefg Person@example.com",
            "https://x.com/author/status/123",
            "456",
            10,
            DiscoveryContactAttribution::AuthorPublished,
        );
        assert_eq!(contacts.len(), 3);
        assert_eq!(contacts[0].normalized_value, "Person@example.com");
        assert_eq!(contacts[1].normalized_value, "https://t.me/team_bot");
        assert_eq!(contacts[2].normalized_value, "https://t.me/+AbCdefg");
        assert!(
            contacts.iter().all(
                |c| c.native_author_id == "456" && c.source_excerpt.contains(&c.original_value)
            )
        );
    }

    #[test]
    fn rejects_guess_handles_shortened_locators_and_invalid_sources() {
        assert!(
            extract(
                "@someone https://t.me/example... https://t.me/share?url=x a..b@example.com",
                "https://x.com/author",
                "456",
                10,
                DiscoveryContactAttribution::AuthorPublished
            )
            .is_empty()
        );
        assert!(
            extract(
                "a@example.com",
                "file:///private",
                "456",
                10,
                DiscoveryContactAttribution::AuthorPublished
            )
            .is_empty()
        );
    }

    #[test]
    fn telegram_labeled_username_is_observed_not_guessed() {
        let contacts = extract(
            "邮箱: creator@example.com\nTelegram: laozhouhengmei77\nTG：@team_bot\n@bare_handle\nTG: shortened...\nTG: toolongabcdefghijklmnopqrstuvwxyz123456789",
            "https://www.youtube.com/channel/UCaaaaaaaaaaaaaaaaaaaaaa",
            "UCaaaaaaaaaaaaaaaaaaaaaa",
            10,
            DiscoveryContactAttribution::AuthorPublished,
        );
        assert_eq!(contacts.len(), 3);
        assert_eq!(
            contacts[1].normalized_value,
            "https://t.me/laozhouhengmei77"
        );
        assert_eq!(contacts[2].normalized_value, "https://t.me/team_bot");
        assert!(
            contacts
                .iter()
                .all(|contact| contact.source_excerpt.contains(&contact.original_value))
        );
    }

    #[test]
    fn qualification_requires_saved_score_and_no_missing_condition() {
        let mut evidence = json!({"assessment":{"reasons":[],"match_rating":{"level":"good"}},"reviewed_match":{"verified":true,"native_author_id":"456","outcome":"supported","level":"good"}});
        assert!(score_passes("qualified", &evidence, "456"));
        assert!(!score_passes("pending", &evidence, "456"));
        assert!(!score_passes("qualified", &evidence, "other"));
        evidence["reviewed_match"]["level"] = json!("low");
        assert!(!score_passes("qualified", &evidence, "456"));
        evidence["reviewed_match"]["level"] = json!("good");
        evidence["assessment"]["reasons"] = json!(["language_unverified"]);
        assert!(!score_passes("qualified", &evidence, "456"));
        evidence["assessment"]["reasons"] = json!([]);
        evidence["assessment"]["match_rating"]["level"] = json!("low");
        assert!(!score_passes("qualified", &evidence, "456"));
    }
}
