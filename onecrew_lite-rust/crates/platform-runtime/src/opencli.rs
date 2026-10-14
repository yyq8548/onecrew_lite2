use onecrew_creator_runtime::{
    DiscoveryCampaign, DiscoveryCandidateCounts, DiscoveryContentRead,
    DiscoveryIntentValidationResult, DiscoveryProfileRead, DiscoverySearchHitInput, DiscoveryStep,
    DiscoveryTranscriptRead, DiscoveryTranscriptSegment, FinishDiscoveryContentInput,
    FinishDiscoveryProfileInput, FinishDiscoverySearchInput, FinishDiscoveryTranscriptInput,
    FinishDiscoveryVideoInput, UpsertDiscoveryCandidateInput, ValidateDiscoveryIntentInput,
    validate_discovery_intent_proposal,
};
pub use onecrew_opencli::{
    BilibiliPublicProfile, OpenCliAddonStatus, OpenCliAuthObservation, TiktokPublicProfile,
};
use onecrew_opencli::{
    CreatorSearchCapability, OpenCli, OpenCliBridgeStatus, OpenCliError, OpenCliInventory,
    OpenCliProfile,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::LocalPlatform;
pub use onecrew_chrome::ChromeSetupPage;

mod facebook_identity;
mod identity;
mod source_author;
mod web_sources;
pub use source_author::OpenCliSourceAuthorTimings;
#[cfg(test)]
mod profile_tests;
mod public_pages;
#[cfg(test)]
mod search_completion_tests;

const VERIFIED_OPENCLI_VERSION: &str = onecrew_opencli::VERIFIED_VERSION;

/// Installed dependency status. Registry presence is not provider authorization.
#[derive(Debug, Clone)]
pub struct OpenCliAvailability {
    pub inventory: OpenCliInventory,
    pub bridge: OpenCliBridgeStatus,
    pub profiles: Vec<OpenCliProfile>,
}

/// Secret-free setup projection for the human onboarding screen.
#[derive(Debug, Clone, Serialize)]
pub struct OpenCliSetupReport {
    pub installed: bool,
    pub version: Option<String>,
    pub required_version: &'static str,
    pub command_path: Option<String>,
    pub command_count: Option<usize>,
    pub daemon_running: bool,
    pub extension_connected: bool,
    pub profiles: Vec<OpenCliProfile>,
    pub search_capabilities: Vec<CreatorSearchCapability>,
    pub addons: Vec<OpenCliAddonStatus>,
    pub issue: Option<&'static str>,
}

/// Secret-free, live source inventory for one-sentence Campaign planning.
/// Registration is a local capability check, not proof of provider login.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OpenCliDiscoverySources {
    pub version: Option<String>,
    pub daemon_running: bool,
    pub extension_connected: bool,
    pub profiles: Vec<OpenCliDiscoveryProfile>,
    pub sources: Vec<OpenCliDiscoverySource>,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OpenCliDiscoveryProfile {
    pub context_id: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OpenCliDiscoverySource {
    pub platform: String,
    pub command: String,
    pub browser_required: bool,
    pub registered: bool,
    pub scope_kind: String,
    pub allowed_values: Vec<String>,
}

fn discovery_scope_shape(platform: &str) -> (&'static str, &'static [&'static str]) {
    match platform {
        "v2ex-node" => ("node", &[]),
        "reddit-subreddit" => ("community", &[]),
        "devto-tag" | "lesswrong-tag" | "lobsters-tag" | "medium-tag" | "stackoverflow-tag" => {
            ("tag", &[])
        }
        "juejin-hot" => ("category", &["backend", "frontend", "android", "ios", "ai"]),
        "producthunt-posts" => (
            "category",
            &[
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
            ],
        ),
        _ => ("query", &[]),
    }
}

impl From<OpenCliSetupReport> for OpenCliDiscoverySources {
    fn from(report: OpenCliSetupReport) -> Self {
        Self {
            version: report.version,
            daemon_running: report.daemon_running,
            extension_connected: report.extension_connected,
            profiles: report
                .profiles
                .into_iter()
                .map(|profile| OpenCliDiscoveryProfile {
                    context_id: profile.context_id,
                    alias: profile.alias,
                })
                .collect(),
            sources: report
                .search_capabilities
                .into_iter()
                .map(|source| {
                    let (scope_kind, allowed_values) = discovery_scope_shape(&source.platform);
                    OpenCliDiscoverySource {
                        platform: source.platform,
                        command: source.command,
                        browser_required: source.browser_required,
                        registered: source.registered,
                        scope_kind: scope_kind.to_owned(),
                        allowed_values: allowed_values
                            .iter()
                            .map(|value| (*value).to_owned())
                            .collect(),
                    }
                })
                .collect(),
            issue: report.issue.map(str::to_owned),
        }
    }
}

fn setup_issue(error: &OpenCliError) -> &'static str {
    match error {
        OpenCliError::Unavailable => "not_installed",
        OpenCliError::Timeout => "timeout",
        OpenCliError::OutputTooLarge => "output_too_large",
        OpenCliError::CommandFailed(_)
        | OpenCliError::RiskControl
        | OpenCliError::InvalidOutput => "check_failed",
        OpenCliError::AuthRequired => "auth_required",
        OpenCliError::VersionChanged => "version_changed",
        OpenCliError::DaemonUnavailable => "daemon_unavailable",
        OpenCliError::ExtensionUnavailable => "extension_unavailable",
        OpenCliError::ProfileUnavailable => "profile_unavailable",
        OpenCliError::ReadCommandUnavailable => "read_command_unavailable",
        OpenCliError::InvalidInput(_) => "invalid_profile",
    }
}

#[derive(Debug)]
pub enum OpenCliDiscoverySearchError {
    Dependency(OpenCliError),
    Campaign(String),
    Provider(OpenCliError),
    /// Context of an attempted advance; contains identifiers, never page output.
    Advance {
        step: Box<DiscoveryStep>,
        failure: Box<OpenCliDiscoverySearchError>,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct OpenCliDiscoverySearchResult {
    pub campaign: DiscoveryCampaign,
    pub returned_hits: usize,
    /// Exact reserved query and call, without another provider read.
    pub search_claim: onecrew_creator_runtime::DiscoverySearchClaim,
    /// This call's bounded provider order, before cross-query source deduplication.
    /// Only expose rows after the owner has accepted the entire completion.
    pub returned_sources: Vec<DiscoverySearchHitInput>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct OpenCliDiscoverySearchInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub profile_ref: String,
    pub limit: u8,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct OpenCliDiscoveryPlatformSearchInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub platform: String,
    pub profile_ref: String,
    pub limit: u8,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct OpenCliDiscoveryVideoInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub profile_ref: String,
    pub search_hit_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenCliDiscoveryAdvanceInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    /// Customer-side negative login checks narrow authority; never add a scope.
    #[serde(default)]
    pub excluded_platforms: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct OpenCliDiscoveryAdvanceResult {
    pub step: DiscoveryStep,
    pub campaign: DiscoveryCampaign,
    pub counts: DiscoveryCandidateCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_stop_code: Option<String>,
    pub transcript: Option<DiscoveryTranscriptRead>,
    pub profile: Option<DiscoveryProfileRead>,
    pub content: Option<DiscoveryContentRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_targets: Option<onecrew_creator_runtime::DiscoveryFacebookTargetsRead>,
    pub identity: Option<onecrew_creator_runtime::DiscoveryIdentityRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facebook_identity: Option<onecrew_creator_runtime::DiscoveryFacebookIdentityRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_author: Option<onecrew_creator_runtime::DiscoverySourceAuthorRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_source: Option<onecrew_creator_runtime::DiscoveryWebSourceRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_author_timings: Option<OpenCliSourceAuthorTimings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_page: Option<onecrew_creator_runtime::DiscoveryPublicPageRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_lookup: Option<onecrew_creator_runtime::DiscoveryContactLookup>,
}

impl LocalPlatform {
    /// Execute at most one approved provider step. Callers may continue only
    /// after reading back this result. Closing a UI never aborts a claimed read.
    pub async fn advance_discovery_campaign(
        &self,
        input: OpenCliDiscoveryAdvanceInput,
    ) -> Result<OpenCliDiscoveryAdvanceResult, OpenCliDiscoverySearchError> {
        self.advance_discovery_campaign_deferred(input, vec![])
            .await
    }

    pub async fn advance_discovery_campaign_deferred(
        &self,
        input: OpenCliDiscoveryAdvanceInput,
        deferred_platforms: Vec<String>,
    ) -> Result<OpenCliDiscoveryAdvanceResult, OpenCliDiscoverySearchError> {
        self.advance_discovery_campaign_scoring(input, deferred_platforms, vec![])
            .await
    }

    pub async fn advance_discovery_campaign_scoring(
        &self,
        mut input: OpenCliDiscoveryAdvanceInput,
        deferred_platforms: Vec<String>,
        scoring_hits: Vec<String>,
    ) -> Result<OpenCliDiscoveryAdvanceResult, OpenCliDiscoverySearchError> {
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        for platform in onecrew_opencli::RETIRED_CREATOR_SOURCES {
            if !input
                .excluded_platforms
                .iter()
                .any(|excluded| excluded == platform)
            {
                input.excluded_platforms.push((*platform).into());
            }
        }
        let step = creators
            .plan_discovery_step_scoring_async(
                input.campaign_id.clone(),
                input.expected_revision,
                input.excluded_platforms,
                deferred_platforms,
                scoring_hits,
            )
            .await
            .map_err(|error| OpenCliDiscoverySearchError::Campaign(error.to_string()))?;
        let mut transcript = None;
        let mut profile = None;
        let mut content = None;
        let mut content_targets = None;
        let mut identity = None;
        let mut facebook_identity = None;
        let mut source_author = None;
        let mut web_source = None;
        let mut source_author_timings = None;
        let mut public_page = None;
        let mut contact_lookup = None;
        let mut saved_stop = None;
        let result = async {
            match &step {
                DiscoveryStep::ReadContactLookup { lookup } => self
                    .read_discovery_contact_lookup_once(lookup.clone())
                    .await
                    .map(|read| contact_lookup = Some(read)),
                DiscoveryStep::ReadPublicPage {
                    profile_ref,
                    candidate_id,
                    requested_url,
                } => self
                    .read_discovery_public_page_once(
                        onecrew_creator_runtime::DiscoveryPublicPageInput {
                            campaign_id: input.campaign_id.clone(),
                            expected_revision: input.expected_revision,
                            profile_ref: profile_ref.clone(),
                            candidate_id: candidate_id.clone(),
                            requested_url: requested_url.clone(),
                        },
                    )
                    .await
                    .map(|read| public_page = Some(read)),
                DiscoveryStep::ReadWebSource { search_hit_id } => self
                    .read_discovery_web_source_once(
                        &input.campaign_id,
                        input.expected_revision,
                        search_hit_id,
                    )
                    .await
                    .map(|read| web_source = Some(read)),
                DiscoveryStep::ReadFacebookIdentity { search_hit_id } => self
                    .read_discovery_facebook_identity_once(
                        &input.campaign_id,
                        input.expected_revision,
                        search_hit_id,
                    )
                    .await
                    .map(|read| facebook_identity = Some(read)),
                DiscoveryStep::ReadSourceAuthor { search_hit_id } => self
                    .read_discovery_source_author_once(
                        &input.campaign_id,
                        input.expected_revision,
                        search_hit_id,
                    )
                    .await
                    .map(|(read, timings)| {
                        source_author = Some(read);
                        source_author_timings = Some(timings);
                    }),
                DiscoveryStep::ReadIdentity { search_hit_id } => self
                    .read_discovery_identity_once(
                        &input.campaign_id,
                        input.expected_revision,
                        search_hit_id,
                    )
                    .await
                    .map(|read| identity = Some(read)),
                DiscoveryStep::ReadContentTargets { search_hit_id } => {
                    let cli = OpenCli::inspect_system()
                        .await
                        .map_err(OpenCliDiscoverySearchError::Dependency)?;
                    let approved = creators
                        .get_discovery_campaign_async(input.campaign_id.clone())
                        .await
                        .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
                    if approved.opencli_version != cli.inventory().version {
                        return Err(OpenCliDiscoverySearchError::Dependency(
                            OpenCliError::VersionChanged,
                        ));
                    }
                    let claim = creators
                        .claim_discovery_facebook_targets_async(
                            input.campaign_id.clone(),
                            input.expected_revision,
                            search_hit_id.clone(),
                        )
                        .await
                        .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
                    let result = if !claim
                        .source_url
                        .starts_with("https://www.facebook.com/profile.php?")
                    {
                        cli.facebook_native_targets(
                            &claim.profile_ref,
                            &claim.author_id,
                            &claim.source_url,
                        )
                        .await
                    } else {
                        cli.facebook_recent_language_urls(&claim.profile_ref, &claim.author_id)
                            .await
                    };
                    let (urls, stop) = match result {
                        Ok(urls) => (Some(urls), None),
                        Err(error) => (None, Some(provider_stop_code(&error).into())),
                    };
                    creators
                        .finish_discovery_facebook_targets_async(claim, urls, stop)
                        .await
                        .map(|read| content_targets = Some(read))
                        .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
                }
                DiscoveryStep::ReadContent { search_hit_id } => self
                    .read_discovery_content_once(
                        &input.campaign_id,
                        input.expected_revision,
                        search_hit_id,
                    )
                    .await
                    .map(|read| content = Some(read)),
                DiscoveryStep::ReadProfile { search_hit_id } => self
                    .read_discovery_profile_once(
                        &input.campaign_id,
                        input.expected_revision,
                        search_hit_id,
                    )
                    .await
                    .map(|read| profile = Some(read)),
                DiscoveryStep::ReadTranscript {
                    profile_ref,
                    search_hit_id,
                } => self
                    .read_discovery_youtube_transcript_once(
                        &input.campaign_id,
                        input.expected_revision,
                        profile_ref,
                        search_hit_id,
                    )
                    .await
                    .map(|read| transcript = Some(read)),
                DiscoveryStep::Search {
                    platform,
                    profile_ref,
                } => self
                    .search_discovery_once(
                        &input.campaign_id,
                        input.expected_revision,
                        platform,
                        profile_ref,
                        20.min(
                            OpenCli::creator_search_max_limit(platform)
                                .map_err(OpenCliDiscoverySearchError::Dependency)?,
                        ),
                    )
                    .await
                    .map(|search| {
                        saved_stop = search
                            .campaign
                            .platform_runs
                            .iter()
                            .find(|run| {
                                &run.platform == platform && &run.profile_ref == profile_ref
                            })
                            .and_then(|run| run.stop_code.clone());
                    }),
                DiscoveryStep::ReadVideo {
                    profile_ref,
                    search_hit_id,
                } => self
                    .verify_discovery_youtube_hit_once(
                        &input.campaign_id,
                        input.expected_revision,
                        profile_ref,
                        search_hit_id,
                    )
                    .await
                    .map(|_| ()),
                _ => Ok(()),
            }
        }
        .await;
        // A provider stop has already committed a checkpoint. Return it and
        // allow the next planning call to select a different authorized scope.
        let provider_stop_code = match &result {
            Err(OpenCliDiscoverySearchError::Provider(error)) => {
                Some(provider_stop_code(error).to_owned())
            }
            _ => saved_stop.or_else(|| {
                facebook_identity
                    .as_ref()
                    .and_then(|read| read.stop_code.clone())
            }),
        };
        if let Err(error) = result
            && !matches!(error, OpenCliDiscoverySearchError::Provider(_))
        {
            return Err(OpenCliDiscoverySearchError::Advance {
                step: Box::new(step),
                failure: Box::new(error),
            });
        }
        let campaign = creators
            .get_discovery_campaign_async(input.campaign_id.clone())
            .await
            .map_err(|error| OpenCliDiscoverySearchError::Advance {
                step: Box::new(step.clone()),
                failure: Box::new(OpenCliDiscoverySearchError::Campaign(error.to_string())),
            })?;
        let counts = creators
            .count_discovery_candidates_async(input.campaign_id)
            .await
            .map_err(|error| OpenCliDiscoverySearchError::Advance {
                step: Box::new(step.clone()),
                failure: Box::new(OpenCliDiscoverySearchError::Campaign(error.to_string())),
            })?;
        Ok(OpenCliDiscoveryAdvanceResult {
            step,
            campaign,
            counts,
            provider_stop_code,
            transcript,
            profile,
            content,
            content_targets,
            identity,
            facebook_identity,
            source_author,
            source_author_timings,
            web_source,
            public_page,
            contact_lookup,
        })
    }

    /// Reserve one call before inspecting the provider. Provider stops become
    /// durable receipts; no text is promoted to a qualification verdict.
    pub async fn read_discovery_content_once(
        &self,
        campaign_id: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryContentRead, OpenCliDiscoverySearchError> {
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign_id.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        if let Some(claim) = creators
            .claim_discovery_language_async(campaign_id.into(), revision, hit.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?
        {
            return self
                .read_discovery_language(claim)
                .await
                .map_err(OpenCliDiscoverySearchError::Campaign);
        }
        let claim = creators
            .claim_discovery_content_async(campaign_id.into(), revision, hit.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let result = async {
            let cli = OpenCli::inspect_system().await?;
            if cli.inventory().version != VERIFIED_OPENCLI_VERSION
                || approved.opencli_version != cli.inventory().version
            {
                return Err(OpenCliError::VersionChanged);
            }
            let value = match claim.platform.as_str() {
                "linkedin" => {
                    let saved = creators
                        .get_discovery_profile_async(
                            claim.campaign_id.clone(),
                            claim.search_hit_id.clone(),
                        )
                        .await
                        .map_err(|_| OpenCliError::InvalidOutput)?;
                    if saved.status != "completed"
                        || saved.stop_code.is_some()
                        || saved.claim.expected_revision != claim.expected_revision
                        || saved.claim.profile_ref != claim.profile_ref
                        || saved.claim.platform != claim.platform
                        || saved.claim.author_id != claim.author_id
                        || saved.claim.author_url != claim.source_url
                        || saved.claim.call_number >= claim.call_number
                    {
                        return Err(OpenCliError::InvalidOutput);
                    }
                    let account = saved
                        .native_profile
                        .as_ref()
                        .and_then(|p| p["account_id"].as_str())
                        .ok_or(OpenCliError::InvalidOutput)?;
                    serde_json::to_value(
                        cli.linkedin_original_content(
                            &claim.profile_ref,
                            &claim.author_id,
                            account,
                        )
                        .await?,
                    )
                }
                "pinterest" => {
                    let saved = creators.get_discovery_profile_async(
                        claim.campaign_id.clone(), claim.search_hit_id.clone(),
                    ).await.map_err(|_| OpenCliError::InvalidOutput)?;
                    if saved.status != "completed" || saved.stop_code.is_some()
                        || saved.claim.expected_revision != claim.expected_revision
                        || saved.claim.profile_ref != claim.profile_ref
                        || saved.claim.platform != claim.platform
                        || saved.claim.author_id != claim.author_id
                        || saved.claim.source_url != claim.source_url
                        || saved.claim.call_number >= claim.call_number
                    {
                        return Err(OpenCliError::InvalidOutput);
                    }
                    let username = saved.claim.author_url
                        .strip_prefix("https://www.pinterest.com/")
                        .and_then(|s| s.strip_suffix('/'))
                        .ok_or(OpenCliError::InvalidInput("pinterest_author"))?;
                    let content = if let Some(pin) = claim.source_url
                        .strip_prefix("https://www.pinterest.com/pin/")
                        .and_then(|s| s.strip_suffix('/'))
                    {
                        cli.pinterest_public_pin_content(&claim.profile_ref, &claim.author_id, username, pin).await?
                    } else if claim.source_url == saved.claim.author_url {
                        cli.pinterest_public_content(
                            &claim.profile_ref,
                            &claim.author_id,
                            username,
                        ).await?
                    } else {
                        return Err(OpenCliError::InvalidInput("pinterest_source"));
                    };
                    serde_json::to_value(content)
                }
                "pixiv" => {
                    let id = claim
                        .source_url
                        .strip_prefix("https://www.pixiv.net/artworks/")
                        .ok_or(OpenCliError::InvalidInput("pixiv_artwork_id"))?;
                    let item = cli
                        .pixiv_original_content(&claim.profile_ref, id, &claim.author_id)
                        .await?;
                    if item.source_url != claim.source_url {
                        return Err(OpenCliError::InvalidOutput);
                    }
                    serde_json::to_value(item)
                }
                "hackernews" => {
                    let id = claim
                        .source_url
                        .strip_prefix("https://news.ycombinator.com/item?id=")
                        .and_then(|id| id.parse::<u64>().ok())
                        .ok_or(OpenCliError::InvalidInput("hackernews_item_id"))?;
                    let item = cli.hackernews_original_item(&claim.profile_ref, id).await?;
                    if item.author_id != claim.author_id || item.source_url != claim.source_url {
                        return Err(OpenCliError::InvalidOutput);
                    }
                    serde_json::to_value(item)
                }
                "zhihu" => {
                    if claim
                        .source_url
                        .starts_with("https://zhuanlan.zhihu.com/p/")
                    {
                        serde_json::to_value(
                            cli.zhihu_original_article(
                                &claim.profile_ref,
                                &claim.author_id,
                                &claim.source_url,
                            )
                            .await?,
                        )
                    } else {
                        serde_json::to_value(
                            cli.zhihu_original_answer(
                                &claim.profile_ref,
                                &claim.author_id,
                                &claim.source_url,
                            )
                            .await?,
                        )
                    }
                }
                "bluesky" => serde_json::to_value(
                    cli.bluesky_post_evidence(&claim.profile_ref, &claim.source_url)
                        .await?,
                ),
                "weibo" => serde_json::to_value(
                    cli.weibo_post_evidence(&claim.profile_ref, &claim.source_url)
                        .await?,
                ),
                "instagram" => {
                    let username = claim
                        .source_url
                        .strip_prefix("https://www.instagram.com/")
                        .and_then(|s| s.strip_suffix('/'))
                        .ok_or(OpenCliError::InvalidInput("instagram_content_target"))?;
                    let post = cli
                        .instagram_recent_original_posts(
                            &claim.profile_ref,
                            username,
                            &claim.author_id,
                        )
                        .await?
                        .into_iter()
                        .next()
                        .ok_or(OpenCliError::InvalidOutput)?;
                    serde_json::to_value(post)
                }
                "facebook" => {
                    let profile = creators
                        .get_discovery_profile_async(campaign_id.into(), hit.into())
                        .await
                        .map_err(|_| OpenCliError::InvalidOutput)?;
                    if profile.status != "completed"
                        || profile.claim.expected_revision != claim.expected_revision
                        || profile.claim.profile_ref != claim.profile_ref
                        || profile.claim.author_id != claim.author_id
                        || profile.claim.call_number >= claim.call_number
                    {
                        return Err(OpenCliError::InvalidOutput);
                    }
                    let native = profile
                        .native_profile
                        .as_ref()
                        .ok_or(OpenCliError::InvalidOutput)?;
                    let content = if native.get("adapter_version").and_then(|v| v.as_str())
                        == Some("2.0.0")
                    {
                        let base = native
                            .get("observed_profile_url")
                            .and_then(|v| v.as_str())
                            .ok_or(OpenCliError::InvalidOutput)?;
                        cli.facebook_native_original(
                            &claim.profile_ref,
                            &claim.author_id,
                            base,
                            &claim.source_url,
                        )
                        .await?
                    } else {
                        cli.facebook_reel_content(
                            &claim.profile_ref,
                            &claim.author_id,
                            &claim.source_url,
                        )
                        .await?
                    };
                    serde_json::to_value(content)
                }
                "reddit" => Ok(cli
                    .reddit_post_content(&claim.profile_ref, &claim.author_id, &claim.source_url)
                    .await?),
                "bilibili" => serde_json::to_value(
                    cli.bilibili_video_evidence(
                        &claim.profile_ref,
                        &claim.source_url,
                        &claim.author_id,
                    )
                    .await?,
                ),
                "twitter" => {
                    let identity = creators
                        .get_discovery_identity_async(campaign_id.into(), hit.into())
                        .await
                        .map_err(|_| OpenCliError::InvalidOutput)?;
                    if identity.status != "completed"
                        || identity.claim.expected_revision != claim.expected_revision
                        || identity.claim.profile_ref != claim.profile_ref
                        || identity.claim.author_handle != claim.author_id
                        || identity.claim.source_url != claim.source_url
                        || identity.claim.call_number >= claim.call_number
                    {
                        return Err(OpenCliError::InvalidOutput);
                    }
                    serde_json::to_value(
                        cli.twitter_post_content(
                            &claim.profile_ref,
                            &claim.author_id,
                            &identity.claim.post_id,
                            identity
                                .account_id
                                .as_deref()
                                .ok_or(OpenCliError::InvalidOutput)?,
                        )
                        .await?,
                    )
                }
                _ => return Err(OpenCliError::InvalidInput("content_platform")),
            }
            .map_err(|_| OpenCliError::InvalidOutput)?;
            if serde_json::to_vec(&value)
                .map_err(|_| OpenCliError::InvalidOutput)?
                .len()
                > 16_000
            {
                return Err(OpenCliError::OutputTooLarge);
            }
            Ok(value)
        }
        .await;
        let (native_content, stop_code) = match result {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(provider_stop_code(&error).to_owned())),
        };
        creators
            .finish_discovery_content_async(FinishDiscoveryContentInput {
                claim,
                native_content,
                stop_code,
            })
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }

    /// Reserve a durable author-bound read before any dependency or provider call.
    /// Failure is checkpointed and never certifies a Creator or retries implicitly.
    pub async fn read_discovery_profile_once(
        &self,
        campaign_id: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryProfileRead, OpenCliDiscoverySearchError> {
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign_id.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let claim = creators
            .claim_discovery_profile_async(campaign_id.into(), revision, hit.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let result = async {
            let cli = OpenCli::inspect_system().await?;
            if cli.inventory().version != VERIFIED_OPENCLI_VERSION
                || approved.opencli_version != cli.inventory().version
            {
                return Err(OpenCliError::VersionChanged);
            }
            let value = if claim.platform == "twitter" {
                let identity = creators
                    .get_discovery_identity_async(campaign_id.into(), hit.into())
                    .await
                    .map_err(|_| OpenCliError::InvalidOutput)?;
                let profile = cli
                    .twitter_identity_profile(&claim.profile_ref, &claim.author_id)
                    .await?;
                if identity.status != "completed"
                    || identity.account_id.as_deref() != Some(profile.account_id.as_str())
                {
                    return Err(OpenCliError::InvalidOutput);
                }
                serde_json::to_value(profile).map_err(|_| OpenCliError::InvalidOutput)?
            } else if claim.platform == "facebook"
                && onecrew_opencli::facebook_native_contact_profile(&format!(
                    "{}/directory_contact_info",
                    claim.source_url.trim_end_matches('/')
                ))
                .is_some()
            {
                serde_json::to_value(
                    cli.facebook_native_profile(
                        &claim.profile_ref,
                        &claim.author_id,
                        &claim.source_url,
                    )
                    .await?,
                )
                .map_err(|_| OpenCliError::InvalidOutput)?
            } else {
                cli.creator_profile_evidence(
                    &claim.profile_ref,
                    &claim.platform,
                    &claim.author_id,
                    &claim.author_url,
                )
                .await?
            };
            if serde_json::to_vec(&value)
                .map_err(|_| OpenCliError::InvalidOutput)?
                .len()
                > 16_000
            {
                return Err(OpenCliError::OutputTooLarge);
            }
            Ok(value)
        }
        .await;
        let (native_profile, stop_code) = match result {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(provider_stop_code(&error).to_owned())),
        };
        creators
            .finish_discovery_profile_async(FinishDiscoveryProfileInput {
                claim,
                native_profile,
                stop_code,
            })
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }

    /// Read only a budget-reserved, metadata-attributed source. The returned
    /// excerpt is material for review; no qualification verdict is changed.
    pub async fn read_discovery_youtube_transcript_once(
        &self,
        campaign_id: &str,
        revision: u32,
        profile: &str,
        hit: &str,
    ) -> Result<DiscoveryTranscriptRead, OpenCliDiscoverySearchError> {
        let cli = OpenCli::inspect_system()
            .await
            .map_err(OpenCliDiscoverySearchError::Dependency)?;
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign_id.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION
            || approved.opencli_version != cli.inventory().version
        {
            return Err(OpenCliDiscoverySearchError::Dependency(
                OpenCliError::VersionChanged,
            ));
        }
        let claim = creators
            .claim_discovery_transcript_async(
                campaign_id.into(),
                revision,
                profile.into(),
                hit.into(),
            )
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let result = cli
            .youtube_transcript(profile, &claim.source_url)
            .await
            .and_then(bounded_transcript_excerpt);
        let (segments, truncated, stop_code) = match result {
            Ok((segments, truncated)) => (segments, truncated, None),
            Err(error) => (vec![], false, Some(provider_stop_code(&error).to_owned())),
        };
        creators
            .finish_discovery_transcript_async(FinishDiscoveryTranscriptInput {
                claim,
                segments,
                truncated,
                stop_code,
            })
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }

    /// One budget-reserved video read. Metadata alone never proves a real demo
    /// or an independently checked tool URL, so the first verdict stays pending
    /// unless a sourced subscriber count already fails the frozen threshold.
    pub async fn verify_discovery_youtube_hit_once(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        profile_ref: &str,
        search_hit_id: &str,
    ) -> Result<DiscoveryCampaign, OpenCliDiscoverySearchError> {
        let cli = OpenCli::inspect_system()
            .await
            .map_err(OpenCliDiscoverySearchError::Dependency)?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION {
            return Err(OpenCliDiscoverySearchError::Dependency(
                OpenCliError::VersionChanged,
            ));
        }
        cli.check_youtube_evidence_profile(profile_ref)
            .await
            .map_err(OpenCliDiscoverySearchError::Dependency)?;
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign_id.to_owned())
            .await
            .map_err(|error| OpenCliDiscoverySearchError::Campaign(error.to_string()))?;
        if approved.opencli_version != cli.inventory().version {
            return Err(OpenCliDiscoverySearchError::Dependency(
                OpenCliError::VersionChanged,
            ));
        }
        let claim = creators
            .claim_discovery_video_async(
                campaign_id.to_owned(),
                expected_revision,
                profile_ref.to_owned(),
                search_hit_id.to_owned(),
            )
            .await
            .map_err(|error| OpenCliDiscoverySearchError::Campaign(error.to_string()))?;
        match cli
            .youtube_video_evidence(profile_ref, &claim.source_url)
            .await
        {
            Ok(video) => {
                // The adapter already validates the complete original description
                // (16 KB). Contact details often appear below the displayed preview.
                let description = video.description;
                let public_links = extract_public_https_links(&description);
                let candidate = UpsertDiscoveryCandidateInput {
                    campaign_id: campaign_id.into(),
                    expected_revision,
                    platform: "youtube".into(),
                    source_profile_ref: profile_ref.into(),
                    platform_account_id: video.channel_id.clone(),
                    display_name: video.author,
                    profile_url: format!("https://www.youtube.com/channel/{}", video.channel_id),
                    evidence: json!({
                        "source_urls": [claim.source_url, video.source_url],
                        "subscribers_raw": video.subscribers,
                        "subscribers_source_url": video.source_url,
                        "captured_at": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                        "video_title": video.title,
                        "video_description": description,
                        "public_links": public_links,
                    }),
                };
                let reserved = claim.clone();
                let saved = creators
                    .finish_discovery_video_async(FinishDiscoveryVideoInput {
                        claim,
                        candidate: Some(candidate),
                        stop_code: None,
                    })
                    .await;
                match saved {
                    Ok(campaign) => Ok(campaign),
                    Err(original) => {
                        creators
                            .finish_discovery_video_async(FinishDiscoveryVideoInput {
                                claim: reserved,
                                candidate: None,
                                stop_code: Some("opencli_invalid_output".into()),
                            })
                            .await
                            .map_err(|_| {
                                OpenCliDiscoverySearchError::Campaign(original.to_string())
                            })?;
                        Err(OpenCliDiscoverySearchError::Provider(
                            OpenCliError::InvalidOutput,
                        ))
                    }
                }
            }
            Err(error) => {
                let stop_code = match &error {
                    OpenCliError::Timeout => "opencli_timeout",
                    OpenCliError::OutputTooLarge => "opencli_output_too_large",
                    OpenCliError::VersionChanged => "opencli_version_changed",
                    OpenCliError::DaemonUnavailable => "opencli_daemon_unavailable",
                    OpenCliError::ExtensionUnavailable => "opencli_extension_unavailable",
                    OpenCliError::ProfileUnavailable => "opencli_profile_unavailable",
                    OpenCliError::ReadCommandUnavailable => "opencli_command_unavailable",
                    OpenCliError::InvalidOutput => "opencli_invalid_output",
                    OpenCliError::InvalidInput(_) => "opencli_invalid_input",
                    OpenCliError::Unavailable => "opencli_unavailable",
                    OpenCliError::CommandFailed(_) => "opencli_command_failed",
                    OpenCliError::RiskControl => "opencli_risk_control",
                    OpenCliError::AuthRequired => "opencli_auth_required",
                };
                creators
                    .finish_discovery_video_async(FinishDiscoveryVideoInput {
                        claim,
                        candidate: None,
                        stop_code: Some(stop_code.into()),
                    })
                    .await
                    .map_err(|failure| {
                        OpenCliDiscoverySearchError::Campaign(failure.to_string())
                    })?;
                Err(OpenCliDiscoverySearchError::Provider(error))
            }
        }
    }

    /// This probe runs fixed local read commands only; it never reads a site.
    pub async fn opencli_setup_report(&self) -> OpenCliSetupReport {
        let cli = match OpenCli::inspect_system().await {
            Ok(cli) => cli,
            Err(error) => {
                return OpenCliSetupReport {
                    installed: false,
                    version: None,
                    required_version: VERIFIED_OPENCLI_VERSION,
                    command_path: None,
                    command_count: None,
                    daemon_running: false,
                    extension_connected: false,
                    profiles: Vec::new(),
                    search_capabilities: Vec::new(),
                    addons: Vec::new(),
                    issue: Some(setup_issue(&error)),
                };
            }
        };
        let inventory = cli.inventory();
        let mut report = OpenCliSetupReport {
            installed: true,
            version: Some(inventory.version),
            required_version: VERIFIED_OPENCLI_VERSION,
            command_path: Some(cli.command_path().to_string_lossy().into_owned()),
            command_count: Some(inventory.command_count),
            daemon_running: false,
            extension_connected: false,
            profiles: Vec::new(),
            search_capabilities: Vec::new(),
            addons: Vec::new(),
            issue: None,
        };
        if report.version.as_deref() != Some(VERIFIED_OPENCLI_VERSION) {
            report.issue = Some("version_incompatible");
            return report;
        }
        report.search_capabilities = cli.ecommerce_search_capabilities().await;
        report.addons = cli.addon_status().await;
        match cli.bridge_status().await {
            Ok(bridge) => {
                report.daemon_running = bridge.daemon_running;
                report.extension_connected = bridge.extension_connected;
                if !bridge.daemon_running {
                    report.issue = Some("daemon_unavailable");
                } else if !bridge.extension_connected {
                    report.issue = Some("extension_unavailable");
                } else {
                    match cli.connected_profiles().await {
                        Ok(profiles) => {
                            report.profiles = profiles;
                            if report.profiles.is_empty() {
                                report.issue = Some("profile_unavailable");
                            }
                        }
                        Err(error) => report.issue = Some(setup_issue(&error)),
                    }
                }
            }
            Err(error) => report.issue = Some(setup_issue(&error)),
        }
        report
    }

    /// Explicitly install the managed dependency, then read actual connectivity.
    pub async fn repair_opencli_installation(
        &self,
        approved: bool,
    ) -> Result<OpenCliSetupReport, String> {
        onecrew_opencli::repair_managed_installation(approved).await?;
        let cli = OpenCli::inspect_system()
            .await
            .map_err(|_| "opencli_repair_readback_failed")?;
        cli.repair_bundled_addons(true)
            .await
            .map_err(|_| "opencli_addon_install_failed")?;
        // A failed bridge check is projected as a connection issue, not a failed
        // dependency installation. Never mark a disconnected extension ready.
        drop(cli.recover_bridge().await);
        Ok(self.opencli_setup_report().await)
    }

    /// Recover only the installed local bridge, then return fresh setup evidence.
    pub async fn recover_opencli_bridge(&self) -> Result<OpenCliSetupReport, OpenCliError> {
        OpenCli::inspect_system().await?.recover_bridge().await?;
        Ok(self.opencli_setup_report().await)
    }

    /// Validate a model's proposed interpretation against the customer's
    /// original sentence and the current local source registry. This performs
    /// no provider read and grants no Campaign approval.
    pub async fn validate_discovery_intent(
        &self,
        input: ValidateDiscoveryIntentInput,
    ) -> Result<DiscoveryIntentValidationResult, String> {
        let encoded = serde_json::to_vec(&input).map_err(|_| "invalid_discovery_proposal")?;
        if encoded.len() > 32_768 {
            return Err("discovery_proposal_too_large".into());
        }
        let cli = OpenCli::inspect_system()
            .await
            .map_err(|_| "discovery_sources_unavailable")?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION {
            return Err("discovery_sources_version_incompatible".into());
        }
        let registered = cli
            .ecommerce_search_capabilities()
            .await
            .into_iter()
            .filter(|source| source.registered)
            .map(|source| source.platform)
            .collect::<Vec<_>>();
        if registered.is_empty() {
            return Err("discovery_sources_unavailable".into());
        }
        let issues =
            validate_discovery_intent_proposal(&input.request, &input.proposal, &registered);
        Ok(DiscoveryIntentValidationResult {
            valid: issues.is_empty(),
            issues,
            registered_source_count: registered.len(),
        })
    }

    /// Explicit local setup only: no profile selection or platform request.
    pub async fn install_opencli_addons(
        &self,
        approved: bool,
    ) -> Result<Vec<OpenCliAddonStatus>, OpenCliError> {
        if !approved {
            return Err(OpenCliError::InvalidInput("approved"));
        }
        OpenCli::inspect_system()
            .await?
            .install_bundled_addons(approved)
            .await
    }

    pub async fn open_opencli_login_page(
        &self,
        profile: &str,
        platform: &str,
    ) -> Result<(), OpenCliError> {
        let cli = OpenCli::inspect_system().await?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION {
            return Err(OpenCliError::VersionChanged);
        }
        cli.open_creator_login_page(profile, platform).await
    }

    /// Bootstrap navigation must work before OpenCLI or its extension exists.
    pub async fn open_opencli_setup_page(&self, page: ChromeSetupPage) -> Result<(), String> {
        onecrew_chrome::open_setup_page(page)
            .await
            .map_err(|error| match error {
                onecrew_chrome::ChromeError::BrowserNotFound => "chrome_not_installed".to_owned(),
                _ => "chrome_setup_page_open_failed".to_owned(),
            })
    }

    pub async fn check_opencli_browser_profile(&self, profile: &str) -> Result<(), OpenCliError> {
        let cli = OpenCli::inspect_system().await?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION {
            return Err(OpenCliError::VersionChanged);
        }
        cli.check_browser_profile(profile).await
    }

    /// One explicit no-navigation site login observation. Its result is not a
    /// Campaign search authorization or proof that content reads will work.
    pub async fn check_opencli_site_auth(
        &self,
        profile: &str,
        platform: &str,
    ) -> Result<OpenCliAuthObservation, String> {
        self.opencli_auth_observations.lock().map_err(|_| "auth_observation_unavailable")?
            .retain(|item| item.profile_ref != profile || item.platform != platform);
        let cli = OpenCli::inspect_system()
            .await
            .map_err(|error| setup_issue(&error).to_owned())?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION {
            return Err("version_incompatible".to_owned());
        }
        let observation = cli.check_creator_auth_status(profile, platform)
            .await
            .map_err(|error| setup_issue(&error).to_owned())?;
        let mut observations = self.opencli_auth_observations.lock().map_err(|_| "auth_observation_unavailable")?;
        observations.retain(|item| item.profile_ref != profile || item.platform != platform);
        if observations.len() >= 57 { observations.remove(0); }
        observations.push(observation.clone());
        Ok(observation)
    }

    /// Shared historical evidence for all workbench browser views. This read
    /// never probes a provider and does not extend observation freshness.
    pub fn opencli_auth_observations(&self, profile: &str) -> Result<Vec<OpenCliAuthObservation>, String> {
        let observations = self.opencli_auth_observations.lock().map_err(|_| "auth_observation_unavailable")?;
        Ok(observations.iter().filter(|item| item.profile_ref == profile).cloned().collect())
    }

    /// Explicit public profile read, not an automatic Campaign step. No persistence,
    /// qualification, background polling or expansion to other account identities.
    /// Future HTTP/MCP consumers must call this owner only on explicit read intent.
    pub async fn read_opencli_tiktok_profile(
        &self,
        profile_ref: &str,
        username: &str,
        expected_author_id: &str,
    ) -> Result<TiktokPublicProfile, OpenCliError> {
        onecrew_opencli::check_tiktok_profile_target(username, expected_author_id)?;
        OpenCli::inspect_system()
            .await?
            .tiktok_public_profile(profile_ref, username, expected_author_id)
            .await
    }

    /// Explicit public profile evidence only; does not create or advance a Campaign.
    pub async fn read_opencli_bilibili_profile(
        &self,
        profile_ref: &str,
        mid: &str,
    ) -> Result<BilibiliPublicProfile, OpenCliError> {
        onecrew_opencli::check_bilibili_profile_target(mid)?;
        OpenCli::inspect_system()
            .await?
            .bilibili_public_profile(profile_ref, mid)
            .await
    }

    /// Probe the user-installed OpenCLI without issuing a site read or write.
    pub async fn inspect_opencli(&self) -> Result<OpenCliAvailability, OpenCliError> {
        let cli = OpenCli::inspect_system().await?;
        let bridge = cli.bridge_status().await?;
        let profiles = if bridge.extension_connected {
            cli.connected_profiles().await?
        } else {
            Vec::new()
        };
        Ok(OpenCliAvailability {
            inventory: cli.inventory(),
            bridge,
            profiles,
        })
    }

    /// One authorized YouTube search step. Provider hints and the completed
    /// checkpoint commit atomically; no Creator entry or platform write occurs.
    pub async fn search_discovery_youtube_once(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        profile_ref: &str,
        limit: u8,
    ) -> Result<OpenCliDiscoverySearchResult, OpenCliDiscoverySearchError> {
        self.search_discovery_once(
            campaign_id,
            expected_revision,
            "youtube",
            profile_ref,
            limit,
        )
        .await
    }

    /// One approved search on an explicitly mapped content platform. Results
    /// are only source-backed leads; qualification requires later verification.
    pub async fn search_discovery_once(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        platform: &str,
        profile_ref: &str,
        limit: u8,
    ) -> Result<OpenCliDiscoverySearchResult, OpenCliDiscoverySearchError> {
        OpenCli::check_creator_search_limit(platform, limit)
            .map_err(OpenCliDiscoverySearchError::Dependency)?;
        let cli = OpenCli::inspect_system()
            .await
            .map_err(OpenCliDiscoverySearchError::Dependency)?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION {
            return Err(OpenCliDiscoverySearchError::Dependency(
                OpenCliError::VersionChanged,
            ));
        }
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign_id.to_owned())
            .await
            .map_err(|error| OpenCliDiscoverySearchError::Campaign(error.to_string()))?;
        if approved.opencli_version != cli.inventory().version {
            return Err(OpenCliDiscoverySearchError::Dependency(
                OpenCliError::VersionChanged,
            ));
        }
        let claim = creators
            .claim_discovery_search_async(
                campaign_id.to_owned(),
                expected_revision,
                platform.into(),
                profile_ref.to_owned(),
            )
            .await
            .map_err(|error| OpenCliDiscoverySearchError::Campaign(error.to_string()))?;
        // creator_search checks the exact profile and read capability before
        // touching the site. Record that failure against the reserved attempt
        // so an unavailable platform cannot block every other platform's
        // first-round completion forever.
        match cli
            .creator_search(profile_ref, platform, &claim.query, limit)
            .await
        {
            Ok(hits) => {
                let search_hits = hits
                    .into_iter()
                    .map(|hit| DiscoverySearchHitInput {
                        author_id: hit.author_id,
                        author_url: hit.author_url,
                        source_url: hit.source_url,
                        title: hit.title,
                        author_label: hit.author_label,
                    })
                    .collect::<Vec<_>>();
                persist_search_hits(&creators, claim, search_hits).await
            }
            Err(error) => {
                let stop_code = match &error {
                    OpenCliError::Timeout => "opencli_timeout",
                    OpenCliError::OutputTooLarge => "opencli_output_too_large",
                    OpenCliError::VersionChanged => "opencli_version_changed",
                    OpenCliError::DaemonUnavailable => "opencli_daemon_unavailable",
                    OpenCliError::ExtensionUnavailable => "opencli_extension_unavailable",
                    OpenCliError::ProfileUnavailable => "opencli_profile_unavailable",
                    OpenCliError::ReadCommandUnavailable => "opencli_command_unavailable",
                    OpenCliError::InvalidOutput => "opencli_invalid_output",
                    OpenCliError::InvalidInput(_) => "opencli_invalid_input",
                    OpenCliError::Unavailable => "opencli_unavailable",
                    OpenCliError::CommandFailed(_) => "opencli_command_failed",
                    OpenCliError::RiskControl => "opencli_risk_control",
                    OpenCliError::AuthRequired => "opencli_auth_required",
                };
                creators
                    .finish_discovery_search_async(FinishDiscoverySearchInput {
                        claim,
                        status: if matches!(error, OpenCliError::RiskControl) {
                            "risk_control_stop".into()
                        } else if matches!(error, OpenCliError::AuthRequired) {
                            "unauthorized".into()
                        } else {
                            "technical_stop".into()
                        },
                        stop_code: Some(stop_code.into()),
                        search_hits: vec![],
                    })
                    .await
                    .map_err(|failure| {
                        OpenCliDiscoverySearchError::Campaign(failure.to_string())
                    })?;
                Err(OpenCliDiscoverySearchError::Provider(error))
            }
        }
    }
}

async fn persist_search_hits(
    creators: &onecrew_creator_runtime::CreatorRuntime,
    claim: onecrew_creator_runtime::DiscoverySearchClaim,
    search_hits: Vec<DiscoverySearchHitInput>,
) -> Result<OpenCliDiscoverySearchResult, OpenCliDiscoverySearchError> {
    let returned_hits = search_hits.len();
    match creators
        .finish_discovery_search_async(FinishDiscoverySearchInput {
            claim: claim.clone(),
            status: "completed".into(),
            stop_code: None,
            search_hits: search_hits.clone(),
        })
        .await
    {
        Ok(campaign) => Ok(OpenCliDiscoverySearchResult {
            campaign,
            returned_hits,
            search_claim: claim,
            returned_sources: search_hits,
        }),
        Err(error) => {
            // A rejected result rolls back its transaction. End this exact claim
            // so a finished provider call cannot leave an orphaned "searching" run.
            // The owner rejects stale claims here too; never overwrite a newer attempt.
            let stopped = creators
                .finish_discovery_search_async(FinishDiscoverySearchInput {
                    claim: claim.clone(),
                    status: "technical_stop".into(),
                    stop_code: Some("search_completion_failed".into()),
                    search_hits: vec![],
                })
                .await;
            match stopped {
                Ok(campaign) => Ok(OpenCliDiscoverySearchResult {
                    campaign,
                    returned_hits: 0,
                    search_claim: claim,
                    returned_sources: vec![],
                }),
                Err(_) => Err(OpenCliDiscoverySearchError::Campaign(error.to_string())),
            }
        }
    }
}

pub fn provider_stop_code(error: &OpenCliError) -> &'static str {
    match error {
        OpenCliError::Timeout => "opencli_timeout",
        OpenCliError::OutputTooLarge => "opencli_output_too_large",
        OpenCliError::VersionChanged => "opencli_version_changed",
        OpenCliError::DaemonUnavailable => "opencli_daemon_unavailable",
        OpenCliError::ExtensionUnavailable => "opencli_extension_unavailable",
        OpenCliError::ProfileUnavailable => "opencli_profile_unavailable",
        OpenCliError::ReadCommandUnavailable => "opencli_command_unavailable",
        OpenCliError::InvalidOutput => "opencli_invalid_output",
        OpenCliError::InvalidInput(_) => "opencli_invalid_input",
        OpenCliError::Unavailable => "opencli_unavailable",
        OpenCliError::CommandFailed(_) => "opencli_command_failed",
        OpenCliError::RiskControl => "opencli_risk_control",
        OpenCliError::AuthRequired => "opencli_auth_required",
    }
}

fn bounded_transcript_excerpt(
    lines: Vec<onecrew_opencli::YoutubeTranscriptLine>,
) -> Result<(Vec<DiscoveryTranscriptSegment>, bool), OpenCliError> {
    let count = lines.len();
    let mut excerpt = Vec::new();
    for line in lines.into_iter().take(200) {
        excerpt.push(DiscoveryTranscriptSegment {
            timestamp: line.timestamp,
            speaker: line.speaker,
            text: line.text,
        });
        if serde_json::to_vec(&excerpt)
            .map_err(|_| OpenCliError::InvalidOutput)?
            .len()
            > 24_000
        {
            excerpt.pop();
            if excerpt.is_empty() {
                return Err(OpenCliError::OutputTooLarge);
            }
            break;
        }
    }
    let truncated = excerpt.len() < count;
    Ok((excerpt, truncated))
}

fn extract_public_https_links(description: &str) -> Vec<String> {
    let mut links = Vec::new();
    for token in description.split_whitespace() {
        if token.contains("...") || token.contains('…') {
            continue;
        }
        let value = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '(' | ')' | '[' | ']' | '<' | '>' | ',' | '.' | ';' | '!' | '"' | '\''
            )
        });
        if value.len() > 2048 || !value.starts_with("https://") {
            continue;
        }
        if url::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        }) && !links.iter().any(|existing| existing == value)
        {
            links.push(value.to_owned());
            if links.len() == 10 {
                break;
            }
        }
    }
    links
}

#[cfg(test)]
mod opencli_discovery_tests {
    use super::extract_public_https_links;

    #[test]
    fn video_description_links_are_hints_not_verified_tools() {
        assert_eq!(
            extract_public_https_links("Try (https://example.com/tool). Also http://unsafe.test"),
            vec!["https://example.com/tool"]
        );
        assert!(extract_public_https_links("Cursor is great; ask me for a link").is_empty());
        assert!(
            extract_public_https_links("https://example.com/tool... https://example.com/tool…")
                .is_empty()
        );
        let description = format!(
            "{}Business: hello@example.org https://example.com/contact",
            "x ".repeat(2500)
        );
        assert_eq!(
            extract_public_https_links(&description),
            vec!["https://example.com/contact"]
        );
    }

    #[test]
    fn transcript_excerpt_preserves_observed_text_and_marks_byte_or_segment_truncation() {
        for text in ["Small sentence".to_owned(), "演示工具".repeat(400)] {
            let lines = (0..201)
                .map(|n| onecrew_opencli::YoutubeTranscriptLine {
                    timestamp: format!("0:{n:02}"),
                    speaker: String::new(),
                    text: text.clone(),
                })
                .collect();
            let (excerpt, truncated) = super::bounded_transcript_excerpt(lines).unwrap();
            assert!(truncated);
            assert!(!excerpt.is_empty() && excerpt.len() <= 200);
            assert!(serde_json::to_vec(&excerpt).unwrap().len() <= 24_000);
            assert_eq!(excerpt[0].text, text);
            assert_eq!(excerpt[0].timestamp, "0:00");
        }
        assert_eq!(
            super::bounded_transcript_excerpt(vec![]).unwrap(),
            (vec![], false)
        );
    }
}

#[cfg(test)]
mod opencli_profile_tests {
    use super::*;

    #[tokio::test]
    async fn invalid_bilibili_profile_target_stops_before_dependency_or_provider_read() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(
            platform
                .read_opencli_bilibili_profile("unused", "../123")
                .await,
            Err(OpenCliError::InvalidInput("bilibili_profile_target"))
        );
    }

    #[tokio::test]
    #[ignore = "one public profile read; requires explicit OPENCLI_TEST_PROFILE and OPENCLI_TEST_BILIBILI_AUTHOR_ID"]
    async fn runtime_bilibili_profile_read_preserves_exact_count_source() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser identity");
        let mid = std::env::var("OPENCLI_TEST_BILIBILI_AUTHOR_ID").expect("observed author MID");
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let result = platform
            .read_opencli_bilibili_profile(&profile, &mid)
            .await
            .unwrap();
        assert_eq!(result.mid, mid);
        assert_eq!(result.evidence_source, "profile_tooltip");
        println!(
            "runtime public profile source={} followers={} tooltip={}; no Campaign or qualification created",
            result.profile_url, result.follower_count, result.follower_count_text
        );
    }

    #[tokio::test]
    async fn invalid_profile_target_stops_before_dependency_or_provider_read() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(
            platform
                .read_opencli_tiktok_profile("unused", "../other", "123")
                .await,
            Err(OpenCliError::InvalidInput("tiktok_profile_target"))
        );
    }

    #[tokio::test]
    #[ignore = "one public profile read; requires explicit OPENCLI_TEST_PROFILE, OPENCLI_TEST_TIKTOK_USERNAME and OPENCLI_TEST_TIKTOK_AUTHOR_ID"]
    async fn runtime_tiktok_profile_read_preserves_native_evidence() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser identity");
        let username =
            std::env::var("OPENCLI_TEST_TIKTOK_USERNAME").expect("observed search author");
        let author_id = std::env::var("OPENCLI_TEST_TIKTOK_AUTHOR_ID").expect("observed author ID");
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let result = platform
            .read_opencli_tiktok_profile(&profile, &username, &author_id)
            .await
            .unwrap();
        assert_eq!(result.id, author_id);
        assert_eq!(result.username, username);
        assert_eq!(result.evidence_source, "profile_bootstrap");
        println!(
            "runtime public profile source={} followers={}; no Campaign or qualification created",
            result.profile_url, result.follower_count
        );
    }
}

#[cfg(test)]
mod opencli_live_campaign_tests {
    use super::*;
    use onecrew_creator_runtime::{
        CreateDiscoveryCampaignInput, DiscoveryPlatformScope, ListDiscoverySearchHitsInput,
    };

    #[tokio::test]
    #[ignore = "live Instagram signed-out probe; requires OPENCLI_TEST_PROFILE with no Instagram session"]
    async fn instagram_login_required_keeps_an_unauthorized_campaign_checkpoint() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser identity");
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let id = "instagram-login-required-probe";
        let creators = platform.creators().unwrap();
        creators
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: id.into(),
                brief: "Verify Instagram customer login gate".into(),
                opencli_version: VERIFIED_OPENCLI_VERSION.into(),
                criteria: json!({"min_followers": 10000}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "instagram".into(),
                    profile_ref: profile.clone(),
                    keywords: vec!["ai".into()],
                }],
                target_count: 1,
                max_calls: Some(1),
            })
            .unwrap();
        creators.approve_discovery_campaign(id, 1).unwrap();
        assert!(matches!(
            platform
                .search_discovery_once(id, 1, "instagram", &profile, 1)
                .await,
            Err(OpenCliDiscoverySearchError::Provider(
                OpenCliError::AuthRequired
            ))
        ));
        let stopped = creators.get_discovery_campaign(id).unwrap();
        assert_eq!(stopped.used_calls, 1);
        assert_eq!(stopped.platform_runs[0].status, "unauthorized");
        assert_eq!(
            stopped.platform_runs[0].stop_code.as_deref(),
            Some("opencli_auth_required")
        );
        assert!(
            creators
                .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                    campaign_id: id.into(),
                    after_id: None,
                    limit: 10,
                })
                .unwrap()
                .is_empty()
        );
        drop(creators);
        drop(platform);
        let reopened = LocalPlatform::open(directory.path()).unwrap();
        let persisted = reopened
            .creators()
            .unwrap()
            .get_discovery_campaign(id)
            .unwrap();
        assert_eq!(persisted.used_calls, 1);
        assert_eq!(persisted.platform_runs[0].status, "unauthorized");
    }

    #[tokio::test]
    #[ignore = "live Instagram signed-in probe; requires OPENCLI_TEST_PROFILE with an Instagram session"]
    async fn instagram_signed_in_search_saves_only_unverified_campaign_leads() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit browser identity");
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let id = "instagram-signed-in-probe";
        let creators = platform.creators().unwrap();
        creators
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: id.into(),
                brief: "Verify Instagram customer account access".into(),
                opencli_version: VERIFIED_OPENCLI_VERSION.into(),
                criteria: json!({"min_followers": 10000}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "instagram".into(),
                    profile_ref: profile.clone(),
                    keywords: vec!["ai".into()],
                }],
                target_count: 1,
                max_calls: Some(1),
            })
            .unwrap();
        creators.approve_discovery_campaign(id, 1).unwrap();
        let result = platform
            .search_discovery_once(id, 1, "instagram", &profile, 1)
            .await
            .unwrap();
        assert_eq!(result.returned_hits, 1);
        assert_eq!(result.campaign.used_calls, 1);
        assert_eq!(result.campaign.platform_runs[0].status, "completed");
        let hits = creators
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: id.into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].platform, "instagram");
        assert!(hits[0].source_url.starts_with("https://www.instagram.com/"));
        assert_eq!(
            creators.count_discovery_candidates(id).unwrap().qualified,
            0
        );
        drop(creators);
        drop(platform);
        let reopened = LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(
            reopened
                .creators()
                .unwrap()
                .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                    campaign_id: id.into(),
                    after_id: None,
                    limit: 10,
                })
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    #[ignore = "live bounded YouTube Campaign; requires dedicated OPENCLI_TEST_PROFILE"]
    async fn automatic_campaign_collects_evidence_and_resumes_from_the_same_budget() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated profile");
        let directory = tempfile::tempdir().unwrap();
        let mut platform = LocalPlatform::open(directory.path()).unwrap();
        let id = "opencli-automatic-evidence-probe";
        platform.creators().unwrap().create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: id.into(), brief: "AI tool KOL evidence probe, not qualification acceptance".into(),
            opencli_version: VERIFIED_OPENCLI_VERSION.into(),
            criteria: json!({"min_followers": 10000, "requires_ai_tool_demo": true, "requires_public_tool_url": true}),
            scopes: vec![DiscoveryPlatformScope { platform: "youtube".into(), profile_ref: profile,
                keywords: vec!["AI tools tutorial".into()] }], target_count: 30, max_calls: Some(5),
        }).unwrap();
        platform
            .creators()
            .unwrap()
            .approve_discovery_campaign(id, 1)
            .unwrap();
        for call in 1..=5 {
            let result = platform
                .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
                    excluded_platforms: vec![],
                    campaign_id: id.into(),
                    expected_revision: 1,
                })
                .await
                .unwrap();
            assert_eq!(result.campaign.used_calls, call);
            println!(
                "automatic_live call={call} step={:?} stops={:?} transcript={:?}",
                result.step,
                result
                    .campaign
                    .platform_runs
                    .iter()
                    .map(|run| (&run.status, &run.stop_code))
                    .collect::<Vec<_>>(),
                result.transcript.as_ref().map(|read| (
                    &read.status,
                    read.segments.len(),
                    read.truncated,
                    &read.stop_code
                ))
            );
            assert_eq!(
                result.counts.qualified, 0,
                "metadata cannot prove tool/demo qualification"
            );
            assert!(
                result
                    .campaign
                    .platform_runs
                    .iter()
                    .all(|run| run.status == "completed"),
                "live provider read must succeed for this probe"
            );
            if call == 1 {
                assert!(matches!(result.step, DiscoveryStep::Search { .. }));
            } else {
                assert!(matches!(
                    result.step,
                    DiscoveryStep::ReadVideo { .. }
                        | DiscoveryStep::ReadTranscript { .. }
                        | DiscoveryStep::ReadPublicPage { .. }
                ));
            }
            if let DiscoveryStep::ReadPublicPage {
                candidate_id,
                requested_url,
                profile_ref,
            } = &result.step
            {
                let page = result
                    .public_page
                    .as_ref()
                    .expect("page step returns its receipt");
                assert_eq!(&page.candidate_id, candidate_id);
                assert_eq!(&page.requested_url, requested_url);
                assert_eq!(&page.profile_ref, profile_ref);
                assert_eq!(page.call_number, call);
                assert_eq!(page.status, "completed");
            }
            if call == 2 {
                drop(platform);
                platform = LocalPlatform::open(directory.path()).unwrap();
            }
        }
        let stopped = platform
            .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
                excluded_platforms: vec![],
                campaign_id: id.into(),
                expected_revision: 1,
            })
            .await
            .unwrap();
        assert_eq!(stopped.step, DiscoveryStep::BudgetExhausted);
        assert_eq!(stopped.campaign.used_calls, 5);
        assert!(stopped.counts.pending + stopped.counts.rejected > 0);
        println!(
            "automatic_live used_calls=5 qualified=0 pending={} rejected={}",
            stopped.counts.pending, stopped.counts.rejected
        );
    }

    #[tokio::test]
    #[ignore = "uses installed OpenCLI and a live public search; no browser site read"]
    async fn unavailable_profile_is_checkpointed_without_blocking_public_search() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let creators = platform.creators().unwrap();
        let id = "opencli-partial-coverage-probe";
        let profile = "onecrew-not-connected-test";
        creators
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: id.into(),
                brief: "Partial coverage must stay resumable".into(),
                opencli_version: VERIFIED_OPENCLI_VERSION.into(),
                criteria: json!({"min_followers": 10000}),
                scopes: ["youtube", "bluesky"]
                    .into_iter()
                    .map(|site| DiscoveryPlatformScope {
                        platform: site.into(),
                        profile_ref: profile.into(),
                        keywords: vec!["AI tools".into()],
                    })
                    .collect(),
                target_count: 30,
                max_calls: Some(3),
            })
            .unwrap();
        creators.approve_discovery_campaign(id, 1).unwrap();
        let error = platform
            .search_discovery_once(id, 1, "youtube", profile, 5)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            OpenCliDiscoverySearchError::Provider(OpenCliError::ProfileUnavailable)
        ));
        let stopped = creators.get_discovery_campaign(id).unwrap();
        assert_eq!(stopped.used_calls, 1);
        let youtube = stopped
            .platform_runs
            .iter()
            .find(|run| run.platform == "youtube")
            .unwrap();
        assert_eq!(youtube.status, "technical_stop");
        assert_eq!(
            youtube.stop_code.as_deref(),
            Some("opencli_profile_unavailable")
        );
        let public = platform
            .search_discovery_once(id, 1, "bluesky", profile, 5)
            .await
            .unwrap();
        assert!(public.returned_hits > 0);
        assert_eq!(public.campaign.used_calls, 2);
        assert!(
            public
                .campaign
                .platform_runs
                .iter()
                .all(|run| run.status != "unsearched")
        );
        // Reopening preserves both the failure and the successful platform.
        drop(creators);
        drop(platform);
        let reopened = LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(
            reopened
                .creators()
                .unwrap()
                .get_discovery_campaign(id)
                .unwrap(),
            public.campaign
        );
    }

    /// Exercises the same owner used by HTTP and MCP, with a disposable store.
    /// No browser identity is forwarded to these public OpenCLI adapters.
    #[tokio::test]
    #[ignore = "reads live public Bluesky/Substack search via installed OpenCLI"]
    async fn approved_public_campaign_persists_leads_and_charges_budget() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let creators = platform.creators().unwrap();
        let campaign_id = "opencli-public-live-probe";
        let scope_ref = "public-probe";
        creators
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: campaign_id.into(),
                brief: "Find public AI-tool creator leads".into(),
                opencli_version: VERIFIED_OPENCLI_VERSION.into(),
                criteria: json!({"min_followers": 10000, "requires_ai_tool_demo": true,
                "requires_public_tool_url": true}),
                scopes: ["bluesky", "substack"]
                    .into_iter()
                    .map(|site| DiscoveryPlatformScope {
                        platform: site.into(),
                        profile_ref: scope_ref.into(),
                        keywords: vec!["AI tools".into()],
                    })
                    .collect(),
                target_count: 30,
                max_calls: Some(2),
            })
            .unwrap();
        let denied = platform
            .search_discovery_once(campaign_id, 1, "bluesky", scope_ref, 5)
            .await;
        assert!(matches!(
            denied,
            Err(OpenCliDiscoverySearchError::Campaign(_))
        ));
        assert_eq!(
            creators
                .get_discovery_campaign(campaign_id)
                .unwrap()
                .used_calls,
            0
        );
        creators.approve_discovery_campaign(campaign_id, 1).unwrap();
        let mut expected_hits = 0;
        for site in ["bluesky", "substack"] {
            let result = platform
                .search_discovery_once(campaign_id, 1, site, scope_ref, 5)
                .await
                .unwrap();
            assert!(result.returned_hits > 0);
            expected_hits += result.returned_hits;
        }
        let hits = creators
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: campaign_id.into(),
                after_id: None,
                limit: 100,
            })
            .unwrap();
        assert_eq!(hits.len(), expected_hits);
        assert!(
            hits.iter()
                .all(|hit| hit.profile_ref == scope_ref && hit.search_keyword == "AI tools")
        );
        assert_eq!(
            creators
                .get_discovery_campaign(campaign_id)
                .unwrap()
                .used_calls,
            2
        );
        assert_eq!(
            creators
                .count_discovery_candidates(campaign_id)
                .unwrap()
                .qualified,
            0
        );
        let exhausted = platform
            .search_discovery_once(campaign_id, 1, "bluesky", scope_ref, 5)
            .await;
        assert!(matches!(
            exhausted,
            Err(OpenCliDiscoverySearchError::Campaign(_))
        ));
        assert_eq!(
            creators
                .get_discovery_campaign(campaign_id)
                .unwrap()
                .used_calls,
            2
        );
        println!("persisted_leads={expected_hits} used_calls=2 qualified=0");
    }

    /// Provider failures remain failures in the report. This verifies that new
    /// scopes are covered once and their exact outcomes survive reopening.
    #[tokio::test]
    #[ignore = "reads live Jike, Weixin and Hacker News using an explicit dedicated profile"]
    async fn expanded_platform_campaign_preserves_coverage_budget_and_stops() {
        probe_expanded_platform_campaign(["jike", "weixin", "hackernews"]).await;
    }

    #[tokio::test]
    #[ignore = "reads live Linux.do, Nowcoder and Sina Blog with an explicit dedicated profile"]
    async fn additional_platform_campaign_preserves_coverage_budget_and_stops() {
        probe_expanded_platform_campaign(["linux-do", "nowcoder", "sinablog"]).await;
    }

    #[tokio::test]
    #[ignore = "reads live Apple Podcasts and Tieba using an explicit connected profile"]
    async fn podcast_and_tieba_campaign_preserves_sources_budget_and_stops() {
        probe_expanded_platform_campaign(["apple-podcasts", "tieba"]).await;
    }

    #[tokio::test]
    #[ignore = "reads live Archive, Pinterest, LinkedIn Learning and Sales Navigator"]
    async fn new_creator_sources_campaign_preserves_leads_and_auth_stop() {
        probe_expanded_platform_campaign([
            "archive",
            "pinterest-pins",
            "linkedin-learning",
            "linkedin-salesnav",
        ])
        .await;
    }

    #[tokio::test]
    #[ignore = "reads live Stack Overflow users and Google Images with an explicit connected profile"]
    async fn username_and_image_search_campaign_persists_source_backed_leads() {
        probe_expanded_platform_campaign(["stackoverflow-users", "google-images"]).await;
    }

    #[tokio::test]
    #[ignore = "reads live V2EX node topics using an explicitly approved node name"]
    async fn v2ex_node_campaign_persists_topic_leads_without_creator_qualification() {
        probe_expanded_platform_campaign(["v2ex-node"]).await;
    }

    #[tokio::test]
    #[ignore = "reads live Reddit community posts using an explicitly approved subreddit name"]
    async fn reddit_subreddit_campaign_persists_topic_leads_without_creator_qualification() {
        probe_expanded_platform_campaign(["reddit-subreddit"]).await;
    }

    #[tokio::test]
    #[ignore = "reads public articles and questions from five approved site tags"]
    async fn tag_platform_campaign_persists_source_backed_leads() {
        probe_expanded_platform_campaign([
            "devto-tag",
            "lesswrong-tag",
            "lobsters-tag",
            "medium-tag",
            "stackoverflow-tag",
        ])
        .await;
    }

    #[tokio::test]
    #[ignore = "reads public Juejin, Product Hunt and Hugging Face creator assets"]
    async fn category_and_demo_campaign_persists_source_backed_leads() {
        probe_expanded_platform_campaign([
            "juejin-hot",
            "producthunt-posts",
            "hf-spaces",
            "hf-models",
            "hf-datasets",
        ])
        .await;
    }

    async fn probe_expanded_platform_campaign<const N: usize>(sites: [&str; N]) {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("dedicated profile");
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let owner = platform.creators().unwrap();
        let id = "expanded-platform-live";
        owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: id.into(), brief: "Find AI tool creators across newly supported sources".into(),
            opencli_version: VERIFIED_OPENCLI_VERSION.into(),
            criteria: json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
            scopes: sites.into_iter().map(|site| DiscoveryPlatformScope {
                platform: site.into(), profile_ref: profile.clone(),
                keywords: vec![match site {
                    "hackernews" => "AI tools",
                    "apple-podcasts" => "Shopify",
                    "tieba" => "跨境电商",
                    "archive" | "pinterest-pins" | "linkedin-learning" | "linkedin-salesnav" => "Shopify",
                    "stackoverflow-users" => "John",
                    "google-images" => "illustration",
                    "v2ex-node" => "python",
                    "reddit-subreddit" => "Python",
                    "devto-tag" | "lesswrong-tag" => "ai",
                    "lobsters-tag" => "programming",
                    "medium-tag" => "technology",
                    "stackoverflow-tag" => "python",
                    "juejin-hot" => "ai",
                    "producthunt-posts" => "marketing-sales",
                    "hf-spaces" => "chatbot",
                    "hf-models" => "llama",
                    "hf-datasets" => "instruction",
                    _ => "AI工具",
                }.into()],
            }).collect(), target_count: 30, max_calls: Some(N as u32),
        }).unwrap();
        assert!(
            platform
                .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
                    excluded_platforms: vec![],
                    campaign_id: id.into(),
                    expected_revision: 1,
                })
                .await
                .is_err()
        );
        assert_eq!(owner.get_discovery_campaign(id).unwrap().used_calls, 0);
        owner.approve_discovery_campaign(id, 1).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for expected_used in 1..=N as u32 {
            let result = platform
                .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
                    excluded_platforms: vec![],
                    campaign_id: id.into(),
                    expected_revision: 1,
                })
                .await
                .unwrap();
            let DiscoveryStep::Search {
                platform: site,
                profile_ref,
            } = result.step
            else {
                panic!("first-round search expected")
            };
            assert_eq!(profile_ref, profile);
            assert!(seen.insert(site.clone()));
            assert_eq!(result.campaign.used_calls, expected_used);
            assert_eq!(result.counts.qualified, 0);
            let run = result
                .campaign
                .platform_runs
                .iter()
                .find(|run| run.platform == site)
                .unwrap();
            println!(
                "expanded_source={site} status={} stop={:?} used_calls={expected_used}",
                run.status, run.stop_code
            );
        }
        let snapshot = owner.get_discovery_campaign(id).unwrap();
        let hits = owner
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: id.into(),
                after_id: None,
                limit: 100,
            })
            .unwrap();
        for site in sites {
            println!(
                "expanded_source={site} persisted_leads={}",
                hits.iter().filter(|hit| hit.platform == site).count()
            );
        }
        // At least the public HN search must produce real, persisted source IDs.
        if sites.contains(&"hackernews") {
            assert!(hits.iter().any(|hit| hit.platform == "hackernews"));
        }
        if sites.contains(&"apple-podcasts") {
            assert!(hits.iter().any(|hit| hit.platform == "apple-podcasts"));
        }
        if sites.contains(&"v2ex-node") {
            assert!(hits.iter().any(|hit| hit.platform == "v2ex-node"));
        }
        if sites.contains(&"reddit-subreddit") {
            assert!(hits.iter().any(|hit| hit.platform == "reddit-subreddit"));
        }
        for site in [
            "devto-tag",
            "lesswrong-tag",
            "lobsters-tag",
            "medium-tag",
            "stackoverflow-tag",
        ] {
            if sites.contains(&site) {
                assert!(hits.iter().any(|hit| hit.platform == site), "{site}");
            }
        }
        for site in [
            "juejin-hot",
            "producthunt-posts",
            "hf-spaces",
            "hf-models",
            "hf-datasets",
        ] {
            if sites.contains(&site) {
                assert!(hits.iter().any(|hit| hit.platform == site), "{site}");
            }
        }
        for expected in ["archive", "pinterest-pins", "linkedin-learning"] {
            if sites.contains(&expected) {
                assert!(hits.iter().any(|hit| hit.platform == expected));
            }
        }
        for expected in ["stackoverflow-users", "google-images"] {
            if sites.contains(&expected) {
                assert!(hits.iter().any(|hit| hit.platform == expected));
            }
        }
        assert!(hits.iter().all(|hit| hit.profile_ref == profile));
        drop(owner);
        drop(platform);
        let reopened = LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(
            reopened
                .creators()
                .unwrap()
                .get_discovery_campaign(id)
                .unwrap(),
            snapshot
        );
        let stopped = reopened
            .advance_discovery_campaign(OpenCliDiscoveryAdvanceInput {
                excluded_platforms: vec![],
                campaign_id: id.into(),
                expected_revision: 1,
            })
            .await
            .unwrap();
        assert_eq!(stopped.step, DiscoveryStep::BudgetExhausted);
        assert_eq!(stopped.campaign.used_calls, N as u32);
        assert_eq!(stopped.counts.qualified, 0);
    }

    /// Manual, bounded integration probe. It reads two public YouTube pages
    /// through the connected official extension and writes only a temp DB.
    #[tokio::test]
    #[ignore = "requires user-connected OpenCLI Chrome profile and live YouTube"]
    async fn approved_campaign_persists_real_search_and_video_evidence() {
        let profile = std::env::var("ONECREW_OPENCLI_PROFILE").expect("dedicated profile ref");
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let creators = platform.creators().unwrap();
        creators.create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "opencli-live-probe".into(),
            brief: "Find AI-tool creators with published demonstrations and verifiable public tool URLs".into(),
            opencli_version: VERIFIED_OPENCLI_VERSION.into(),
            criteria: json!({"min_subscribers": 10000, "requires_ai_tool_demo": true,
                "requires_public_tool_url": true}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(), profile_ref: profile.clone(),
                keywords: vec!["AI tools demo".into()],
            }],
            target_count: 30, max_calls: Some(2),
        }).unwrap();
        creators
            .approve_discovery_campaign("opencli-live-probe", 1)
            .unwrap();
        let search = platform
            .search_discovery_youtube_once("opencli-live-probe", 1, &profile, 10)
            .await
            .expect("live search");
        assert!(search.returned_hits > 0);
        let hits = creators
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: "opencli-live-probe".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert!(!hits.is_empty());
        let campaign = platform
            .verify_discovery_youtube_hit_once("opencli-live-probe", 1, &profile, &hits[0].id)
            .await
            .expect("live video read");
        assert_eq!(campaign.used_calls, 2);
        let counts = creators
            .count_discovery_candidates("opencli-live-probe")
            .unwrap();
        assert_eq!(counts.qualified + counts.pending + counts.rejected, 1);
    }
}

mod contact_audit;
