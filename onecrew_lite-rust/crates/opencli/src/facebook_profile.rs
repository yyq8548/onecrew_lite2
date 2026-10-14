use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacebookPublicProfile {
    pub adapter_version: String,
    pub id: String,
    pub display_name: String,
    pub follower_count: Option<u64>,
    pub follower_text: String,
    pub profile_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_profile_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contact_links: Vec<String>,
    pub evidence_source: String,
}

pub(crate) fn check_target(id: &str) -> Result<(), OpenCliError> {
    if !(1..=20).contains(&id.len())
        || id.starts_with('0')
        || !id.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(OpenCliError::InvalidInput("facebook_profile_target"));
    }
    Ok(())
}

fn exact_count(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let number = text
        .strip_suffix(" followers")
        .or_else(|| text.strip_suffix(" follower"))?;
    let groups: Vec<_> = number.split(',').collect();
    if number.is_empty()
        || (number.starts_with('0') && number != "0")
        || !groups
            .iter()
            .all(|group| !group.is_empty() && group.bytes().all(|b| b.is_ascii_digit()))
        || (groups.len() > 1
            && (!(1..=3).contains(&groups[0].len())
                || groups[1..].iter().any(|group| group.len() != 3)))
    {
        return None;
    }
    number
        .replace(',', "")
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= 9_007_199_254_740_991)
}

fn parse(output: &str, id: &str) -> Result<FacebookPublicProfile, OpenCliError> {
    if output.len() > 4000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<FacebookPublicProfile> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.remove(0);
    let source_valid = match (
        row.adapter_version.as_str(),
        row.observed_profile_url.as_deref(),
    ) {
        ("1.0.0", None) => row.evidence_source == "visible_numeric_profile_header"
            && row.contact_links.len() <= 1
            && row.contact_links.iter().all(|link| {
                link == &format!(
                    "https://www.facebook.com/profile.php?id={id}&sk=about_contact_and_basic_info"
                )
            }),
        ("2.0.0", Some(base)) => {
            crate::facebook_identity::check_source(base).is_ok()
                && !base.ends_with('/')
                && row.evidence_source == "native_profile_header_response"
                && row.contact_links.len() <= 2
                && row
                    .contact_links
                    .iter()
                    .all(|link| facebook_native_contact_profile(link) == Some(base))
                && row
                    .contact_links
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    == row.contact_links.len()
        }
        _ => false,
    };
    if !source_valid
        || row.id != id
        || row.profile_url != format!("https://www.facebook.com/profile.php?id={id}")
        || row.display_name.trim().is_empty()
        || row.display_name.chars().count() > 200
        || row.display_name.chars().any(char::is_control)
        || row.follower_text.chars().count() > 100
        || row.follower_text.chars().any(char::is_control)
        || row.follower_count != exact_count(&row.follower_text)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FacebookContact {
    adapter_version: String,
    id: String,
    source_url: String,
    #[serde(default)]
    observed_profile_url: Option<String>,
    title: String,
    contact_text: String,
    contact_links: Vec<String>,
    truncated: bool,
}

/// Route only the precise author contact tab already observed on a profile.
pub fn facebook_contact_author(url: &str) -> Option<&str> {
    let id = url
        .strip_prefix("https://www.facebook.com/profile.php?id=")?
        .strip_suffix("&sk=about_contact_and_basic_info")?;
    check_target(id).ok()?;
    Some(id)
}

/// Only the native About field routes observed on a source-bound vanity profile.
pub fn facebook_native_contact_profile(source: &str) -> Option<&str> {
    let base = source
        .strip_suffix("/directory_contact_info")
        .or_else(|| source.strip_suffix("/directory_links"))?;
    crate::facebook_identity::check_source(base).ok()?;
    (!base.ends_with('/')).then_some(base)
}

fn parse_contact(output: &str, id: &str) -> Result<crate::PublicWebRead, OpenCliError> {
    parse_contact_bound(output, id, None)
}

fn parse_contact_bound(
    output: &str,
    id: &str,
    native: Option<(&str, &str)>,
) -> Result<crate::PublicWebRead, OpenCliError> {
    if output.len() > 80_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<FacebookContact> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.remove(0);
    let source_valid = match native {
        None => {
            row.adapter_version == "1.0.0"
                && row.observed_profile_url.is_none()
                && facebook_contact_author(&row.source_url) == Some(id)
        }
        Some((base, source)) => {
            row.adapter_version == "2.0.0"
                && row.observed_profile_url.as_deref() == Some(base)
                && row.source_url == source
                && facebook_native_contact_profile(source) == Some(base)
        }
    };
    if !source_valid
        || row.id != id
        || row.title.trim().is_empty()
        || row.title.chars().count() > 200
        || row.title.chars().any(char::is_control)
        || row.contact_text.chars().count() > 12_000
        || row.contact_text.contains('\0')
        || row.contact_links.len() > 10
        || row.contact_links.iter().any(|link| {
            link.len() > 2048
                || link.chars().any(char::is_control)
                || url::Url::parse(link).map_or(true, |url| {
                    !matches!(url.scheme(), "http" | "https")
                        || url.host_str().is_none()
                        || !url.username().is_empty()
                        || url.password().is_some()
                })
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    // Keep contact links before partial page text so long multilingual content
    // cannot crowd the observed official destinations out of the bounded receipt.
    let excerpt = format!("{}\n{}", row.contact_links.join("\n"), row.contact_text);
    let mut end = excerpt.len().min(20_000);
    while !excerpt.is_char_boundary(end) {
        end -= 1;
    }
    Ok(crate::PublicWebRead {
        requested_url: row.source_url.clone(),
        document_url: row.source_url,
        title: row.title,
        markdown_excerpt: excerpt[..end].to_owned(),
        truncated: row.truncated || end < excerpt.len(),
    })
}

impl OpenCli {
    pub async fn facebook_native_profile(
        &self,
        profile: &str,
        id: &str,
        source: &str,
    ) -> Result<FacebookPublicProfile, OpenCliError> {
        validate_profile(profile)?;
        check_target(id)?;
        crate::facebook_identity::check_source(source)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-facebook-profile", "native-read")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-facebook-profile",
                "native-read",
                "-f",
                "json",
                "--",
                id,
                source,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        let row = parse(&output, id)?;
        if row.adapter_version != "2.0.0"
            || row.observed_profile_url.as_deref() != Some(source.trim_end_matches('/'))
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(row)
    }
    pub async fn facebook_native_contact_page(
        &self,
        profile: &str,
        id: &str,
        base: &str,
        source: &str,
    ) -> Result<crate::PublicWebRead, OpenCliError> {
        validate_profile(profile)?;
        check_target(id)?;
        if facebook_native_contact_profile(source) != Some(base) {
            return Err(OpenCliError::InvalidInput("facebook_native_contact_source"));
        }
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-facebook-profile", "native-contact")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-facebook-profile",
                "native-contact",
                "-f",
                "json",
                "--",
                id,
                base,
                source,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        parse_contact_bound(&output, id, Some((base, source)))
    }
    pub async fn facebook_contact_page(
        &self,
        profile: &str,
        author_id: &str,
    ) -> Result<crate::PublicWebRead, OpenCliError> {
        validate_profile(profile)?;
        check_target(author_id)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-facebook-profile", "contact")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-facebook-profile",
                "contact",
                "-f",
                "json",
                "--",
                author_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        parse_contact(&output, author_id)
    }

    pub async fn facebook_public_profile(
        &self,
        profile: &str,
        author_id: &str,
    ) -> Result<FacebookPublicProfile, OpenCliError> {
        validate_profile(profile)?;
        check_target(author_id)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-facebook-profile", "read")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-facebook-profile",
                "read",
                "-f",
                "json",
                "--",
                author_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        let row = parse(&output, author_id)?;
        if row.adapter_version != "1.0.0" {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires selected connected profile; one saved Facebook source, no search"]
    async fn facebook_native_about_contact_acceptance() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser profile");
        let bridge = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            bridge.repair_bundled_addons(true).await.unwrap();
        }
        // Registry must be inspected after repair; the previous registry predates
        // native-read/native-contact and must not falsely reject those commands.
        let bridge = OpenCli::inspect_system().await.unwrap();
        let base = "https://www.facebook.com/LinusTech";
        let id = "100044572203205";
        let start = std::time::Instant::now();
        let row = bridge
            .facebook_native_profile(&profile, id, base)
            .await
            .unwrap();
        println!(
            "Facebook native profile elapsed_ms={}",
            start.elapsed().as_millis()
        );
        let source = format!("{base}/directory_contact_info");
        assert!(row.contact_links.contains(&source));
        let start = std::time::Instant::now();
        let contact = bridge
            .facebook_native_contact_page(&profile, id, base, &source)
            .await
            .unwrap();
        println!(
            "Facebook native contact elapsed_ms={}",
            start.elapsed().as_millis()
        );
        assert_eq!(contact.requested_url, source);
        assert_eq!(contact.document_url, source);
        assert!(
            contact.markdown_excerpt.contains('@'),
            "native Email section not captured"
        );
        println!("Facebook source-bound native Email section PASS; value withheld");
    }
    #[test]
    fn native_profile_and_contact_require_same_observed_source_and_closed_fields() {
        let base = "https://www.facebook.com/LinusTech";
        let source = format!("{base}/directory_contact_info");
        let row = serde_json::json!({"adapter_version":"2.0.0","id":"123","display_name":"Creator","follower_count":null,"follower_text":"1.3M followers",
            "profile_url":"https://www.facebook.com/profile.php?id=123","observed_profile_url":base,"contact_links":[source],"evidence_source":"native_profile_header_response"});
        assert_eq!(
            parse(&serde_json::json!([row.clone()]).to_string(), "123")
                .unwrap()
                .observed_profile_url
                .as_deref(),
            Some(base)
        );
        let mut wrong = row.clone();
        wrong["contact_links"] =
            serde_json::json!(["https://www.facebook.com/Other/directory_links"]);
        assert!(parse(&serde_json::json!([wrong]).to_string(), "123").is_err());
        let mut wrong = row;
        wrong["viewer_id"] = serde_json::json!("private");
        assert!(parse(&serde_json::json!([wrong]).to_string(), "123").is_err());
        let mut contact = serde_json::json!({"adapter_version":"2.0.0","id":"123","source_url":source,"observed_profile_url":base,
            "title":"Creator","contact_text":"public@example.com","contact_links":[],"truncated":true});
        let output = serde_json::json!([contact.clone()]).to_string();
        assert!(parse_contact_bound(&output, "123", Some((base, &source))).is_ok());
        assert!(parse_contact(&output, "123").is_err());
        assert!(parse_contact_bound(&output, "456", Some((base, &source))).is_err());
        assert!(
            parse_contact_bound(
                &output,
                "123",
                Some((base, &format!("{base}/directory_links")))
            )
            .is_err()
        );
        contact["contact_links"] = serde_json::json!(["https://user:secret@example.com"]);
        assert!(
            parse_contact_bound(
                &serde_json::json!([contact]).to_string(),
                "123",
                Some((base, &source))
            )
            .is_err()
        );
    }
    #[test]
    fn exact_counts_never_expand_abbreviations_or_accept_malformed_groups() {
        for (text, expected) in [
            ("1.3K followers", None),
            ("1,2 followers", None),
            ("012 followers", None),
            ("followers", None),
            ("0 followers", Some(0)),
            ("12,345 followers", Some(12345)),
            ("1 follower", Some(1)),
        ] {
            assert_eq!(exact_count(text), expected, "{text}");
        }
        for id in ["", "0123", "creator", "123&sk=followers"] {
            assert!(check_target(id).is_err());
        }
    }
    #[test]
    fn profile_rejects_identity_drift_and_invented_count_precision() {
        let mut row = serde_json::json!({"adapter_version":"1.0.0","id":"123","display_name":"Observed name",
            "follower_count":null,"follower_text":"1.3K followers","profile_url":"https://www.facebook.com/profile.php?id=123",
            "evidence_source":"visible_numeric_profile_header"});
        assert!(
            parse(&serde_json::json!([row.clone()]).to_string(), "123")
                .unwrap()
                .follower_count
                .is_none()
        );
        row["follower_count"] = serde_json::json!(1300);
        assert_eq!(
            parse(&serde_json::json!([row.clone()]).to_string(), "123"),
            Err(OpenCliError::InvalidOutput)
        );
        row["follower_count"] = serde_json::Value::Null;
        assert_eq!(
            parse(&serde_json::json!([row]).to_string(), "456"),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn contact_page_binds_author_and_bounds_multilingual_text() {
        let mut row = serde_json::json!({"adapter_version":"1.0.0","id":"123", "source_url":"https://www.facebook.com/profile.php?id=123&sk=about_contact_and_basic_info",
            "title":"Creator", "contact_text":"business@example.com", "contact_links":["https://creator.example/contact"], "truncated":true});
        let result = parse_contact(&serde_json::json!([row.clone()]).to_string(), "123").unwrap();
        assert!(result.truncated);
        assert!(result.markdown_excerpt.contains("business@example.com"));
        assert!(
            result
                .markdown_excerpt
                .starts_with("https://creator.example/contact")
        );
        assert!(parse_contact(&serde_json::json!([row.clone()]).to_string(), "456").is_err());
        row["contact_text"] = serde_json::json!("語".repeat(12_000));
        let result = parse_contact(&serde_json::json!([row.clone()]).to_string(), "123").unwrap();
        assert!(result.markdown_excerpt.len() <= 20_000 && result.truncated);
        row["contact_links"] = serde_json::json!(["https://user:secret@creator.example/"]);
        assert!(parse_contact(&serde_json::json!([row.clone()]).to_string(), "123").is_err());
        row["contact_links"] = serde_json::json!([]);
        row["source_url"] =
            serde_json::json!("https://www.facebook.com/profile.php?id=123&sk=timeline");
        assert!(parse_contact(&serde_json::json!([row]).to_string(), "123").is_err());
    }

    #[test]
    fn only_exact_observed_contact_tab_routes_to_specialized_reader() {
        assert_eq!(
            facebook_contact_author(
                "https://www.facebook.com/profile.php?id=123&sk=about_contact_and_basic_info"
            ),
            Some("123")
        );
        for url in [
            "https://www.facebook.com/profile.php?id=123",
            "https://www.facebook.com/profile.php?id=0123&sk=about_contact_and_basic_info",
            "https://www.facebook.com/profile.php?id=123&sk=about_contact_and_basic_info#x",
            "https://evil.example/profile.php?id=123&sk=about_contact_and_basic_info",
        ] {
            assert_eq!(facebook_contact_author(url), None);
        }
    }

    #[tokio::test]
    #[ignore = "requires exact installed source add-on and explicitly selected connected profile"]
    async fn facebook_profile_contract_matches_observed_numeric_author() {
        let profile =
            std::env::var("OPENCLI_TEST_PROFILE").expect("explicit acceptance browser profile");
        let bridge = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            bridge.repair_bundled_addons(true).await.unwrap();
        }
        let row = bridge
            .creator_profile_evidence(
                &profile,
                "facebook",
                "61590270912364",
                "https://www.facebook.com/profile.php?id=61590270912364",
            )
            .await
            .unwrap();
        assert_eq!(row["id"], "61590270912364");
        assert_eq!(row["evidence_source"], "visible_numeric_profile_header");
        assert!(
            row["display_name"]
                .as_str()
                .is_some_and(|name| !name.is_empty())
        );
        println!(
            "Facebook observed numeric profile binding PASS; exact follower count present={}",
            row["follower_count"].is_number()
        );
        if std::env::var("OPENCLI_TEST_PROFILE_CONTACT").as_deref() == Ok("true") {
            let links = row["contact_links"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if let Some(url) = links.first().and_then(serde_json::Value::as_str) {
                assert_eq!(facebook_contact_author(url), Some("61590270912364"));
                let page = bridge
                    .facebook_contact_page(&profile, "61590270912364")
                    .await
                    .unwrap();
                assert_eq!(page.requested_url, url);
                assert_eq!(page.document_url, url);
                println!(
                    "Facebook observed contact tab PASS; text bytes={}",
                    page.markdown_excerpt.len()
                );
            } else {
                println!("Facebook contact tab not observed; no contact request made");
            }
        }
    }
}
