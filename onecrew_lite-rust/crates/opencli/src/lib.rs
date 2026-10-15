//! Bounded platform reads and local setup for the user-installed OpenCLI executable.
//!
//! This crate owns the CLI process boundary and its per-site output decoding.
//! It never exposes an arbitrary command executor or a platform write method.

mod addons;
mod auth_status;
mod bilibili;
mod bilibili_language;
mod bilibili_profile;
mod bilibili_video;
mod bluesky;
mod creator_profile;
mod creator_search;
mod douyin;
pub use douyin::{DouyinComment, DouyinCommentWindow, DouyinContent, DouyinPublicProfile};
mod facebook_identity;
mod facebook_language;
mod facebook_profile;
mod hackernews;
mod instagram_language;
mod instagram_profile;
#[cfg(unix)]
mod installation;
mod linkedin;
mod login_page;
mod managed_installation;
mod native_comments;
mod pinterest;
mod pixiv;
pub use linkedin::{
    LinkedinComment, LinkedinContent, LinkedinPublicProfile, linkedin_contact_handle,
};
pub use pinterest::{PinterestContent, PinterestPublicProfile};
pub use pixiv::{PixivContent, PixivPublicProfile};
mod process;
mod recovery;
mod reddit_language;
mod reddit_profile;
mod registry;
mod tiktok;
mod tiktok_language;
mod tiktok_profile;
mod twitter_identity;
mod twitter_post_content;
mod twitter_post_identity;
mod twitter_profile;
mod twitter_tweets;
mod web;
mod weibo_language;
mod weibo_post;
mod weibo_profile;
mod youtube;
mod youtube_evidence;
mod youtube_language;
mod zhihu;

pub use addons::{OpenCliAddonState, OpenCliAddonStatus};
pub use auth_status::{OpenCliAuthObservation, OpenCliAuthState};
pub use bilibili::BilibiliSearchVideo;
pub use bilibili_profile::{BilibiliPublicProfile, check_bilibili_profile_target};
pub use bilibili_video::BilibiliVideoEvidence;
pub use bluesky::{BlueskyOriginalComment, BlueskyOriginalPost, BlueskyPublicProfile};
pub use creator_search::{CreatorSearchCapability, CreatorSearchHit, RETIRED_CREATOR_SOURCES};
pub use facebook_identity::FacebookSourceIdentity;
pub use facebook_language::{FacebookLanguageComment, FacebookLanguageSample, FacebookReelContent};
pub use facebook_profile::{
    FacebookPublicProfile, facebook_contact_author, facebook_native_contact_profile,
};
pub use hackernews::{HackerNewsOriginalComment, HackerNewsOriginalItem, HackerNewsPublicProfile};
pub use instagram_language::InstagramOriginalPost;
pub use instagram_profile::InstagramPublicProfile;
pub use managed_installation::{VERIFIED_VERSION, repair_managed_installation};
pub use native_comments::NativeCommentSource;
pub use reddit_profile::RedditPublicProfile;
pub use registry::{OpenCliInventory, OpenCliSiteInventory};
pub use tiktok::TiktokSearchVideo;
pub use tiktok_profile::{TiktokPublicProfile, check_tiktok_profile_target};
pub use twitter_identity::TwitterIdentityProfile;
pub use twitter_post_content::TwitterPostContent;
pub use twitter_post_identity::TwitterPostIdentity;
pub use twitter_profile::TwitterPublicProfile;
pub use twitter_tweets::TwitterRecentContent;
pub use web::{PublicWebRead, validate_public_web_url};
pub use weibo_language::WeiboOriginalComment;
pub use weibo_post::WeiboPostEvidence;
pub use weibo_profile::WeiboPublicProfile;
pub use youtube::{YoutubeChannel, YoutubeSearchHit, YoutubeTranscriptLine, YoutubeVideo};
pub use youtube_evidence::{YoutubeChannelContact, YoutubeDescriptionLink, YoutubeVideoEvidence};
pub use zhihu::{
    ZhihuOriginalAnswer, ZhihuOriginalArticle, ZhihuOriginalComment, ZhihuPublicProfile,
};

use std::{path::PathBuf, time::Duration};

use thiserror::Error;

use process::run;
use registry::Registry;

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeEvidenceFailure {
    Unavailable,
    Layout,
    Identity,
    Description,
}

impl YoutubeEvidenceFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "youtube_video_unavailable",
            Self::Layout => "youtube_watch_layout_unsupported",
            Self::Identity => "youtube_author_identity_invalid",
            Self::Description => "youtube_description_unavailable",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OpenCliError {
    #[error("OpenCLI executable is unavailable")]
    Unavailable,
    #[error("OpenCLI command timed out")]
    Timeout,
    #[error("OpenCLI output exceeded its limit")]
    OutputTooLarge,
    #[error("OpenCLI command failed with exit code {0:?}")]
    CommandFailed(Option<i32>),
    #[error("YouTube video evidence failed: {0:?}")]
    YoutubeEvidence(YoutubeEvidenceFailure),
    #[error("OpenCLI provider requested human verification or rate limited the read")]
    RiskControl,
    #[error("OpenCLI provider account is not signed in or not authorized")]
    AuthRequired,
    #[error("OpenCLI output is invalid")]
    InvalidOutput,
    #[error("OpenCLI installed version changed during this run")]
    VersionChanged,
    #[error("OpenCLI daemon is not running")]
    DaemonUnavailable,
    #[error("OpenCLI browser extension is not connected")]
    ExtensionUnavailable,
    #[error("selected OpenCLI browser profile is not connected")]
    ProfileUnavailable,
    #[error("OpenCLI required read command is unavailable")]
    ReadCommandUnavailable,
    #[error("OpenCLI input is invalid: {0}")]
    InvalidInput(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCliBridgeStatus {
    pub daemon_running: bool,
    pub extension_connected: bool,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct OpenCliProfile {
    pub context_id: String,
    pub alias: Option<String>,
}

/// Registry-pinned OpenCLI reader. Construct after user selected a dedicated
/// browser profile; every browser read rechecks version and bridge status.
pub struct OpenCli {
    executable: PathBuf,
    script: Option<PathBuf>,
    version: String,
    registry: Registry,
}

impl OpenCli {
    /// Resolve the installed CLI without forwarding user input through an npm
    /// batch shim. On Windows we execute Node and the package entrypoint.
    pub async fn inspect_system() -> Result<Self, OpenCliError> {
        if let Some((node, script)) = managed_installation::installed_entrypoint().await? {
            return Self::inspect_with_script(node, Some(script)).await;
        }
        #[cfg(windows)]
        {
            let directories = std::env::var_os("PATH")
                .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
                .unwrap_or_default();
            let app_roots = std::env::var_os("LOCALAPPDATA")
                .map(|value| {
                    let local = PathBuf::from(value);
                    vec![local.join("OpenCLIApp"), local.join("Programs/OpenCLIApp")]
                })
                .unwrap_or_default();
            let (node, script) =
                windows_installation(&directories, &app_roots).ok_or(OpenCliError::Unavailable)?;
            Self::inspect_with_script(node, Some(script)).await
        }
        #[cfg(unix)]
        {
            installation::inspect_system().await
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(OpenCliError::Unavailable)
        }
    }

    /// Inspect registered commands without opening a browser or starting a
    /// daemon. A caller can pass a trusted absolute executable in tests.
    pub async fn inspect(executable: PathBuf) -> Result<Self, OpenCliError> {
        #[cfg(windows)]
        if executable
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "cmd" | "bat" | "ps1"
                )
            })
        {
            return Err(OpenCliError::InvalidInput("executable"));
        }
        Self::inspect_with_script(executable, None).await
    }

    async fn inspect_with_script(
        executable: PathBuf,
        script: Option<PathBuf>,
    ) -> Result<Self, OpenCliError> {
        let version = run(
            &executable,
            script.as_deref(),
            &["--version"],
            PROBE_TIMEOUT,
        )
        .await?;
        let version = version.trim();
        if version.is_empty()
            || version.len() > 64
            || !version.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            return Err(OpenCliError::InvalidOutput);
        }
        let commands = run(
            &executable,
            script.as_deref(),
            &["list", "-f", "json"],
            PROBE_TIMEOUT,
        )
        .await?;
        let registry = Registry::parse(&commands)?;
        Ok(Self {
            executable,
            script,
            version: version.to_owned(),
            registry,
        })
    }

    #[must_use]
    pub fn inventory(&self) -> OpenCliInventory {
        self.registry.inventory(&self.version)
    }

    /// The pinned command entrypoint, for local installation diagnostics only.
    /// Windows runs this script through Node rather than through an npm shim.
    #[must_use]
    pub fn command_path(&self) -> &std::path::Path {
        self.script.as_deref().unwrap_or(&self.executable)
    }

    /// Only mapped commands with compatible local runtime and exact bundled
    /// adapters. Availability does not prove login or successful provider reads.
    pub async fn ecommerce_search_capabilities(&self) -> Vec<CreatorSearchCapability> {
        if self.version != VERIFIED_VERSION {
            return Vec::new();
        }
        creator_search::available_capabilities(
            creator_search::ecommerce_capabilities(&self.registry),
            &self.addon_status().await,
        )
    }

    /// List only currently connected extension identities. No browser page is read.
    pub async fn connected_profiles(&self) -> Result<Vec<OpenCliProfile>, OpenCliError> {
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &["profile", "list"],
            PROBE_TIMEOUT,
        )
        .await?;
        Ok(parse_connected_profiles(&output))
    }

    /// `daemon status` reports state without a browser connectivity probe.
    pub async fn bridge_status(&self) -> Result<OpenCliBridgeStatus, OpenCliError> {
        let output = run(
            &self.executable,
            self.script.as_deref(),
            &["daemon", "status"],
            PROBE_TIMEOUT,
        )
        .await?;
        parse_bridge_status(&output)
    }

    /// Start a stopped local daemon and read back extension connectivity.
    /// A running daemon is never restarted; no site or browser identity is changed.
    pub async fn recover_bridge(&self) -> Result<OpenCliBridgeStatus, OpenCliError> {
        recovery::recover(self).await
    }

    async fn require_browser_read(
        &self,
        profile: &str,
        site: &str,
        name: &str,
    ) -> Result<(), OpenCliError> {
        if !self.registry.has_browser_read(site, name) {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        self.check_browser_profile(profile).await
    }

    /// Verify the exact extension identity independently of any site's commands.
    /// This checks the local bridge, not platform login or browsing permissions.
    pub async fn check_browser_profile(&self, profile: &str) -> Result<(), OpenCliError> {
        validate_profile(profile)?;
        self.require_version().await?;
        let status = self.bridge_status().await?;
        if !status.daemon_running {
            return Err(OpenCliError::DaemonUnavailable);
        }
        if !status.extension_connected {
            return Err(OpenCliError::ExtensionUnavailable);
        }
        let profiles = run(
            &self.executable,
            self.script.as_deref(),
            &["profile", "list"],
            PROBE_TIMEOUT,
        )
        .await?;
        if !profile_is_connected(&profiles, profile) {
            return Err(OpenCliError::ProfileUnavailable);
        }
        Ok(())
    }

    async fn require_version(&self) -> Result<(), OpenCliError> {
        let actual = run(
            &self.executable,
            self.script.as_deref(),
            &["--version"],
            PROBE_TIMEOUT,
        )
        .await?;
        if actual.trim() != self.version {
            return Err(OpenCliError::VersionChanged);
        }
        Ok(())
    }

    /// Read-only preflight for the exact profile and registered YouTube search
    /// command. It does not open a provider page or consume a Campaign call.
    pub async fn check_youtube_search_profile(&self, profile: &str) -> Result<(), OpenCliError> {
        validate_profile(profile)?;
        self.require_browser_read(profile, "youtube", "search")
            .await
    }

    /// Check one explicitly mapped discovery command in the installed registry.
    pub async fn check_creator_search_profile(
        &self,
        profile: &str,
        site: &str,
    ) -> Result<(), OpenCliError> {
        validate_profile(profile)?;
        if site == "tiktok" {
            return self.check_tiktok_search_profile(profile).await;
        }
        if site == "bilibili" {
            return self.check_bilibili_search_profile(profile).await;
        }
        let command = creator_search::command(site).ok_or(OpenCliError::ReadCommandUnavailable)?;
        let command_site = creator_search::command_site(site);
        if creator_search::is_public(site) {
            if !self.registry.has_public_read(command_site, command) {
                return Err(OpenCliError::ReadCommandUnavailable);
            }
            return self.require_version().await;
        }
        self.require_browser_read(profile, command_site, command)
            .await
    }

    /// Installed adapter limits, shared by manual reads and automatic planning.
    pub fn creator_search_max_limit(site: &str) -> Result<u8, OpenCliError> {
        if creator_search::command(site).is_none() {
            return Err(OpenCliError::ReadCommandUnavailable);
        }
        Ok(match site {
            "zhihu" | "pinterest" => 20,
            "linkedin" | "weixin" => 10,
            "douyin" => 30,
            "tieba" => 20,
            "v2ex-node" => 20,
            "devto-tag" | "lobsters-tag" => 20,
            "medium-tag" => 25,
            "hupu" | "pixiv" | "weread" => 20,
            "dribbble-shots" | "dribbble-designers" => 30,
            "linkedin-salesnav" => 25,
            // A single Pinterest result page was observed reliably at five;
            // larger bounded reads can fail during provider pagination.
            "pinterest-pins" | "pinterest-boards" => 5,
            "brave" => 18,
            "duckduckgo" => 10,
            "yahoo" => 7,
            _ => 50,
        })
    }

    pub fn check_creator_search_limit(site: &str, limit: u8) -> Result<(), OpenCliError> {
        let maximum = Self::creator_search_max_limit(site)?;
        if limit == 0 || limit > maximum {
            return Err(OpenCliError::InvalidInput("limit"));
        }
        Ok(())
    }

    /// A bounded, registry-gated read. The site and command are fixed by our
    /// adapter table; neither Campaign text nor callers can select a CLI verb.
    pub async fn creator_search(
        &self,
        profile: &str,
        site: &str,
        query: &str,
        limit: u8,
    ) -> Result<Vec<CreatorSearchHit>, OpenCliError> {
        if query.trim().is_empty()
            || query.chars().count() > 160
            || query.chars().any(char::is_control)
            || (site == "v2ex-node"
                && (query.len() > 64
                    || !query
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_alphanumeric)
                    || !query.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'-' | b'_')
                    })))
            || (site == "reddit-subreddit"
                && (query.len() > 64
                    || !query
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')))
            || (site == "juejin-hot"
                && !["backend", "frontend", "android", "ios", "ai"].contains(&query))
            || (site == "producthunt-posts"
                && ![
                    "ai-agents",
                    "ai-coding-agents",
                    "ai-code-editors",
                    "ai-chatbots",
                    "ai-workflow-automation",
                    "vibe-coding",
                    "developer-tools",
                    "productivity",
                    "design-creative",
                    "marketing-sales",
                    "no-code-platforms",
                    "llms",
                    "finance",
                    "social-community",
                    "engineering-development",
                ]
                .contains(&query))
        {
            return Err(OpenCliError::InvalidInput("query"));
        }
        Self::check_creator_search_limit(site, limit)?;
        if site == "bilibili" {
            return Ok(self
                .bilibili_search(profile, query, limit)
                .await?
                .into_iter()
                .map(CreatorSearchHit::from)
                .collect());
        }
        if site == "tiktok" {
            return Ok(self
                .tiktok_search(profile, query, limit)
                .await?
                .into_iter()
                .map(CreatorSearchHit::from)
                .collect());
        }
        let bundled = creator_search::bundled_search_addon(site);
        if let Some(id) = bundled {
            addons::require_ready(id).await?;
        }
        self.check_creator_search_profile(profile, site).await?;
        let args = creator_search::arguments(profile, site, query, limit);
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = match run(
            &self.executable,
            self.script.as_deref(),
            &args,
            READ_TIMEOUT,
        )
        .await
        {
            Ok(output) => output,
            // Browser adapters can report EMPTY_RESULT for blocked or changed
            // pages. Their exit code cannot certify a genuine zero-result search.
            Err(OpenCliError::CommandFailed(Some(66)))
                if !creator_search::ambiguous_empty_result(site) =>
            {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        if let Some(id) = bundled {
            addons::require_ready(id).await?;
        }
        creator_search::parse(site, &output, limit)
    }

    async fn youtube_read(
        &self,
        profile: &str,
        name: &str,
        subject: &str,
        limit: Option<u8>,
    ) -> Result<String, OpenCliError> {
        validate_profile(profile)?;
        self.require_browser_read(profile, "youtube", name).await?;
        let mut args = vec!["--profile", profile, "youtube", name];
        let limit_string = limit.map(|value| value.to_string());
        if let Some(value) = limit_string.as_deref() {
            args.extend(["--limit", value]);
        }
        args.extend(["-f", "json", "--", subject]);
        run(
            &self.executable,
            self.script.as_deref(),
            &args,
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn youtube_search(
        &self,
        profile: &str,
        query: &str,
        limit: u8,
    ) -> Result<Vec<YoutubeSearchHit>, OpenCliError> {
        if query.trim().is_empty()
            || query.chars().count() > 160
            || query.chars().any(char::is_control)
        {
            return Err(OpenCliError::InvalidInput("query"));
        }
        if !(1..=50).contains(&limit) {
            return Err(OpenCliError::InvalidInput("limit"));
        }
        let output = match self
            .youtube_read(profile, "search", query, Some(limit))
            .await
        {
            Ok(output) => output,
            // OpenCLI's documented EX_NOINPUT is an empty search, not a stop.
            Err(OpenCliError::CommandFailed(Some(66))) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        youtube::parse_search(&output)
    }

    pub async fn youtube_channel(
        &self,
        profile: &str,
        channel: &str,
    ) -> Result<YoutubeChannel, OpenCliError> {
        youtube::validate_channel(channel)?;
        let output = self
            .youtube_read(profile, "channel", channel, Some(10))
            .await?;
        youtube::parse_channel(&output)
    }

    pub async fn youtube_video(
        &self,
        profile: &str,
        video: &str,
    ) -> Result<YoutubeVideo, OpenCliError> {
        let video_id = youtube::video_id(video)?;
        let output = self.youtube_read(profile, "video", &video_id, None).await?;
        youtube::parse_video(&output, &video_id)
    }

    pub async fn youtube_transcript(
        &self,
        profile: &str,
        video: &str,
    ) -> Result<Vec<YoutubeTranscriptLine>, OpenCliError> {
        let video_id = youtube::video_id(video)?;
        let output = match self
            .youtube_read(profile, "transcript", &video_id, None)
            .await
        {
            Ok(output) => output,
            // Official EX_NOINPUT means the video has no caption tracks.
            Err(OpenCliError::CommandFailed(Some(66))) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        youtube::parse_transcript(&output)
    }

    /// Read one observed public link. The runtime must first bind it to approved
    /// Campaign evidence and reserve budget; this is not a public arbitrary-URL tool.
    /// DNS preflight is defense in depth, not redirect or DNS-rebinding isolation.
    pub async fn read_public_web(
        &self,
        profile: &str,
        source_url: &str,
    ) -> Result<PublicWebRead, OpenCliError> {
        validate_profile(profile)?;
        validate_public_web_url(source_url)?;
        self.require_browser_read(profile, "web", "read").await?;
        web::check_public_destination(source_url).await?;
        let output = process::run_with_diagnostics(
            &self.executable,
            self.script.as_deref(),
            &[
                "--profile",
                profile,
                "web",
                "read",
                "--url",
                source_url,
                "--stdout",
                "--download-images",
                "false",
                "--frames",
                "none",
                "--diagnose",
                "--wait",
                "3",
            ],
            READ_TIMEOUT,
        )
        .await?;
        let read = web::parse(source_url, &output.stdout, &output.stderr)?;
        web::check_public_destination(&read.document_url).await?;
        Ok(read)
    }
}

fn validate_profile(profile: &str) -> Result<(), OpenCliError> {
    if profile.is_empty()
        || profile.len() > 64
        || !profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(OpenCliError::InvalidInput("profile"));
    }
    Ok(())
}

fn parse_bridge_status(output: &str) -> Result<OpenCliBridgeStatus, OpenCliError> {
    let daemon = output
        .lines()
        .find_map(|line| line.strip_prefix("Daemon: "));
    let extension = output
        .lines()
        .find_map(|line| line.strip_prefix("Extension: "));
    // OpenCLI returns early when stopped, without an Extension line.
    if daemon == Some("not running") && extension.is_none() {
        return Ok(OpenCliBridgeStatus {
            daemon_running: false,
            extension_connected: false,
        });
    }
    let (Some(daemon), Some(extension)) = (daemon, extension) else {
        return Err(OpenCliError::InvalidOutput);
    };
    let extension_count = extension
        .split_once(" profiles connected")
        .or_else(|| extension.split_once(" profile connected"))
        .and_then(|(count, _)| count.parse::<u16>().ok());
    Ok(OpenCliBridgeStatus {
        daemon_running: daemon.starts_with("running"),
        extension_connected: extension.starts_with("connected")
            || extension_count.is_some_and(|count| count > 0),
    })
}

fn profile_is_connected(output: &str, selected: &str) -> bool {
    parse_connected_profiles(output)
        .iter()
        .any(|profile| profile.context_id == selected || profile.alias.as_deref() == Some(selected))
}

fn parse_connected_profiles(output: &str) -> Vec<OpenCliProfile> {
    output
        .lines()
        .filter_map(|line| {
            let names = line.strip_prefix("  ")?.split_once(" — connected")?.0;
            let mut parts = names.split_whitespace();
            let context_id = parts.next()?;
            validate_profile(context_id).ok()?;
            let alias = parts.next().filter(|value| *value != "default");
            if alias.is_some_and(|value| validate_profile(value).is_err()) {
                return None;
            }
            Some(OpenCliProfile {
                context_id: context_id.to_owned(),
                alias: alias.map(str::to_owned),
            })
        })
        .collect()
}

#[cfg(windows)]
fn windows_installation(
    path_directories: &[PathBuf],
    app_roots: &[PathBuf],
) -> Option<(PathBuf, PathBuf)> {
    // Only fixed layouts are accepted. Never execute an app-managed .cmd shim:
    // provider search terms must remain argv, not command-interpreter input.
    let path_node = path_directories
        .iter()
        .map(|directory| directory.join("node.exe"))
        .find(|path| path.is_file());
    for root in app_roots {
        for resources in [root.clone(), root.join("resources")] {
            let script = resources.join("node_modules/@jackwener/opencli/dist/src/main.js");
            if !script.is_file() {
                continue;
            }
            let node = [
                resources.join("node.exe"),
                resources.join("node_modules/node/bin/node.exe"),
                root.join("node.exe"),
            ]
            .into_iter()
            .find(|path| path.is_file())
            .or_else(|| path_node.clone());
            if let Some(node) = node {
                return Some((node, script));
            }
        }
    }
    for directory in path_directories {
        if !directory.join("opencli.cmd").is_file() {
            continue;
        }
        let script = directory.join("node_modules/@jackwener/opencli/dist/src/main.js");
        if script.is_file()
            && let Some(node) = path_node.clone()
        {
            return Some((node, script));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_status_accepts_stopped_daemon_without_extension_line() {
        let status = parse_bridge_status("Daemon: not running\n").expect("stopped daemon status");
        assert!(!status.daemon_running);
        assert!(!status.extension_connected);
    }

    #[test]
    fn daemon_status_distinguishes_disconnected_extension() {
        let status = parse_bridge_status("Daemon: running (PID 123)\nExtension: disconnected\n")
            .expect("valid status");
        assert!(status.daemon_running);
        assert!(!status.extension_connected);
        let multiple = parse_bridge_status(
            "Daemon: running (PID 123)\nExtension: 2 profiles connected, none selected — run `opencli profile use <name>`\n",
        )
        .expect("multiple connected profiles");
        assert!(multiple.extension_connected);
        assert_eq!(
            parse_bridge_status("Daemon: running\n"),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn stale_daemon_is_not_reported_as_ready_even_when_extension_is_connected() {
        let status = parse_bridge_status(
            "Daemon: stale (PID 123)\nVersion: v1.8.6 (CLI v1.8.8; run: opencli daemon restart)\nExtension: connected (v1.0.24)\n",
        ).unwrap();
        assert!(!status.daemon_running);
        assert!(status.extension_connected);
    }

    #[test]
    fn profile_is_one_explicit_alias() {
        assert_eq!(validate_profile("dedicated.1"), Ok(()));
        assert_eq!(
            validate_profile(""),
            Err(OpenCliError::InvalidInput("profile"))
        );
        assert_eq!(
            validate_profile("../Default"),
            Err(OpenCliError::InvalidInput("profile"))
        );
    }

    #[test]
    fn selected_profile_must_be_connected() {
        let listed = "Connected Browser Bridge profiles\n\n  abc work default — connected v1.0.24\n\nDisconnected saved profiles:\n  old stale — not connected\n";
        assert!(profile_is_connected(listed, "abc"));
        assert!(profile_is_connected(listed, "work"));
        assert!(!profile_is_connected(listed, "old"));
        assert!(!profile_is_connected(listed, "other"));
        assert_eq!(
            parse_connected_profiles(listed),
            vec![OpenCliProfile {
                context_id: "abc".into(),
                alias: Some("work".into()),
            }]
        );
        assert_eq!(
            parse_connected_profiles(
                "Connected Browser Bridge profiles\n\n  jazks96e — connected v1.0.24\n  8pqz35v6 — connected v1.0.24\n"
            ),
            vec![
                OpenCliProfile {
                    context_id: "jazks96e".into(),
                    alias: None
                },
                OpenCliProfile {
                    context_id: "8pqz35v6".into(),
                    alias: None
                },
            ]
        );
    }

    #[test]
    fn creator_search_limits_are_checked_before_campaign_spend() {
        assert_eq!(OpenCli::check_creator_search_limit("pinterest", 20), Ok(()));
        assert_eq!(
            OpenCli::check_creator_search_limit("pinterest", 21),
            Err(OpenCliError::InvalidInput("limit"))
        );
        assert_eq!(OpenCli::check_creator_search_limit("linkedin", 10), Ok(()));
        assert_eq!(
            OpenCli::check_creator_search_limit("linkedin", 11),
            Err(OpenCliError::InvalidInput("limit"))
        );
        assert_eq!(
            OpenCli::check_creator_search_limit("douyin", 31),
            Err(OpenCliError::InvalidInput("limit"))
        );
        for site in ["pinterest-pins", "v2ex-node"] {
            assert_eq!(
                OpenCli::check_creator_search_limit(site, 5),
                Err(OpenCliError::ReadCommandUnavailable)
            );
        }
        assert_eq!(
            OpenCli::check_creator_search_limit("unknown", 1),
            Err(OpenCliError::ReadCommandUnavailable)
        );
    }

    #[cfg(windows)]
    #[test]
    fn official_app_layout_is_discovered_without_executing_its_shim() {
        let directory = tempfile::tempdir().unwrap();
        let app = directory.path().join("OpenCLIApp");
        let script = app.join("node_modules/@jackwener/opencli/dist/src/main.js");
        let node = app.join("node.exe");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, b"").unwrap();
        std::fs::write(&node, b"").unwrap();
        assert_eq!(windows_installation(&[], &[app]), Some((node, script)));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn batch_shims_are_not_executable_adapters() {
        assert!(matches!(
            OpenCli::inspect(PathBuf::from("opencli.cmd")).await,
            Err(OpenCliError::InvalidInput("executable"))
        ));
    }

    #[tokio::test]
    #[ignore = "requires the separately installed OpenCLI executable"]
    async fn installed_opencli_inventory_is_readable() {
        let bridge = OpenCli::inspect_system()
            .await
            .expect("installed OpenCLI registry");
        let inventory = bridge.inventory();
        assert!(inventory.command_count > 0);
        assert!(inventory.sites.iter().any(|site| site.site == "youtube"));
        assert!(
            bridge
                .bridge_status()
                .await
                .expect("daemon status")
                .extension_connected
        );
    }

    #[tokio::test]
    #[ignore = "requires Node.js; local fake bridge only, no platform reads"]
    async fn browser_connection_does_not_require_youtube_or_read_a_site() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-connect.cjs");
        std::fs::write(&script, r#"
const args = process.argv.slice(2).join(' ');
if (args === '--version') console.log('1.8.8');
else if (args === 'list -f json') console.log(JSON.stringify([
  {command:'instagram/search',site:'instagram',name:'search',access:'read',browser:true}
]));
else if (args === 'daemon status') console.log('Daemon: running\nExtension: connected\n');
else if (args === 'profile list') console.log('Connected Browser Bridge profiles\n\n  dedicated — connected v1.0.24\n');
else process.exit(90);
"#).unwrap();
        let cli = OpenCli::inspect_with_script(PathBuf::from("node"), Some(script))
            .await
            .unwrap();
        assert_eq!(cli.check_browser_profile("dedicated").await, Ok(()));
        assert_eq!(
            cli.check_browser_profile("other").await,
            Err(OpenCliError::ProfileUnavailable)
        );
        assert_eq!(
            cli.check_youtube_search_profile("dedicated").await,
            Err(OpenCliError::ReadCommandUnavailable)
        );
        assert_eq!(
            cli.check_creator_search_profile("dedicated", "instagram")
                .await,
            Ok(())
        );
    }

    #[tokio::test]
    #[ignore = "requires installed OpenCLI and OPENCLI_TEST_PROFILE; no site read"]
    async fn ecommerce_connection_inventory_matches_installed_bridge() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated profile");
        let cli = OpenCli::inspect_system().await.expect("installed OpenCLI");
        assert_eq!(cli.inventory().version, "1.8.8");
        cli.check_browser_profile(&profile)
            .await
            .expect("selected bridge identity");
        let capabilities = cli.ecommerce_search_capabilities().await;
        assert_eq!(capabilities.len(), 52);
        assert!(capabilities.iter().all(|item| item.registered));
        println!(
            "local connection verified; ecommerce search entries={} (not site-login proof)",
            capabilities.len()
        );
    }

    #[tokio::test]
    #[ignore = "requires the separately installed OpenCLI 1.8.8 registry"]
    async fn official_creator_search_commands_exist_in_installed_registry() {
        let bridge = OpenCli::inspect_system().await.expect("installed OpenCLI");
        assert_eq!(bridge.inventory().version, "1.8.8");
        for site in [
            "youtube",
            "twitter",
            "instagram",
            "weixin",
            "zhihu",
            "weibo",
            "douyin",
            "facebook",
            "reddit",
            "linkedin",
            "pinterest",
            "bluesky",
            "medium",
            "substack",
            "apple-podcasts",
            "tieba",
            "jike",
            "hackernews",
            "linux-do",
            "nowcoder",
            "sinablog",
            "1point3acres",
            "hupu",
            "pixiv",
            "stackoverflow",
            "pinterest-pins",
            "pinterest-boards",
            "linkedin-salesnav",
            "linkedin-learning",
            "archive",
            "weread",
            "google",
            "brave",
            "duckduckgo",
            "yahoo",
            "dribbble-shots",
            "dribbble-designers",
            "google-images",
            "stackoverflow-users",
            "douban-books",
            "douban-music",
        ] {
            if creator_search::RETIRED_CREATOR_SOURCES.contains(&site) {
                continue;
            }
            let command = creator_search::command(site).expect("mapped site");
            let command_site = creator_search::command_site(site);
            assert!(
                if creator_search::is_public(site) {
                    bridge.registry.has_public_read(command_site, command)
                } else {
                    bridge.registry.has_browser_read(command_site, command)
                },
                "{command_site}/{command}"
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires connected Chrome extension and OPENCLI_TEST_PROFILE"]
    async fn youtube_read_contract_matches_connected_extension() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile alias");
        let bridge = OpenCli::inspect_system()
            .await
            .expect("installed OpenCLI registry");
        let hits = bridge
            .youtube_search(&profile, "AI tool demo", 2)
            .await
            .expect("YouTube search");
        assert!(hits.len() <= 2);
        let video = bridge
            .youtube_video(&profile, "LR04bU_yV5k")
            .await
            .expect("YouTube video");
        assert_eq!(video.video_id, "LR04bU_yV5k");
        let channel = bridge
            .youtube_channel(&profile, &video.channel_id)
            .await
            .expect("YouTube channel");
        assert_eq!(channel.channel_id, video.channel_id);
    }

    #[tokio::test]
    #[ignore = "requires Node.js; fake CLI verifies subprocess routing without network or Chrome"]
    async fn public_search_skips_browser_and_ambiguous_empty_results_stop() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-opencli.cjs");
        std::fs::write(&script, r#"
const args = process.argv.slice(2);
if (args.join(' ') === '--version') console.log('1.8.8');
else if (args.join(' ') === 'list -f json') console.log(JSON.stringify([
  {command:'bluesky/search', site:'bluesky', name:'search', access:'read', browser:false, strategy:'public'},
  {command:'youtube/search', site:'youtube', name:'search', access:'read', browser:true, strategy:'cookie'},
  {command:'onecrew-search-tieba/search', site:'onecrew-search-tieba', name:'search', access:'read', browser:true, strategy:'cookie'},
  {command:'onecrew-search-facebook/search', site:'onecrew-search-facebook', name:'search', access:'read', browser:true, strategy:'cookie'}
]));
else if (args.join(' ') === 'daemon status') console.log('Daemon: running\nExtension: connected');
else if (args.join(' ') === 'profile list') console.log('  frozen-profile — connected v1.0.24');
else if (JSON.stringify(args) === JSON.stringify(['bluesky','search','--limit','2','-f','json','--','--profile=other']))
  console.log(JSON.stringify([{handle:'creator.bsky.social',name:'Creator'}]));
else if (JSON.stringify(args) === JSON.stringify(['--profile','frozen-profile','onecrew-search-tieba','search','--limit','2','-f','json','--','AI tools']))
  process.exit(66);
else if (JSON.stringify(args) === JSON.stringify(['--profile','frozen-profile','onecrew-search-facebook','search','--limit','2','-f','json','--','AI tools']))
  process.exit(66);
else process.exit(90);
"#).unwrap();
        let bridge = OpenCli::inspect_with_script(PathBuf::from("node"), Some(script.clone()))
            .await
            .unwrap();
        for invalid in ["AI tools", "-python", "Python", "python/ai"] {
            assert_eq!(
                bridge
                    .creator_search("frozen-profile", "v2ex-node", invalid, 2)
                    .await,
                Err(OpenCliError::InvalidInput("query"))
            );
        }
        for invalid in ["AI tools", "r/Python", "python/ai"] {
            assert_eq!(
                bridge
                    .creator_search("frozen-profile", "reddit-subreddit", invalid, 2)
                    .await,
                Err(OpenCliError::InvalidInput("query"))
            );
        }
        for (site, invalid) in [
            ("juejin-hot", "marketing-sales"),
            ("producthunt-posts", "ai"),
        ] {
            assert_eq!(
                bridge
                    .creator_search("frozen-profile", site, invalid, 2)
                    .await,
                Err(OpenCliError::InvalidInput("query"))
            );
        }
        let hits = bridge
            .creator_search("frozen-profile", "bluesky", "--profile=other", 2)
            .await
            .unwrap();
        assert_eq!(
            hits[0].source_url,
            "https://bsky.app/profile/creator.bsky.social"
        );
        assert_eq!(
            bridge
                .creator_search("frozen-profile", "youtube", "AI tools", 2)
                .await,
            Err(OpenCliError::CommandFailed(Some(90)))
        );
        assert_eq!(
            bridge
                .creator_search("frozen-profile", "tieba", "AI tools", 2)
                .await,
            Err(OpenCliError::CommandFailed(Some(66)))
        );
        assert_eq!(
            bridge
                .creator_search("frozen-profile", "facebook", "AI tools", 2)
                .await,
            Err(OpenCliError::CommandFailed(Some(66)))
        );
        std::fs::write(&script, "console.log('1.8.9');").unwrap();
        assert_eq!(
            bridge
                .creator_search("frozen-profile", "bluesky", "AI tools", 2)
                .await,
            Err(OpenCliError::VersionChanged)
        );
    }

    #[tokio::test]
    #[ignore = "reads live OpenCLI search; requires OPENCLI_TEST_SITE, OPENCLI_TEST_PROFILE and OPENCLI_TEST_QUERY"]
    async fn creator_search_contract_matches_installed_provider() {
        let site = std::env::var("OPENCLI_TEST_SITE").expect("explicit site");
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile scope");
        let query = std::env::var("OPENCLI_TEST_QUERY").expect("explicit search query");
        let bridge = OpenCli::inspect_system().await.expect("installed OpenCLI");
        assert_eq!(bridge.inventory().version, "1.8.8");
        let hits = bridge
            .creator_search(&profile, &site, &query, 5)
            .await
            .expect("live search");
        assert!(
            !hits.is_empty(),
            "live acceptance requires actual leads, not only successful exit"
        );
        println!(
            "site={site} returned_leads={} first_source={}",
            hits.len(),
            hits[0].source_url
        );
        if matches!(site.as_str(), "bilibili" | "tiktok" | "twitter") {
            for hit in &hits {
                assert!(hit.author_id.as_ref().is_some_and(|id| !id.is_empty()));
                assert!(
                    hit.author_url
                        .as_ref()
                        .is_some_and(|url| url.starts_with("https://"))
                );
            }
            println!("source-bound author identities retained={}", hits.len());
        }
    }

    #[tokio::test]
    #[ignore = "requires installed OpenCLI, dedicated profile and an explicit public test URL"]
    async fn public_web_read_contract_matches_connected_extension() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile");
        let url = std::env::var("OPENCLI_TEST_PUBLIC_URL").expect("explicit public URL");
        let bridge = OpenCli::inspect_system().await.unwrap();
        assert_eq!(bridge.inventory().version, "1.8.8");
        let read = bridge.read_public_web(&profile, &url).await.unwrap();
        assert_eq!(read.requested_url, url);
        assert!(!read.markdown_excerpt.trim().is_empty());
        assert!(read.markdown_excerpt.len() <= 20_000);
        println!(
            "public_web document_url={} excerpt_bytes={} truncated={}",
            read.document_url,
            read.markdown_excerpt.len(),
            read.truncated
        );
    }

    #[tokio::test]
    #[ignore = "requires Node.js and public DNS; fake CLI verifies fixed read-only options"]
    async fn public_web_process_disables_exports_and_retains_only_typed_diagnostics() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-web.cjs");
        std::fs::write(&script, r#"
const args = process.argv.slice(2);
if (args.join(' ') === '--version') console.log('1.8.8');
else if (args.join(' ') === 'list -f json') console.log(JSON.stringify([
  {command:'web/read',site:'web',name:'read',access:'read',browser:true,strategy:'cookie'}
]));
else if (args.join(' ') === 'daemon status') console.log('Daemon: running\nExtension: connected');
else if (args.join(' ') === 'profile list') console.log('  frozen — connected v1.0.24');
else if (JSON.stringify(args) === JSON.stringify(['--profile','frozen','web','read','--url','https://example.com/','--stdout','--download-images','false','--frames','none','--diagnose','--wait','3'])) {
  console.log('# Tool\n> 原文链接: https://example.com/\n\n---\n\nAn observed public tool page.');
  console.error('[web-read diagnose]\nurl: https://example.com/tool\nframes: 0\nnetwork_capture: enabled\n  synthetic-private-diagnostic');
} else process.exit(90);
"#).unwrap();
        let bridge = OpenCli::inspect_with_script(PathBuf::from("node"), Some(script))
            .await
            .unwrap();
        assert_eq!(
            bridge.read_public_web("frozen", "https://127.0.0.1/").await,
            Err(OpenCliError::InvalidInput("public_url"))
        );
        assert_eq!(
            bridge
                .read_public_web("other", "https://example.com/")
                .await,
            Err(OpenCliError::ProfileUnavailable)
        );
        let read = bridge
            .read_public_web("frozen", "https://example.com/")
            .await
            .unwrap();
        assert_eq!(read.document_url, "https://example.com/tool");
        assert!(
            !serde_json::to_string(&read)
                .unwrap()
                .contains("synthetic-private-diagnostic")
        );
    }

    #[tokio::test]
    #[ignore = "requires Node.js; fake CLI verifies video identity without network or Chrome"]
    async fn youtube_read_process_keeps_profile_and_canonical_video_identity() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-opencli.cjs");
        std::fs::write(&script, r#"
const args = process.argv.slice(2);
if (args.join(' ') === '--version') console.log('1.8.8');
else if (args.join(' ') === 'list -f json') console.log(JSON.stringify([
  {command:'youtube/video', site:'youtube', name:'video', access:'read', browser:true, strategy:'cookie'},
  {command:'youtube/transcript', site:'youtube', name:'transcript', access:'read', browser:true, strategy:'cookie'},
  {command:'youtube/search', site:'youtube', name:'search', access:'read', browser:true, strategy:'cookie'}
]));
else if (args.join(' ') === 'daemon status') console.log('Daemon: running\nExtension: connected');
else if (args.join(' ') === 'profile list') console.log('Connected Browser Bridge profiles\n\n  frozen — connected v1.0.24\n');
else if (JSON.stringify(args) === JSON.stringify(['--profile','frozen','youtube','video','-f','json','--','dQw4w9WgXcQ']))
  console.log(JSON.stringify([
    {field:'videoId',value:'LR04bU_yV5k'}, {field:'channelId',value:'UC1234567890'},
    {field:'title',value:'Unrelated video'}, {field:'channel',value:'Unrelated creator'}
  ]));
else if (JSON.stringify(args) === JSON.stringify(['--profile','frozen','youtube','search','--limit','2','-f','json','--','--profile=other']))
  console.log('[]');
else if (JSON.stringify(args) === JSON.stringify(['--profile','frozen','youtube','transcript','-f','json','--','dQw4w9WgXcQ'])) process.exit(66);
else if (JSON.stringify(args) === JSON.stringify(['--profile','frozen','youtube','transcript','-f','json','--','LR04bU_yV5k'])) process.exit(1);
else process.exit(90);
"#).unwrap();
        let bridge = OpenCli::inspect_with_script(PathBuf::from("node"), Some(script))
            .await
            .unwrap();
        assert_eq!(
            bridge
                .youtube_video("frozen", "https://youtube.com/watch?v=dQw4w9WgXcQ&t=30s")
                .await,
            Err(OpenCliError::InvalidOutput)
        );
        assert!(
            bridge
                .youtube_search("frozen", "--profile=other", 2)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            bridge
                .youtube_transcript("frozen", "dQw4w9WgXcQ")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            bridge.youtube_transcript("frozen", "LR04bU_yV5k").await,
            Err(OpenCliError::CommandFailed(Some(1)))
        );
    }

    #[tokio::test]
    #[ignore = "requires a connected Chrome extension and reliable YouTube transcript navigation"]
    async fn youtube_transcript_contract_matches_connected_extension() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit profile alias");
        let bridge = OpenCli::inspect_system()
            .await
            .expect("installed OpenCLI registry");
        let transcript = bridge
            .youtube_transcript(&profile, "LR04bU_yV5k")
            .await
            .expect("YouTube transcript");
        assert!(!transcript.is_empty());
    }
}
