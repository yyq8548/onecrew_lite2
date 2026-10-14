//! Bounded X profile read for an observed search handle. A handle is mutable;
//! this receipt does not prove a permanent account ID or residence.

use serde::Serialize;
use serde_json::Value;

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, process::run, validate_profile};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TwitterPublicProfile {
    pub handle: String,
    pub profile_url: String,
    pub name: String,
    pub bio: String,
    pub self_declared_location: String,
    /// The installed adapter maps a missing native count to zero. Zero is
    /// therefore unknown here, never evidence that the account has no fans.
    pub follower_count: Option<u64>,
    pub identity_kind: &'static str,
}

pub(crate) fn valid_handle(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 15
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

impl OpenCli {
    pub async fn twitter_public_profile(
        &self,
        profile: &str,
        expected_handle: &str,
    ) -> Result<TwitterPublicProfile, OpenCliError> {
        if !valid_handle(expected_handle) || expected_handle != expected_handle.to_ascii_lowercase()
        {
            return Err(OpenCliError::InvalidInput("author_handle"));
        }
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        self.require_browser_read(profile, "twitter", "profile")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "twitter",
                "profile",
                "-f",
                "json",
                "--",
                expected_handle,
            ],
            READ_TIMEOUT,
        )
        .await?;
        parse(&output, expected_handle)
    }
}

fn parse(output: &str, expected_handle: &str) -> Result<TwitterPublicProfile, OpenCliError> {
    let rows: Vec<Value> = serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    let [row] = rows.as_slice() else {
        return Err(OpenCliError::InvalidOutput);
    };
    let field = |name: &str| row.get(name).and_then(Value::as_str).unwrap_or_default();
    let handle = field("screen_name");
    let name = field("name");
    let bio = field("bio");
    let location = field("location");
    if !valid_handle(handle)
        || !handle.eq_ignore_ascii_case(expected_handle)
        || name.trim().is_empty()
        || name.chars().count() > 200
        || name.chars().any(char::is_control)
        || bio.chars().count() > 2000
        || location.chars().count() > 200
        || bio.chars().any(|ch| ch.is_control() && ch != '\n')
        || location.chars().any(char::is_control)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let followers = row
        .get("followers")
        .and_then(Value::as_u64)
        .ok_or(OpenCliError::InvalidOutput)?;
    if followers > 9_007_199_254_740_991 {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(TwitterPublicProfile {
        handle: expected_handle.into(),
        profile_url: format!("https://x.com/{expected_handle}"),
        name: name.into(),
        bio: bio.into(),
        self_declared_location: location.into(),
        follower_count: (followers > 0).then_some(followers),
        identity_kind: "mutable_handle",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_profile_keeps_only_bounded_public_fields_and_unknown_zero() {
        let profile = parse(
            r#"[{"screen_name":"Creator_1","name":"Creator","bio":"AI demos","location":"California","followers":12000,"url":"https://private.example"}]"#,
            "creator_1",
        )
        .unwrap();
        assert_eq!(profile.handle, "creator_1");
        assert_eq!(profile.profile_url, "https://x.com/creator_1");
        assert_eq!(profile.follower_count, Some(12000));
        assert_eq!(profile.self_declared_location, "California");
        assert!(
            !serde_json::to_string(&profile)
                .unwrap()
                .contains("private.example")
        );
        let unknown = parse(
            r#"[{"screen_name":"creator_1","name":"Creator","followers":0}]"#,
            "creator_1",
        )
        .unwrap();
        assert_eq!(unknown.follower_count, None);
    }

    #[test]
    fn x_profile_rejects_mismatched_or_unreadable_identity() {
        for output in [
            r#"[{"screen_name":"other","name":"Creator","followers":12000}]"#,
            r#"[{"screen_name":"creator_1","name":"","followers":12000}]"#,
            r#"[{"screen_name":"creator_1","name":"Creator","followers":"12K"}]"#,
            r#"[{"screen_name":"creator_1","name":"Creator","followers":-1}]"#,
            r#"[]"#,
        ] {
            assert_eq!(parse(output, "creator_1"), Err(OpenCliError::InvalidOutput));
        }
    }

    #[tokio::test]
    #[ignore = "one public X profile read; requires OPENCLI_TEST_PROFILE and OPENCLI_TEST_TWITTER_HANDLE"]
    async fn x_profile_matches_observed_handle_live() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser profile");
        let handle = std::env::var("OPENCLI_TEST_TWITTER_HANDLE").expect("observed X handle");
        let expected = handle.to_ascii_lowercase();
        let cli = OpenCli::inspect_system().await.unwrap();
        let result = cli
            .twitter_public_profile(&profile, &expected)
            .await
            .unwrap();
        assert_eq!(result.handle, expected);
        assert_eq!(result.profile_url, format!("https://x.com/{expected}"));
        println!(
            "X handle={} observed_followers={:?} identity_kind={}",
            result.handle, result.follower_count, result.identity_kind
        );
    }
}
