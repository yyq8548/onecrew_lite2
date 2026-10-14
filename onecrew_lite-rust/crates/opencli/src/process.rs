use std::{path::Path, process::Stdio, time::Duration};

use tokio::{io::AsyncReadExt, process::Command, time::timeout};

use crate::OpenCliError;

const MAX_STDOUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_STDERR_BYTES: u64 = 32 * 1024;

pub(super) async fn run(
    executable: &Path,
    script: Option<&Path>,
    args: &[&str],
    deadline: Duration,
) -> Result<String, OpenCliError> {
    Ok(run_environment(executable, script, args, deadline, &[])
        .await?
        .stdout)
}

/// Diagnostics stay private to the adapter; never expose raw stderr to callers.
pub(super) struct ProcessOutput {
    pub stdout: String,
    pub stderr: Vec<u8>,
}

pub(super) async fn run_with_diagnostics(
    executable: &Path,
    script: Option<&Path>,
    args: &[&str],
    deadline: Duration,
) -> Result<ProcessOutput, OpenCliError> {
    run_environment_inner(executable, script, args, deadline, &[], true).await
}

pub(super) async fn run_environment(
    executable: &Path,
    script: Option<&Path>,
    args: &[&str],
    deadline: Duration,
    environment: &[(std::ffi::OsString, std::ffi::OsString)],
) -> Result<ProcessOutput, OpenCliError> {
    run_environment_inner(executable, script, args, deadline, environment, false).await
}

async fn run_environment_inner(
    executable: &Path,
    script: Option<&Path>,
    args: &[&str],
    deadline: Duration,
    environment: &[(std::ffi::OsString, std::ffi::OsString)],
    web_diagnostics: bool,
) -> Result<ProcessOutput, OpenCliError> {
    let mut command = Command::new(executable);
    command.envs(environment.iter().cloned());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: background probes must not flash a console.
    // Node's detached OpenCLI daemon inherits cwd. A plugin-cache cwd keeps
    // that version directory locked on Windows, preventing Codex upgrades.
    #[cfg(windows)]
    if let Some(directory) = script.and_then(Path::parent).filter(|path| path.is_absolute()) {
        command.current_dir(directory);
    }
    if let Some(script) = script {
        command.arg(script);
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| OpenCliError::Unavailable)?;
    let stdout = child.stdout.take().ok_or(OpenCliError::InvalidOutput)?;
    let stderr = child.stderr.take().ok_or(OpenCliError::InvalidOutput)?;
    let result = timeout(deadline, async {
        tokio::try_join!(
            read_limited(stdout, MAX_STDOUT_BYTES),
            async {
                if web_diagnostics {
                    read_web_diagnostics(stderr).await
                } else {
                    read_limited(stderr, MAX_STDERR_BYTES).await
                }
            },
            async { child.wait().await.map_err(|_| OpenCliError::Unavailable) },
        )
    })
    .await;
    let (stdout, stderr, status) = match result {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            drop(child.kill().await);
            return Err(error);
        }
        Err(_) => {
            drop(child.kill().await);
            return Err(OpenCliError::Timeout);
        }
    };
    if !status.success() {
        return Err(classify_failure(status.code(), &stdout, &stderr));
    }
    Ok(ProcessOutput {
        stdout: String::from_utf8(stdout).map_err(|_| OpenCliError::InvalidOutput)?,
        stderr,
    })
}

fn classify_failure(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> OpenCliError {
    let output = format!(
        "{} {}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    )
    .to_ascii_lowercase();
    if code == Some(77)
        || output.contains("auth_required")
        || output.contains("not logged in")
        // Reviewed Instagram read adapters emit this for an unauthorized session.
        // The generic sign-in suggestion alone also occurs on non-auth HTTP errors.
        || output.contains("http 401 - make sure you are logged in to instagram")
    {
        OpenCliError::AuthRequired
    } else if [
        "captcha",
        "rate limit",
        "rate-limit",
        "too many requests",
        "http 429",
        "status 429",
        "verify you are human",
        "unusual traffic",
        // Exact stop emitted by the reviewed weixin/search adapter.
        "sogou weixin blocked this search request",
        // Fixed native reader stops must quarantine that provider, rather
        // than masquerade as a source-local technical failure.
        "linkedin verification requires human review",
        "douyin verification requires human review",
        "pixiv verification requires human review",
        "facebook verification requires human review",
    ]
    .iter()
    .any(|marker| output.contains(marker))
    {
        OpenCliError::RiskControl
    } else if output.contains("onecrew_instagram_recent_source_")
        || output.contains("onecrew_instagram_recent_invalid_")
        || output.contains("invalid native pinterest input or result")
        || [
            "onecrew_linkedin_invalid",
            "onecrew_weibo_profile_invalid",
            "onecrew_weibo_followers_invalid",
            "onecrew_weibo_id_conflict",
            "onecrew_weibo_id_invalid",
            "onecrew_weibo_recent_invalid",
            "onecrew_weibo_recent_id_invalid",
            "onecrew_douyin_invalid",
            "onecrew_pixiv_invalid",
            "onecrew_pinterest_invalid",
        ]
        .iter()
        .any(|marker| output.contains(marker))
        || [
            "onecrew_pinterest_read_input",
            "onecrew_pinterest_read_source",
            "onecrew_pinterest_read_session",
            "onecrew_pinterest_read_response",
            "onecrew_pinterest_read_resource",
        ]
        .iter()
        .any(|marker| output.contains(marker))
        || [
            "onecrew_facebook_source_invalid",
            "onecrew_facebook_source_ambiguous",
            "onecrew_facebook_comments_invalid",
            "onecrew_facebook_comments_unavailable",
            "onecrew_fb_recent_invalid",
            "onecrew_fb_recent_unavailable",
            "onecrew_fb_identity_invalid",
            "onecrew_fb_profile_invalid",
            "onecrew_fb_contact_invalid",
            "onecrew_fb_original_unavailable",
        ]
        .iter()
        .any(|marker| output.contains(marker))
    {
        // Reviewed adapters refuse source drift, ambiguous owners and missing
        // native evidence. None is an empty search or proof of missing login.
        OpenCliError::InvalidOutput
    } else if code == Some(75) && output.lines().any(|line| line.trim() == "code: timeout") {
        // Exit 75 also covers SESSION_BUSY; require OpenCLI's machine code.
        OpenCliError::Timeout
    } else {
        OpenCliError::CommandFailed(code)
    }
}

async fn read_limited(
    stream: impl tokio::io::AsyncRead + Unpin,
    max: u64,
) -> Result<Vec<u8>, OpenCliError> {
    let mut bytes = Vec::new();
    stream
        .take(max + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| OpenCliError::InvalidOutput)?;
    if bytes.len() as u64 > max {
        return Err(OpenCliError::OutputTooLarge);
    }
    Ok(bytes)
}

/// Drain optional network detail without retaining resource URLs. Keep every
/// other diagnostic line bounded, including destination and duplicate markers
/// that the web parser uses to validate the source contract.
async fn read_web_diagnostics(
    mut stream: impl tokio::io::AsyncRead + Unpin,
) -> Result<Vec<u8>, OpenCliError> {
    let mut bytes = Vec::new();
    let mut line = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut in_diagnostics = false;
    let mut skip_network_line = false;
    loop {
        let count = stream
            .read(&mut chunk)
            .await
            .map_err(|_| OpenCliError::InvalidOutput)?;
        if count == 0 {
            bytes.extend_from_slice(&line);
            return Ok(bytes);
        }
        for &byte in &chunk[..count] {
            if skip_network_line {
                if byte == b'\n' {
                    skip_network_line = false;
                }
                continue;
            }
            line.push(byte);
            if in_diagnostics
                && [
                    b"  GET ".as_slice(),
                    b"  POST ",
                    b"  PUT ",
                    b"  PATCH ",
                    b"  DELETE ",
                    b"  HEAD ",
                    b"  OPTIONS ",
                    b"  CONNECT ",
                    b"  TRACE ",
                ]
                .iter()
                .any(|prefix| line == *prefix)
            {
                line.clear();
                skip_network_line = true;
                continue;
            }
            if bytes.len() + line.len() > MAX_STDERR_BYTES as usize {
                return Err(OpenCliError::OutputTooLarge);
            }
            if byte == b'\n' {
                if line == b"[web-read diagnose]\n" || line == b"[web-read diagnose]\r\n" {
                    in_diagnostics = true;
                }
                bytes.append(&mut line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "requires Node.js; verifies the real daemon launch directory"]
    async fn node_child_uses_script_directory_not_plugin_directory() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("cwd.cjs");
        std::fs::write(&script, "process.stdout.write(process.cwd())").unwrap();
        let output = run(Path::new("node"), Some(&script), &[], Duration::from_secs(10))
            .await.unwrap();
        assert_eq!(std::fs::canonicalize(output.trim()).unwrap(),
            std::fs::canonicalize(directory.path()).unwrap());
    }
    #[tokio::test]
    async fn web_diagnostics_preserve_duplicate_source_markers_and_error_lines() {
        let diagnostic = b"[web-read diagnose]\nurl: https://example.com/channel\n  GET 200 text/plain data:ignored\nerror: AUTH_REQUIRED\n[web-read diagnose]\nurl: https://example.com/other\n";
        let kept = read_web_diagnostics(diagnostic.as_slice()).await.unwrap();
        assert!(String::from_utf8_lossy(&kept).contains("error: AUTH_REQUIRED"));
        assert_eq!(
            crate::web::parse(
                "https://example.com/channel",
                "# Channel\n> 原文链接: https://example.com/channel\n\n---\n\nBody",
                &kept
            ),
            Err(OpenCliError::InvalidOutput),
        );
        assert_eq!(
            classify_failure(Some(1), b"", &kept),
            OpenCliError::AuthRequired
        );
    }

    #[tokio::test]
    async fn web_diagnostics_do_not_drop_unmarked_or_oversized_metadata() {
        for prefix in ["  GET ", "[web-read diagnose]\nurl: "] {
            let input = format!("{prefix}{}", "x".repeat(MAX_STDERR_BYTES as usize));
            assert_eq!(
                read_web_diagnostics(input.as_bytes()).await,
                Err(OpenCliError::OutputTooLarge)
            );
        }
        let mut input = b"[web-read diagnose]\nurl: https://example.com/\n  POST ".to_vec();
        input.extend(std::iter::repeat_n(b'x', 520000));
        input.extend_from_slice(b"\nerror: rate limit\n");
        let kept = read_web_diagnostics(input.as_slice()).await.unwrap();
        assert_eq!(
            classify_failure(Some(1), b"", &kept),
            OpenCliError::RiskControl
        );
    }
    #[tokio::test]
    #[ignore = "requires Node.js; exercises the real subprocess output boundary"]
    async fn web_diagnostics_drain_large_network_lines_without_losing_source_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("web-diagnostics.cjs");
        std::fs::write(&script, r#"
process.stdout.write('# Channel\n> 原文链接: https://example.com/channel\n\n---\n\nPublic channel information.');
process.stderr.write('[web-read diagnose]\nurl: https://example.com/channel\nframes: 0\nnetwork_capture: enabled\n  GET 200 image/png data:image/png;base64,' + 'A'.repeat(520000) + '\n');
"#).unwrap();
        let output = run_with_diagnostics(
            Path::new("node"),
            Some(&script),
            &[],
            Duration::from_secs(10),
        )
        .await
        .expect("optional network diagnostics must not reject a usable page");
        let page = crate::web::parse(
            "https://example.com/channel",
            &output.stdout,
            &output.stderr,
        )
        .unwrap();
        assert_eq!(page.document_url, "https://example.com/channel");
        assert_eq!(page.markdown_excerpt, "Public channel information.");
        assert!(output.stderr.len() < 1024);
        assert_eq!(
            run(
                Path::new("node"),
                Some(&script),
                &[],
                Duration::from_secs(10)
            )
            .await,
            Err(OpenCliError::OutputTooLarge),
            "ordinary command stderr must retain its strict size limit",
        );
    }
    #[test]
    fn facebook_missing_or_ambiguous_evidence_is_not_a_login_or_empty_result() {
        for marker in [
            "ONECREW_FACEBOOK_SOURCE_INVALID",
            "ONECREW_FACEBOOK_SOURCE_AMBIGUOUS",
            "ONECREW_FACEBOOK_COMMENTS_INVALID",
            "ONECREW_FACEBOOK_COMMENTS_UNAVAILABLE",
            "ONECREW_FB_RECENT_INVALID",
            "ONECREW_FB_RECENT_UNAVAILABLE",
        ] {
            assert_eq!(
                classify_failure(Some(1), b"", marker.as_bytes()),
                OpenCliError::InvalidOutput
            );
        }
        assert_eq!(
            classify_failure(Some(1), b"", b"Unclassified Facebook command error"),
            OpenCliError::CommandFailed(Some(1))
        );
    }
    #[test]
    fn instagram_feed_source_drift_is_invalid_not_empty_or_auth() {
        for marker in [
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_PATH_ROOT",
            "ONECREW_INSTAGRAM_RECENT_SOURCE_RESPONSE_ORIGIN",
            "ONECREW_INSTAGRAM_RECENT_INVALID_OWNER",
            "ONECREW_INSTAGRAM_RECENT_INVALID_POST",
        ] {
            assert_eq!(
                classify_failure(Some(1), b"", marker.as_bytes()),
                OpenCliError::InvalidOutput
            );
        }
        assert_eq!(
            classify_failure(Some(1), b"", b"ONECREW_INSTAGRAM_RECENT_HTTP_500"),
            OpenCliError::CommandFailed(Some(1))
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn background_child_has_no_console_and_preserves_stdout() {
        let output = run(
            Path::new("powershell.exe"),
            None,
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type 'using System; using System.Runtime.InteropServices; public class ConsoleProbe { [DllImport(\"kernel32.dll\")] public static extern IntPtr GetConsoleWindow(); }'; [ConsoleProbe]::GetConsoleWindow().ToInt64()",
            ],
            Duration::from_secs(30),
        )
        .await
        .expect("background child must complete");
        assert_eq!(output.trim(), "0", "background child must have no console");
    }

    #[test]
    fn instagram_unauthorized_is_not_a_navigation_or_generic_http_failure() {
        assert_eq!(
            classify_failure(
                Some(1),
                b"",
                b"HTTP 401 - make sure you are logged in to Instagram"
            ),
            OpenCliError::AuthRequired
        );
        for output in [
            "HTTP 500 - make sure you are logged in to Instagram",
            "HTTP 403 - make sure you are logged in to Instagram",
            "Navigation rejected",
        ] {
            assert_eq!(
                classify_failure(Some(1), b"", output.as_bytes()),
                OpenCliError::CommandFailed(Some(1))
            );
        }
        assert_eq!(
            classify_failure(
                Some(1),
                b"",
                b"HTTP 429 - make sure you are logged in to Instagram"
            ),
            OpenCliError::RiskControl
        );
    }

    #[test]
    fn rate_limit_is_not_labeled_as_generic_technical_failure() {
        assert_eq!(
            classify_failure(Some(1), b"HTTP 429 too many requests", b""),
            OpenCliError::RiskControl
        );
        assert_eq!(
            classify_failure(Some(1), b"Navigation rejected", b""),
            OpenCliError::CommandFailed(Some(1))
        );
        assert_eq!(
            classify_failure(Some(77), b"AUTH_REQUIRED", b""),
            OpenCliError::AuthRequired
        );
        assert_eq!(
            classify_failure(Some(1), b"", b"Sogou Weixin blocked this search request"),
            OpenCliError::RiskControl
        );
    }

    #[test]
    fn native_reader_verification_and_source_drift_keep_their_failure_scope() {
        assert_eq!(
            classify_failure(Some(1), b"ONECREW_WEIBO_PROFILE_INVALID", b""),
            OpenCliError::InvalidOutput
        );
        for message in [
            "LinkedIn verification requires human review",
            "Douyin verification requires human review",
            "Pixiv verification requires human review",
            "Facebook verification requires human review",
        ] {
            assert_eq!(
                classify_failure(Some(1), b"", message.as_bytes()),
                OpenCliError::RiskControl
            );
        }
        for message in [
            "ONECREW_LINKEDIN_INVALID_CONTACT_DIALOG_OWNER",
            "ONECREW_LINKEDIN_INVALID_CONTACT_UNAVAILABLE",
            "ONECREW_DOUYIN_INVALID",
            "ONECREW_PIXIV_INVALID",
            "ONECREW_PINTEREST_INVALID",
            "ONECREW_PINTEREST_READ_INPUT",
            "ONECREW_PINTEREST_READ_SOURCE",
            "ONECREW_PINTEREST_READ_SESSION",
            "ONECREW_PINTEREST_READ_RESPONSE",
            "ONECREW_PINTEREST_READ_RESOURCE",
            "ONECREW_FB_IDENTITY_INVALID",
            "ONECREW_FB_PROFILE_INVALID",
            "ONECREW_FB_CONTACT_INVALID",
            "ONECREW_FB_ORIGINAL_UNAVAILABLE",
        ] {
            assert_eq!(
                classify_failure(Some(1), message.as_bytes(), b""),
                OpenCliError::InvalidOutput
            );
        }
        assert_eq!(
            classify_failure(Some(1), b"", b"Other verification failed"),
            OpenCliError::CommandFailed(Some(1))
        );
        for message in [
            "ONECREW_LINKEDIN_CONTENT_UNAVAILABLE",
            "ONECREW_PINTEREST_READ_CAPTION_UNAVAILABLE inspected=5 type=0 author=0 repin=5 ad=0",
        ] {
            // Missing originals on one account do not prove a broken parser.
            assert_eq!(
                classify_failure(Some(1), message.as_bytes(), b""),
                OpenCliError::CommandFailed(Some(1))
            );
        }
        assert_eq!(
            classify_failure(Some(77), b"ONECREW_LINKEDIN_INVALID", b"AUTH_REQUIRED"),
            OpenCliError::AuthRequired
        );
        assert_eq!(
            classify_failure(Some(2), b"", b"Invalid native Pinterest input or result"),
            OpenCliError::InvalidOutput
        );
    }

    #[test]
    fn temporary_failure_needs_explicit_timeout_code() {
        assert_eq!(
            classify_failure(
                Some(75),
                b"",
                b"error:\n  code: TIMEOUT\n  message: xiaohongshu search content timed out after 5s"
            ),
            OpenCliError::Timeout
        );
        for output in ["error:\n  code: SESSION_BUSY", "temporary failure"] {
            assert_eq!(
                classify_failure(Some(75), b"", output.as_bytes()),
                OpenCliError::CommandFailed(Some(75))
            );
        }
    }
}
