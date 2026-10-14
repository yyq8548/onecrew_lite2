//! Fixed OpenCLI `web read` output contract; provider text is untrusted evidence.

use std::net::IpAddr;

use serde::Serialize;
use url::{Host, Url};

use crate::OpenCliError;

const MAX_EXCERPT_BYTES: usize = 20_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PublicWebRead {
    pub requested_url: String,
    pub document_url: String,
    pub title: String,
    pub markdown_excerpt: String,
    pub truncated: bool,
}

/// Reject non-public locators and visibly truncated links, never reconstruct them.
pub fn validate_public_web_url(value: &str) -> Result<(), OpenCliError> {
    let invalid = || OpenCliError::InvalidInput("public_url");
    if value.len() > 2048
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
        || value.contains("...")
        || value.contains('…')
    {
        return Err(invalid());
    }
    let url = Url::parse(value).map_err(|_| invalid())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query_pairs().any(|(key, _)| sensitive_parameter(&key))
        || url.fragment().is_some_and(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes())
                .any(|(key, _)| sensitive_parameter(&key))
        })
    {
        return Err(invalid());
    }
    let Some(Host::Domain(host)) = url.host() else {
        return Err(invalid());
    };
    if !host.contains('.')
        || host.ends_with('.')
        || [
            "localhost",
            "local",
            "internal",
            "invalid",
            "test",
            "onion",
            "home.arpa",
        ]
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
    {
        return Err(invalid());
    }
    Ok(())
}

fn sensitive_parameter(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "access_token" | "refresh_token" | "id_token" | "api_key" | "password" | "authorization"
    )
}

pub(super) async fn check_public_destination(value: &str) -> Result<(), OpenCliError> {
    validate_public_web_url(value)?;
    let url = Url::parse(value).map_err(|_| OpenCliError::InvalidInput("public_url"))?;
    let host = url
        .host_str()
        .ok_or(OpenCliError::InvalidInput("public_url"))?;
    let addresses = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::lookup_host((host, 443)),
    )
    .await
    .map_err(|_| OpenCliError::Timeout)?
    .map_err(|_| OpenCliError::InvalidInput("public_url"))?;
    let mut found = false;
    for address in addresses {
        found = true;
        if !public_address(address.ip()) {
            return Err(OpenCliError::InvalidInput("public_url"));
        }
    }
    if !found {
        return Err(OpenCliError::InvalidInput("public_url"));
    }
    Ok(())
}

fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_documentation()
                && !ip.is_broadcast()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 198 && (18..=19).contains(&b))
                && !(a == 192 && b == 0)
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            // Admit native global unicast only. Mapped IPv4 and translation
            // ranges must not turn a private endpoint into an apparent public one.
            segments[0] & 0xe000 == 0x2000
                && !(segments[0] == 0x2001 && segments[1] < 0x200)
                && !(segments[0] == 0x2001 && segments[1] == 0xdb8)
                && segments[0] != 0x2002
                && !(segments[0] == 0x3fff && segments[1] < 0x1000)
        }
    }
}

pub(super) fn parse(
    requested: &str,
    stdout: &str,
    stderr: &[u8],
) -> Result<PublicWebRead, OpenCliError> {
    validate_public_web_url(requested)?;
    let diagnostic = std::str::from_utf8(stderr).map_err(|_| OpenCliError::InvalidOutput)?;
    let mut blocks = diagnostic.split("[web-read diagnose]\n").skip(1);
    let block = blocks.next().ok_or(OpenCliError::InvalidOutput)?;
    if blocks.next().is_some() {
        return Err(OpenCliError::InvalidOutput);
    }
    let destination = block
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("url: "))
        .ok_or(OpenCliError::InvalidOutput)?;
    validate_public_web_url(destination).map_err(|_| OpenCliError::InvalidOutput)?;
    let normalized = stdout.replace("\r\n", "\n");
    let (header, body) = normalized
        .split_once("\n\n---\n\n")
        .ok_or(OpenCliError::InvalidOutput)?;
    let title = header
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .filter(|title| !title.trim().is_empty() && title.chars().count() <= 500)
        .ok_or(OpenCliError::InvalidOutput)?;
    let sources: Vec<_> = header
        .lines()
        .filter_map(|line| line.strip_prefix("> 原文链接: "))
        .collect();
    if sources != [requested] || body.trim().is_empty() || body.contains('\0') {
        return Err(OpenCliError::InvalidOutput);
    }
    let gate_title = title
        .trim()
        .trim_end_matches(['.', '…', '!'])
        .to_lowercase();
    let gate_body = body.to_lowercase();
    if [
        "just a moment",
        "verify you are human",
        "checking your browser",
        "security verification",
        "安全验证",
        "安全驗證",
        "人机验证",
        "人機驗證",
        "un momento",
        "un instant",
        "einen moment",
        "um momento",
        "подождите",
        "しばらくお待ちください",
        "잠시만 기다려 주세요",
    ]
    .contains(&gate_title.as_str())
        || (body.len() < 3000
            && [
                "performing security verification",
                "enable javascript and cookies to continue",
                "checking your browser before",
                "请完成人机验证",
                "請完成人機驗證",
            ]
            .iter()
            .any(|phrase| gate_body.contains(phrase)))
    {
        return Err(OpenCliError::RiskControl);
    }
    let mut end = body.len().min(MAX_EXCERPT_BYTES);
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    Ok(PublicWebRead {
        requested_url: requested.into(),
        document_url: destination.into(),
        title: title.into(),
        markdown_excerpt: body[..end].into(),
        truncated: end < body.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_pages_are_risk_control_not_successful_contact_reads() {
        let diagnostic = b"[web-read diagnose]\nurl: https://example.com/\n";
        for (title, text) in [
            (
                "Just a moment...",
                "Checking your browser before proceeding",
            ),
            ("Verify you are human", "Cloudflare Ray ID"),
            ("安全验证", "请完成人机验证"),
            ("Un momento...", "Cloudflare Ray ID"),
        ] {
            let output = format!("# {title}\n> 原文链接: https://example.com/\n\n---\n\n{text}");
            assert!(matches!(
                parse("https://example.com/", &output, diagnostic),
                Err(OpenCliError::RiskControl)
            ));
        }
        let ordinary = "# CAPTCHA article\n> 原文链接: https://example.com/\n\n---\n\nAn article about captcha with business contact friends@example.com";
        assert!(parse("https://example.com/", ordinary, diagnostic).is_ok());
    }

    #[test]
    fn rejects_private_credentialed_or_truncated_locators() {
        for url in [
            "http://example.com/",
            "https://localhost/",
            "https://x.local/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://2130706433/",
            "https://user@example.com/",
            "https://example.com:444/",
            "https://example.com/path...",
            "https://example.com/…",
            "https://example.com/#access_token=synthetic",
            "https://example.com/?api_key=synthetic",
            "https://example.com/a\nb",
            "--profile=other",
        ] {
            assert!(validate_public_web_url(url).is_err(), "{url}");
        }
        assert!(validate_public_web_url("https://example.com/tool?ref=creator").is_ok());
        assert!(validate_public_web_url("https://example.com/tool#how-it-works").is_ok());
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "2001:db8::1",
            "2002:7f00:1::",
            "3fff::1",
        ] {
            assert!(!public_address(address.parse().unwrap()), "{address}");
        }
        assert!(public_address("93.184.215.14".parse().unwrap()));
        assert!(public_address("2606:4700::1111".parse().unwrap()));
    }

    #[test]
    fn observed_destination_is_not_inferred_from_input_frontmatter() {
        let body = "# Tool\n> 原文链接: https://example.com/\n\n---\n\nUse this AI tool.";
        let diagnostic = b"[web-read diagnose]\nurl: https://www.example.com/tool\nframes: 0\n";
        let result = parse("https://example.com/", body, diagnostic).unwrap();
        assert_eq!(result.document_url, "https://www.example.com/tool");
        assert_eq!(result.markdown_excerpt, "Use this AI tool.");
        assert!(!result.truncated);
        assert!(parse("https://example.com/", body, b"").is_err());
        assert!(
            parse(
                "https://example.com/",
                body,
                b"[web-read diagnose]\nurl: https://127.0.0.1/\n"
            )
            .is_err()
        );
        assert!(parse("https://other.example.com/", body, diagnostic).is_err());
        assert!(parse("https://example.com/", "", diagnostic).is_err());
        let duplicate = [diagnostic.as_slice(), diagnostic.as_slice()].concat();
        assert!(parse("https://example.com/", body, &duplicate).is_err());
    }

    #[test]
    fn excerpts_preserve_utf8_and_explicit_truncation() {
        let text = "人工智能工具".repeat(2000);
        let body = format!("# Tool\n> 原文链接: https://example.com/\n\n---\n\n{text}");
        let result = parse(
            "https://example.com/",
            &body,
            b"[web-read diagnose]\nurl: https://example.com/\n",
        )
        .unwrap();
        assert!(result.truncated);
        assert!(result.markdown_excerpt.len() <= MAX_EXCERPT_BYTES);
        assert!(text.starts_with(&result.markdown_excerpt));
    }
}
