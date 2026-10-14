//! Source-bound public identity resolution for one observed Facebook vanity URL.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacebookSourceIdentity {
    pub adapter_version: String,
    pub source_url: String,
    pub id: String,
    pub display_name: String,
    pub profile_url: String,
    pub evidence_source: String,
}

pub(crate) fn check_source(source: &str) -> Result<(), OpenCliError> {
    let handle = source
        .strip_prefix("https://www.facebook.com/")
        .map(|value| value.strip_suffix('/').unwrap_or(value))
        .ok_or(OpenCliError::InvalidInput("facebook_identity_source"))?;
    let reserved = [
        "about",
        "ads",
        "business",
        "challenge",
        "checkpoint",
        "directory",
        "events",
        "friends",
        "gaming",
        "groups",
        "help",
        "home",
        "login",
        "login.php",
        "marketplace",
        "messages",
        "notifications",
        "pages",
        "photo",
        "photos",
        "privacy",
        "profile",
        "profile.php",
        "recover",
        "reel",
        "reels",
        "search",
        "settings",
        "share",
        "stories",
        "two_step_verification",
        "watch",
    ];
    if !(1..=100).contains(&handle.len())
        || !handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.')
        || handle.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        || reserved.contains(&handle.to_ascii_lowercase().as_str())
    {
        return Err(OpenCliError::InvalidInput("facebook_identity_source"));
    }
    Ok(())
}

fn parse(output: &str, source: &str) -> Result<FacebookSourceIdentity, OpenCliError> {
    check_source(source)?;
    if output.len() > 4000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<FacebookSourceIdentity> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.remove(0);
    if row.adapter_version != "1.0.0"
        || row.source_url != source
        || super::facebook_profile::check_target(&row.id).is_err()
        || check_source(&row.profile_url).is_err()
        || !row
            .profile_url
            .trim_end_matches('/')
            .eq_ignore_ascii_case(source.trim_end_matches('/'))
        || row.display_name.trim().is_empty()
        || row.display_name.chars().count() > 200
        || row.display_name.chars().any(char::is_control)
        || row.evidence_source != "native_profile_header_response"
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

impl OpenCli {
    /// Resolve only a saved public profile. This does not save or qualify a Creator.
    pub async fn facebook_source_identity(
        &self,
        profile: &str,
        source: &str,
    ) -> Result<FacebookSourceIdentity, OpenCliError> {
        validate_profile(profile)?;
        check_source(source)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-facebook-profile", "identity")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-facebook-profile",
                "identity",
                "-f",
                "json",
                "--",
                source,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        parse(&output, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> serde_json::Value {
        json!({"adapter_version":"1.0.0","source_url":"https://www.facebook.com/LinusTech",
            "id":"100044572203205","display_name":"Linus Tech Tips",
            "profile_url":"https://www.facebook.com/LinusTech","evidence_source":"native_profile_header_response"})
    }

    #[test]
    fn observed_native_header_binds_the_numeric_id_to_the_saved_source() {
        let row = parse(
            &json!([fixture()]).to_string(),
            "https://www.facebook.com/LinusTech",
        )
        .unwrap();
        assert_eq!(row.id, "100044572203205");
    }

    #[test]
    fn native_identity_rejects_source_drift_private_fields_and_ambiguous_rows() {
        for changes in [
            json!({"source_url":"https://www.facebook.com/AnotherPage"}),
            json!({"profile_url":"https://www.facebook.com/AnotherPage"}),
            json!({"id":"LinusTech"}),
            json!({"evidence_source":"visible_name"}),
            json!({"cookie":"must-not-cross-boundary"}),
        ] {
            let mut row = fixture();
            row.as_object_mut()
                .unwrap()
                .extend(changes.as_object().unwrap().clone());
            assert_eq!(
                parse(
                    &json!([row]).to_string(),
                    "https://www.facebook.com/LinusTech"
                ),
                Err(OpenCliError::InvalidOutput)
            );
        }
        assert_eq!(
            parse(
                &json!([fixture(), fixture()]).to_string(),
                "https://www.facebook.com/LinusTech"
            ),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn identity_targets_refuse_non_profile_routes_before_any_provider_read() {
        for source in [
            "https://www.facebook.com/search",
            "https://www.facebook.com/LinusTech/posts/123",
            "https://www.facebook.com/LinusTech?viewer=1",
            "https://www.facebook.com/LinusTech#x",
            "https://user:secret@www.facebook.com/LinusTech",
            "https://evil.example/LinusTech",
            "https://www.facebook.com/123",
            "https://www.facebook.com/checkpoint",
            "https://www.facebook.com/login.php",
            "https://www.facebook.com/LinusTech//",
        ] {
            assert!(check_source(source).is_err());
        }
    }

    #[tokio::test]
    #[ignore = "one observed saved profile; explicitly selected bridge; no search or customer write"]
    async fn actual_saved_facebook_vanity_profile_has_native_identity() {
        let browser = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit connected identity");
        let source =
            std::env::var("ONECREW_TEST_FACEBOOK_SOURCE").expect("observed saved profile source");
        let expected =
            std::env::var("ONECREW_TEST_FACEBOOK_ID").expect("independently observed native ID");
        let mut cli = OpenCli::inspect_system().await.unwrap();
        if std::env::var("OPENCLI_TEST_REPAIR_ADDONS").as_deref() == Ok("true") {
            cli.repair_bundled_addons(true).await.unwrap();
            cli = OpenCli::inspect_system().await.unwrap();
        }
        let started = std::time::Instant::now();
        let row = cli
            .facebook_source_identity(&browser, &source)
            .await
            .unwrap();
        assert_eq!(row.id, expected);
        assert_eq!(row.source_url, source);
        println!(
            "Facebook saved vanity source native identity PASS; elapsed_ms={}; numeric_id_bound=true",
            started.elapsed().as_millis()
        );
    }
}
