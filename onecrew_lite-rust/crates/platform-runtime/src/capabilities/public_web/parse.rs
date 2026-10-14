//! Pure HTML/structured-data extraction; no network or application state.

use std::{collections::BTreeSet, sync::LazyLock};

use regex::Regex;
use schemars::JsonSchema;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}\b").expect("email regex")
});

/// The page layer a link or address was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkHubLayer {
    /// A server-rendered `<a href>` outside the page's own navigation.
    Anchor,
    /// A `sameAs` entry inside any `application/ld+json` block.
    JsonLd,
    /// The `__NEXT_DATA__` hydration payload: social icons and link tiles.
    Hydration,
}

/// One outbound link as found on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkHubLink {
    pub layer: LinkHubLayer,
    pub url: String,
    /// The anchor text or the owner-written tile title, when there is one.
    pub text: Option<String>,
}

/// One email address as found on the page, lowercased.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkHubEmail {
    pub layer: LinkHubLayer,
    pub value: String,
}

/// What one link-hub page publishes, deduplicated within the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkHubExtraction {
    pub source_url: String,
    pub links: Vec<LinkHubLink>,
    pub emails: Vec<LinkHubEmail>,
}

/// One page's extraction state: the layers share the sink so a destination
/// surfaced by anchors, JSON-LD and hydration data lands once.
struct PageExtraction {
    base_host: String,
    links: Vec<LinkHubLink>,
    emails: Vec<LinkHubEmail>,
    seen_links: BTreeSet<String>,
    seen_emails: BTreeSet<String>,
}

impl PageExtraction {
    fn push_link(&mut self, layer: LinkHubLayer, target: &str, text: Option<&str>) {
        let Ok(target) = Url::parse(target) else {
            return;
        };
        if !matches!(target.scheme(), "http" | "https")
            || !target.username().is_empty()
            || target.password().is_some()
        {
            return;
        }
        let target_host = target
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.")
            .to_ascii_lowercase();
        if target_host.is_empty() || target_host == self.base_host {
            return;
        }
        let url = String::from(target);
        if !self.seen_links.insert(url.to_ascii_lowercase()) {
            return;
        }
        self.links.push(LinkHubLink {
            layer,
            url,
            text: text
                .map(collapse_whitespace)
                .filter(|text| !text.is_empty()),
        });
    }

    fn push_email_matches(&mut self, layer: LinkHubLayer, text: &str) {
        let matches: Vec<String> = EMAIL
            .find_iter(text)
            .map(|matched| matched.as_str().to_ascii_lowercase())
            .collect();
        for value in matches {
            if self.seen_emails.insert(value.clone()) {
                self.emails.push(LinkHubEmail { layer, value });
            }
        }
    }

    /// A destination as one layer reports it: a `mailto:` is an address,
    /// anything else a link.
    fn push_destination(&mut self, layer: LinkHubLayer, url: &str, text: Option<&str>) {
        if let Some(mailbox) = url.strip_prefix("mailto:") {
            self.push_email_matches(layer, mailbox);
        } else {
            self.push_link(layer, url, text);
        }
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Linktree renders its top social icons as href-less buttons and keeps
/// the destinations in structured data, so anchors alone are lossy:
/// extraction layers SSR anchors, JSON-LD `sameAs`, and the
/// `__NEXT_DATA__` hydration payload (socialLinks incl. EMAIL_ADDRESS,
/// plus every configured tile's url and owner-written title). Unknown or
/// future page schemas simply yield whatever the surviving layers find.
pub(super) fn parse_link_hub(body: &str, initial_url: &Url, final_url: &Url) -> LinkHubExtraction {
    let mut extraction = LinkHubExtraction {
        source_url: initial_url.to_string(),
        links: Vec::new(),
        emails: Vec::new(),
    };
    let base_host = final_url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let document = Html::parse_document(body);
    let mut page = PageExtraction {
        base_host,
        links: Vec::new(),
        emails: Vec::new(),
        seen_links: BTreeSet::new(),
        seen_emails: BTreeSet::new(),
    };
    extract_anchor_layer(&document, final_url, &mut page);
    extract_json_ld_layer(&document, &mut page);
    extract_hydration_layer(&document, &mut page);
    extraction.links = page.links;
    extraction.emails = page.emails;
    extraction
}

/// The "Explore other Linktrees" recommendation modal advertises OTHER
/// people's profiles; its card text once minted a phantom `raz@guy.raz`
/// owner email.
fn inside_recommendation_modal(anchor: &ElementRef<'_>) -> bool {
    anchor
        .ancestors()
        .filter_map(ElementRef::wrap)
        .any(|node| node.value().attr("data-seo-modal-panel").is_some())
}

fn extract_anchor_layer(document: &Html, base_url: &Url, page: &mut PageExtraction) {
    let selector = Selector::parse("a[href]").expect("static anchor selector");
    for anchor in document.select(&selector) {
        if inside_recommendation_modal(&anchor) {
            continue;
        }
        let Some(href) = anchor.value().attr("href").map(str::trim) else {
            continue;
        };
        if let Some(mailbox) = href.strip_prefix("mailto:") {
            page.push_email_matches(LinkHubLayer::Anchor, mailbox);
            continue;
        }
        let Ok(target) = Url::parse(href).or_else(|_| base_url.join(href)) else {
            continue;
        };
        let target_host = target
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.")
            .to_ascii_lowercase();
        if target_host == page.base_host {
            // Internal navigation and profile recommendations: neither the
            // link nor its text is the owner's published data.
            continue;
        }
        let text = anchor.text().collect::<String>();
        page.push_email_matches(LinkHubLayer::Anchor, &text);
        page.push_link(LinkHubLayer::Anchor, target.as_str(), Some(&text));
    }
}

/// JSON-LD `sameAs` anywhere inside any ld+json block (ProfilePage's
/// mainEntity today; the recursive walk survives schema reshuffles).
fn extract_json_ld_layer(document: &Html, page: &mut PageExtraction) {
    let selector =
        Selector::parse(r#"script[type="application/ld+json"]"#).expect("static ld+json selector");
    for script in document.select(&selector) {
        let text = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        collect_same_as(&value, page);
    }
}

fn collect_same_as(value: &serde_json::Value, page: &mut PageExtraction) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, member) in map {
                if key == "sameAs" {
                    for entry in member.as_array().into_iter().flatten() {
                        if let Some(url) = entry.as_str() {
                            page.push_destination(LinkHubLayer::JsonLd, url, None);
                        }
                    }
                } else {
                    collect_same_as(member, page);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_same_as(item, page);
            }
        }
        _ => {}
    }
}

/// `__NEXT_DATA__.props.pageProps`: `socialLinks` carry the href-less top
/// icons (type EMAIL_ADDRESS uses a mailto url), and `links` carry every
/// configured tile: CLASSIC/EXTENSION urls plus owner-written titles like
/// "Contacto (mail@example.com)". Tile definitions are owner config;
/// visitor form input never appears here.
fn extract_hydration_layer(document: &Html, page: &mut PageExtraction) {
    let selector =
        Selector::parse(r#"script[id="__NEXT_DATA__"]"#).expect("static next-data selector");
    for script in document.select(&selector) {
        let text = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(page_props) = value.pointer("/props/pageProps") else {
            continue;
        };
        for social in page_props
            .get("socialLinks")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(url) = social.get("url").and_then(|value| value.as_str()) {
                page.push_destination(LinkHubLayer::Hydration, url, None);
            }
        }
        for link in page_props
            .get("links")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
        {
            let title = link.get("title").and_then(|value| value.as_str());
            if let Some(title) = title {
                page.push_email_matches(LinkHubLayer::Hydration, title);
            }
            let Some(url) = link.get("url").and_then(|value| value.as_str()) else {
                continue;
            };
            if url.is_empty() {
                continue;
            }
            page.push_destination(LinkHubLayer::Hydration, url, title);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_fixture(body: &str, source_url: &str) -> LinkHubExtraction {
        let source = Url::parse(source_url).unwrap();
        parse_link_hub(body, &source, &source)
    }

    #[test]
    fn redirected_page_uses_final_location_and_keeps_initial_provenance() {
        let initial = Url::parse("https://linktr.ee/original").unwrap();
        let final_url = Url::parse("http://beacons.ai/creator/profile").unwrap();
        let extraction = parse_link_hub(
            r#"<a href="//example.com/contact">Website</a>
               <a href="../navigation">not-owner@example.com</a>
               <a href="https://linktr.ee/original">Original page</a>"#,
            &initial,
            &final_url,
        );
        assert_eq!(extraction.source_url, initial.as_str());
        assert_eq!(
            urls(&extraction),
            ["http://example.com/contact", "https://linktr.ee/original"]
        );
        assert!(extraction.emails.is_empty());
    }

    fn urls(extraction: &LinkHubExtraction) -> Vec<&str> {
        extraction
            .links
            .iter()
            .map(|link| link.url.as_str())
            .collect()
    }

    fn emails(extraction: &LinkHubExtraction) -> Vec<&str> {
        extraction
            .emails
            .iter()
            .map(|email| email.value.as_str())
            .collect()
    }

    #[test]
    fn preserves_public_links_as_found_and_ignores_internal_navigation() {
        let source_url = "https://linktr.ee/kriptomessi";
        let extraction = parse_fixture(
            r#"
                <html><body>
                  <a href="https://twitter.com/Kriptomessi">Twitter</a>
                  <a href="https://twitter.com/Kriptomessi">Same destination, deduplicated in-page</a>
                  <a href="https://x.com/Kriptomessi">Another destination, kept as found</a>
                  <a href="https://www.youtube.com/@Kriptomessi">YouTube</a>
                  <a href="https://t.me/kriptomessitg">Telegram</a>
                  <a href="https://www.instagram.com/kriptomessi/">Instagram</a>
                  <a href="mailto:Contact@Kriptomessi.com?subject=Hello">Contact Mail</a>
                  <a href="https://kriptomessi.com/contact">Website</a>
                  <a href="/report/kriptomessi">Report</a>
                  <a href="https://linktr.ee/privacy">Privacy</a>
                </body></html>
            "#,
            source_url,
        );

        assert_eq!(extraction.source_url, source_url);
        assert_eq!(
            urls(&extraction),
            [
                "https://twitter.com/Kriptomessi",
                "https://x.com/Kriptomessi",
                "https://www.youtube.com/@Kriptomessi",
                "https://t.me/kriptomessitg",
                "https://www.instagram.com/kriptomessi/",
                "https://kriptomessi.com/contact",
            ],
            "one page reports each destination once, as found; no classification here"
        );
        assert_eq!(emails(&extraction), ["contact@kriptomessi.com"]);
        assert_eq!(
            extraction.links[0],
            LinkHubLink {
                layer: LinkHubLayer::Anchor,
                url: "https://twitter.com/Kriptomessi".into(),
                text: Some("Twitter".into()),
            }
        );
        assert!(
            extraction
                .links
                .iter()
                .all(|link| link.layer == LinkHubLayer::Anchor && !link.url.contains("linktr.ee"))
        );
    }

    #[test]
    fn recovers_structured_destinations_from_a_real_linktree_snapshot() {
        // Trimmed from the live 2026-08-19 SSR capture: top social icons are
        // href-less buttons, destinations live in JSON-LD sameAs and
        // __NEXT_DATA__, the owner email only in a FORM tile's title.
        let extraction = parse_fixture(
            include_str!("../../toolkits/fixtures/linktree_visionariobtc.html"),
            "https://linktr.ee/visionariobtc",
        );
        let urls = urls(&extraction);
        assert!(urls.contains(&"https://instagram.com/visionariobtc"));
        assert!(urls.contains(&"https://x.com/visionario_btc"));
        assert!(urls.iter().any(|url| url.contains("youtube.com/")));
        assert!(urls.iter().any(|url| url.contains("facebook.com")));
        assert!(urls.iter().any(|url| url.contains("linkedin.com")));
        assert!(urls.iter().any(|url| url.contains("calendly.com")));
        for layer in [LinkHubLayer::JsonLd, LinkHubLayer::Hydration] {
            assert!(
                extraction.links.iter().any(|link| link.layer == layer),
                "the {layer:?} layer recovers destinations anchors cannot"
            );
        }
        assert_eq!(
            emails(&extraction),
            ["vbtc.contacto@gmail.com"],
            "the owner email comes from the FORM tile title; the recommendation \
             modal must not mint phantom addresses"
        );
        assert!(
            extraction
                .links
                .iter()
                .all(|link| !link.url.contains("linktr.ee") && !link.url.contains("guy.raz"))
                && extraction
                    .emails
                    .iter()
                    .all(|email| !email.value.contains("guy.raz")),
            "internal navigation and recommended profiles are not the owner's data"
        );
    }

    #[test]
    fn recovers_the_email_social_icon_and_fediverse_links() {
        let extraction = parse_fixture(
            include_str!("../../toolkits/fixtures/linktree_bitcoinmx.html"),
            "https://linktr.ee/bitcoinmx",
        );
        let urls = urls(&extraction);
        assert!(urls.iter().any(|url| url.contains("bitcoinmx")));
        assert!(urls.contains(&"https://instagram.com/mxbitcoin"));
        assert!(urls.contains(&"https://mastodon.social/@bitcoinmx"));
        assert_eq!(emails(&extraction), ["bitcoinmx@proton.me"]);
        assert!(urls.iter().all(|url| !url.contains("linktr.ee")));
    }
}
