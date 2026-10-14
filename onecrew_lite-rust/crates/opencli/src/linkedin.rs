//! Public profile identity bound to an observed /in/ locator. The compose URI
//! is read as native identity evidence; no message or editor is ever opened.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LinkedinPublicProfile {
    pub adapter_version: String,
    pub author_handle: String,
    pub account_id: String,
    pub author_label: String,
    pub profile_url: String,
    pub follower_count: Option<u64>,
    pub follower_text: String,
    pub headline: String,
    pub bio: String,
    pub public_links: Vec<String>,
    pub contact_links: Vec<String>,
    pub evidence_source: String,
    pub bio_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LinkedinContent {
    pub account_id: String,
    pub author_handle: String,
    pub profile_url: String,
    pub post_id: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
    pub evidence_source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkedinContact {
    adapter_version: String,
    author_handle: String,
    account_id: String,
    source_url: String,
    title: String,
    contact_text: String,
    contact_links: Vec<String>,
    truncated: bool,
    evidence_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LinkedinComment {
    pub id: String,
    pub parent_post_id: String,
    pub text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkedinComments {
    author_handle: String,
    account_id: String,
    source_url: String,
    comments: Vec<LinkedinComment>,
    evidence_source: String,
}

fn numeric_id(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}

fn activity_id(source: &str) -> Option<&str> {
    source
        .strip_prefix("https://www.linkedin.com/feed/update/urn:li:activity:")?
        .strip_suffix('/')
        .filter(|id| numeric_id(id))
}

fn parse_comments(
    output: &str,
    handle: &str,
    account: &str,
    source: &str,
) -> Result<Vec<LinkedinComment>, OpenCliError> {
    let post = activity_id(source).ok_or(OpenCliError::InvalidInput("linkedin_source"))?;
    if !valid_handle(handle) || !valid_account(account) {
        return Err(OpenCliError::InvalidInput("linkedin_identity"));
    }
    if output.len() > 16_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<LinkedinComments> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.remove(0);
    let mut seen = std::collections::HashSet::new();
    if row.author_handle != handle
        || row.account_id != account
        || row.source_url != source
        || row.evidence_source != "visible_same_post_comments"
        || row.comments.len() > 5
        || row.comments.iter().any(|c| {
            !numeric_id(&c.id)
                || c.parent_post_id != post
                || !seen.insert(&c.id)
                || c.text.len() > 2000
                || c.text.contains('\0')
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row.comments)
}

/// Recognize only the observed profile's exact public contact overlay.
pub fn linkedin_contact_handle(source: &str) -> Option<&str> {
    let handle = source
        .strip_prefix("https://www.linkedin.com/in/")?
        .strip_suffix("/overlay/contact-info/")?;
    valid_handle(handle).then_some(handle)
}

fn parse_contact(
    output: &str,
    handle: &str,
    account: &str,
) -> Result<crate::PublicWebRead, OpenCliError> {
    if output.len() > 32_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<LinkedinContact> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.remove(0);
    if !valid_handle(handle)
        || !valid_account(account)
        || row.adapter_version != "1.0.0"
        || row.author_handle != handle
        || row.account_id != account
        || linkedin_contact_handle(&row.source_url) != Some(handle)
        || row.title.trim().is_empty()
        || row.title.len() > 512
        || row.title.chars().count() > 200
        || row.title.chars().any(char::is_control)
        || row.contact_text.len() > 8000
        || row.contact_text.contains('\0')
        || row.contact_links.len() > 10
        || row.contact_links.iter().any(|link| {
            !link.starts_with("https://") || crate::validate_public_web_url(link).is_err()
        })
        || !row.truncated
        || row.evidence_source != "visible_contact_dialog"
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let excerpt = format!("{}\n{}", row.contact_links.join("\n"), row.contact_text);
    if excerpt.len() > 20_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    Ok(crate::PublicWebRead {
        requested_url: row.source_url.clone(),
        document_url: row.source_url,
        title: row.title,
        markdown_excerpt: excerpt,
        truncated: true,
    })
}

pub(crate) fn valid_handle(handle: &str) -> bool {
    (1..=128).contains(&handle.len())
        && handle.as_bytes()[0].is_ascii_alphanumeric()
        && handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        && !matches!(handle.to_ascii_lowercase().as_str(), "me" | "self")
}
pub(crate) fn valid_account(id: &str) -> bool {
    id.strip_prefix("ACo").is_some_and(|body| {
        (16..=125).contains(&body.len())
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}
fn exact_followers(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let number = text
        .strip_suffix(" followers")
        .or_else(|| text.strip_suffix(" follower"))?;
    let groups: Vec<_> = number.split(',').collect();
    if groups
        .iter()
        .any(|group| group.is_empty() || !group.bytes().all(|byte| byte.is_ascii_digit()))
        || (groups.len() > 1
            && (groups[0].len() > 3 || groups.iter().skip(1).any(|group| group.len() != 3)))
        || (number.starts_with('0') && number != "0")
    {
        return None;
    }
    number
        .replace(',', "")
        .parse::<u64>()
        .ok()
        .filter(|value| *value <= 9_007_199_254_740_991)
}
fn parse_profile(output: &str, handle: &str) -> Result<LinkedinPublicProfile, OpenCliError> {
    if !valid_handle(handle) {
        return Err(OpenCliError::InvalidInput("linkedin_handle"));
    }
    if output.len() > 32_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<LinkedinPublicProfile> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.remove(0);
    let source = format!("https://www.linkedin.com/in/{handle}/");
    if row.adapter_version != "1.0.0"
        || row.author_handle != handle
        || !valid_account(&row.account_id)
        || row.profile_url != source
        || row.author_label.trim().is_empty()
        || row.author_label.len() > 512
        || row.author_label.chars().any(char::is_control)
        || row.headline.len() > 2000
        || row.headline.contains('\0')
        || row.bio.len() > 8000
        || row.bio.contains('\0')
        || row.follower_text.len() > 100
        || row.follower_text.contains('\0')
        || row.follower_count != exact_followers(&row.follower_text)
        || row
            .follower_count
            .is_some_and(|count| count > 9_007_199_254_740_991)
        || row.public_links.len() > 10
        || row.public_links.iter().any(|link| {
            !link.starts_with("https://") || crate::validate_public_web_url(link).is_err()
        })
        || row.contact_links.len() > 1
        || row
            .contact_links
            .iter()
            .any(|link| link != &format!("{source}overlay/contact-info/"))
        || row.evidence_source != "visible_profile_header"
        || !row.bio_truncated
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

fn parse_content(
    output: &str,
    handle: &str,
    account: &str,
) -> Result<LinkedinContent, OpenCliError> {
    if !valid_handle(handle) || !valid_account(account) {
        return Err(OpenCliError::InvalidInput("linkedin_identity"));
    }
    if output.len() > 24_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<LinkedinContent> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.remove(0);
    if row.account_id != account
        || row.author_handle != handle
        || row.profile_url != format!("https://www.linkedin.com/in/{handle}/")
        || !(1..=20).contains(&row.post_id.len())
        || row.post_id.starts_with('0')
        || !row.post_id.bytes().all(|byte| byte.is_ascii_digit())
        || row.source_url
            != format!(
                "https://www.linkedin.com/feed/update/urn:li:activity:{}/",
                row.post_id
            )
        || row.text.trim().is_empty()
        || row.text.len() > 16_000
        || row.text.contains('\0')
        || !row.text_truncated
        || row.evidence_source != "visible_author_activity"
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

impl OpenCli {
    pub async fn linkedin_contact_page(
        &self,
        profile: &str,
        handle: &str,
        account: &str,
    ) -> Result<crate::PublicWebRead, OpenCliError> {
        if !valid_account(account) {
            return Err(OpenCliError::InvalidInput("linkedin_account"));
        }
        let output = self
            .linkedin_evidence(profile, "contact", handle, Some(account), None)
            .await?;
        parse_contact(&output, handle, account)
    }
    pub async fn linkedin_public_profile(
        &self,
        profile: &str,
        handle: &str,
    ) -> Result<LinkedinPublicProfile, OpenCliError> {
        let output = self
            .linkedin_evidence(profile, "profile", handle, None, None)
            .await?;
        parse_profile(&output, handle)
    }

    pub async fn linkedin_original_content(
        &self,
        profile: &str,
        handle: &str,
        account: &str,
    ) -> Result<LinkedinContent, OpenCliError> {
        if !valid_account(account) {
            return Err(OpenCliError::InvalidInput("linkedin_account"));
        }
        let output = self
            .linkedin_evidence(profile, "content", handle, Some(account), None)
            .await?;
        parse_content(&output, handle, account)
    }

    pub async fn linkedin_original_comments(
        &self,
        profile: &str,
        handle: &str,
        account: &str,
        source: &str,
    ) -> Result<Vec<LinkedinComment>, OpenCliError> {
        if !valid_account(account) || activity_id(source).is_none() {
            return Err(OpenCliError::InvalidInput("linkedin_identity"));
        }
        let output = self
            .linkedin_evidence(profile, "comments", handle, Some(account), Some(source))
            .await?;
        parse_comments(&output, handle, account, source)
    }

    async fn linkedin_evidence(
        &self,
        profile: &str,
        kind: &str,
        handle: &str,
        account: Option<&str>,
        source: Option<&str>,
    ) -> Result<String, OpenCliError> {
        if !valid_handle(handle) {
            return Err(OpenCliError::InvalidInput("linkedin_handle"));
        }
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-linkedin-reader", kind)
            .await?;
        let mut args = vec![
            "--profile",
            profile,
            "onecrew-linkedin-reader",
            kind,
            "-f",
            "json",
            "--",
            handle,
        ];
        if let Some(account) = account {
            args.push(account);
        }
        if let Some(source) = source {
            args.push(source);
        }
        let output = crate::process::run_environment(
            &self.executable,
            self.script.as_deref(),
            &args,
            READ_TIMEOUT,
            // LinkedIn omits profile sections in a hidden Browser Bridge window.
            &[("OPENCLI_WINDOW".into(), "foreground".into())],
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        Ok(output.stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn fixture() -> Value {
        json!({"adapter_version":"1.0.0", "author_handle":"creator", "account_id":"ACoAAA123456789abcdefghijk",
            "author_label":"Creator", "profile_url":"https://www.linkedin.com/in/creator/",
            "follower_count":1606, "follower_text":"1,606 followers", "headline":"Keyboard reviews",
            "bio":"Business studio@example.org", "public_links":["https://creator.example/contact"],
            "contact_links":["https://www.linkedin.com/in/creator/overlay/contact-info/"],
            "evidence_source":"visible_profile_header", "bio_truncated":true})
    }

    #[test]
    fn comments_bind_first_five_native_parent_ids_and_reject_unrelated_or_private_rows() {
        let account = "ACoAAA123456789abcdefghijk";
        let source = "https://www.linkedin.com/feed/update/urn:li:activity:7480296707809804288/";
        let fixture = json!({"author_handle":"creator", "account_id":account, "source_url":source,
            "comments":[{"id":"7480303860733911040","parent_post_id":"7480296707809804288","text":""}],
            "evidence_source":"visible_same_post_comments"});
        assert_eq!(
            parse_comments(
                &json!([fixture.clone()]).to_string(),
                "creator",
                account,
                source
            )
            .unwrap()[0]
                .text,
            ""
        );
        for field in [
            "account_id",
            "author_handle",
            "source_url",
            "evidence_source",
            "session",
        ] {
            let mut wrong = fixture.clone();
            wrong[field] = json!("unrelated");
            assert!(
                parse_comments(&json!([wrong]).to_string(), "creator", account, source).is_err()
            );
        }
        for (field, bad) in [
            ("parent_post_id", json!("7481027555072962560")),
            ("id", json!("07480303860733911040")),
            ("text", json!("界".repeat(667))),
            ("session", json!("private")),
        ] {
            let mut wrong = fixture.clone();
            wrong["comments"][0][field] = bad;
            assert!(
                parse_comments(&json!([wrong]).to_string(), "creator", account, source).is_err()
            );
        }
        for count in [2, 6] {
            let mut wrong = fixture.clone();
            wrong["comments"] = json!(vec![fixture["comments"][0].clone(); count]);
            assert!(
                parse_comments(&json!([wrong]).to_string(), "creator", account, source).is_err()
            );
        }
    }

    #[test]
    fn contact_overlay_requires_exact_locator_native_account_and_bounded_public_fields() {
        let account = "ACoAAA123456789abcdefghijk";
        let source = "https://www.linkedin.com/in/creator/overlay/contact-info/";
        let fixture = json!({"adapter_version":"1.0.0", "author_handle":"creator", "account_id":account,
            "source_url":source, "title":"Creator", "contact_text":"studio@example.org",
            "contact_links":["https://t.me/creator_business"], "truncated":true, "evidence_source":"visible_contact_dialog"});
        let row = parse_contact(&json!([fixture.clone()]).to_string(), "creator", account).unwrap();
        assert_eq!(row.document_url, source);
        assert!(row.markdown_excerpt.contains("studio@example.org"));
        assert!(
            row.markdown_excerpt
                .contains("https://t.me/creator_business")
        );
        for (field, wrong) in [
            ("account_id", json!("ACoAAA987654321otheraccount")),
            ("author_handle", json!("other")),
            ("source_url", json!("https://www.linkedin.com/in/creator/")),
            ("title", json!("a".repeat(201))),
            ("contact_text", json!("界".repeat(2667))),
            (
                "contact_links",
                json!(["https://user:secret@creator.example/"]),
            ),
            ("truncated", json!(false)),
            ("evidence_source", json!("search_preview")),
            ("session", json!("private")),
        ] {
            let mut value = fixture.clone();
            value[field] = wrong;
            assert!(
                parse_contact(&json!([value]).to_string(), "creator", account).is_err(),
                "{field}"
            );
        }
        assert!(parse_contact("[]", "creator", account).is_err());
        assert!(
            parse_contact(
                &json!([fixture.clone(), fixture]).to_string(),
                "creator",
                account
            )
            .is_err()
        );
        for wrong in [
            "https://www.linkedin.com/in/other/overlay/contact-info/?next=creator",
            "https://www.linkedin.com/in/me/overlay/contact-info/",
            "https://www.linkedin.com/company/creator/overlay/contact-info/",
        ] {
            assert!(linkedin_contact_handle(wrong).is_none());
        }
    }
    #[test]
    fn observed_profile_binds_native_id_and_distinguishes_unknown_from_zero() {
        for (count, text) in [
            (json!(1606), "1,606 followers"),
            (json!(0), "0 followers"),
            (Value::Null, "1.6K followers"),
        ] {
            let mut value = fixture();
            value["follower_count"] = count.clone();
            value["follower_text"] = json!(text);
            let row = parse_profile(&json!([value]).to_string(), "creator").unwrap();
            assert_eq!(serde_json::to_value(row.follower_count).unwrap(), count);
            assert!(valid_account(&row.account_id));
        }
    }
    #[test]
    fn changed_identity_private_fields_and_unauthorized_contact_sources_are_rejected() {
        for (key, wrong) in [
            ("author_handle", json!("other")),
            ("account_id", json!("creator")),
            ("follower_count", json!(0)),
            ("follower_text", json!("16,06 followers")),
            ("profile_url", json!("https://www.linkedin.com/in/other/")),
            (
                "public_links",
                json!(["https://user:secret@creator.example/"]),
            ),
            (
                "contact_links",
                json!(["https://www.linkedin.com/in/other/overlay/contact-info/"]),
            ),
            ("bio_truncated", json!(false)),
            ("evidence_source", json!("search_preview")),
            ("session", json!("private")),
        ] {
            let mut value = fixture();
            value[key] = wrong;
            assert!(
                parse_profile(&json!([value]).to_string(), "creator").is_err(),
                "{key}"
            );
        }
        for handle in ["me", "self", "a/b", ""] {
            assert!(!valid_handle(handle));
        }
        assert!(parse_profile("[]", "creator").is_err());
        assert!(parse_profile(&json!([fixture(), fixture()]).to_string(), "creator").is_err());
    }

    #[tokio::test]
    #[ignore = "one user-provided Ashley locator; public native identity and About email only, no search or recommendation"]
    async fn ashley_public_about_email_is_bound_to_native_profile() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit connected identity");
        let bridge = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            bridge.repair_bundled_addons(true).await.unwrap();
        }
        let started = std::time::Instant::now();
        let row = bridge
            .linkedin_public_profile(&profile, "ashleymarsh-conexo")
            .await
            .expect("user-provided profile must return native evidence");
        assert_eq!(row.author_handle, "ashleymarsh-conexo");
        assert_eq!(
            row.profile_url,
            "https://www.linkedin.com/in/ashleymarsh-conexo/"
        );
        assert!(valid_account(&row.account_id));
        println!(
            "Ashley native profile; elapsed_ms={}; evidence={}",
            started.elapsed().as_millis(),
            serde_json::to_string(&row).unwrap()
        );
        assert!(
            row.bio.contains("am@conexo-tech.co"),
            "user screenshot email must be present in this author's native About evidence"
        );
    }

    #[tokio::test]
    #[ignore = "requires exact installed source add-on and explicitly selected connected profile"]
    async fn linkedin_contract_matches_observed_author_and_contact_entry() {
        let profile =
            std::env::var("OPENCLI_TEST_PROFILE").expect("explicit acceptance browser profile");
        let bridge = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            bridge.repair_bundled_addons(true).await.unwrap();
        }
        let handle = "rosalienewcombe";
        let started = std::time::Instant::now();
        let row = bridge
            .linkedin_public_profile(&profile, handle)
            .await
            .expect("observed native profile");
        println!(
            "LinkedIn profile PASS; elapsed_ms={}; exact_follower_count={}; contact_entries={}",
            started.elapsed().as_millis(),
            row.follower_count.is_some(),
            row.contact_links.len()
        );
        assert_eq!(row.author_handle, handle);
        assert!(valid_account(&row.account_id));
        if std::env::var("OPENCLI_TEST_PROFILE_CONTENT").as_deref() == Ok("true") {
            let started = std::time::Instant::now();
            let content = bridge
                .linkedin_original_content(&profile, handle, &row.account_id)
                .await
                .expect("original Activity bound to actually observed account");
            assert_eq!(content.account_id, row.account_id);
            assert_eq!(content.profile_url, row.profile_url);
            println!(
                "LinkedIn original content PASS; elapsed_ms={}; original_bytes={}",
                started.elapsed().as_millis(),
                content.text.len()
            );
        }
        let source = format!("https://www.linkedin.com/in/{handle}/overlay/contact-info/");
        if std::env::var("OPENCLI_TEST_PROFILE_CONTACT").as_deref() == Ok("true") {
            if row.contact_links.contains(&source) {
                let started = std::time::Instant::now();
                let contact = bridge
                    .linkedin_contact_page(&profile, handle, &row.account_id)
                    .await
                    .expect("observed native contact entry");
                assert_eq!(contact.requested_url, source);
                assert_eq!(contact.document_url, source);
                println!(
                    "LinkedIn contact entry PASS; elapsed_ms={}; contact_text_bytes={}",
                    started.elapsed().as_millis(),
                    contact.markdown_excerpt.len()
                );
            } else {
                println!("LinkedIn contact entry not observed; no contact request made");
            }
        }
    }

    #[test]
    fn original_activity_requires_native_account_profile_locator_and_actual_post_source() {
        let account = "ACoAAA123456789abcdefghijk";
        let fixture = json!({"account_id": account, "author_handle": "creator", "profile_url": "https://www.linkedin.com/in/creator/",
            "post_id": "7432844064649293824", "source_url": "https://www.linkedin.com/feed/update/urn:li:activity:7432844064649293824/",
            "text": "Keyboard review. Contact studio@example.org", "text_truncated": true, "evidence_source": "visible_author_activity"});
        let row = parse_content(&json!([fixture.clone()]).to_string(), "creator", account).unwrap();
        assert_eq!(row.account_id, account);
        assert!(row.text.contains("studio@example.org"));
        for (key, wrong) in [
            ("account_id", json!("ACoAAA987654321otheraccount")),
            ("author_handle", json!("other")),
            ("profile_url", json!("https://www.linkedin.com/in/other/")),
            ("post_id", json!("07432844064649293824")),
            ("source_url", json!("https://www.linkedin.com/in/creator/")),
            ("text", json!("界".repeat(5334))),
            ("text", json!("bad\0body")),
            ("text_truncated", json!(false)),
            ("evidence_source", json!("search_preview")),
            ("session", json!("private")),
        ] {
            let mut value = fixture.clone();
            value[key] = wrong;
            assert!(
                parse_content(&json!([value]).to_string(), "creator", account).is_err(),
                "{key}"
            );
        }
        assert!(parse_content("[]", "creator", account).is_err());
        assert!(
            parse_content(
                &json!([fixture.clone(), fixture]).to_string(),
                "creator",
                account
            )
            .is_err()
        );
        assert!(parse_content("[]", "creator", "creator").is_err());
    }
}
