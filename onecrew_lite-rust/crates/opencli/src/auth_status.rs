//! One-site OpenCLI login observation. The provider's identity and raw error
//! fields stay inside this adapter and are never serialized to OneCrew clients.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{OpenCli, OpenCliError, addons, creator_search, process::run};

const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpenCliAuthState {
    LoggedIn,
    SessionDetected,
    NotLoggedIn,
    Unknown,
    Error,
    PublicRead,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenCliAuthObservation {
    pub platform: String,
    pub profile_ref: String,
    pub status: OpenCliAuthState,
    pub checked_at: u64,
    /// `full` reads the allowlisted provider identity; `skipped` reads no site.
    pub method: &'static str,
    pub issue: Option<&'static str>,
}

#[derive(Deserialize)]
struct OpenCliAuthRow {
    site: String,
    status: String,
    checked: String,
    #[serde(default)]
    error: String,
    // `identity` is discarded. Raw errors stay private to this adapter.
}

fn auth_probe_issue(error: &str) -> &'static str {
    let message = error.to_ascii_lowercase();
    if [
        "http 429",
        "status 429",
        "too many requests",
        "rate limit",
        "rate-limit",
    ]
    .iter()
    .any(|marker| message.contains(marker))
    {
        "auth_probe_rate_limited"
    } else if message.contains("navigation rejected") {
        "auth_probe_navigation_failed"
    } else if message.contains("timeout") || message.contains("timed out") {
        "auth_probe_timeout"
    } else {
        "auth_probe_failed"
    }
}

fn auth_site(platform: &str) -> &str {
    match platform {
        "bilibili" => "onecrew-bilibili-auth",
        "tiktok" => "onecrew-tiktok-auth",
        "instagram" => "onecrew-instagram-auth",
        "facebook" => "onecrew-facebook-auth",
        "zhihu" => "onecrew-zhihu-auth",
        "douyin" => "onecrew-douyin-auth",
        "linkedin" => "onecrew-linkedin-auth",
        "linkedin-learning" => "onecrew-linkedin-learning-auth",
        "weibo" => "onecrew-weibo-auth",
        "medium" => "onecrew-medium-auth",
        "tieba" => "onecrew-tieba-auth",
        "nowcoder" => "onecrew-nowcoder-auth",
        "douban-books" | "douban-music" => "onecrew-douban-auth",
        "pixiv" => "onecrew-pixiv-auth",
        // OneCrew search add-ons are not the provider's auth adapter.
        "twitter" => "onecrew-twitter-auth",
        "reddit" => platform,
        _ => creator_search::command_site(platform),
    }
}

// These registered whoami implementations require a current-account identity
// from the provider page/API. Do not enable every whoami by name: some return
// cookie-derived IDs or unrelated profile links even when identity is unknown.
fn has_reviewed_identity_probe(site: &str) -> bool {
    matches!(
        site,
        "onecrew-bilibili-auth"
            | "onecrew-tiktok-auth"
            | "onecrew-instagram-auth"
            | "onecrew-facebook-auth"
            | "onecrew-zhihu-auth"
            | "onecrew-douyin-auth"
            | "onecrew-linkedin-auth"
            | "onecrew-linkedin-learning-auth"
            | "onecrew-weibo-auth"
            | "onecrew-medium-auth"
            | "onecrew-tieba-auth"
            | "onecrew-nowcoder-auth"
            | "onecrew-douban-auth"
            | "onecrew-pixiv-auth"
            | "reddit"
            | "jike"
            | "instagram"
            | "onecrew-twitter-auth"
            | "linkedin"
            | "linkedin-learning"
            | "linux-do"
            | "pixiv"
            | "zhihu"
            | "weibo"
            | "tiktok"
    )
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_auth_status(
    output: &str,
    expected_site: &str,
) -> Result<(OpenCliAuthState, &'static str, Option<&'static str>), OpenCliError> {
    let rows: Vec<OpenCliAuthRow> =
        serde_json::from_str(output).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() != 1 || rows[0].site != expected_site {
        return Err(OpenCliError::InvalidOutput);
    }
    let row = &rows[0];
    match (row.status.as_str(), row.checked.as_str()) {
        ("logged_in", "quick") => Ok((OpenCliAuthState::SessionDetected, "quick", None)),
        ("not_logged_in", "quick") => Ok((OpenCliAuthState::NotLoggedIn, "quick", None)),
        ("logged_in", "full") => Ok((OpenCliAuthState::LoggedIn, "full", None)),
        ("not_logged_in", "full") => Ok((OpenCliAuthState::NotLoggedIn, "full", None)),
        ("error", "full")
            if row.error == "COMMAND_EXEC: Current-account identity could not be verified" =>
        {
            Ok((
                OpenCliAuthState::Unknown,
                "full",
                Some("auth_signal_unavailable"),
            ))
        }
        ("error", "full") => Ok((
            OpenCliAuthState::Error,
            "full",
            Some(auth_probe_issue(&row.error)),
        )),
        ("unknown", "quick") => Ok((
            OpenCliAuthState::Unknown,
            "quick",
            Some("auth_signal_unavailable"),
        )),
        ("unknown", "skipped") => Ok((
            OpenCliAuthState::Unknown,
            "skipped",
            Some("auth_probe_unavailable"),
        )),
        ("error", "quick") => Ok((
            OpenCliAuthState::Error,
            "quick",
            Some(auth_probe_issue(&row.error)),
        )),
        _ => Err(OpenCliError::InvalidOutput),
    }
}

impl OpenCli {
    /// Customer-requested, bounded login check. Reviewed identity probes use
    /// full mode; cookie-only probes cannot establish a valid login.
    /// Never refresh auth or return identity to the client.
    pub async fn check_creator_auth_status(
        &self,
        profile: &str,
        platform: &str,
    ) -> Result<OpenCliAuthObservation, OpenCliError> {
        if creator_search::command(platform).is_none() {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        self.check_browser_profile(profile).await?;
        let observed = |status, method, issue| OpenCliAuthObservation {
            platform: platform.to_owned(),
            profile_ref: profile.to_owned(),
            status,
            checked_at: now_epoch_seconds(),
            method,
            issue,
        };
        if creator_search::is_public(platform) {
            return Ok(observed(OpenCliAuthState::PublicRead, "skipped", None));
        }
        // A LinkedIn login says nothing about the separate paid Sales Navigator entitlement.
        if platform == "linkedin-salesnav" {
            return Ok(observed(
                OpenCliAuthState::Unknown,
                "skipped",
                Some("entitlement_unverified"),
            ));
        }
        let site = auth_site(platform);
        let addon = match platform {
            "bilibili" | "tiktok" | "instagram" | "facebook" | "zhihu" | "douyin" | "linkedin"
            | "linkedin-learning" | "weibo" | "medium" | "tieba" | "nowcoder" | "douban-books"
            | "douban-music" | "pixiv" | "twitter" => Some("onecrew-site-auth"),
            _ => None,
        };
        if let Some(addon) = addon {
            if self.version != "1.8.8" {
                return Err(OpenCliError::VersionChanged);
            }
            if addons::require_ready(addon).await.is_err() {
                return Ok(observed(
                    OpenCliAuthState::Unknown,
                    "skipped",
                    Some("auth_addon_required"),
                ));
            }
        }
        if !self.registry.has_browser_read(site, "whoami") {
            return Ok(observed(
                OpenCliAuthState::Unknown,
                "skipped",
                Some("auth_probe_unavailable"),
            ));
        }
        let full = has_reviewed_identity_probe(site);
        let mut args = vec![
            "--profile",
            profile,
            "auth",
            "status",
            "--site",
            site,
            "--concurrency",
            "1",
            "--timeout",
            if full { "20" } else { "5" },
            "-f",
            "json",
        ];
        if full {
            args.push("--full");
        }
        let output = match run(
            &self.executable,
            self.script.as_deref(),
            &args,
            if full {
                Duration::from_secs(30)
            } else {
                AUTH_STATUS_TIMEOUT
            },
        )
        .await
        {
            Ok(output) => output,
            Err(OpenCliError::Timeout) => {
                return Ok(observed(
                    OpenCliAuthState::Error,
                    if full { "full" } else { "quick" },
                    Some("auth_probe_timeout"),
                ));
            }
            Err(OpenCliError::RiskControl) => {
                return Ok(observed(
                    OpenCliAuthState::Error,
                    if full { "full" } else { "quick" },
                    Some("auth_probe_rate_limited"),
                ));
            }
            Err(error) => return Err(error),
        };
        if let Some(addon) = addon {
            addons::require_ready(addon).await?;
        }
        let (status, method, issue) = parse_auth_status(&output, site)?;
        if method != "skipped" && method != if full { "full" } else { "quick" } {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(observed(status, method, issue))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_native_auth_is_independent_of_search_adapter_aliases() {
        assert_eq!(auth_site("bilibili"), "onecrew-bilibili-auth");
        assert!(has_reviewed_identity_probe(auth_site("bilibili")));
        assert_eq!(auth_site("twitter"), "onecrew-twitter-auth");
        assert!(has_reviewed_identity_probe(auth_site("twitter")));
        for platform in ["reddit"] {
            assert_ne!(creator_search::command_site(platform), platform);
            assert_eq!(auth_site(platform), platform);
            assert!(has_reviewed_identity_probe(auth_site(platform)));
        }
    }

    #[test]
    fn absent_self_signal_is_unknown_not_logout_or_transport_failure() {
        let output = r#"[{"site":"onecrew-medium-auth","status":"error","checked":"full","error":"COMMAND_EXEC: Current-account identity could not be verified","identity":"private"}]"#;
        assert_eq!(
            parse_auth_status(output, "onecrew-medium-auth"),
            Ok((
                OpenCliAuthState::Unknown,
                "full",
                Some("auth_signal_unavailable")
            ))
        );
    }

    #[test]
    fn failed_identity_checks_never_imply_logout_or_expose_provider_errors() {
        for (error, issue) in [
            (
                "COMMAND_ERROR: HTTP 429 from Instagram /users/info",
                "auth_probe_rate_limited",
            ),
            ("Navigation rejected.", "auth_probe_navigation_failed"),
            ("TIMEOUT: probe timed out after 20s", "auth_probe_timeout"),
            ("provider failure cookie=secret", "auth_probe_failed"),
        ] {
            let output = serde_json::json!([{"site":"instagram", "status":"error", "checked":"full", "error":error, "identity":"private"}]).to_string();
            assert_eq!(
                parse_auth_status(&output, "instagram"),
                Ok((OpenCliAuthState::Error, "full", Some(issue)))
            );
        }
    }

    #[test]
    fn parses_only_one_expected_safe_status_and_discards_identity() {
        let output = r#"[{"site":"instagram","status":"logged_in","checked":"quick","identity":"private-name","error":"cookie=secret"}]"#;
        assert_eq!(
            parse_auth_status(output, "instagram"),
            Ok((OpenCliAuthState::SessionDetected, "quick", None))
        );
        assert_eq!(
            parse_auth_status(output, "facebook"),
            Err(OpenCliError::InvalidOutput)
        );
        assert_eq!(
            parse_auth_status("[]", "instagram"),
            Err(OpenCliError::InvalidOutput)
        );
        assert_eq!(
            parse_auth_status("not-json", "instagram"),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn no_whoami_signal_never_becomes_logged_in() {
        let output = r#"[{"site":"youtube","status":"unknown","checked":"skipped","error":"quickCheck not implemented"}]"#;
        assert_eq!(
            parse_auth_status(output, "youtube"),
            Ok((
                OpenCliAuthState::Unknown,
                "skipped",
                Some("auth_probe_unavailable")
            ))
        );
        assert_eq!(auth_site("douban-books"), "onecrew-douban-auth");
        for platform in [
            "tiktok",
            "instagram",
            "facebook",
            "zhihu",
            "douyin",
            "linkedin",
        ] {
            let site = auth_site(platform);
            assert_eq!(site, format!("onecrew-{platform}-auth"));
            assert!(has_reviewed_identity_probe(site));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reviewed_probe_uses_selected_profile_and_exposes_no_identity() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-opencli.sh");
        std::fs::write(&script, r##"#!/bin/sh
case "$*" in
  "--version") printf '1.8.8\n' ;;
  "list -f json") printf '%s\n' '[{"command":"jike/whoami","site":"jike","name":"whoami","access":"read","browser":true},{"command":"reddit/whoami","site":"reddit","name":"whoami","access":"read","browser":true},{"command":"instagram/search","site":"instagram","name":"search","access":"read","browser":true},{"command":"instagram/whoami","site":"instagram","name":"whoami","access":"read","browser":true},{"command":"linkedin/salesnav-search","site":"linkedin","name":"salesnav-search","access":"read","browser":true},{"command":"archive/search","site":"archive","name":"search","access":"read","browser":false,"strategy":"public"}]' ;;
  "daemon status") printf 'Daemon: running\nExtension: connected\n' ;;
  "profile list") printf 'Connected Browser Bridge profiles\n\n  dedicated — connected v1.0.24\n' ;;
  "--profile dedicated auth status --site instagram --concurrency 1 --timeout 20 -f json --full") printf '%s\n' '[{"site":"instagram","status":"logged_in","checked":"full","identity":"private-user","error":"cookie=private"}]' ;;
  "--profile dedicated auth status --site jike --concurrency 1 --timeout 20 -f json --full") printf '%s\n' '[{"site":"jike","status":"logged_in","checked":"full","identity":"private-jike","error":"cookie=secret"}]' ;;
  "--profile dedicated auth status --site reddit --concurrency 1 --timeout 20 -f json --full") printf '%s\n' '[{"site":"reddit","status":"not_logged_in","checked":"full"}]' ;;
  *) exit 90 ;;
esac
"##).unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();
        let cli = OpenCli::inspect(script).await.unwrap();
        assert!(matches!(
            cli.check_creator_auth_status("dedicated", "jike").await,
            Err(OpenCliError::ReadCommandUnavailable)
        ));
        let reddit = cli
            .check_creator_auth_status("dedicated", "reddit")
            .await
            .unwrap();
        assert_eq!(reddit.status, OpenCliAuthState::NotLoggedIn);
        assert_eq!(reddit.method, "full");
        assert!(matches!(
            cli.check_creator_auth_status("dedicated", "linkedin-salesnav")
                .await,
            Err(OpenCliError::ReadCommandUnavailable)
        ));
        assert!(matches!(
            cli.check_creator_auth_status("dedicated", "hf-spaces")
                .await,
            Err(OpenCliError::ReadCommandUnavailable)
        ));
        assert_eq!(
            cli.check_creator_auth_status("dedicated", "not-a-source")
                .await,
            Err(OpenCliError::ReadCommandUnavailable)
        );
    }
}
