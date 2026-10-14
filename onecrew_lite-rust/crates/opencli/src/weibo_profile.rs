//! One bounded native Weibo profile read. UIDs are identities, not usernames.
use crate::{OpenCli, OpenCliError, READ_TIMEOUT, addons, process::run, validate_profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WeiboPublicProfile {
    pub id: String,
    pub name: String,
    pub follower_count: Option<u64>,
    pub profile_url: String,
    pub bio: String,
    pub self_declared_location: String,
}

pub(crate) fn check_target(id: &str, source: &str) -> Result<(), OpenCliError> {
    if !(1..=20).contains(&id.len())
        || id.starts_with('0')
        || !id.bytes().all(|b| b.is_ascii_digit())
        || source != format!("https://weibo.com/u/{id}")
    {
        return Err(OpenCliError::InvalidInput("weibo_profile_source"));
    }
    Ok(())
}

fn parse(output: &str, id: &str) -> Result<WeiboPublicProfile, OpenCliError> {
    if output.len() > 16000 {
        return Err(OpenCliError::OutputTooLarge);
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    let [value] = rows.as_slice() else {
        return Err(OpenCliError::InvalidOutput);
    };
    let keys = [
        "id",
        "name",
        "follower_count",
        "profile_url",
        "bio",
        "self_declared_location",
    ];
    if !value
        .as_object()
        .is_some_and(|v| v.len() == keys.len() && keys.iter().all(|key| v.contains_key(*key)))
    {
        return Err(OpenCliError::InvalidOutput);
    }
    let row: WeiboPublicProfile =
        serde_json::from_value(value.clone()).map_err(|_| OpenCliError::InvalidOutput)?;
    if row.id != id
        || check_target(id, &row.profile_url).is_err()
        || row.name.trim().is_empty()
        || row.name.chars().count() > 200
        || row.name.chars().any(char::is_control)
        || row.bio.chars().count() > 2000
        || row.bio.contains('\0')
        || row.self_declared_location.chars().count() > 200
        || row.self_declared_location.contains('\0')
        || row
            .follower_count
            .is_some_and(|n| n > 9_007_199_254_740_991)
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(row)
}

impl OpenCli {
    pub async fn weibo_public_profile(
        &self,
        profile: &str,
        author_id: &str,
    ) -> Result<WeiboPublicProfile, OpenCliError> {
        check_target(author_id, &format!("https://weibo.com/u/{author_id}"))?;
        validate_profile(profile)?;
        self.require_version().await?;
        addons::require_ready("onecrew-browser-search").await?;
        self.require_browser_read(profile, "onecrew-weibo-reader", "profile")
            .await?;
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "onecrew-weibo-reader",
                "profile",
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
        parse(&output, author_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(count: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"id":"12345","name":"Author","follower_count":count,"profile_url":"https://weibo.com/u/12345","bio":"商务联系: creator@example.com","self_declared_location":""})
    }
    #[test]
    fn weibo_native_profile_preserves_unknown_zero_and_public_contacts() {
        for (count, expected) in [
            (serde_json::Value::Null, None),
            (serde_json::json!(0), Some(0)),
            (serde_json::json!(15000), Some(15000)),
        ] {
            let parsed = parse(&serde_json::json!([row(count)]).to_string(), "12345").unwrap();
            assert_eq!(parsed.follower_count, expected);
            assert!(parsed.bio.contains("creator@example.com"));
        }
    }
    #[test]
    fn weibo_profile_rejects_identity_redirect_missing_count_and_unreviewed_fields() {
        for (key, value) in [
            ("id", serde_json::json!("999")),
            ("profile_url", serde_json::json!("https://weibo.com/u/999")),
            ("follower_count", serde_json::json!(9007199254740992u64)),
            ("name", serde_json::json!("")),
            ("cookie", serde_json::json!("must not cross")),
        ] {
            let mut value_row = row(serde_json::json!(15000));
            value_row[key] = value;
            assert!(parse(&serde_json::json!([value_row]).to_string(), "12345").is_err());
        }
        let mut missing = row(serde_json::Value::Null);
        missing.as_object_mut().unwrap().remove("follower_count");
        assert!(parse(&serde_json::json!([missing]).to_string(), "12345").is_err());
        assert!(check_target("12345", "https://weibo.com/u/12345?redirect=1").is_err());
        assert!(check_target("0123", "https://weibo.com/u/0123").is_err());
    }
}
