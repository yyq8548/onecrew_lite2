//! Native identity observed on a current X profile. This does not bind an older
//! search post to that account. Campaign owners must compare an independent
//! post-author receipt before using it as author-bound profile evidence.
use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TwitterIdentityProfile {
    pub adapter_version: String,
    pub account_id: String,
    pub handle: String,
    pub profile_url: String,
    pub name: String,
    pub bio: String,
    pub self_declared_location: String,
    pub follower_count: Option<u64>,
    pub identity_kind: String,
    pub evidence_source: String,
}

impl OpenCli {
    /// Read one observed handle through the separately installed, byte-checked
    /// add-on. Never falls back to the official handle-only profile projection.
    pub async fn twitter_identity_profile(
        &self,
        profile: &str,
        expected_handle: &str,
    ) -> Result<TwitterIdentityProfile, OpenCliError> {
        if !super::twitter_profile::valid_handle(expected_handle)
            || expected_handle != expected_handle.to_ascii_lowercase()
        {
            return Err(OpenCliError::InvalidInput("author_handle"));
        }
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        addons::require_ready("onecrew-twitter-identity").await?;
        self.require_browser_read(profile, "onecrew-twitter-identity", "read")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-twitter-identity",
                "read",
                "-f",
                "json",
                "--",
                expected_handle,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-twitter-identity").await?;
        parse(&output, expected_handle)
    }
}

fn parse(output: &str, expected: &str) -> Result<TwitterIdentityProfile, OpenCliError> {
    if output.len() > 16_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<TwitterIdentityProfile> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.pop().ok_or(OpenCliError::InvalidOutput)?;
    let text = |value: &str, maximum, multiline| {
        value.chars().count() <= maximum
            && !value
                .chars()
                .any(|ch| ch.is_control() && !(multiline && ch == '\n'))
    };
    if row.adapter_version != "1.0.0"
        || row.identity_kind != "native_id"
        || row.evidence_source != "x_user_by_screen_name"
        || row.account_id.is_empty()
        || row.account_id.len() > 20
        || row.account_id.starts_with('0')
        || !row.account_id.bytes().all(|byte| byte.is_ascii_digit())
        || !super::twitter_profile::valid_handle(&row.handle)
        || row.handle != expected
        || row.profile_url != format!("https://x.com/{expected}")
        || row.name.trim().is_empty()
        || !text(&row.name, 200, false)
        || !text(&row.bio, 2000, true)
        || !text(&row.self_declared_location, 200, false)
        || row
            .follower_count
            .is_some_and(|count| count > 9_007_199_254_740_991)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        json!([{"adapter_version":"1.0.0","account_id":"1234567890123456789",
            "handle":"creator_1","profile_url":"https://x.com/creator_1","name":"Creator",
            "bio":"Demo\nnotes","self_declared_location":"Self-entered",
            "follower_count":0,"identity_kind":"native_id","evidence_source":"x_user_by_screen_name"}])
    }

    #[test]
    fn native_identity_preserves_decimal_id_and_distinguishes_unknown_from_zero() {
        let mut value = fixture();
        let row = parse(&value.to_string(), "creator_1").unwrap();
        assert_eq!(row.account_id, "1234567890123456789");
        assert_eq!(row.follower_count, Some(0));
        value[0]["follower_count"] = Value::Null;
        assert_eq!(
            parse(&value.to_string(), "creator_1")
                .unwrap()
                .follower_count,
            None
        );
    }

    #[test]
    fn native_identity_rejects_unsafe_mismatched_or_untrusted_projection() {
        for (field, wrong) in [
            ("account_id", json!(1234567890123456789_u64)),
            ("account_id", json!("0")),
            ("account_id", json!("01")),
            ("account_id", json!("123x")),
            ("account_id", json!("1".repeat(21))),
            ("handle", json!("other")),
            ("handle", json!("Creator_1")),
            ("profile_url", json!("https://x.com/creator_1?token=secret")),
            ("follower_count", json!(-1)),
            ("follower_count", json!(9007199254740992_u64)),
            ("name", json!("")),
            ("bio", json!("a\0b")),
            ("identity_kind", json!("mutable_handle")),
            ("adapter_version", json!("2.0.0")),
            ("cookie", json!("must-not-escape")),
        ] {
            let mut value = fixture();
            value[0][field] = wrong;
            assert_eq!(
                parse(&value.to_string(), "creator_1"),
                Err(OpenCliError::InvalidOutput),
                "{field}"
            );
        }
        assert_eq!(parse("[]", "creator_1"), Err(OpenCliError::InvalidOutput));
        assert_eq!(
            parse(&" ".repeat(16001), "creator_1"),
            Err(OpenCliError::OutputTooLarge)
        );
    }

    #[tokio::test]
    async fn identity_read_rejects_invalid_targets_and_versions_before_dispatch() {
        let cli = OpenCli {
            executable: "must-not-execute".into(),
            script: None,
            version: "changed".into(),
            registry: crate::registry::Registry::parse(
                r#"[{"command":"twitter/profile","site":"twitter","name":"profile","access":"read","browser":true}]"#,
            )
            .unwrap(),
        };
        for handle in ["", "Creator", "creator?token=secret", "creator/name"] {
            assert_eq!(
                cli.twitter_identity_profile("dedicated", handle).await,
                Err(OpenCliError::InvalidInput("author_handle"))
            );
        }
        assert_eq!(
            cli.twitter_identity_profile("dedicated", "creator_1").await,
            Err(OpenCliError::VersionChanged)
        );
    }

    #[tokio::test]
    #[ignore = "one public X identity read; requires explicit current profile and observed handle"]
    async fn observed_x_handle_has_native_identity_live() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit current profile");
        let handle = std::env::var("OPENCLI_TEST_TWITTER_HANDLE").expect("observed handle");
        let cli = OpenCli::inspect_system().await.unwrap();
        let result = cli
            .twitter_identity_profile(&profile, &handle)
            .await
            .unwrap();
        assert_eq!(result.handle, handle);
        assert_eq!(result.identity_kind, "native_id");
        println!("X native identity read passed; no Campaign or qualification was written");
    }
}
