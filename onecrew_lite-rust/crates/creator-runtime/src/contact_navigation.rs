//! Contact lead selection changes read order, never author identity or qualification.

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Skip explicit third-party credits, not an author's business representative.
/// Unknown labels remain eligible. A separate occurrence outside a credits block
/// keeps the URL eligible; section labels never establish account ownership.
pub(crate) fn third_party_link(source: &str, text: &str) -> bool {
    let mut credits = false;
    let mut found = false;
    for line in text.lines() {
        let label = line.trim().trim_start_matches('#').trim().to_lowercase();
        if label.is_empty() {
            credits = false;
            continue;
        }
        let explicit = [
            "follow the staff",
            "follow our staff",
            "follow the guests",
            "guest:",
            "guests:",
            "edited by:",
            "filmed by:",
            "关注工作人员",
            "關注工作人員",
            "嘉宾：",
            "嘉賓：",
            "制作人员：",
            "製作人員：",
            "出演者：",
            "ゲスト：",
            "출연진:",
            "게스트:",
            "invitados:",
            "invités :",
            "gäste:",
        ]
        .iter()
        .any(|prefix| label.starts_with(prefix));
        if !line.contains("https://") && (label.ends_with(':') || label.ends_with('：')) {
            credits = explicit;
        } else if explicit {
            credits = true;
        }
        if crate::discovery_public_pages::description_observes_link(line, source) {
            found = true;
            if !credits {
                return false;
            }
        }
    }
    found
}

/// Search summaries are navigation hints only. Require the observed title or
/// URL to identify the searched author before spending a page read.
pub fn contact_search_lead_matches(
    author: &str,
    username: Option<&str>,
    source: &str,
    title: &str,
) -> bool {
    let Ok(url) = url::Url::parse(source) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || non_contact(source)
    {
        return false;
    }
    let title_words = words(title);
    [Some(author), username].into_iter().flatten().any(|name| {
        let name_words = words(name);
        if name_words.is_empty() {
            return false;
        }
        let compact = name_words.concat();
        if compact.chars().count() < 2 {
            return false;
        }
        // CJK names commonly touch adjacent title words without spaces.
        ((priority(source, None) <= 2
            || ["商务", "联系", "合作", "contact", "email"].iter().any(|word| title.to_lowercase().contains(word)))
            && name.chars().any(|c| matches!(c, '\u{3400}'..='\u{9fff}' | '\u{3040}'..='\u{30ff}' | '\u{ac00}'..='\u{d7af}'))
            && title.to_lowercase().contains(&name.to_lowercase()))
            || ((priority(source, None) <= 2 || title_words.iter().any(|word| matches!(word.as_str(), "contact" | "email" | "business" | "partnership"))) && title_words
            .windows(name_words.len())
            .any(|window| window == name_words))
            || url
                .host_str()
                .unwrap_or_default()
                .split('.')
                .any(|label| words(label).concat() == compact)
            || (crate::discovery_contact_lookup::exact_username(source).is_some() && url
                .path_segments()
                .into_iter()
                .flatten()
                .any(|segment| words(segment.trim_start_matches('@')).concat() == compact))
    })
}

/// Native biography first, then explicit contact pages and profile/link hubs.
/// Unknown short links remain eligible, after direct destinations.
pub(crate) fn priority(source: &str, native_profile: Option<&str>) -> u8 {
    if native_profile == Some(source) {
        return 0;
    }
    let Ok(url) = url::Url::parse(source) else {
        return 9;
    };
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.");
    if url.path().split('/').any(|p| {
        matches!(
            p.to_ascii_lowercase().as_str(),
            "contact"
                | "contact-us"
                | "contacts"
                | "business"
                | "cooperation"
                | "collaboration"
                | "partnerships"
                | "kontakt"
                | "contacto"
                | "contato"
                | "contatti"
                | "kontaktieren"
                | "contacter"
        )
    }) {
        return 1;
    }
    if matches!(
        host,
        "linktr.ee" | "beacons.ai" | "bio.site" | "carrd.co" | "solo.to" | "t.me"
    ) || crate::discovery_contact_lookup::exact_username(source).is_some()
    {
        return 2;
    }
    if matches!(
        host,
        "bit.ly" | "tinyurl.com" | "t.co" | "shorturl.at" | "goo.gl"
    ) {
        return 8;
    }
    4
}

/// Only explicit promotion on the same observed line is skipped. Do not infer
/// a shortener's destination, and keep links labeled for contact or a website.
pub(crate) fn promotional(source: &str, text: &str) -> bool {
    let lines: Vec<_> = text.lines().collect();
    lines.iter().enumerate().any(|(index, line)| {
        if !crate::discovery_public_pages::description_observes_link(line, source) {
            return false;
        }
        let line = if line.trim() == source {
            format!(
                "{} {}",
                index.checked_sub(1).map_or("", |previous| lines[previous]),
                line
            )
        } else {
            line.to_string()
        }
        .trim()
        .to_lowercase();
        if [
            "contact", "business", "website", "official", "email", "联系", "商务", "官网",
        ]
        .iter()
        .any(|word| line.contains(word))
        {
            return false;
        }
        // Explicit labels observed in live descriptions. Unknown product names
        // and unlabeled short links remain eligible; contact labels win above.
        line.contains("use code ")
            || line.contains("discount code ")
            || line.contains("sizing chart")
            || [
                "typing demo",
                "my tutorials",
                "my unboxing",
                "ranking spreadsheet",
                "my edited version",
                "affiliate links",
                "keyboard supplies",
                "sponsored links",
                "video setup",
                "photo setup",
                "stream setup",
                "newsletter",
                "get your",
                "my gear",
                "my setup",
            ]
            .iter()
            .any(|label| line.contains(label))
            || line.starts_with('$')
            || [
                "buy ",
                "shop ",
                "my mouse",
                "my books:",
                "my affiliates:",
                "购买",
                "购买链接",
                "优惠链接",
            ]
            .iter()
            .any(|word| line.starts_with(word))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_filters_irrelevant_reposts_without_certifying_matching_identity() {
        for (url, title) in [
            ("https://t.me/s/HW_HUB/21842", "HARDWARE HUB"),
            (
                "https://sponsorship.so/top-influencers/electronics",
                "Top 50 Electronics YouTube Influencers",
            ),
            (
                "https://reddit.com/r/watches/comments/123/repost",
                "Rolex Datejust showcase",
            ),
            ("https://notrunner.example/contact", "Unrelated contact"),
        ] {
            assert!(!contact_search_lead_matches(
                "Hardware Canucks",
                None,
                url,
                title
            ));
        }
        assert!(contact_search_lead_matches(
            "小Lin说",
            None,
            "https://agency.example/talent/42",
            "小Lin说商务合作联系方式"
        ));
        assert!(contact_search_lead_matches(
            "Hardware Canucks",
            None,
            "https://hardwarecanucks.com/contact",
            "Contact"
        ));
        assert!(contact_search_lead_matches(
            "Hardware Canucks",
            None,
            "https://agency.example/talent/123",
            "Hardware Canucks | Business contact"
        ));
        assert!(contact_search_lead_matches(
            "博主甲",
            Some("creator_x"),
            "https://x.com/creator_x",
            "Creator"
        ));
        assert!(!contact_search_lead_matches(
            "Runner",
            None,
            "https://notrunner.example/contact",
            "Runnerup contact"
        ));
        assert!(!contact_search_lead_matches(
            "Runner",
            None,
            "https://amazon.com/products/runner",
            "Runner"
        ));
    }
    #[test]
    fn cjk_name_in_unrelated_article_does_not_authorize_contact_read() {
        assert!(!contact_search_lead_matches(
            "小Lin说",
            None,
            "https://news.example/articles/123",
            "小Lin说与会计师讨论经济"
        ));
        assert!(contact_search_lead_matches(
            "小Lin说",
            None,
            "https://www.instagram.com/xiaolin",
            "小Lin说"
        ));
    }

    #[test]
    fn explicit_promotions_are_skipped_but_unknown_shorteners_and_contact_links_remain() {
        assert!(promotional(
            "https://bit.ly/item",
            "Buy shoes https://bit.ly/item"
        ));
        assert!(promotional(
            "https://bit.ly/item",
            "$65 mouse https://bit.ly/item"
        ));
        assert!(!promotional(
            "https://bit.ly/item",
            "Official website https://bit.ly/item"
        ));
        assert!(!promotional(
            "https://bit.ly/item",
            "Buy consulting: business contact https://bit.ly/item"
        ));
        assert!(!promotional(
            "https://bit.ly/item",
            "Other https://bit.ly/item"
        ));
    }
}

#[cfg(test)]
mod live_regressions {
    use super::*;
    #[test]
    fn contact_navigation_rejects_observed_non_contact_routes() {
        for source in [
            "https://www.youtube.com/channel/UC123/join",
            "https://youtu.be/6gYVGUBOsvA",
            "https://www.reddit.com/r/MouseReview/comments/123/creator",
            "https://rover.ebay.com/rover/1/123",
            "https://collabs.shop/uhjmep",
            "https://cherryxtrfy.com/mice/mz1-wireless/",
            "https://wooting.io/wooting-60he-v2?partner_id=DiamondLobby",
        ] {
            assert!(non_contact(source), "{source}");
        }
        assert!(!non_contact("https://creator.example/contact"));
        assert!(!non_contact("https://twitter.com/creator"));
        assert!(promotional(
            "https://cherryxtrfy.com/pad",
            "My mouse pad: https://cherryxtrfy.com/pad"
        ));
    }
    #[test]
    fn contact_navigation_skips_live_discount_and_sizing_links() {
        assert!(promotional(
            "https://gamersupps.gg/moist",
            "Get Goof Juice and use code MOIST https://gamersupps.gg/moist"
        ));
        assert!(promotional(
            "https://bit.ly/RJNSIZING",
            "RJN's Sizing Charts - https://bit.ly/RJNSIZING"
        ));
        assert!(!promotional(
            "https://creator.example/contact",
            "Business contact for discount code inquiries https://creator.example/contact"
        ));
        assert!(!promotional(
            "https://creator.example",
            "I write code https://creator.example"
        ));
        assert!(!promotional(
            "https://bit.ly/unknown",
            "Zygen NP-01 - https://bit.ly/unknown"
        ));
    }
    #[test]
    fn contact_navigation_social_aliases_share_read_identity() {
        assert_eq!(
            read_key("https://twitter.com/boardzyxd/"),
            read_key("https://x.com/boardzyxd")
        );
        assert_ne!(read_key("https://x.com/one"), read_key("https://x.com/two"));
    }
}

pub(crate) fn non_contact(source: &str) -> bool {
    if crate::discovery_public_pages::is_product_link(source) {
        return true;
    }
    let Ok(url) = url::Url::parse(source) else {
        return true;
    };
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.");
    let path = url.path();
    if matches!(
        host,
        "docs.google.com" | "drive.google.com" | "inex.osekai.net" | "osekai.net"
    ) || (host == "tiktok.com" && path.starts_with("/discover/"))
        || (host == "reddit.com" && path.starts_with("/r/"))
    {
        return true;
    }
    matches!(
        host,
        "youtu.be"
            | "collabs.shop"
            | "rover.ebay.com"
            | "rocketreach.co"
            | "contactout.com"
            | "muckrack.com"
            | "instainflu.com"
            | "scrumball.com"
            | "shop.anker.com"
            | "ankerfast.club"
            | "lddy.no"
            | "rstyle.me"
            | "go.magik.ly"
            | "c.klarna.com"
            | "aritzia.com"
            | "kit.co"
            | "store.cablemod.com"
            | "go.hototools.com"
            | "hototools.com"
    ) || (host == "wooting.io" && path != "/contact" && path != "/contact/")
        || ((host == "x.com" || host == "twitter.com") && path.contains("/status/"))
        || (host == "instagram.com"
            && ["/p/", "/reel/", "/reels/"]
                .iter()
                .any(|p| path.starts_with(p)))
        || (host == "tiktok.com" && path.contains("/video/"))
        || (host == "linkedin.com" && ["/posts/", "/feed/"].iter().any(|p| path.starts_with(p)))
        || path.split('/').any(|p| {
            matches!(
                p,
                "form-submission-response"
                    | "checkout"
                    | "cart"
                    | "shop"
                    | "store"
                    | "stores"
                    | "product"
                    | "privacy-policy"
                    | "terms-of-service"
                    | "discord"
            )
        })
        || (host == "bit.ly"
            && ["keychron", "k65plus", "hyper-next-drive"]
                .iter()
                .any(|slug| path.to_lowercase().contains(slug)))
        || (host == "youtube.com"
            && (path == "/watch"
                || path == "/playlist"
                || path.starts_with("/shorts/")
                || path.ends_with("/join")))
        || (host == "reddit.com" && path.contains("/comments/"))
        || (host == "cherryxtrfy.com"
            && (path.starts_with("/mice/") || path.starts_with("/xtrfy-")))
        || url
            .query_pairs()
            .any(|(k, _)| matches!(k.as_ref(), "partner_id" | "sca_ref" | "aff_id"))
}
pub(crate) fn read_key(source: &str) -> String {
    let Ok(mut url) = url::Url::parse(source) else {
        return source.into();
    };
    if matches!(
        url.host_str(),
        Some("twitter.com" | "www.twitter.com" | "www.x.com")
    ) && url.set_host(Some("x.com")).is_err()
    {
        return source.into();
    }
    url.set_fragment(None);
    if matches!(url.host_str(), Some("youtube.com" | "www.youtube.com")) {
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if (parts.len() == 3 && parts[0] == "channel" && parts[2] == "about")
            || (parts.len() == 2 && parts[0].starts_with('@') && parts[1] == "about")
        {
            let path = format!("/{}", parts[..parts.len() - 1].join("/"));
            url.set_path(&path);
            url.set_query(None);
        }
    }
    if crate::discovery_contact_lookup::exact_username(url.as_str()).is_some() {
        url.set_query(None);
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&path);
    }
    url.to_string()
}

/// Use only an explicit social profile URL in author-owned text, never display names.
pub(crate) fn linked_username(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"https://[^\s<>\[\]()]+").expect("constant profile URL pattern");
    re.find_iter(text)
        .filter(|m| !third_party_link(m.as_str().trim_end_matches([',', '.', ';']), text))
        .filter_map(|m| {
            crate::discovery_contact_lookup::exact_username(
                m.as_str().trim_end_matches([',', '.', ';']),
            )
        })
        .next()
}

/// Contact discovery language is the approved content language, not the UI locale.
/// New drafts carry translated phrases from their existing planner call; historical
/// drafts use common locale aliases without another model or platform request.
pub(crate) fn search_query(name: &str, language: &str, translated: &[String]) -> String {
    let aliases = language.trim().to_lowercase();
    let terms = match aliases.split(['-', '_']).next().unwrap_or("") {
        "中文" | "汉语" | "繁体中文" | "简体中文" | "chinese" | "zh" => {
            "邮箱 OR 商务合作 OR 联系 OR 官网"
        }
        "日语" | "日本語" | "japanese" | "ja" => "メール OR お問い合わせ OR 仕事依頼",
        "韩语" | "한국어" | "korean" | "ko" => "이메일 OR 문의 OR 비즈니스",
        "西班牙语" | "español" | "spanish" | "es" => "correo OR contacto OR colaboración",
        "法语" | "français" | "french" | "fr" => "courriel OR contact OR partenariat",
        "德语" | "deutsch" | "german" | "de" => "E-Mail OR Kontakt OR Kooperation",
        "葡萄牙语" | "português" | "portuguese" | "pt" => "email OR contato OR parceria",
        "意大利语" | "italiano" | "italian" | "it" => "email OR contatti OR collaborazione",
        "俄语" | "русский" | "russian" | "ru" => "почта OR контакты OR сотрудничество",
        "阿拉伯语" | "العربية" | "arabic" | "ar" => "بريد OR تواصل OR تعاون",
        "印地语" | "हिन्दी" | "hindi" | "hi" => "ईमेल OR संपर्क OR सहयोग",
        "泰语" | "ไทย" | "thai" | "th" => "อีเมล OR ติดต่อ OR งานโฆษณา",
        "越南语" | "vietnamese" | "vi" => "email OR liên hệ OR hợp tác",
        "印尼语" | "indonesian" | "id" => "email OR kontak OR kerja sama",
        "土耳其语" | "turkish" | "tr" => "e-posta OR iletişim OR işbirliği",
        "荷兰语" | "dutch" | "nl" => "e-mail OR contact OR samenwerking",
        "波兰语" | "polish" | "pl" => "email OR kontakt OR współpraca",
        "乌克兰语" | "ukrainian" | "uk" => "пошта OR контакти OR співпраця",
        _ => "contact",
    };
    let localized = if !translated.is_empty()
        && translated.len() <= 6
        && translated.iter().all(|t| {
            !t.trim().is_empty()
                && t.chars().count() <= 50
                && t.chars().all(|c| {
                    !c.is_control()
                        && !matches!(
                            c,
                            '"' | '(' | ')' | ':' | '|' | '<' | '>' | '\\' | '/' | '@'
                        )
                })
        }) {
        translated
            .iter()
            .map(|t| format!("\"{}\"", t.trim()))
            .collect::<Vec<_>>()
            .join(" OR ")
    } else {
        terms.into()
    };
    format!("\"{name}\" ({localized} OR email OR Telegram)")
}

/// Prefer author profiles and localized contact destinations to third-party mentions.
pub fn contact_search_lead_priority(
    author: &str,
    username: Option<&str>,
    source: &str,
    title: &str,
) -> u8 {
    let base = priority(source, None);
    if base <= 2 {
        return base;
    }
    let Ok(url) = url::Url::parse(source) else {
        return 9;
    };
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.");
    if [Some(author), username].into_iter().flatten().any(|n| {
        host.split('.')
            .any(|part| words(part).concat() == words(n).concat())
    }) {
        return 3;
    }
    let text = title.to_lowercase();
    if [
        "email",
        "contact",
        "business",
        "商务",
        "邮箱",
        "聯絡",
        "お問い合わせ",
        "문의",
        "kontakt",
        "contato",
        "contatti",
        "correo",
        "联系",
        "partnership",
        "сотрудничество",
        "تواصل",
    ]
    .iter()
    .any(|p| text.contains(p))
    {
        return 4;
    }
    7
}

#[cfg(test)]
mod enrichment_tests {
    use super::*;
    #[test]
    fn arbitrary_planner_language_terms_and_legacy_aliases_preserve_author_identifiers() {
        for (lang, word) in [
            ("ja", "お問い合わせ"),
            ("es-ES", "colaboración"),
            ("de", "Kooperation"),
            ("ru", "сотрудничество"),
            ("ar", "تواصل"),
            ("pt-BR", "contato"),
            ("th", "ติดต่อ"),
        ] {
            let q = search_query("ThinkPadCreator", lang, &[]);
            assert!(q.contains(word));
            assert!(q.starts_with("\"ThinkPadCreator\""));
        }
        assert!(
            search_query(
                "Creator",
                "芬兰语",
                &["yhteystiedot".into(), "yhteistyö".into()]
            )
            .contains("yhteistyö")
        );
        assert!(!search_query("Creator", "芬兰语", &["site:evil".into()]).contains("evil"));
        for term in ["संपर्क", "อีเมล", "ইমেইল"] {
            assert!(search_query("Creator", "custom", &[term.into()]).contains(term));
        }
        assert!(
            contact_search_lead_priority("Creator", None, "https://creator.example/", "Home")
                < contact_search_lead_priority(
                    "Creator",
                    None,
                    "https://forum.example/posts/1",
                    "Discuss Creator"
                )
        );
    }
}

#[cfg(test)]
mod quality_regressions {
    use super::*;
    #[test]
    fn contact_navigation_staff_block_does_not_expand_or_seed_username() {
        let text = "Follow the staff:\nAdam https://twitter.com/adam\nEric https://eric.example\n\nFollow The Studio:\nhttps://twitter.com/TheStudio\nhttps://www.instagram.com/thestudio/";
        assert!(third_party_link("https://twitter.com/adam", text));
        assert!(third_party_link("https://eric.example", text));
        assert!(!third_party_link("https://twitter.com/TheStudio", text));
        assert!(!third_party_link(
            "https://www.instagram.com/thestudio/",
            text
        ));
        assert_eq!(linked_username(text).as_deref(), Some("TheStudio"));
    }

    #[test]
    fn contact_navigation_credits_do_not_hide_separate_contact_evidence() {
        for header in [
            "Follow the staff:",
            "嘉宾：",
            "ゲスト：",
            "게스트:",
            "invitados:",
            "invités :",
            "gäste:",
        ] {
            let source = "https://instagram.com/guest";
            let text =
                format!("{header}\n{source}\nBusiness contact:\nhttps://agency.example/contact");
            assert!(third_party_link(source, &text), "{header}");
            assert!(!third_party_link("https://agency.example/contact", &text));
            assert!(!third_party_link(
                source,
                &format!("{text}\n\nOfficial account:\n{source}")
            ));
        }
        assert!(!third_party_link(
            "https://agency.example/contact",
            "Management team: https://agency.example/contact"
        ));
        assert!(!third_party_link(
            "https://instagram.com/guest",
            "Unrecognized label:\nhttps://instagram.com/guest"
        ));
        assert!(!third_party_link(
            "https://instagram.com/not-present",
            "Follow the staff:\nhttps://instagram.com/guest"
        ));
    }
    #[test]
    fn same_name_staff_and_athlete_pages_are_not_contact_leads() {
        for (source, title) in [
            (
                "https://blandinfoundation.org/about/our-people/staff-members/kyle/",
                "Kyle Erickson",
            ),
            (
                "https://uclabruins.com/player-bio-kyle-erickson-ucla-official-athletic-site",
                "Kyle Erickson - UCLA",
            ),
            (
                "https://contactout.com/Kyle-Erickson-15142389",
                "Kyle Erickson email",
            ),
        ] {
            assert!(!contact_search_lead_matches(
                "Kyle Erickson",
                Some("io.kyle"),
                source,
                title
            ));
        }
    }
    #[test]
    fn observed_product_and_setup_routes_are_filtered() {
        for source in [
            "https://kit.co/bvan1412/2022-stream-video-setup",
            "https://go.magik.ly/ml/2821k/",
            "https://store.cablemod.com?ref=54789",
            "https://hototools.com/pages/contact-us",
        ] {
            assert!(non_contact(source));
        }
        assert!(promotional(
            "https://bit.ly/keychronk3max",
            "My gear:\nhttps://bit.ly/keychronk3max"
        ));
        assert!(!promotional(
            "https://creator.example/contact",
            "Business contact:\nhttps://creator.example/contact"
        ));
    }
    #[test]
    fn live_low_value_routes_are_not_contact_leads() {
        for source in [
            "https://x.com/switch_click/status/1538159885094879232",
            "https://instagram.com/p/DMQwTS6h1BV/",
            "https://rocketreach.co/reid-wilking-email_353543122",
            "https://muckrack.com/media-outlet/diamondlobby",
            "https://wooting.io/wooting-60he-v2",
            "https://wooting.io/project-optimum",
            "https://shop.anker.com/2SgX9n",
            "https://www.reidwilking.com/form-submission-response",
        ] {
            assert!(non_contact(source), "{source}");
        }
        assert!(!non_contact("https://diamondlobby.com/contact/"));
        assert!(!non_contact("https://www.instagram.com/only_optimum/"));
    }
    #[test]
    fn youtube_about_is_same_read_identity() {
        assert_eq!(
            read_key("https://www.youtube.com/channel/UCGTxQKN8nd47EDvnbBWIY-g/about"),
            read_key("https://www.youtube.com/channel/UCGTxQKN8nd47EDvnbBWIY-g")
        );
    }
}

#[cfg(test)]
mod fashion_regressions {
    #[test]
    fn keyboard_contact_navigation_skips_files_games_and_community_pages() {
        for source in [
            "https://docs.google.com/spreadsheets/d/ranking/edit",
            "https://drive.google.com/file/d/skin/view",
            "https://inex.osekai.net/profiles/15806513",
            "https://www.tiktok.com/discover/jkzu123_",
            "https://www.reddit.com/r/HipyoTech/",
        ] {
            assert!(super::non_contact(source), "{source}");
        }
        assert!(!super::non_contact("https://creator.example/contact"));
        assert!(!super::non_contact("https://instagram.com/creator"));
        assert!(super::promotional(
            "https://bit.ly/demos",
            "My XL typing demos: https://bit.ly/demos"
        ));
        assert!(super::promotional(
            "https://creator.example/gear",
            "My keyboard ranking spreadsheet:\nhttps://creator.example/gear"
        ));
        assert!(!super::promotional(
            "https://creator.example/contact",
            "Business contact:\nhttps://creator.example/contact"
        ));
    }
    #[test]
    fn rejects_shopping_and_playlist_contact_routes() {
        for source in [
            "https://rstyle.me/+abc",
            "https://c.klarna.com/al/GQGb/",
            "https://www.aritzia.com/us/en/contact-us",
            "https://youtube.com/playlist?list=PLabc",
        ] {
            assert!(super::non_contact(source), "{source}");
        }
        assert!(!super::non_contact("https://creator.example/contact"));
        assert!(!super::non_contact(
            "https://www.shopltk.com/explore/Suggyl"
        ));
    }
}
