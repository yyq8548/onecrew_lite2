//! Durable Campaign authorization. A draft never authorizes provider reads.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CreatorRepositoryCallError, CreatorRuntime};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryPlatformScope {
    pub platform: String,
    pub profile_ref: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct CreateDiscoveryCampaignInput {
    /// Caller-selected stable ID also makes uncertain local write outcomes safe to read back.
    pub id: String,
    pub brief: String,
    pub opencli_version: String,
    pub criteria: Value,
    pub scopes: Vec<DiscoveryPlatformScope>,
    pub target_count: u32,
    pub max_calls: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DiscoveryCampaign {
    pub id: String,
    pub phase: String,
    pub revision: u32,
    pub brief: String,
    pub opencli_version: String,
    pub criteria: Value,
    pub scopes: Vec<DiscoveryPlatformScope>,
    pub platform_runs: Vec<DiscoveryPlatformRun>,
    pub target_count: u32,
    pub max_calls: Option<u32>,
    pub used_calls: u32,
    pub approved_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Bounded local history for selecting a saved plan. Reading never authorizes work.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryCampaignSummary {
    pub id: String,
    pub brief: String,
    pub phase: String,
    pub target_count: u32,
    pub used_calls: u32,
    pub max_calls: Option<u32>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryPlatformRun {
    pub platform: String,
    pub profile_ref: String,
    pub status: String,
    pub cursor: Option<String>,
    pub calls_used: u32,
    pub stop_code: Option<String>,
    pub updated_at: i64,
}

/// Only definite command/parse failures may advance to a *different* already
/// approved keyword automatically. An interrupted or timed-out read is an
/// uncertain provider outcome and needs explicit recovery.
pub(crate) fn can_advance_search_after_stop(stop_code: Option<&str>) -> bool {
    matches!(
        stop_code,
        Some(
            "opencli_command_failed"
                | "opencli_command_unavailable"
                | "opencli_invalid_output"
                | "opencli_output_too_large"
                | "search_completion_failed"
        )
    )
}

/// A completed read whose response cannot be used is local to its target.
/// Its receipt remains failed; only the platform scheduling gate is released.
pub(crate) fn source_local_read_failure(code: Option<&str>) -> bool {
    matches!(
        code,
        Some(
            "opencli_command_failed"
                | "youtube_video_unavailable"
                | "youtube_watch_layout_unsupported"
                | "youtube_author_identity_invalid"
                | "youtube_description_unavailable"
                | "opencli_invalid_output"
                | "opencli_output_too_large"
                | "opencli_timeout"
        )
    )
}

pub(crate) fn read_platform_status(code: Option<&str>) -> &'static str {
    match code {
        None | Some("language_sources_exhausted") => "completed",
        Some("opencli_risk_control") => "risk_control_stop",
        Some("opencli_auth_required") => "unauthorized",
        code if source_local_read_failure(code) => "completed",
        _ => "technical_stop",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct UpsertDiscoveryCandidateInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub platform: String,
    pub source_profile_ref: String,
    pub platform_account_id: String,
    pub display_name: String,
    pub profile_url: String,
    /// Bounded, source-backed summary. Full page bodies are not stored.
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DiscoveryCandidate {
    pub id: String,
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub platform: String,
    pub source_profile_ref: String,
    pub platform_account_id: String,
    pub display_name: String,
    pub profile_url: String,
    pub verdict: String,
    pub evidence: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryCandidateCounts {
    #[serde(default)]
    pub public_page_errors: u32,
    pub qualified: u32,
    pub pending: u32,
    pub rejected: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryReadStop {
    pub platform: String,
    pub profile_ref: String,
    pub operation: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryCampaignIdInput {
    pub campaign_id: String,
}

/// A proposal derived only from durable approved state. Execution still claims
/// its own budget and rechecks the revision; planning never authorizes a read.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiscoveryStep {
    Search {
        platform: String,
        profile_ref: String,
    },
    ReadVideo {
        profile_ref: String,
        search_hit_id: String,
    },
    ReadTranscript {
        profile_ref: String,
        search_hit_id: String,
    },
    ReadProfile {
        search_hit_id: String,
    },
    ReadIdentity {
        search_hit_id: String,
    },
    ReadFacebookIdentity {
        search_hit_id: String,
    },
    ReadWebSource {
        search_hit_id: String,
    },
    ReadSourceAuthor {
        search_hit_id: String,
    },
    ReadContent {
        search_hit_id: String,
    },
    ReadContentTargets {
        search_hit_id: String,
    },
    ReadContactLookup {
        lookup: crate::DiscoveryContactLookup,
    },
    ReadPublicPage {
        profile_ref: String,
        candidate_id: String,
        requested_url: String,
    },
    Busy,
    TargetReached,
    BudgetExhausted,
    NeedsEvidence,
    SearchExhausted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ApproveDiscoveryCampaignInput {
    pub campaign_id: String,
    pub expected_revision: u32,
}

/// A separately authorized increase to the read budget. This does not change
/// the approved criteria, platform scopes or evidence revision.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IncreaseDiscoveryBudgetInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub expected_max_calls: u32,
    pub new_max_calls: u32,
    /// Stable client-selected ID makes an uncertain response safe to retry.
    pub decision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IncreaseDiscoveryTargetInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub expected_target_count: u32,
    pub new_target_count: u32,
    /// Stable client-selected ID makes an uncertain response safe to retry.
    pub decision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ListDiscoveryCandidatesInput {
    pub campaign_id: String,
    pub after_id: Option<String>,
    pub limit: u16,
}

/// One approved, budgeted provider search. The query is selected from the
/// frozen Campaign scope, never supplied by an untrusted caller at execution.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoverySearchClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub platform: String,
    pub profile_ref: String,
    pub call_number: u32,
    pub query: String,
}

/// Reservation for one YouTube video metadata read from a saved search hit.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryVideoClaim {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub profile_ref: String,
    pub search_hit_id: String,
    pub source_url: String,
    pub call_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct FinishDiscoveryVideoInput {
    pub claim: DiscoveryVideoClaim,
    pub candidate: Option<UpsertDiscoveryCandidateInput>,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FinishDiscoverySearchInput {
    pub claim: DiscoverySearchClaim,
    /// `completed`, `technical_stop`, `risk_control_stop`, or `unauthorized`.
    pub status: String,
    pub stop_code: Option<String>,
    /// Bounded source hints committed atomically with a completed search.
    pub search_hits: Vec<DiscoverySearchHitInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoverySearchHitInput {
    pub source_url: String,
    pub title: String,
    pub author_label: String,
    pub author_id: Option<String>,
    pub author_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoverySearchHit {
    pub id: String,
    pub campaign_id: String,
    pub platform: String,
    pub profile_ref: String,
    pub search_keyword: String,
    pub source_url: String,
    pub title: String,
    pub author_label: String,
    pub captured_at: i64,
    pub author_id: Option<String>,
    pub author_url: Option<String>,
    pub author_observed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ListDiscoverySearchHitsInput {
    pub campaign_id: String,
    pub after_id: Option<String>,
    pub limit: u16,
}

pub(crate) fn validate_discovery_candidate(
    input: &UpsertDiscoveryCandidateInput,
) -> Result<(), String> {
    if input.expected_revision == 0
        || input.campaign_id.is_empty()
        || input.platform != "youtube"
        || input.source_profile_ref.is_empty()
        || !(10..=64).contains(&input.platform_account_id.len())
        || !input.platform_account_id.starts_with("UC")
        || !input
            .platform_account_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || input.display_name.trim().is_empty()
        || input.display_name.chars().count() > 200
    {
        return Err("Discovery candidate identity is invalid".into());
    }
    let url = url::Url::parse(&input.profile_url)
        .map_err(|_| "Discovery candidate profile URL is invalid".to_string())?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("www.youtube.com" | "youtube.com"))
        || url.path() != format!("/channel/{}", input.platform_account_id)
    {
        return Err("Discovery candidate profile URL must match the YouTube channel".into());
    }
    let evidence = input
        .evidence
        .as_object()
        .ok_or_else(|| "Discovery evidence must be an object".to_string())?;
    if evidence.contains_key("manual_approval")
        || evidence.contains_key("reviewed_residence")
        || evidence.contains_key("reviewed_apparel_content")
        || evidence.contains_key("reviewed_category_content")
    {
        return Err("Reviewed Campaign evidence must come from the review owner".into());
    }
    let sources = evidence
        .get("source_urls")
        .and_then(Value::as_array)
        .ok_or_else(|| "Discovery evidence needs source_urls".to_string())?;
    if sources.is_empty()
        || sources.len() > 20
        || sources.iter().any(|source| {
            source.as_str().is_none_or(|source| {
                source.len() > 2048
                    || url::Url::parse(source).map_or(true, |url| url.scheme() != "https")
            })
        })
        || input.evidence.to_string().len() > 16_384
    {
        return Err("Discovery evidence must contain bounded HTTPS sources".into());
    }
    Ok(())
}

pub(crate) fn validate_create(input: &CreateDiscoveryCampaignInput) -> Result<(), String> {
    if input.id.is_empty()
        || input.id.len() > 128
        || !input
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Campaign id must be 1-128 safe ASCII characters".into());
    }
    if input.brief.trim().is_empty() || input.brief.chars().count() > 4000 {
        return Err("Campaign brief must be 1-4000 characters".into());
    }
    if input.opencli_version.len() > 64
        || input.opencli_version.split('.').count() != 3
        || input
            .opencli_version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("Campaign requires the inspected OpenCLI version".into());
    }
    if !input.criteria.is_object() || input.criteria.to_string().len() > 16_384 {
        return Err("Campaign criteria must be a bounded object".into());
    }
    let criteria = input.criteria.as_object().expect("checked above");
    if criteria.contains_key("min_subscribers") && criteria.contains_key("min_followers") {
        return Err("Campaign cannot set two competing follower thresholds".into());
    }
    for key in ["min_subscribers", "min_followers"] {
        if criteria
            .get(key)
            .is_some_and(|value| value.as_u64().is_none())
        {
            return Err("Campaign follower threshold must be a nonnegative integer".into());
        }
    }
    for key in ["requires_ai_tool_demo", "requires_public_tool_url"] {
        if criteria
            .get(key)
            .is_some_and(|value| value.as_bool().is_none())
        {
            return Err("Campaign evidence requirement must be a boolean".into());
        }
    }
    for key in ["region", "language"] {
        if criteria.get(key).is_some_and(|value| {
            value.as_str().is_none_or(|value| {
                value.chars().count() > 160 || value.chars().any(char::is_control)
            })
        }) {
            return Err("Campaign region and language must be bounded text".into());
        }
    }
    // The reviewed OpenCLI creator catalog currently has 57 entry points.
    // Keep a bounded ceiling while allowing one approved plan to cover all of them.
    if input.scopes.is_empty() || input.scopes.len() > 64 {
        return Err("Campaign must have 1-64 platform scopes".into());
    }
    if input.target_count == 0 || input.max_calls.is_some_and(|max| max == 0 || max > 10_000) {
        return Err("Campaign target and bounded call budget are required".into());
    }
    let mut keys = std::collections::HashSet::new();
    for scope in &input.scopes {
        if scope.platform.is_empty()
            || scope.platform.len() > 64
            || !scope
                .platform
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || scope.profile_ref.is_empty()
            || scope.profile_ref.len() > 64
            || !scope
                .profile_ref
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || scope.keywords.is_empty()
            || scope.keywords.len() > 20
            || scope.keywords.iter().any(|keyword| {
                keyword.trim().is_empty()
                    || keyword.chars().count() > 160
                    || keyword.chars().any(char::is_control)
            })
        {
            return Err("Campaign scope is invalid".into());
        }
        if !keys.insert((&scope.platform, &scope.profile_ref)) {
            return Err("Campaign has duplicate platform/profile scope".into());
        }
    }
    Ok(())
}

impl CreatorRuntime {
    pub async fn discovery_read_stops_async(
        &self,
        campaign: String,
    ) -> Result<Vec<DiscoveryReadStop>, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            owner.repository.discovery_read_stops(&campaign)
        })
        .await
    }
    pub async fn plan_discovery_step_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
    ) -> Result<DiscoveryStep, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            owner.plan_discovery_step(&campaign_id, expected_revision)
        })
        .await
    }

    pub fn plan_discovery_step(
        &self,
        campaign_id: &str,
        expected_revision: u32,
    ) -> Result<DiscoveryStep, String> {
        self.plan_discovery_step_excluding(campaign_id, expected_revision, &[])
    }

    pub async fn plan_discovery_step_excluding_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
        excluded_platforms: Vec<String>,
    ) -> Result<DiscoveryStep, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            owner.plan_discovery_step_excluding(
                &campaign_id,
                expected_revision,
                &excluded_platforms,
            )
        })
        .await
    }

    pub fn plan_discovery_step_excluding(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        excluded_platforms: &[String],
    ) -> Result<DiscoveryStep, String> {
        self.plan_discovery_step_deferred(campaign_id, expected_revision, excluded_platforms, &[])
    }

    pub async fn plan_discovery_step_deferred_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
        excluded_platforms: Vec<String>,
        deferred_platforms: Vec<String>,
    ) -> Result<DiscoveryStep, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            owner.plan_discovery_step_deferred(
                &campaign_id,
                expected_revision,
                &excluded_platforms,
                &deferred_platforms,
            )
        })
        .await
    }

    pub async fn plan_discovery_step_scoring_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
        excluded_platforms: Vec<String>,
        deferred_platforms: Vec<String>,
        scoring_hits: Vec<String>,
    ) -> Result<DiscoveryStep, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            let fallback = owner.plan_discovery_step_deferred(
                &campaign_id,
                expected_revision,
                &excluded_platforms,
                &deferred_platforms,
            )?;
            if matches!(
                fallback,
                DiscoveryStep::TargetReached | DiscoveryStep::BudgetExhausted
            ) {
                return Ok(fallback);
            }
            let campaign = owner.get_discovery_campaign(&campaign_id)?;
            if campaign
                .platform_runs
                .iter()
                .any(|run| run.status == "searching")
            {
                return Ok(DiscoveryStep::Busy);
            }
            for hit in &scoring_hits {
                let source = owner.repository.read_discovery_review_basis(
                    &campaign_id,
                    expected_revision,
                    hit,
                )?;
                if !excluded_platforms.contains(&source.hit.platform)
                    && let Some(step) = owner.repository.next_discovery_independent_read(
                        &campaign_id,
                        expected_revision,
                        hit,
                        &scoring_hits,
                    )?
                {
                    return Ok(step);
                }
            }
            Ok(fallback)
        })
        .await
    }

    /// A scoring lane temporarily owns these platforms' material. Keep the
    /// target count global, and distinguish temporary backpressure from exhaustion.
    pub fn plan_discovery_step_deferred(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        excluded_platforms: &[String],
        deferred_platforms: &[String],
    ) -> Result<DiscoveryStep, String> {
        let mut campaign = self.get_discovery_campaign(campaign_id)?;
        if campaign.phase != "active" || campaign.revision != expected_revision {
            return Err("Campaign is not active at the approved revision".into());
        }
        if campaign
            .platform_runs
            .iter()
            .any(|run| run.status == "searching")
        {
            return Ok(DiscoveryStep::Busy);
        }
        let counts = self
            .repository
            .count_discovery_candidates_excluding(campaign_id, excluded_platforms)?;
        if counts.qualified >= campaign.target_count {
            return Ok(DiscoveryStep::TargetReached);
        }
        let mut blocked = excluded_platforms.to_vec();
        for platform in deferred_platforms {
            if !blocked.contains(platform) {
                blocked.push(platform.clone());
            }
        }
        let excluded_platforms = blocked.as_slice();
        campaign
            .scopes
            .retain(|scope| !excluded_platforms.contains(&scope.platform));
        campaign
            .platform_runs
            .retain(|run| !excluded_platforms.contains(&run.platform));
        let first_round = campaign.scopes.iter().find_map(|scope| {
            campaign
                .platform_runs
                .iter()
                .find(|run| run.platform == scope.platform && run.status == "unsearched")
        });
        if campaign
            .max_calls
            .is_some_and(|max| campaign.used_calls >= max)
        {
            return Ok(DiscoveryStep::BudgetExhausted);
        }
        // An interrupted provider search has an unknown external outcome.
        // Preserve its charged attempt and stop receipt; the automatic runner
        // must not silently repeat it after a service restart.
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "youtube")
            && let Some(hit) = self.repository.next_discovery_transcript_hit(campaign_id)?
        {
            return Ok(DiscoveryStep::ReadTranscript {
                profile_ref: hit.profile_ref,
                search_hit_id: hit.id,
            });
        }
        if let Some(page) = self
            .repository
            .next_discovery_public_page(campaign_id, excluded_platforms)?
        {
            return Ok(DiscoveryStep::ReadPublicPage {
                profile_ref: page.profile_ref,
                candidate_id: page.candidate_id,
                requested_url: page.requested_url,
            });
        }
        // Contact search is intentionally not scheduled. Native account reads and
        // author-published pages remain available; Google support is retained.
        // Inspect saved author content before spending the next call on another
        // profile. The content claim itself requires that author's completed
        // native profile, so this only advances evidence-ready authors.
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "youtube")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "youtube")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "twitter")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "twitter")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "bilibili")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "bilibili")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "reddit")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "reddit")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "tiktok")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "tiktok")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        // Instagram, like Facebook, keeps ordinary search without automatic post expansion.
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "weibo")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "weibo")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "bluesky")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "bluesky")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "zhihu")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "zhihu")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "hackernews")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "hackernews")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "pinterest")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "pinterest")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "pixiv")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "pixiv")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "linkedin")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "linkedin")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "douyin")
            && let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "douyin")?
        {
            return Ok(DiscoveryStep::ReadContent { search_hit_id });
        }
        if let Some(search_hit_id) = self
            .repository
            .next_discovery_source_author_hit(campaign_id, excluded_platforms)?
        {
            return Ok(DiscoveryStep::ReadSourceAuthor { search_hit_id });
        }
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "facebook")
        {
            if let Some(search_hit_id) = self
                .repository
                .next_discovery_content_hit(campaign_id, "facebook")?
            {
                return Ok(DiscoveryStep::ReadContent { search_hit_id });
            }
            if let Some(search_hit_id) = self
                .repository
                .next_discovery_facebook_target_hit(campaign_id)?
            {
                return Ok(DiscoveryStep::ReadContentTargets { search_hit_id });
            }
        }
        // Instagram retains its existing restriction. Facebook consumes bounded
        // saved originals before another profile; comments still require ambiguity.
        if let Some(search_hit_id) = self
            .repository
            .next_discovery_profile_hit(campaign_id, excluded_platforms)?
        {
            if self
                .repository
                .discovery_profile_needs_identity(campaign_id, &search_hit_id)?
            {
                return Ok(DiscoveryStep::ReadIdentity { search_hit_id });
            }
            return Ok(DiscoveryStep::ReadProfile { search_hit_id });
        }
        if let Some(search_hit_id) = self
            .repository
            .next_discovery_facebook_identity_hit(campaign_id, excluded_platforms)?
        {
            return Ok(DiscoveryStep::ReadFacebookIdentity { search_hit_id });
        }
        // Finish the current author's content, score and contact chain before
        // starting another author or expanding platform coverage. Scores and
        // contacts are committed by the step's owner before planning again.
        if !excluded_platforms
            .iter()
            .any(|platform| platform == "youtube")
            && let Some(hit) = self.repository.next_discovery_video_hit(campaign_id)?
        {
            return Ok(DiscoveryStep::ReadVideo {
                profile_ref: hit.profile_ref,
                search_hit_id: hit.id,
            });
        }
        for origin in ["google", "yahoo"] {
            if !excluded_platforms
                .iter()
                .any(|platform| platform == origin || platform == "youtube")
                && let Some(search_hit_id) = self
                    .repository
                    .next_discovery_content_hit(campaign_id, origin)?
            {
                return Ok(DiscoveryStep::ReadContent { search_hit_id });
            }
        }
        if let Some(search_hit_id) = self
            .repository
            .next_discovery_web_source_hit(campaign_id, excluded_platforms)?
        {
            return Ok(DiscoveryStep::ReadWebSource { search_hit_id });
        }
        if let Some(run) = first_round {
            return Ok(DiscoveryStep::Search {
                platform: run.platform.clone(),
                profile_ref: run.profile_ref.clone(),
            });
        }
        // After saved authors and initial sources, recover a definite source
        // failure with the next distinct approved keyword.
        for scope in &campaign.scopes {
            if campaign.platform_runs.iter().any(|run| {
                run.platform == scope.platform
                    && run.profile_ref == scope.profile_ref
                    && run.status == "technical_stop"
                    && can_advance_search_after_stop(run.stop_code.as_deref())
            }) && self
                .repository
                .next_discovery_search_keyword(campaign_id, scope)?
                .is_some()
            {
                return Ok(DiscoveryStep::Search {
                    platform: scope.platform.clone(),
                    profile_ref: scope.profile_ref.clone(),
                });
            }
        }
        for scope in &campaign.scopes {
            if campaign.platform_runs.iter().any(|run| {
                run.platform == scope.platform
                    && run.profile_ref == scope.profile_ref
                    && run.status == "completed"
            }) && self
                .repository
                .next_discovery_search_keyword(campaign_id, scope)?
                .is_some()
            {
                return Ok(DiscoveryStep::Search {
                    platform: scope.platform.clone(),
                    profile_ref: scope.profile_ref.clone(),
                });
            }
        }
        Ok(if !deferred_platforms.is_empty() {
            DiscoveryStep::Busy
        } else if counts.pending > 0 {
            DiscoveryStep::NeedsEvidence
        } else {
            DiscoveryStep::SearchExhausted
        })
    }

    pub async fn claim_discovery_video_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
        profile_ref: String,
        search_hit_id: String,
    ) -> Result<DiscoveryVideoClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_video(
                &campaign_id,
                expected_revision,
                &profile_ref,
                &search_hit_id,
            )
        })
        .await
    }

    pub fn claim_discovery_video(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        profile_ref: &str,
        search_hit_id: &str,
    ) -> Result<DiscoveryVideoClaim, String> {
        self.repository.claim_discovery_video(
            campaign_id,
            expected_revision,
            profile_ref,
            search_hit_id,
        )
    }

    pub async fn finish_discovery_video_async(
        &self,
        input: FinishDiscoveryVideoInput,
    ) -> Result<DiscoveryCampaign, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.finish_discovery_video(input))
            .await
    }

    pub fn finish_discovery_video(
        &self,
        input: FinishDiscoveryVideoInput,
    ) -> Result<DiscoveryCampaign, String> {
        if let Some(candidate) = &input.candidate {
            validate_discovery_candidate(candidate)?;
        }
        self.repository.finish_discovery_video(input)
    }

    /// Remaining recommendation slots, using the same exclusions as the planner.
    pub async fn discovery_remaining_slots_async(
        &self,
        campaign_id: String,
        excluded_platforms: Vec<String>,
    ) -> Result<u32, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            let campaign = owner.get_discovery_campaign(&campaign_id)?;
            let counts = owner
                .repository
                .count_discovery_candidates_excluding(&campaign_id, &excluded_platforms)?;
            Ok(campaign.target_count.saturating_sub(counts.qualified))
        })
        .await
    }

    pub async fn count_discovery_candidates_async(
        &self,
        campaign_id: String,
    ) -> Result<DiscoveryCandidateCounts, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            owner.count_discovery_candidates(&campaign_id)
        })
        .await
    }

    pub fn count_discovery_candidates(
        &self,
        campaign_id: &str,
    ) -> Result<DiscoveryCandidateCounts, String> {
        if campaign_id.is_empty() {
            return Err("Candidate counts require a Campaign".into());
        }
        self.repository.count_discovery_candidates(campaign_id)
    }

    pub async fn list_discovery_search_hits_async(
        &self,
        input: ListDiscoverySearchHitsInput,
    ) -> Result<Vec<DiscoverySearchHit>, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || owner.list_discovery_search_hits(input))
            .await
    }

    pub fn list_discovery_search_hits(
        &self,
        input: ListDiscoverySearchHitsInput,
    ) -> Result<Vec<DiscoverySearchHit>, String> {
        if input.campaign_id.is_empty() || !(1..=100).contains(&input.limit) {
            return Err("Search hit page requires a Campaign and limit 1-100".into());
        }
        self.repository.list_discovery_search_hits(input)
    }

    pub async fn claim_discovery_search_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
        platform: String,
        profile_ref: String,
    ) -> Result<DiscoverySearchClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.claim_discovery_search(&campaign_id, expected_revision, &platform, &profile_ref)
        })
        .await
    }

    pub fn claim_discovery_search(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        platform: &str,
        profile_ref: &str,
    ) -> Result<DiscoverySearchClaim, String> {
        self.repository.claim_discovery_search(
            campaign_id,
            expected_revision,
            platform,
            profile_ref,
        )
    }

    pub async fn finish_discovery_search_async(
        &self,
        input: FinishDiscoverySearchInput,
    ) -> Result<DiscoveryCampaign, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.finish_discovery_search(input))
            .await
    }

    pub fn finish_discovery_search(
        &self,
        input: FinishDiscoverySearchInput,
    ) -> Result<DiscoveryCampaign, String> {
        self.repository.finish_discovery_search(input)
    }

    /// Startup recovery only: an in-flight read from a dead process remains
    /// budget-spent and must not be mistaken for a completed search.
    pub fn interrupt_discovery_searches(&self) -> Result<u32, String> {
        self.repository.interrupt_discovery_searches()
    }

    pub async fn create_discovery_campaign_async(
        &self,
        input: CreateDiscoveryCampaignInput,
    ) -> Result<DiscoveryCampaign, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.create_discovery_campaign(input))
            .await
    }

    pub fn create_discovery_campaign(
        &self,
        input: CreateDiscoveryCampaignInput,
    ) -> Result<DiscoveryCampaign, String> {
        validate_create(&input)?;
        self.repository.create_discovery_campaign(input)
    }

    pub async fn approve_discovery_campaign_async(
        &self,
        id: String,
        expected_revision: u32,
    ) -> Result<DiscoveryCampaign, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.approve_discovery_campaign(&id, expected_revision)
        })
        .await
    }

    pub fn approve_discovery_campaign(
        &self,
        id: &str,
        expected_revision: u32,
    ) -> Result<DiscoveryCampaign, String> {
        self.repository
            .approve_discovery_campaign(id, expected_revision)
    }

    pub async fn increase_discovery_budget_async(
        &self,
        account: String,
        input: IncreaseDiscoveryBudgetInput,
    ) -> Result<DiscoveryCampaign, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.increase_discovery_budget(&account, input))
            .await
    }

    pub fn increase_discovery_budget(
        &self,
        account: &str,
        input: IncreaseDiscoveryBudgetInput,
    ) -> Result<DiscoveryCampaign, String> {
        if account.trim().is_empty()
            || account.len() > 256
            || input.campaign_id.is_empty()
            || input.campaign_id.len() > 128
            || input.decision_id.trim().is_empty()
            || input.decision_id.len() > 128
            || input.expected_revision == 0
            || input.expected_max_calls == 0
            || input.new_max_calls <= input.expected_max_calls
            || input.new_max_calls > 10_000
        {
            return Err("budget_increase_input_invalid".into());
        }
        self.repository.increase_discovery_budget(account, input)
    }

    pub async fn increase_discovery_target_async(
        &self,
        account: String,
        input: IncreaseDiscoveryTargetInput,
    ) -> Result<DiscoveryCampaign, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.increase_discovery_target(&account, input))
            .await
    }

    pub fn increase_discovery_target(
        &self,
        account: &str,
        input: IncreaseDiscoveryTargetInput,
    ) -> Result<DiscoveryCampaign, String> {
        if account.trim().is_empty()
            || account.len() > 256
            || input.campaign_id.is_empty()
            || input.campaign_id.len() > 128
            || input.decision_id.trim().is_empty()
            || input.decision_id.len() > 128
            || input.expected_revision == 0
            || input.expected_target_count == 0
            || input.new_target_count == 0
            || input.new_target_count == input.expected_target_count
            || input.new_target_count > 10_000
        {
            return Err("target_increase_input_invalid".into());
        }
        self.repository.increase_discovery_target(account, input)
    }

    pub async fn get_discovery_campaign_async(
        &self,
        id: String,
    ) -> Result<DiscoveryCampaign, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || owner.get_discovery_campaign(&id))
            .await
    }

    pub fn get_discovery_campaign(&self, id: &str) -> Result<DiscoveryCampaign, String> {
        self.repository.get_discovery_campaign(id)
    }

    pub async fn discovery_library_table_name_async(
        &self,
        id: String,
    ) -> Result<crate::DiscoveryLibraryTableName, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            owner.repository.discovery_library_table_name(&id)
        })
        .await
    }

    pub async fn recent_discovery_campaigns_async(
        &self,
    ) -> Result<Vec<DiscoveryCampaignSummary>, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || {
            owner.repository.recent_discovery_campaigns()
        })
        .await
    }

    pub async fn upsert_discovery_candidate_async(
        &self,
        input: UpsertDiscoveryCandidateInput,
    ) -> Result<DiscoveryCandidate, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.upsert_discovery_candidate(input))
            .await
    }

    pub fn upsert_discovery_candidate(
        &self,
        input: UpsertDiscoveryCandidateInput,
    ) -> Result<DiscoveryCandidate, String> {
        validate_discovery_candidate(&input)?;
        self.repository.upsert_discovery_candidate(input)
    }

    pub async fn discovery_recommendation_revision_async(
        &self,
        campaign: String,
    ) -> Result<String, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || {
            repository.discovery_recommendation_revision(&campaign)
        })
        .await
    }

    /// Exact local lookup for scheduling; this does not grant recommendation authority.
    pub async fn discovery_candidate_is_qualified_async(
        &self,
        campaign: String,
        candidate: String,
    ) -> Result<bool, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || {
            repository.discovery_candidate_is_qualified(&campaign, &candidate)
        })
        .await
    }

    pub async fn list_discovery_candidates_async(
        &self,
        input: ListDiscoveryCandidatesInput,
    ) -> Result<Vec<DiscoveryCandidate>, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_discovery_read_segment(move || owner.list_discovery_candidates(input))
            .await
    }

    pub fn list_discovery_candidates(
        &self,
        input: ListDiscoveryCandidatesInput,
    ) -> Result<Vec<DiscoveryCandidate>, String> {
        if input.campaign_id.is_empty() || !(1..=100).contains(&input.limit) {
            return Err("Candidate page requires a Campaign and limit 1-100".into());
        }
        self.repository.list_discovery_candidates(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recent_plans_are_bounded_deterministic_and_do_not_authorize_or_consume_reads() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert!(
            owner
                .recent_discovery_campaigns_async()
                .await
                .unwrap()
                .is_empty()
        );
        let mut before = Vec::new();
        for index in 0..6 {
            before.push(
                owner
                    .create_discovery_campaign(CreateDiscoveryCampaignInput {
                        id: format!("recent-{index}"),
                        brief: format!("Shoes plan {index}"),
                        opencli_version: "1.8.8".into(),
                        criteria: serde_json::json!({"min_followers":8000}),
                        scopes: vec![DiscoveryPlatformScope {
                            platform: "youtube".into(),
                            profile_ref: "selected".into(),
                            keywords: vec!["shoes".into()],
                        }],
                        target_count: 100,
                        max_calls: Some(400),
                    })
                    .unwrap(),
            );
        }
        let summaries = owner.recent_discovery_campaigns_async().await.unwrap();
        assert_eq!(
            summaries
                .iter()
                .map(|plan| plan.id.as_str())
                .collect::<Vec<_>>(),
            vec!["recent-5", "recent-4", "recent-3", "recent-2", "recent-1"]
        );
        for summary in summaries {
            assert_eq!(summary.phase, "draft");
            assert_eq!(summary.used_calls, 0);
            assert_eq!(
                summary.brief,
                before
                    .iter()
                    .find(|plan| plan.id == summary.id)
                    .unwrap()
                    .brief
            );
        }
        for plan in before {
            assert_eq!(owner.get_discovery_campaign(&plan.id).unwrap(), plan);
        }
        drop(owner);
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened
                .recent_discovery_campaigns_async()
                .await
                .unwrap()
                .len(),
            5
        );
    }

    #[test]
    fn scoring_deferral_keeps_other_scopes_available_without_charging() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "pipeline".into(),
                brief: "Mouse reviewers".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_followers": 10000}),
                scopes: ["youtube", "bilibili"]
                    .into_iter()
                    .map(|platform| DiscoveryPlatformScope {
                        platform: platform.into(),
                        profile_ref: "dedicated".into(),
                        keywords: vec!["mouse".into()],
                    })
                    .collect(),
                target_count: 10,
                max_calls: Some(20),
            })
            .unwrap();
        owner.approve_discovery_campaign("pipeline", 1).unwrap();
        assert!(
            matches!(owner.plan_discovery_step_deferred("pipeline", 1, &[], &["youtube".into()]).unwrap(),
            DiscoveryStep::Search { platform, .. } if platform == "bilibili")
        );
        assert!(matches!(
            owner
                .plan_discovery_step_deferred(
                    "pipeline",
                    1,
                    &[],
                    &["youtube".into(), "bilibili".into()]
                )
                .unwrap(),
            DiscoveryStep::Busy
        ));
        assert!(
            matches!(owner.plan_discovery_step_deferred("pipeline", 1, &[], &[]).unwrap(),
            DiscoveryStep::Search { platform, .. } if platform == "youtube")
        );
        assert_eq!(
            owner.get_discovery_campaign("pipeline").unwrap().used_calls,
            0
        );
    }

    #[test]
    fn automatic_steps_cover_scopes_checkpoint_failures_and_never_count_pending_as_target() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "advance-test".into(),
                brief: "AI tool creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_followers": 10000}),
                scopes: ["youtube", "bilibili"]
                    .into_iter()
                    .map(|platform| DiscoveryPlatformScope {
                        platform: platform.into(),
                        profile_ref: "dedicated".into(),
                        keywords: vec!["AI tools".into()],
                    })
                    .collect(),
                target_count: 1,
                max_calls: Some(5),
            })
            .unwrap();
        assert!(owner.plan_discovery_step("advance-test", 1).is_err());
        owner.approve_discovery_campaign("advance-test", 1).unwrap();
        assert!(
            matches!(owner.plan_discovery_step("advance-test", 1).unwrap(), DiscoveryStep::Search { platform, .. } if platform == "youtube")
        );
        assert!(
            matches!(owner.plan_discovery_step_excluding("advance-test", 1, &["youtube".into()]).unwrap(),
            DiscoveryStep::Search { platform, .. } if platform == "bilibili")
        );
        assert_eq!(
            owner
                .get_discovery_campaign("advance-test")
                .unwrap()
                .used_calls,
            0
        );
        assert_eq!(
            owner
                .get_discovery_campaign("advance-test")
                .unwrap()
                .platform_runs[0]
                .status,
            "unsearched"
        );
        assert!(owner.plan_discovery_step("advance-test", 2).is_err());
        let video_url = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        for platform in ["youtube", "bilibili"] {
            // Seed both saved platform checkpoints explicitly. The automatic
            // planner now enriches the first author's hit before the next search.
            let profile_ref = "dedicated";
            if platform == "bilibili" {
                assert!(matches!(
                    owner.plan_discovery_step("advance-test", 1).unwrap(),
                    DiscoveryStep::ReadVideo { .. }
                ));
            }
            let claim = owner
                .claim_discovery_search("advance-test", 1, platform, profile_ref)
                .unwrap();
            assert_eq!(
                owner.plan_discovery_step("advance-test", 1).unwrap(),
                DiscoveryStep::Busy
            );
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim,
                    status: if platform == "youtube" {
                        "completed"
                    } else {
                        "risk_control_stop"
                    }
                    .into(),
                    stop_code: (platform != "youtube").then(|| "opencli_risk_control".into()),
                    search_hits: if platform == "youtube" {
                        vec![DiscoverySearchHitInput {
                            source_url: video_url.into(),
                            title: "AI tool demo".into(),
                            author_label: "Creator".into(),
                            author_id: None,
                            author_url: None,
                        }]
                    } else {
                        vec![]
                    },
                })
                .unwrap();
        }
        let DiscoveryStep::ReadVideo {
            profile_ref,
            search_hit_id,
        } = owner.plan_discovery_step("advance-test", 1).unwrap()
        else {
            panic!("unread saved video must be selected");
        };
        let interrupted = owner
            .claim_discovery_video("advance-test", 1, &profile_ref, &search_hit_id)
            .unwrap();
        owner.interrupt_discovery_searches().unwrap();
        assert_eq!(
            owner.plan_discovery_step("advance-test", 1).unwrap(),
            DiscoveryStep::ReadVideo {
                profile_ref: profile_ref.clone(),
                search_hit_id: search_hit_id.clone(),
            }
        );
        let claim = owner
            .claim_discovery_video("advance-test", 1, &profile_ref, &search_hit_id)
            .unwrap();
        assert!(claim.call_number > interrupted.call_number);
        owner
            .finish_discovery_video(FinishDiscoveryVideoInput {
                claim,
                stop_code: None,
                candidate: Some(UpsertDiscoveryCandidateInput {
                    campaign_id: "advance-test".into(),
                    expected_revision: 1,
                    platform: "youtube".into(),
                    source_profile_ref: profile_ref,
                    platform_account_id: "UC1234567890".into(),
                    display_name: "Creator".into(),
                    profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
                    evidence: serde_json::json!({"source_urls": [video_url]}),
                }),
            })
            .unwrap();
        assert_eq!(
            owner
                .count_discovery_candidates("advance-test")
                .unwrap()
                .pending,
            1
        );
        assert!(matches!(
            owner.plan_discovery_step("advance-test", 1).unwrap(),
            DiscoveryStep::ReadTranscript { .. }
        ));
        drop(owner);
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert!(matches!(
            owner.plan_discovery_step("advance-test", 1).unwrap(),
            DiscoveryStep::ReadTranscript { .. }
        ));
        assert_eq!(
            owner
                .get_discovery_campaign("advance-test")
                .unwrap()
                .used_calls,
            4
        );
    }

    #[test]
    fn automatic_steps_stop_before_unapproved_over_budget_or_failed_retries() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        for (id, max_calls) in [("budget-stop", 1), ("failure-stop", 5)] {
            owner
                .create_discovery_campaign(CreateDiscoveryCampaignInput {
                    id: id.into(),
                    brief: "AI creators".into(),
                    opencli_version: "1.8.8".into(),
                    criteria: serde_json::json!({}),
                    scopes: vec![DiscoveryPlatformScope {
                        platform: "youtube".into(),
                        profile_ref: "dedicated".into(),
                        keywords: vec!["AI tools".into(), "AI demos".into()],
                    }],
                    target_count: 30,
                    max_calls: Some(max_calls),
                })
                .unwrap();
            owner.approve_discovery_campaign(id, 1).unwrap();
            let claim = owner
                .claim_discovery_search(id, 1, "youtube", "dedicated")
                .unwrap();
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim,
                    status: "technical_stop".into(),
                    stop_code: Some("opencli_timeout".into()),
                    search_hits: vec![],
                })
                .unwrap();
            let expected = if max_calls == 1 {
                DiscoveryStep::BudgetExhausted
            } else {
                DiscoveryStep::SearchExhausted
            };
            assert_eq!(owner.plan_discovery_step(id, 1).unwrap(), expected);
            if max_calls == 1 {
                assert_eq!(
                    owner
                        .plan_discovery_step_deferred(id, 1, &[], &["youtube".into()])
                        .unwrap(),
                    DiscoveryStep::BudgetExhausted
                );
            }
            assert_eq!(owner.get_discovery_campaign(id).unwrap().used_calls, 1);
        }
    }

    #[test]
    fn rejects_unbounded_or_ambiguous_scope() {
        let mut input = CreateDiscoveryCampaignInput {
            id: "campaign-1".into(),
            brief: "AI tool creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_followers": 10000}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "work".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 20,
            max_calls: Some(50),
        };
        assert!(validate_create(&input).is_ok());
        input.scopes.push(input.scopes[0].clone());
        assert!(validate_create(&input).is_err());
        input.scopes.pop();
        input.max_calls = Some(0);
        assert!(validate_create(&input).is_err());
        input.max_calls = Some(50);
        input.scopes[0].keywords = vec!["AI tools\nignored".into()];
        assert!(validate_create(&input).is_err());
        input.scopes[0].keywords = vec!["AI tools".into()];
        input.criteria = serde_json::json!({"note": "x".repeat(16_384)});
        assert!(validate_create(&input).is_err());
        input.criteria = serde_json::json!({"min_subscribers": "10000"});
        assert!(validate_create(&input).is_err());
        input.criteria = serde_json::json!({"min_subscribers": -1});
        assert!(validate_create(&input).is_err());
    }

    #[test]
    fn accepts_full_creator_source_catalog_with_bounded_scope() {
        let mut input = CreateDiscoveryCampaignInput {
            id: "all-sources".into(),
            brief: "Search all supported creator sources".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_followers": 10000}),
            scopes: (0..57)
                .map(|index| DiscoveryPlatformScope {
                    platform: format!("source-{index}"),
                    profile_ref: "work".into(),
                    keywords: vec!["apparel".into()],
                })
                .collect(),
            target_count: 30,
            max_calls: Some(200),
        };
        assert!(validate_create(&input).is_ok());
        input
            .scopes
            .extend((57..65).map(|index| DiscoveryPlatformScope {
                platform: format!("source-{index}"),
                profile_ref: "work".into(),
                keywords: vec!["apparel".into()],
            }));
        assert!(validate_create(&input).is_err());
    }

    #[test]
    fn draft_needs_exact_user_approval_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let input = CreateDiscoveryCampaignInput {
            id: "campaign-1".into(),
            brief: "Find AI tool creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_followers": 10000}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "work".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 20,
            max_calls: Some(50),
        };
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        let draft = owner.create_discovery_campaign(input).unwrap();
        assert_eq!(draft.phase, "draft");
        assert_eq!(draft.approved_at, None);
        assert!(draft.platform_runs.is_empty());
        assert!(owner.approve_discovery_campaign(&draft.id, 2).is_err());
        let approved = owner.approve_discovery_campaign(&draft.id, 1).unwrap();
        assert_eq!(approved.phase, "active");
        assert!(approved.approved_at.is_some());
        assert_eq!(approved.platform_runs.len(), 1);
        assert_eq!(approved.platform_runs[0].status, "unsearched");
        assert!(owner.approve_discovery_campaign(&draft.id, 1).is_err());
        drop(owner);
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened.get_discovery_campaign(&draft.id).unwrap(),
            approved
        );
    }

    #[test]
    fn budget_increase_is_audited_idempotent_and_keeps_criteria_frozen() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "budget-campaign".into(), brief: "Clothing abroad".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_subscribers":10000,"region":"中国大陆以外","category":"服装"}),
            scopes: vec![DiscoveryPlatformScope { platform:"youtube".into(),
                profile_ref:"dedicated".into(), keywords:vec!["clothing export".into()] }],
            target_count:30, max_calls: Some(200),
        }).unwrap();
        owner
            .approve_discovery_campaign("budget-campaign", 1)
            .unwrap();
        let input = IncreaseDiscoveryBudgetInput {
            campaign_id: "budget-campaign".into(),
            expected_revision: 1,
            expected_max_calls: 200,
            new_max_calls: 400,
            decision_id: "customer-decision-1".into(),
        };
        let before = owner.get_discovery_campaign("budget-campaign").unwrap();
        let updated = owner
            .increase_discovery_budget("account-1", input.clone())
            .unwrap();
        assert_eq!(updated.max_calls, Some(400));
        assert_eq!(updated.revision, before.revision);
        assert_eq!(updated.criteria, before.criteria);
        assert_eq!(updated.scopes, before.scopes);
        assert_eq!(updated.used_calls, before.used_calls);
        assert_eq!(
            owner
                .increase_discovery_budget("account-1", input.clone())
                .unwrap()
                .max_calls,
            Some(400)
        );
        assert!(
            owner
                .increase_discovery_budget("account-2", input.clone())
                .is_err()
        );
        assert!(
            owner
                .increase_discovery_budget(
                    "account-1",
                    IncreaseDiscoveryBudgetInput {
                        decision_id: "customer-decision-2".into(),
                        ..input.clone()
                    }
                )
                .is_err()
        );
        assert!(
            owner
                .increase_discovery_budget(
                    "account-1",
                    IncreaseDiscoveryBudgetInput {
                        decision_id: "lower".into(),
                        expected_max_calls: 400,
                        new_max_calls: 300,
                        ..input
                    }
                )
                .is_err()
        );
        drop(owner);
        let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        let row: (i64,i64,i64) = db.query_row(
            "SELECT count(*),min(previous_max_calls),max(new_max_calls) FROM discovery_budget_increases
             WHERE discovery_campaign_id='budget-campaign' AND operator_account_id='account-1'",
            [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(row, (1, 200, 400));
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened
                .get_discovery_campaign("budget-campaign")
                .unwrap()
                .max_calls,
            Some(400)
        );
    }

    #[test]
    fn candidates_stay_pending_and_never_enter_creator_library_implicitly() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "campaign-2".into(),
                brief: "Find creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_followers": 10000}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI demo".into()],
                }],
                target_count: 20,
                max_calls: Some(50),
            })
            .unwrap();
        let candidate = UpsertDiscoveryCandidateInput {
            campaign_id: "campaign-2".into(),
            expected_revision: 1,
            platform: "youtube".into(),
            source_profile_ref: "dedicated".into(),
            platform_account_id: "UCiv_dpEdqEJKPAdc0wpFAkA".into(),
            display_name: "Greg Baugues".into(),
            profile_url: "https://www.youtube.com/channel/UCiv_dpEdqEJKPAdc0wpFAkA".into(),
            evidence: serde_json::json!({
                "source_urls": ["https://www.youtube.com/watch?v=LR04bU_yV5k"],
                "subscribers_raw": "1.16万位订阅者"
            }),
        };
        assert!(owner.upsert_discovery_candidate(candidate.clone()).is_err());
        owner.approve_discovery_campaign("campaign-2", 1).unwrap();
        let saved = owner.upsert_discovery_candidate(candidate.clone()).unwrap();
        assert_eq!(saved.verdict, "pending");
        let mut forged_review = candidate.clone();
        forged_review.evidence["reviewed_residence"] = serde_json::json!({"verified":true});
        assert!(owner.upsert_discovery_candidate(forged_review).is_err());
        let mut forged_category = candidate.clone();
        forged_category.evidence["reviewed_category_content"] =
            serde_json::json!({"verified":true,"category":"鞋子"});
        assert!(owner.upsert_discovery_candidate(forged_category).is_err());
        let mut other_profile = candidate;
        other_profile.source_profile_ref = "personal".into();
        assert!(owner.upsert_discovery_candidate(other_profile).is_err());
        let listed = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "campaign-2".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(listed, vec![saved]);
        let connection = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        let creators = connection
            .query_row("SELECT COUNT(*) FROM creators", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap();
        assert_eq!(creators, 0);
    }

    #[test]
    fn candidate_verdict_uses_frozen_criteria_and_persists_after_restart() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "verdicts".into(),
                brief: "AI creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_subscribers": 10000,
                "requires_ai_tool_demo": true, "requires_public_tool_url": true}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI demo".into()],
                }],
                target_count: 30,
                max_calls: Some(50),
            })
            .unwrap();
        owner.approve_discovery_campaign("verdicts", 1).unwrap();
        let video = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        let mut candidate = UpsertDiscoveryCandidateInput {
            campaign_id: "verdicts".into(),
            expected_revision: 1,
            platform: "youtube".into(),
            source_profile_ref: "dedicated".into(),
            platform_account_id: "UC1234567890".into(),
            display_name: "AI Maker".into(),
            profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
            evidence: serde_json::json!({"source_urls": [video],
                "subscribers_raw": "12.4K subscribers", "subscribers_source_url": video, "captured_at": 1,
                "own_ai_tool_demo": {"verified": true, "channel_id": "UC1234567890", "source_url": video},
                "public_tool_url": {"verified": true, "tool_name": "Tool", "url": "https://example.com/tool", "source_url": video}}),
        };
        let qualified = owner.upsert_discovery_candidate(candidate.clone()).unwrap();
        assert_eq!(qualified.verdict, "qualified");
        // Raw qualification and ordinary evidence do not publish a recommendation.
        let revision = || {
            owner
                .repository
                .discovery_recommendation_revision("verdicts")
                .unwrap()
        };
        assert_eq!(revision(), "[]");
        let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        db.execute("INSERT INTO discovery_contact_observations
            (candidate_id,discovery_campaign_id,criteria_revision,native_author_id,kind,original_value,normalized_value,source_url,source_excerpt,captured_at,attribution)
            VALUES(?1,'verdicts',1,'UC1234567890','email','hello@example.com','hello@example.com','https://example.com/contact','contact',1,'author_published')", [&qualified.id]).unwrap();
        let with_contact = revision();
        assert_ne!(with_contact, "[]");
        db.execute(
            "UPDATE discovery_candidates SET updated_at=updated_at+1 WHERE id=?1",
            [&qualified.id],
        )
        .unwrap();
        assert_eq!(revision(), with_contact);
        db.execute(
            "UPDATE discovery_candidates SET display_name='Changed name' WHERE id=?1",
            [&qualified.id],
        )
        .unwrap();
        assert_ne!(revision(), with_contact);
        db.execute(
            "UPDATE discovery_candidates SET display_name='AI Maker' WHERE id=?1",
            [&qualified.id],
        )
        .unwrap();
        db.execute("UPDATE discovery_contact_observations SET normalized_value='new@example.com' WHERE candidate_id=?1", [&qualified.id]).unwrap();
        assert_ne!(revision(), with_contact);
        db.execute(
            "DELETE FROM discovery_contact_observations WHERE candidate_id=?1",
            [&qualified.id],
        )
        .unwrap();
        assert_eq!(revision(), "[]");
        assert_eq!(
            qualified.evidence["assessment"]["subscribers_normalized"],
            12_400
        );
        candidate.platform_account_id = "UC0987654321".into();
        candidate.profile_url = "https://www.youtube.com/channel/UC0987654321".into();
        candidate.evidence["subscribers_raw"] = serde_json::json!("9.9K subscribers");
        let rejected = owner.upsert_discovery_candidate(candidate).unwrap();
        assert_eq!(rejected.verdict, "rejected");
        drop(owner);
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        let listed = reopened
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "verdicts".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|item| item.verdict == "qualified"));
        assert!(listed.iter().any(|item| item.verdict == "rejected"));
    }

    #[test]
    fn target_is_progress_not_a_limit_and_pending_never_fills_the_gap() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "target-20".into(),
                brief: "AI tool creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_subscribers": 10000,
                "requires_ai_tool_demo": true, "requires_public_tool_url": true}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI tools".into()],
                }],
                target_count: 20,
                max_calls: Some(50),
            })
            .unwrap();
        crate::discovery_contacts::legacy_approval_fixture(directory.path());
        owner.approve_discovery_campaign("target-20", 1).unwrap();
        let video = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        for index in 0..21 {
            let channel = format!("UC{index:020}");
            let mut evidence = serde_json::json!({"source_urls": [video],
                "subscribers_raw": "12.4K subscribers", "subscribers_source_url": video, "captured_at": 1,
                "own_ai_tool_demo": {"verified": true, "channel_id": channel, "source_url": video},
                "public_tool_url": {"verified": true, "tool_name": "AI Tool", "url": "https://example.com/tool", "source_url": video}});
            if index >= 14 {
                evidence.as_object_mut().unwrap().remove("public_tool_url");
            }
            owner
                .upsert_discovery_candidate(UpsertDiscoveryCandidateInput {
                    campaign_id: "target-20".into(),
                    expected_revision: 1,
                    platform: "youtube".into(),
                    source_profile_ref: "dedicated".into(),
                    platform_account_id: channel.clone(),
                    display_name: format!("Creator {index}"),
                    profile_url: format!("https://www.youtube.com/channel/{channel}"),
                    evidence,
                })
                .unwrap();
        }
        let counts = owner.count_discovery_candidates("target-20").unwrap();
        assert_eq!(
            counts,
            DiscoveryCandidateCounts {
                public_page_errors: 0,
                qualified: 14,
                pending: 7,
                rejected: 0
            }
        );
        assert_eq!(
            owner
                .get_discovery_campaign("target-20")
                .unwrap()
                .target_count,
            20
        );
        // Scheduling needs one current candidate, never every candidate's JSON.
        let candidate = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "target-20".into(),
                after_id: None,
                limit: 100,
            })
            .unwrap()
            .into_iter()
            .find(|row| row.verdict == "qualified")
            .unwrap();
        assert!(
            owner
                .repository
                .discovery_candidate_is_qualified("target-20", &candidate.id)
                .unwrap()
        );
        assert!(
            !owner
                .repository
                .discovery_candidate_is_qualified("other", &candidate.id)
                .unwrap()
        );
        assert!(
            !owner
                .repository
                .discovery_candidate_is_qualified("target-20", "missing")
                .unwrap()
        );
        let started = std::time::Instant::now();
        for _ in 0..100 {
            std::hint::black_box(
                owner
                    .list_discovery_candidates(ListDiscoveryCandidatesInput {
                        campaign_id: "target-20".into(),
                        after_id: None,
                        limit: 100,
                    })
                    .unwrap(),
            );
        }
        let scan_us = started.elapsed().as_micros();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert!(
                owner
                    .repository
                    .discovery_candidate_is_qualified("target-20", &candidate.id)
                    .unwrap()
            );
        }
        eprintln!(
            "candidate lookup, 21 rows, 100 iterations: list_us={scan_us}, exact_us={}",
            started.elapsed().as_micros()
        );
        let connection = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        let creators: u32 = connection
            .query_row("SELECT COUNT(*) FROM creators", [], |row| row.get(0))
            .unwrap();
        assert_eq!(creators, 0);
    }

    #[test]
    fn video_read_precharges_budget_and_recovers_without_accepting_stale_completion() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "video-proof".into(),
                brief: "AI creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_subscribers": 10000, "category": "鞋子"}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI demo".into()],
                }],
                target_count: 30,
                max_calls: Some(6),
            })
            .unwrap();
        owner.approve_discovery_campaign("video-proof", 1).unwrap();
        let search = owner
            .claim_discovery_search("video-proof", 1, "youtube", "dedicated")
            .unwrap();
        let source_url = "https://www.youtube.com/watch?v=LR04bU_yV5k&pp=search";
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: search,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![
                    DiscoverySearchHitInput {
                        source_url: source_url.into(),
                        title: "AI demo".into(),
                        author_label: "Maker".into(),
                        author_id: None,
                        author_url: None,
                    },
                    DiscoverySearchHitInput {
                        source_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
                        title: "Second result".into(),
                        author_label: "Other".into(),
                        author_id: None,
                        author_url: None,
                    },
                    DiscoverySearchHitInput {
                        source_url: "https://www.youtube.com/watch?v=7RV6CC6DqJ8".into(),
                        title: "Third result".into(),
                        author_label: "Other".into(),
                        author_id: None,
                        author_url: None,
                    },
                ],
            })
            .unwrap();
        let hits = owner
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: "video-proof".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        let hit = hits
            .iter()
            .find(|hit| hit.source_url == source_url)
            .unwrap();
        let old = owner
            .claim_discovery_video("video-proof", 1, "dedicated", &hit.id)
            .unwrap();
        assert_eq!(old.call_number, 2);
        assert!(
            owner
                .claim_discovery_video("video-proof", 1, "dedicated", &hit.id)
                .is_err()
        );
        owner
            .finish_discovery_video(FinishDiscoveryVideoInput {
                claim: old,
                candidate: None,
                stop_code: Some("opencli_command_failed".into()),
            })
            .unwrap();
        let after_failure = owner.get_discovery_campaign("video-proof").unwrap();
        assert_eq!(after_failure.platform_runs[0].status, "completed");
        assert_eq!(after_failure.platform_runs[0].stop_code, None);
        assert_ne!(
            owner
                .repository
                .next_discovery_video_hit("video-proof")
                .unwrap()
                .unwrap()
                .id,
            hit.id,
            "automatic planning must skip a failed source and continue other hits"
        );
        let connection = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT stop_code FROM discovery_video_read_failures WHERE search_hit_id=?1",
                    [&hit.id],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "opencli_command_failed"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM discovery_video_completions WHERE search_hit_id=?1",
                    [&hit.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0,
            "a failed read must never become completed evidence"
        );
        let interrupted = owner
            .claim_discovery_video("video-proof", 1, "dedicated", &hit.id)
            .unwrap();
        assert_eq!(interrupted.call_number, 3);
        drop(owner);
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(reopened.interrupt_discovery_searches().unwrap(), 1);
        assert_eq!(
            reopened
                .get_discovery_campaign("video-proof")
                .unwrap()
                .platform_runs[0]
                .stop_code
                .as_deref(),
            Some("interrupted_video_read")
        );
        let current = reopened
            .claim_discovery_video("video-proof", 1, "dedicated", &hit.id)
            .unwrap();
        assert_eq!(current.call_number, 4);
        let mut forged = current.clone();
        forged.source_url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into();
        assert!(
            reopened
                .finish_discovery_video(FinishDiscoveryVideoInput {
                    claim: forged,
                    candidate: None,
                    stop_code: Some("opencli_timeout".into()),
                })
                .is_err()
        );
        assert!(
            reopened
                .finish_discovery_video(FinishDiscoveryVideoInput {
                    claim: interrupted,
                    candidate: None,
                    stop_code: Some("opencli_timeout".into()),
                })
                .is_err()
        );
        let canonical = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        let completed = reopened
            .finish_discovery_video(FinishDiscoveryVideoInput {
                claim: current,
                candidate: Some(UpsertDiscoveryCandidateInput {
                    campaign_id: "video-proof".into(),
                    expected_revision: 1,
                    platform: "youtube".into(),
                    source_profile_ref: "dedicated".into(),
                    platform_account_id: "UC1234567890".into(),
                    display_name: "Maker".into(),
                    profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
                    evidence: serde_json::json!({"source_urls": [source_url, canonical],
                    "subscribers_raw": "11.6K subscribers", "subscribers_source_url": canonical, "captured_at": 1}),
                }),
                stop_code: None,
            })
            .unwrap();
        assert_eq!(completed.used_calls, 4);
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM discovery_video_read_failures WHERE search_hit_id=?1",
                    [&hit.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0,
            "a successful explicit retry clears the source-level stop"
        );
        assert!(
            reopened
                .claim_discovery_video("video-proof", 1, "dedicated", &hit.id)
                .is_err()
        );
        assert_eq!(
            reopened
                .get_discovery_campaign("video-proof")
                .unwrap()
                .used_calls,
            4
        );
        let saved = reopened
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "video-proof".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].verdict, "pending");
        let proof_db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        let binding: (String, String, i64) = proof_db.query_row(
            "SELECT candidate_id,native_author_id,criteria_revision FROM discovery_video_identity_bindings WHERE search_hit_id=?1",
            [&hit.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(binding, (saved[0].id.clone(), "UC1234567890".into(), 1));
        let second = hits
            .iter()
            .find(|candidate| candidate.source_url.ends_with("dQw4w9WgXcQ"))
            .unwrap();
        let second_claim = reopened
            .claim_discovery_video("video-proof", 1, "dedicated", &second.id)
            .unwrap();
        // A stale qualified label with no persisted category proof must not
        // survive the next source-bound evidence update.
        proof_db
            .execute(
                "UPDATE discovery_candidates SET verdict='qualified' WHERE id=?1",
                [&saved[0].id],
            )
            .unwrap();
        reopened
            .finish_discovery_video(FinishDiscoveryVideoInput {
                claim: second_claim,
                candidate: Some(UpsertDiscoveryCandidateInput {
                    campaign_id: "video-proof".into(),
                    expected_revision: 1,
                    platform: "youtube".into(),
                    source_profile_ref: "dedicated".into(),
                    platform_account_id: "UC1234567890".into(),
                    display_name: "Maker".into(),
                    profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
                    evidence: serde_json::json!({"source_urls": [second.source_url],
                    "subscribers_raw": "", "subscribers_source_url": second.source_url,
                    "captured_at": 2, "video_title": "Another AI demo"}),
                }),
                stop_code: None,
            })
            .unwrap();
        let merged = reopened
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "video-proof".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].verdict, "pending");
        assert_eq!(
            merged[0].evidence["assessment"]["subscribers_normalized"],
            11600
        );
        assert_eq!(merged[0].evidence["subscribers_raw"], "11.6K subscribers");
        assert!(
            merged[0].evidence["source_urls"]
                .as_array()
                .unwrap()
                .iter()
                .any(|url| url.as_str() == Some(source_url))
        );
        let other = hits
            .iter()
            .find(|candidate| candidate.source_url.ends_with("7RV6CC6DqJ8"))
            .unwrap();
        let risk = reopened
            .claim_discovery_video("video-proof", 1, "dedicated", &other.id)
            .unwrap();
        let stopped = reopened
            .finish_discovery_video(FinishDiscoveryVideoInput {
                claim: risk,
                candidate: None,
                stop_code: Some("opencli_risk_control".into()),
            })
            .unwrap();
        assert_eq!(stopped.used_calls, 6);
        assert_eq!(stopped.platform_runs[0].status, "risk_control_stop");
        assert!(
            reopened
                .claim_discovery_video("video-proof", 1, "dedicated", &other.id)
                .is_err()
        );
    }

    #[test]
    fn searches_consume_budget_before_read_and_cover_every_scope_first() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "coverage".into(),
                brief: "Find creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({}),
                scopes: vec![
                    DiscoveryPlatformScope {
                        platform: "youtube".into(),
                        profile_ref: "dedicated".into(),
                        keywords: vec!["AI tools".into(), "AI demos".into()],
                    },
                    DiscoveryPlatformScope {
                        platform: "other".into(),
                        profile_ref: "dedicated".into(),
                        keywords: vec!["AI tools".into()],
                    },
                ],
                target_count: 20,
                max_calls: Some(3),
            })
            .unwrap();
        assert!(
            owner
                .claim_discovery_search("coverage", 1, "youtube", "dedicated")
                .is_err()
        );
        owner.approve_discovery_campaign("coverage", 1).unwrap();
        let first = owner
            .claim_discovery_search("coverage", 1, "youtube", "dedicated")
            .unwrap();
        assert_eq!(first.query, "AI tools");
        assert!(
            owner
                .claim_discovery_search("coverage", 1, "youtube", "dedicated")
                .is_err()
        );
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: first.clone(),
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![DiscoverySearchHitInput {
                    source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k".into(),
                    title: "AI tool demo".into(),
                    author_label: "Demo channel".into(),
                    author_id: None,
                    author_url: None,
                }],
            })
            .unwrap();
        let hits = owner
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: "coverage".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].search_keyword, "AI tools");
        assert!(
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim: first,
                    status: "completed".into(),
                    stop_code: None,
                    search_hits: vec![],
                })
                .is_err()
        );
        assert!(matches!(
            owner.plan_discovery_step("coverage", 1).unwrap(),
            DiscoveryStep::ReadVideo { .. }
        ));
        let other = owner
            .claim_discovery_search("coverage", 1, "other", "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: other,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![],
            })
            .unwrap();
        let second = owner
            .claim_discovery_search("coverage", 1, "youtube", "dedicated")
            .unwrap();
        assert_eq!(second.query, "AI demos");
        let campaign = owner.get_discovery_campaign("coverage").unwrap();
        assert_eq!(campaign.used_calls, 3);
        assert!(
            owner
                .claim_discovery_search("coverage", 1, "other", "dedicated")
                .is_err()
        );
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: second,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![],
            })
            .unwrap();
    }

    #[test]
    fn definite_search_failure_waits_for_saved_author_before_next_approved_keyword() {
        assert_definite_search_failure_recovers("opencli_command_failed");
    }

    #[test]
    fn unavailable_search_command_recovers_within_original_approved_scope() {
        assert_definite_search_failure_recovers("opencli_command_unavailable");
    }

    fn assert_definite_search_failure_recovers(stop_code: &str) {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "search-recovery".into(),
                brief: "Find apparel creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_followers": 10000}),
                scopes: vec![
                    DiscoveryPlatformScope {
                        platform: "facebook".into(),
                        profile_ref: "dedicated".into(),
                        keywords: vec!["apparel ecommerce".into(), "fashion brand growth".into()],
                    },
                    DiscoveryPlatformScope {
                        platform: "youtube".into(),
                        profile_ref: "dedicated".into(),
                        keywords: vec!["apparel ecommerce".into()],
                    },
                ],
                target_count: 30,
                max_calls: Some(4),
            })
            .unwrap();
        owner
            .approve_discovery_campaign("search-recovery", 1)
            .unwrap();
        let failed = owner
            .claim_discovery_search("search-recovery", 1, "facebook", "dedicated")
            .unwrap();
        assert_eq!(failed.query, "apparel ecommerce");
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: failed,
                status: "technical_stop".into(),
                stop_code: Some(stop_code.into()),
                search_hits: vec![],
            })
            .unwrap();
        let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        let saved_query: String = db.query_row(
            "SELECT active_query FROM discovery_platform_runs WHERE discovery_campaign_id='search-recovery' AND platform='facebook'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(saved_query, "apparel ecommerce");
        // Older technical-stop receipts cleared active_query. Their charged
        // call count still prevents a repeat of the failed first keyword.
        db.execute(
            "UPDATE discovery_platform_runs SET active_query=NULL WHERE discovery_campaign_id='search-recovery' AND platform='facebook'",
            [],
        ).unwrap();
        assert!(matches!(
            owner.plan_discovery_step("search-recovery", 1).unwrap(),
            DiscoveryStep::Search { platform, .. } if platform == "youtube"
        ));
        assert_eq!(
            owner
                .get_discovery_campaign("search-recovery")
                .unwrap()
                .used_calls,
            1
        );
        let youtube = owner
            .claim_discovery_search("search-recovery", 1, "youtube", "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: youtube,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![DiscoverySearchHitInput {
                    source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k".into(),
                    title: "Apparel growth interview".into(),
                    author_label: "Brand founder".into(),
                    author_id: None,
                    author_url: None,
                }],
            })
            .unwrap();
        let DiscoveryStep::ReadVideo {
            profile_ref,
            search_hit_id,
        } = owner.plan_discovery_step("search-recovery", 1).unwrap()
        else {
            panic!("saved author precedes another keyword on a failed source");
        };
        let claim = owner
            .claim_discovery_video("search-recovery", 1, &profile_ref, &search_hit_id)
            .unwrap();
        owner
            .finish_discovery_video(FinishDiscoveryVideoInput {
                claim,
                candidate: None,
                stop_code: Some("opencli_command_failed".into()),
            })
            .unwrap();
        assert!(matches!(
            owner.plan_discovery_step("search-recovery", 1).unwrap(),
            DiscoveryStep::Search { platform, .. } if platform == "facebook"
        ));
        let second = owner
            .claim_discovery_search("search-recovery", 1, "facebook", "dedicated")
            .unwrap();
        assert_eq!(second.query, "fashion brand growth");
        assert_eq!(
            owner
                .get_discovery_campaign("search-recovery")
                .unwrap()
                .used_calls,
            4
        );
    }

    #[test]
    fn interrupted_read_is_not_free_and_stale_completion_cannot_overwrite_retry() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "recovery".into(),
                brief: "Find creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI tools".into(), "AI demos".into()],
                }],
                target_count: 10,
                max_calls: Some(4),
            })
            .unwrap();
        owner.approve_discovery_campaign("recovery", 1).unwrap();
        let abandoned = owner
            .claim_discovery_search("recovery", 1, "youtube", "dedicated")
            .unwrap();
        drop(owner);
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(reopened.interrupt_discovery_searches().unwrap(), 1);
        let interrupted = reopened.get_discovery_campaign("recovery").unwrap();
        assert_eq!(interrupted.used_calls, 1);
        assert_eq!(
            interrupted.platform_runs[0].stop_code.as_deref(),
            Some("interrupted_read")
        );
        assert_eq!(
            reopened.plan_discovery_step("recovery", 1).unwrap(),
            DiscoveryStep::SearchExhausted
        );
        let retried = reopened
            .claim_discovery_search("recovery", 1, "youtube", "dedicated")
            .unwrap();
        assert_eq!(retried.query, abandoned.query);
        assert_eq!(retried.call_number, 2);
        assert!(
            reopened
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim: abandoned,
                    status: "completed".into(),
                    stop_code: None,
                    search_hits: vec![],
                })
                .is_err()
        );
        let completed = reopened
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: retried,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![],
            })
            .unwrap();
        assert_eq!(completed.used_calls, 2);
        assert_eq!(completed.platform_runs[0].status, "completed");
        drop(reopened);
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert!(matches!(
            reopened.plan_discovery_step("recovery", 1).unwrap(),
            DiscoveryStep::Search { .. }
        ));
        let next = reopened
            .claim_discovery_search("recovery", 1, "youtube", "dedicated")
            .unwrap();
        assert_eq!(next.query, "AI demos");
        assert_eq!(next.call_number, 3);
        reopened
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: next,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![],
            })
            .unwrap();
        assert_eq!(
            reopened.plan_discovery_step("recovery", 1).unwrap(),
            DiscoveryStep::SearchExhausted
        );
        assert_eq!(
            reopened
                .get_discovery_campaign("recovery")
                .unwrap()
                .used_calls,
            3
        );
    }

    #[test]
    fn discovery_video_receipts_survive_bounded_summary_eviction_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "many-videos".into(),
                brief: "AI creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_followers": 10000}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI tools".into()],
                }],
                target_count: 30,
                max_calls: Some(30),
            })
            .unwrap();
        owner.approve_discovery_campaign("many-videos", 1).unwrap();
        let search = owner
            .claim_discovery_search("many-videos", 1, "youtube", "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim: search,
                status: "completed".into(),
                stop_code: None,
                search_hits: (0..23)
                    .map(|n| DiscoverySearchHitInput {
                        source_url: format!("https://www.youtube.com/watch?v={n:011}"),
                        title: "AI demo".into(),
                        author_label: "Same creator".into(),
                        author_id: None,
                        author_url: None,
                    })
                    .collect(),
            })
            .unwrap();
        let mut read_ids = std::collections::HashSet::new();
        for _ in 0..23 {
            // Exercise the metadata ledger independently of the new content lane.
            let hit = owner
                .repository
                .next_discovery_video_hit("many-videos")
                .unwrap()
                .unwrap();
            let profile_ref = hit.profile_ref;
            let search_hit_id = hit.id;
            assert!(read_ids.insert(search_hit_id.clone()));
            let claim = owner
                .claim_discovery_video("many-videos", 1, &profile_ref, &search_hit_id)
                .unwrap();
            let evidence = serde_json::json!({"source_urls": [claim.source_url]});
            owner
                .finish_discovery_video(FinishDiscoveryVideoInput {
                    claim,
                    stop_code: None,
                    candidate: Some(UpsertDiscoveryCandidateInput {
                        campaign_id: "many-videos".into(),
                        expected_revision: 1,
                        platform: "youtube".into(),
                        source_profile_ref: profile_ref,
                        platform_account_id: "UC1234567890".into(),
                        display_name: "Same creator".into(),
                        profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
                        evidence,
                    }),
                })
                .unwrap();
        }
        drop(owner);
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert!(
            owner
                .repository
                .next_discovery_video_hit("many-videos")
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            owner.plan_discovery_step("many-videos", 1).unwrap(),
            DiscoveryStep::ReadTranscript { .. }
        ));
        let candidates = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "many-videos".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].evidence["source_urls"]
                .as_array()
                .unwrap()
                .len(),
            20
        );
        assert_eq!(
            owner
                .count_discovery_candidates("many-videos")
                .unwrap()
                .qualified,
            0
        );
        for id in read_ids {
            assert!(
                owner
                    .claim_discovery_video("many-videos", 1, "dedicated", &id)
                    .is_err()
            );
        }
        assert_eq!(
            owner
                .get_discovery_campaign("many-videos")
                .unwrap()
                .used_calls,
            24
        );
    }

    #[test]
    fn search_hints_and_completion_commit_together_and_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "hints".into(),
                brief: "Find creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI demos".into()],
                }],
                target_count: 5,
                max_calls: Some(1),
            })
            .unwrap();
        owner.approve_discovery_campaign("hints", 1).unwrap();
        let claim = owner
            .claim_discovery_search("hints", 1, "youtube", "dedicated")
            .unwrap();
        assert!(
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim: claim.clone(),
                    status: "completed".into(),
                    stop_code: None,
                    search_hits: vec![DiscoverySearchHitInput {
                        source_url: "http://www.youtube.com/watch?v=LR04bU_yV5k".into(),
                        title: "Demo".into(),
                        author_label: "Author".into(),
                        author_id: None,
                        author_url: None,
                    }],
                })
                .is_err()
        );
        assert_eq!(
            owner.get_discovery_campaign("hints").unwrap().platform_runs[0].status,
            "searching"
        );
        assert!(
            owner
                .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                    campaign_id: "hints".into(),
                    after_id: None,
                    limit: 10,
                })
                .unwrap()
                .is_empty()
        );
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![DiscoverySearchHitInput {
                    source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k".into(),
                    title: "AI tool demo".into(),
                    author_label: "Demo channel".into(),
                    author_id: None,
                    author_url: None,
                }],
            })
            .unwrap();
        drop(owner);
        let reopened =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened
                .get_discovery_campaign("hints")
                .unwrap()
                .platform_runs[0]
                .status,
            "completed"
        );
        let saved = reopened
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: "hints".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            saved[0].source_url,
            "https://www.youtube.com/watch?v=LR04bU_yV5k"
        );
    }
}
