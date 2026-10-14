//! Exact native Pinterest account receipts, separate from saved-pin ownership.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PinterestPublicProfile {
    pub author_id: String,
    pub username: String,
    pub author_label: String,
    pub profile_url: String,
    pub follower_count: Option<u64>,
    pub bio: String,
    pub public_links: Vec<String>,
}

/// Caption returned by a native Pin whose pinner matches the saved account.
/// This partial public text does not prove authorship of the image itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PinterestContent {
    pub pin_id: String,
    pub author_id: String,
    pub author_username: String,
    pub profile_url: String,
    pub source_url: String,
    pub text: String,
    pub text_truncated: bool,
}
fn parse_content(output: &str, id: &str, username: &str) -> Result<PinterestContent, OpenCliError> {
    if output.len() > 24000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let values: Vec<PinterestContent> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if values.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = values.into_iter().next().expect("one row");
    if !valid_id(&row.pin_id)
        || !valid_id(id)
        || row.author_id != id
        || !valid_username(username)
        || row.author_username != username
        || row.profile_url != format!("https://www.pinterest.com/{username}/")
        || row.source_url != format!("https://www.pinterest.com/pin/{}/", row.pin_id)
        || row.text.trim().is_empty()
        || row.text.len() > 16000
        || row.text.contains('\0')
        || !row.text_truncated
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

pub(crate) fn valid_id(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}
pub(crate) fn valid_username(username: &str) -> bool {
    (1..=30).contains(&username.len())
        && username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
pub(crate) fn parse_search(
    value: &serde_json::Value,
) -> Result<crate::CreatorSearchHit, OpenCliError> {
    if value["provider"] == "onecrew-pinterest-pin-search-v1" {
        return parse_pin_search(value);
    }
    let field = |name| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    };
    let expected = format!("https://www.pinterest.com/{}/", field("username"));
    let keys = [
        "author_id",
        "username",
        "author_label",
        "author_url",
        "source_url",
        "title",
    ];
    if !value
        .as_object()
        .is_some_and(|row| row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key)))
        || !valid_id(field("author_id"))
        || !valid_username(field("username"))
        || field("author_url") != expected
        || field("source_url") != expected
        || field("author_label").trim().is_empty()
        || field("author_label").len() > 512
        || field("author_label").chars().any(char::is_control)
        || field("title") != field("author_label")
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(crate::CreatorSearchHit {
        source_url: expected.clone(),
        title: field("title").into(),
        author_label: field("author_label").into(),
        author_id: Some(field("author_id").into()),
        author_url: Some(expected),
    })
}

fn parse_pin_search(value: &serde_json::Value) -> Result<crate::CreatorSearchHit, OpenCliError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PinHit {
        provider: String,
        pin_id: String,
        author_id: String,
        username: String,
        author_label: String,
        author_url: String,
        source_url: String,
        title: String,
    }
    let row: PinHit =
        serde_json::from_value(value.clone()).map_err(|_| OpenCliError::InvalidOutput)?;
    if row.provider != "onecrew-pinterest-pin-search-v1"
        || !valid_id(&row.pin_id)
        || !valid_id(&row.author_id)
        || !valid_username(&row.username)
        || row.author_url != format!("https://www.pinterest.com/{}/", row.username)
        || row.source_url != format!("https://www.pinterest.com/pin/{}/", row.pin_id)
        || row.author_label.trim().is_empty()
        || row.author_label.len() > 512
        || row.author_label.chars().any(char::is_control)
        || row.title.trim().is_empty()
        || row.title.chars().count() > 500
        || row
            .title
            .chars()
            .any(|c| c.is_control() && !c.is_whitespace())
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(crate::CreatorSearchHit {
        source_url: row.source_url,
        title: row.title.split_whitespace().collect::<Vec<_>>().join(" "),
        author_label: row.author_label,
        author_id: Some(row.author_id),
        author_url: Some(row.author_url),
    })
}
fn parse_profile(
    output: &str,
    id: &str,
    username: &str,
) -> Result<PinterestPublicProfile, OpenCliError> {
    if output.len() > 20000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let values: Vec<serde_json::Value> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if values.len() != 1 || values[0].get("follower_count").is_none() {
        return Err(OpenCliError::InvalidOutput);
    }
    let row: PinterestPublicProfile =
        serde_json::from_value(values.into_iter().next().expect("one row"))
            .map_err(|_| OpenCliError::InvalidOutput)?;
    if !valid_id(&row.author_id)
        || row.author_id != id
        || !valid_username(&row.username)
        || row.username != username
        || row.profile_url != format!("https://www.pinterest.com/{username}/")
        || row.author_label.trim().is_empty()
        || row.author_label.len() > 512
        || row.author_label.chars().any(char::is_control)
        || row.bio.len() > 8000
        || row.bio.contains('\0')
        || row
            .follower_count
            .is_some_and(|count| count > 9_007_199_254_740_991)
        || row.public_links.len() > 1
        || row
            .public_links
            .iter()
            .any(|url| crate::web::validate_public_web_url(url).is_err())
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

impl OpenCli {
    pub async fn pinterest_public_content(
        &self,
        profile: &str,
        author_id: &str,
        username: &str,
    ) -> Result<PinterestContent, OpenCliError> {
        self.read_pinterest_content(profile, author_id, username, None)
            .await
    }

    pub async fn pinterest_public_pin_content(
        &self,
        profile: &str,
        author_id: &str,
        username: &str,
        pin_id: &str,
    ) -> Result<PinterestContent, OpenCliError> {
        if !valid_id(pin_id) {
            return Err(OpenCliError::InvalidInput("pinterest_pin"));
        }
        self.read_pinterest_content(profile, author_id, username, Some(pin_id))
            .await
    }

    async fn read_pinterest_content(
        &self,
        profile: &str,
        author_id: &str,
        username: &str,
        pin_id: Option<&str>,
    ) -> Result<PinterestContent, OpenCliError> {
        if !valid_id(author_id) || !valid_username(username) {
            return Err(OpenCliError::InvalidInput("pinterest_author"));
        }
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        let command = if pin_id.is_some() {
            "content-pin"
        } else {
            "content"
        };
        self.require_browser_read(profile, "onecrew-pinterest-reader", command)
            .await?;
        let mut arguments = vec![
            "--profile",
            profile,
            "onecrew-pinterest-reader",
            command,
            "-f",
            "json",
            username,
            author_id,
        ];
        if let Some(pin) = pin_id {
            arguments.push(pin);
        }
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &arguments,
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        let content = parse_content(&output, author_id, username)?;
        if pin_id.is_some_and(|pin| pin != content.pin_id) {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(content)
    }
    pub async fn pinterest_public_profile(
        &self,
        profile: &str,
        author_id: &str,
        username: &str,
    ) -> Result<PinterestPublicProfile, OpenCliError> {
        if !valid_id(author_id) || !valid_username(username) {
            return Err(OpenCliError::InvalidInput("pinterest_author"));
        }
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-pinterest-reader", "profile")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-pinterest-reader",
                "profile",
                "-f",
                "json",
                username,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        parse_profile(&output, author_id, username)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn native() -> serde_json::Value {
        json!({"author_id":"9876543210123456789","username":"native_author","author_label":"Display Name",
            "profile_url":"https://www.pinterest.com/native_author/","follower_count":null,"bio":"Business: studio@example.org","public_links":["https://studio.example.org/contact"]})
    }
    #[test]
    fn native_caption_requires_matching_account_and_observed_pin_with_partial_text() {
        let row = json!({"pin_id":"456","author_id":"123","author_username":"native_author",
            "profile_url":"https://www.pinterest.com/native_author/","source_url":"https://www.pinterest.com/pin/456/",
            "text":"Keyboard review. Business studio@example.org","text_truncated":true});
        assert_eq!(
            parse_content(&json!([row.clone()]).to_string(), "123", "native_author")
                .unwrap()
                .pin_id,
            "456"
        );
        for (key, bad) in [
            ("pin_id", json!(456)),
            ("author_id", json!("999")),
            ("author_username", json!("other")),
            ("profile_url", json!("https://www.pinterest.com/other/")),
            ("source_url", json!("https://www.pinterest.com/pin/789/")),
            ("text_truncated", json!(false)),
            ("session", json!("private")),
            ("text", json!("")),
        ] {
            let mut wrong = row.clone();
            wrong[key] = bad;
            assert!(
                parse_content(&json!([wrong]).to_string(), "123", "native_author").is_err(),
                "{key}"
            );
        }
    }
    #[test]
    fn search_requires_observed_native_id_and_same_author_profile() {
        let native = json!({"author_id":"9876543210123456789","username":"native_author","author_label":"Display Name",
            "author_url":"https://www.pinterest.com/native_author/","source_url":"https://www.pinterest.com/native_author/","title":"Display Name"});
        let hit = parse_search(&native).unwrap();
        assert_eq!(hit.author_id.as_deref(), Some("9876543210123456789"));
        for (key, bad) in [
            ("author_id", json!("Display Name")),
            ("author_url", json!("https://www.pinterest.com/other/")),
            ("source_url", json!("https://www.pinterest.com/pin/123/")),
            ("username", json!("native_author?other=1")),
        ] {
            let mut value = native.clone();
            value[key] = bad;
            assert!(parse_search(&value).is_err());
        }
    }
    #[test]
    fn pin_search_retains_separate_content_and_account_bindings() {
        let row = json!({"provider":"onecrew-pinterest-pin-search-v1","pin_id":"456",
            "author_id":"123","username":"native_author","author_label":"Native author",
            "author_url":"https://www.pinterest.com/native_author/",
            "source_url":"https://www.pinterest.com/pin/456/","title":"Mouse review\nTwo weeks of testing"});
        let hit = parse_search(&row).unwrap();
        assert_eq!(hit.source_url, "https://www.pinterest.com/pin/456/");
        assert_eq!(hit.author_id.as_deref(), Some("123"));
        assert_eq!(
            hit.author_url.as_deref(),
            Some("https://www.pinterest.com/native_author/")
        );
        assert_eq!(hit.title, "Mouse review Two weeks of testing");
        for (key, bad) in [
            ("pin_id", json!("789")),
            (
                "source_url",
                json!("https://www.pinterest.com/native_author/"),
            ),
            ("author_url", json!("https://www.pinterest.com/another/")),
            ("author_id", json!("Native author")),
            ("title", json!("bad\u{0}text")),
            ("title", json!("x".repeat(501))),
            ("private_payload", json!(true)),
        ] {
            let mut invalid = row.clone();
            invalid[key] = bad;
            assert!(parse_search(&invalid).is_err(), "{key}");
        }
    }
    #[test]
    fn native_id_and_unknown_followers_survive_without_display_name_inference() {
        let row = parse_profile(
            &json!([native()]).to_string(),
            "9876543210123456789",
            "native_author",
        )
        .unwrap();
        assert_eq!(row.follower_count, None);
        assert_eq!(row.author_id, "9876543210123456789");
        for count in [0, 12000] {
            let mut value = native();
            value["follower_count"] = count.into();
            assert_eq!(
                parse_profile(&json!([value]).to_string(), &row.author_id, &row.username)
                    .unwrap()
                    .follower_count,
                Some(count)
            );
        }
    }
    #[test]
    fn mismatched_identity_private_links_and_unobserved_fields_are_rejected() {
        for (key, bad) in [
            ("author_id", json!("Display Name")),
            ("username", json!("other")),
            (
                "profile_url",
                json!("https://www.pinterest.com/native_author/?other=1"),
            ),
            ("follower_count", json!("12000")),
            ("public_links", json!(["https://127.0.0.1/"])),
            ("unexpected", json!("not observed")),
        ] {
            let mut value = native();
            value[key] = bad;
            assert!(
                parse_profile(
                    &json!([value]).to_string(),
                    "9876543210123456789",
                    "native_author"
                )
                .is_err()
            );
        }
        let mut value = native();
        value.as_object_mut().unwrap().remove("follower_count");
        assert!(
            parse_profile(
                &json!([value]).to_string(),
                "9876543210123456789",
                "native_author"
            )
            .is_err()
        );
        assert!(parse_profile("[]", "9876543210123456789", "native_author").is_err());
    }
}
