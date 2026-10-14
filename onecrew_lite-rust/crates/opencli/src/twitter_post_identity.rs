//! Binds one observed X post to its native author, not merely to today's handle.
//! This provider receipt alone proves neither content qualification nor residence.
use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TwitterPostIdentity {
    pub adapter_version: String,
    pub post_id: String,
    pub account_id: String,
    pub handle: String,
    pub profile_url: String,
    pub source_url: String,
    pub identity_kind: String,
    pub evidence_source: String,
}

pub(super) fn numeric_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

impl OpenCli {
    /// One bounded read through the exact installed add-on and selected profile.
    /// No fallback, pagination, installation, Campaign write, or qualification.
    pub async fn twitter_post_identity(
        &self,
        profile: &str,
        expected_handle: &str,
        expected_post_id: &str,
    ) -> Result<TwitterPostIdentity, OpenCliError> {
        if !super::twitter_profile::valid_handle(expected_handle)
            || expected_handle != expected_handle.to_ascii_lowercase()
            || !numeric_id(expected_post_id)
        {
            return Err(OpenCliError::InvalidInput("post_author_target"));
        }
        validate_profile(profile)?;
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        addons::require_ready("onecrew-twitter-post-identity").await?;
        self.require_browser_read(profile, "onecrew-twitter-post-identity", "read")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-twitter-post-identity",
                "read",
                "-f",
                "json",
                "--",
                expected_handle,
                expected_post_id,
            ],
            READ_TIMEOUT,
        )
        .await?;
        self.require_version().await?;
        addons::require_ready("onecrew-twitter-post-identity").await?;
        parse(&output, expected_handle, expected_post_id)
    }
}

fn parse(
    output: &str,
    expected_handle: &str,
    expected_post_id: &str,
) -> Result<TwitterPostIdentity, OpenCliError> {
    if output.len() > 4_000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let mut rows: Vec<TwitterPostIdentity> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = rows.pop().ok_or(OpenCliError::InvalidOutput)?;
    if !valid_identity(&row, expected_handle, expected_post_id) {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

pub(super) fn valid_identity(
    row: &TwitterPostIdentity,
    expected_handle: &str,
    expected_post_id: &str,
) -> bool {
    !(row.adapter_version != "1.0.0"
        || row.identity_kind != "native_post_author_id"
        || row.evidence_source != "x_tweet_detail"
        || !numeric_id(&row.account_id)
        || !numeric_id(&row.post_id)
        || row.post_id != expected_post_id
        || row.handle != expected_handle
        || !super::twitter_profile::valid_handle(&row.handle)
        || row.profile_url != format!("https://x.com/{expected_handle}")
        || row.source_url != format!("https://x.com/{expected_handle}/status/{expected_post_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        json!([{"adapter_version":"1.0.0", "post_id":"1234567890123456789",
            "account_id":"987654321012345678", "handle":"creator_1",
            "profile_url":"https://x.com/creator_1",
            "source_url":"https://x.com/creator_1/status/1234567890123456789",
            "identity_kind":"native_post_author_id", "evidence_source":"x_tweet_detail"}])
    }

    #[test]
    fn post_identity_preserves_native_ids_without_numeric_rounding() {
        let row = parse(&fixture().to_string(), "creator_1", "1234567890123456789").unwrap();
        assert_eq!(row.account_id, "987654321012345678");
        assert_eq!(row.post_id, "1234567890123456789");
    }

    #[test]
    fn post_identity_rejects_mismatches_unsafe_sources_and_extra_fields() {
        for (field, wrong) in [
            ("account_id", json!(987654321012345678_u64)),
            ("account_id", json!("0")),
            ("account_id", json!("01")),
            ("account_id", json!("1".repeat(21))),
            ("post_id", json!("123")),
            ("post_id", json!(1234567890123456789_u64)),
            ("handle", json!("other")),
            ("handle", json!("Creator_1")),
            ("profile_url", json!("https://x.com/creator_1?secret=1")),
            (
                "source_url",
                json!("https://x.com/other/status/1234567890123456789"),
            ),
            ("identity_kind", json!("mutable_handle")),
            ("evidence_source", json!("x_user_by_screen_name")),
            ("adapter_version", json!("2.0.0")),
            ("cookie", json!("must-not-escape")),
        ] {
            let mut value = fixture();
            value[0][field] = wrong;
            assert_eq!(
                parse(&value.to_string(), "creator_1", "1234567890123456789"),
                Err(OpenCliError::InvalidOutput),
                "{field}"
            );
        }
        assert_eq!(
            parse("[]", "creator_1", "123"),
            Err(OpenCliError::InvalidOutput)
        );
        assert_eq!(
            parse(&" ".repeat(4001), "creator_1", "123"),
            Err(OpenCliError::OutputTooLarge)
        );
    }

    #[tokio::test]
    async fn post_identity_invalid_target_and_version_never_dispatch() {
        let cli = OpenCli {
            executable: "must-not-execute".into(),
            script: None,
            version: "changed".into(),
            registry: crate::registry::Registry::parse(
                r#"[{"command":"twitter/thread","site":"twitter","name":"thread","access":"read","browser":true}]"#,
            )
            .unwrap(),
        };
        for (handle, post) in [
            ("", "123"),
            ("Creator", "123"),
            ("creator", "0"),
            ("creator", "01"),
            ("creator", "123?token=secret"),
        ] {
            assert_eq!(
                cli.twitter_post_identity("dedicated", handle, post).await,
                Err(OpenCliError::InvalidInput("post_author_target"))
            );
        }
        assert_eq!(
            cli.twitter_post_identity("dedicated", "creator", "123")
                .await,
            Err(OpenCliError::VersionChanged)
        );
    }

    #[tokio::test]
    #[ignore = "one public post identity read; requires explicit profile and observed post target"]
    async fn observed_x_post_has_native_author_live() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit current profile");
        let handle = std::env::var("OPENCLI_TEST_TWITTER_HANDLE").expect("observed author handle");
        let post = std::env::var("OPENCLI_TEST_TWITTER_POST").expect("observed post ID");
        let cli = OpenCli::inspect_system().await.unwrap();
        let result = cli
            .twitter_post_identity(&profile, &handle, &post)
            .await
            .unwrap();
        assert_eq!(result.handle, handle);
        assert_eq!(result.post_id, post);
        println!("Native focal-post author verified; no Campaign or qualification written");
    }
}
