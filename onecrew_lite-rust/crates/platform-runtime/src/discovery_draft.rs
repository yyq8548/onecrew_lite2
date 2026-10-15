//! Browser AI planning over the existing Creator draft owner. MCP agents already
//! plan with their host model and use campaign.create; this bridge never calls MCP.
use onecrew_creator_runtime::{
    CreateDiscoveryCampaignInput, CreatorRepositoryCallError, DiscoveryCampaign,
    DiscoveryIntentProposal, DiscoveryPlatformScope, ProposedConstraint,
    validate_discovery_intent_proposal,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

use crate::{LocalPlatform, OpenCliDiscoverySources};

pub(crate) mod codex;
mod progress;
mod expansion;
mod query_audit;
pub use progress::DraftProgress;
pub(crate) use progress::{DraftPhase, DraftProgressState};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareDiscoveryDraftInput {
    pub id: String,
    pub brief: String,
    pub profile_ref: String,
    pub opencli_version: String,
    /// None asks AI to interpret the request; Some is an explicit form edit.
    pub target_count: Option<u32>,
    pub max_calls: Option<u32>,
    pub min_followers_override: Option<u64>,
    pub criteria_overrides: Option<DraftCriteriaOverrides>,
    #[serde(default)]
    pub scope_overrides: Vec<DraftScopeOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftScopeOverride {
    pub platform: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftCriteriaOverrides {
    pub region: Option<String>,
    pub language: Option<String>,
    pub category: Option<String>,
    pub requires_ai_tool_demo: bool,
    pub requires_public_tool_url: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryDraftError {
    InvalidInput,
    AuthenticationRequired,
    AccountChanged,
    Busy,
    DependencyUnavailable,
    CodexUnavailable,
    CodexLoginRequired,
    CodexFailed,
    CodexTimedOut,
    InvalidPlan,
    InvalidIntent,
    InvalidScope,
    IncompleteCoverage,
    UnsupportedConditions(Vec<String>),
    DraftConflict,
    ReadUnavailable,
    OutcomeUnknown,
}

impl DiscoveryDraftError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput => "discovery_draft_input_invalid",
            Self::AuthenticationRequired => "discovery_draft_login_required",
            Self::AccountChanged => "discovery_draft_account_changed",
            Self::Busy => "discovery_draft_busy",
            Self::DependencyUnavailable => "discovery_draft_browser_unavailable",
            Self::CodexUnavailable => "discovery_draft_codex_unavailable",
            Self::CodexLoginRequired => "discovery_draft_codex_login_required",
            Self::CodexFailed => "discovery_draft_codex_failed",
            Self::CodexTimedOut => "discovery_draft_codex_timeout",
            Self::InvalidPlan => "discovery_draft_plan_invalid",
            Self::InvalidIntent => "discovery_draft_intent_invalid",
            Self::InvalidScope => "discovery_draft_scope_invalid",
            Self::IncompleteCoverage => "discovery_draft_coverage_incomplete",
            Self::UnsupportedConditions(_) => "discovery_draft_conditions_unsupported",
            Self::DraftConflict => "discovery_draft_conflict",
            Self::ReadUnavailable => "discovery_draft_read_unavailable",
            Self::OutcomeUnknown => "discovery_draft_outcome_unknown",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CodexDraftPlan {
    #[serde(default)]
    table_subject: String,
    target_count: ProposedConstraint<u64>,
    max_calls: ProposedConstraint<u64>,
    #[serde(default)]
    contact_search_terms: Vec<String>,
    intent: DiscoveryIntentProposal,
    scopes: Vec<DraftScope>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DraftScope {
    platform: String,
    keywords: Vec<String>,
}

fn validate_input(input: &PrepareDiscoveryDraftInput) -> Result<(), DiscoveryDraftError> {
    if input.id.is_empty()
        || input.id.len() > 128
        || !input
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        || input.brief.trim().is_empty()
        || input.brief.len() > 4000
        || input.profile_ref.is_empty()
        || input.profile_ref.len() > 128
        || input.profile_ref.chars().any(char::is_control)
        || input.opencli_version != "1.8.8"
        || input.target_count.is_some_and(|n| !(1..=10000).contains(&n))
        || input
            .max_calls
            .is_some_and(|max| !(1..=10000).contains(&max))
        || input
            .min_followers_override
            .is_some_and(|value| value > 9_007_199_254_740_991)
        || input.criteria_overrides.as_ref().is_some_and(|c| {
            [&c.region, &c.language, &c.category]
                .into_iter()
                .flatten()
                .any(|v| v.len() > 4000 || v.chars().any(char::is_control))
        })
        || input.scope_overrides.len() > 64
        || input.scope_overrides.iter().any(|s| {
            s.platform.is_empty()
                || s.platform.len() > 128
                || s.keywords.is_empty()
                || s.keywords.len() > 20
                || s.keywords.iter().any(|k| {
                    k.trim().is_empty() || k.len() > 160 || k.chars().any(char::is_control)
                })
        })
        || input
            .scope_overrides
            .iter()
            .map(|s| &s.platform)
            .collect::<BTreeSet<_>>()
            .len()
            != input.scope_overrides.len()
    {
        return Err(DiscoveryDraftError::InvalidInput);
    }
    Ok(())
}

fn matches_request(campaign: &DiscoveryCampaign, input: &PrepareDiscoveryDraftInput) -> bool {
    campaign.id == input.id
        && campaign.brief == input.brief
        && campaign.opencli_version == input.opencli_version
        && campaign.phase == "draft"
        && campaign.criteria["region"].as_str() == Some("")
        && campaign.used_calls == 0
        && input.target_count.is_none_or(|n| campaign.target_count == n)
        && input.max_calls.is_none_or(|n| campaign.max_calls == Some(n))
        && input
            .min_followers_override
            .is_none_or(|value| campaign.criteria["min_subscribers"].as_u64() == Some(value))
        && input.criteria_overrides.as_ref().is_none_or(|c| {
            [("language", &c.language), ("category", &c.category)]
                .into_iter()
                .all(|(key, value)| {
                    value
                        .as_ref()
                        .is_none_or(|v| campaign.criteria[key].as_str() == Some(v.as_str()))
                })
                && campaign.criteria["requires_ai_tool_demo"].as_bool()
                    == Some(c.requires_ai_tool_demo)
                && campaign.criteria["requires_public_tool_url"].as_bool()
                    == Some(c.requires_public_tool_url)
        })
        && !campaign.scopes.is_empty()
        && input.scope_overrides.iter().all(|requested| {
            campaign.scopes.iter().any(|s| {
                s.platform == requested.platform
                    && (s.keywords == requested.keywords
                        || (campaign.criteria["language"]
                            .as_str()
                            .is_some_and(|l| !l.trim().is_empty())
                            && !named_scope(&requested.platform)
                            && !s.keywords.is_empty()
                            && s.keywords.len() <= requested.keywords.len()))
            })
        })
        && campaign
            .scopes
            .iter()
            .all(|s| s.profile_ref == input.profile_ref)
}

fn text_value(constraint: &ProposedConstraint<String>) -> &str {
    match constraint {
        ProposedConstraint::Specified { value, .. } => value,
        ProposedConstraint::Unspecified | ProposedConstraint::Unrestricted { .. } => "",
    }
}

fn build_draft(
    input: &PrepareDiscoveryDraftInput,
    sources: &OpenCliDiscoverySources,
    mut plan: CodexDraftPlan,
) -> Result<CreateDiscoveryCampaignInput, DiscoveryDraftError> {
    if let ProposedConstraint::Unrestricted { excerpt } = &plan.target_count {
        if excerpt.trim().is_empty() || !input.brief.contains(excerpt) {
            return Err(DiscoveryDraftError::InvalidIntent);
        }
        return Err(DiscoveryDraftError::UnsupportedConditions(vec![excerpt.clone()]));
    }
    let target_count = input.target_count.unwrap_or(
        quoted_execution_count(&input.brief, &plan.target_count)?.unwrap_or(30),
    );
    let max_calls = input.max_calls.or(quoted_execution_count(&input.brief, &plan.max_calls)?);
    // Residence is retired for new Web drafts. Historical approved criteria
    // remain unchanged; this does not migrate or re-evaluate existing Campaigns.
    plan.intent.region = ProposedConstraint::Unspecified;
    // Typed settings are supported even when the planner puts their exact
    // quotation into the wrong bucket. Unrelated hard conditions still block.
    if let Some(settings) = &input.criteria_overrides {
        for (label, value, target) in [
            (
                "Content language",
                &settings.language,
                &mut plan.intent.language,
            ),
            (
                "Content category",
                &settings.category,
                &mut plan.intent.category,
            ),
        ] {
            if let Some(value) = value {
                let line = format!("{label}: {value}");
                if input.brief.lines().any(|text| text == line) {
                    *target = ProposedConstraint::Specified {
                        value: value.clone(),
                        excerpt: line.clone(),
                    };
                    plan.intent.other_hard_conditions.retain(|condition| {
                        condition.excerpt != line && condition.excerpt != *value
                    });
                }
            }
        }
    }
    let registered: Vec<_> = sources
        .sources
        .iter()
        .filter(|s| s.registered)
        .map(|s| s.platform.clone())
        .collect();
    // Bilingual hints can normalize to one suggested query. Deduplicate before
    // intent validation while retaining all distinct topics.
    let mut suggested = BTreeSet::new();
    plan.intent
        .suggested_keywords
        .retain(|query| suggested.insert(query.trim().to_lowercase()));
    let issues = validate_discovery_intent_proposal(&input.brief, &plan.intent, &registered);
    if issues
        .iter()
        .any(|i| i.code != "unsupported_hard_condition")
    {
        log::warn!("discovery draft {} rejected intent: {:?}", input.id, issues);
        return Err(DiscoveryDraftError::InvalidIntent);
    }
    if issues
        .iter()
        .any(|i| i.code == "unsupported_hard_condition")
    {
        // Only validated quotations from this request cross the UI boundary.
        // Never expose model descriptions or silently weaken qualifications.
        let mut excerpts = Vec::new();
        for condition in &plan.intent.other_hard_conditions {
            if !excerpts.contains(&condition.excerpt) {
                excerpts.push(condition.excerpt.clone());
            }
        }
        return Err(DiscoveryDraftError::UnsupportedConditions(excerpts));
    }
    if plan.scopes.len() > registered.len() {
        return Err(DiscoveryDraftError::InvalidScope);
    }
    let mut seen = BTreeSet::new();
    let language = input
        .criteria_overrides
        .as_ref()
        .and_then(|settings| settings.language.as_deref())
        .unwrap_or_else(|| text_value(&plan.intent.language));
    // Translate free-text customer terms before freezing a new draft. Never
    // translate identifiers or fall back to the original wrong-language terms.
    for requested in &input.scope_overrides {
        let source = sources
            .sources
            .iter()
            .find(|s| s.registered && s.platform == requested.platform)
            .ok_or(DiscoveryDraftError::InvalidPlan)?;
        let translate = !language.trim().is_empty()
            && !named_scope(&requested.platform)
            && source.allowed_values.is_empty();
        if let Some(scope) = plan
            .scopes
            .iter_mut()
            .find(|s| s.platform == requested.platform)
        {
            if translate {
                if scope.keywords.len() != requested.keywords.len() {
                    return Err(DiscoveryDraftError::InvalidScope);
                }
                // Equivalent bilingual inputs often translate to the same query.
                let mut unique = BTreeSet::new();
                scope
                    .keywords
                    .retain(|query| unique.insert(query.trim().to_lowercase()));
            } else {
                scope.keywords.clone_from(&requested.keywords);
            }
        } else if translate {
            return Err(DiscoveryDraftError::InvalidScope);
        } else {
            plan.scopes.push(DraftScope {
                platform: requested.platform.clone(),
                keywords: requested.keywords.clone(),
            });
        }
    }
    for scope in &mut plan.scopes {
        let named = scope.platform.ends_with("-tag")
            || scope.platform.ends_with("-node")
            || scope.platform.ends_with("-subreddit")
            || sources.sources.iter().any(|source| {
                source.platform == scope.platform && !source.allowed_values.is_empty()
            });
        if named {
            continue;
        }
        if is_english(language)
            && scope.keywords.iter().any(|query| {
                !query.chars().any(|c| c.is_ascii_alphabetic())
                    || query.chars().any(non_latin_script)
            })
        {
            return Err(DiscoveryDraftError::InvalidScope);
        }
        // Script checks catch obvious retrieval mismatches, not creator-language
        // qualification. Site identifiers remain unchanged.
        if let Some(matches) = query_script(language) {
            let manual = input
                .scope_overrides
                .iter()
                .any(|s| s.platform == scope.platform)
                && !language.trim().is_empty();
            if !scope
                .keywords
                .iter()
                .any(|query| query.chars().any(matches))
                || (manual
                    && scope
                        .keywords
                        .iter()
                        .any(|query| !query.chars().any(matches)))
            {
                return Err(DiscoveryDraftError::InvalidScope);
            }
            scope
                .keywords
                .sort_by_key(|query| !query.chars().any(matches));
        }
    }
    for scope in &plan.scopes {
        let source = sources
            .sources
            .iter()
            .find(|s| s.registered && s.platform == scope.platform)
            .ok_or(DiscoveryDraftError::InvalidPlan)?;
        if !seen.insert(&scope.platform)
            || scope.keywords.is_empty()
            || scope.keywords.len() > 20
            || scope
                .keywords
                .iter()
                .any(|k| k.trim().is_empty() || k.len() > 160 || k.chars().any(char::is_control))
            || (!source.allowed_values.is_empty()
                && scope
                    .keywords
                    .iter()
                    .any(|k| !source.allowed_values.contains(k)))
            || plan
                .intent
                .excluded_platforms
                .iter()
                .any(|p| p.platform == scope.platform)
        {
            log::warn!(
                "discovery draft {} rejected registered scope {}",
                input.id,
                source.platform
            );
            return Err(DiscoveryDraftError::InvalidScope);
        }
    }
    // Relevance determines keywords and order, not permission to omit sources.
    // Only customer-quoted exclusions may narrow the default registered scope.
    if registered.iter().any(|platform| {
        !plan.scopes.iter().any(|scope| &scope.platform == platform)
            && !plan
                .intent
                .excluded_platforms
                .iter()
                .any(|excluded| &excluded.platform == platform)
    }) {
        log::warn!(
            "discovery draft {} rejected incomplete source coverage",
            input.id
        );
        return Err(DiscoveryDraftError::IncompleteCoverage);
    }
    let threshold = input
        .min_followers_override
        .unwrap_or(match plan.intent.min_subscribers {
            ProposedConstraint::Specified { value, .. } => value,
            ProposedConstraint::Unrestricted { .. } => 0,
            ProposedConstraint::Unspecified => 2000,
        });
    if plan.contact_search_terms.len() > 6
        || plan.contact_search_terms.iter().any(|term| {
            term.trim().is_empty()
                || term.chars().count() > 50
                || term.chars().any(|c| {
                    c.is_control()
                        || matches!(
                            c,
                            '"' | '(' | ')' | ':' | '|' | '<' | '>' | '\\' | '/' | '@'
                        )
                })
        })
    {
        return Err(DiscoveryDraftError::InvalidPlan);
    }
    let mut criteria = json!({"category":text_value(&plan.intent.category),
        "region":"", "language":text_value(&plan.intent.language),
        "min_subscribers":threshold, "requires_ai_tool_demo":false, "requires_public_tool_url":false});
    if let Some(c) = &input.criteria_overrides {
        for (key, value) in [("language", &c.language), ("category", &c.category)] {
            if let Some(value) = value {
                criteria[key] = json!(value);
            }
        }
        criteria["requires_ai_tool_demo"] = json!(c.requires_ai_tool_demo);
        criteria["requires_public_tool_url"] = json!(c.requires_public_tool_url);
    }
    if !plan.contact_search_terms.is_empty() {
        criteria["contact_search_terms"] = json!(plan.contact_search_terms);
    }
    // Approval covers additional relevant queries on these exact sources only.
    // Explicit manual keyword plans retain their finite scope.
    criteria["automatic_query_expansion"] = json!(input.scope_overrides.is_empty());
    if !plan.table_subject.trim().is_empty() {
        criteria["table_subject"] = json!(plan.table_subject.trim().chars().filter(|c| !c.is_control()).take(40).collect::<String>());
    }
    Ok(CreateDiscoveryCampaignInput {
        id: input.id.clone(),
        brief: input.brief.clone(),
        opencli_version: input.opencli_version.clone(),
        target_count,
        max_calls,
        criteria,
        scopes: plan
            .scopes
            .into_iter()
            .map(|s| DiscoveryPlatformScope {
                platform: s.platform,
                keywords: s.keywords,
                profile_ref: input.profile_ref.clone(),
            })
            .collect(),
    })
}

fn quoted_execution_count(
    request: &str,
    constraint: &ProposedConstraint<u64>,
) -> Result<Option<u32>, DiscoveryDraftError> {
    match constraint {
        ProposedConstraint::Specified { value, excerpt }
            if (1..=10000).contains(value)
                && !excerpt.trim().is_empty()
                && request.contains(excerpt) => Ok(Some(*value as u32)),
        ProposedConstraint::Unspecified => Ok(None),
        ProposedConstraint::Unrestricted { excerpt }
            if !excerpt.trim().is_empty() && request.contains(excerpt) => Ok(None),
        _ => Err(DiscoveryDraftError::InvalidIntent),
    }
}

fn named_scope(platform: &str) -> bool {
    platform.ends_with("-tag") || platform.ends_with("-node") || platform.ends_with("-subreddit")
}
fn is_english(language: &str) -> bool {
    matches!(
        language.trim().to_lowercase().as_str(),
        "英文" | "英语" | "english" | "en" | "en-us" | "en-gb"
    )
}
fn non_latin_script(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{9fff}' | '\u{3040}'..='\u{30ff}' | '\u{ac00}'..='\u{d7af}' | '\u{1100}'..='\u{11ff}' | '\u{0400}'..='\u{052f}' | '\u{0600}'..='\u{06ff}')
}

fn query_script(language: &str) -> Option<fn(char) -> bool> {
    match language.trim().to_lowercase().as_str() {
        "中文" | "汉语" | "chinese" | "zh" | "zh-cn" | "zh-tw" => {
            Some(|c| matches!(c, '\u{3400}'..='\u{9fff}'))
        }
        "日语" | "日文" | "japanese" | "ja" => {
            Some(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}'))
        }
        "韩语" | "韩文" | "korean" | "ko" => {
            Some(|c| matches!(c, '\u{ac00}'..='\u{d7af}' | '\u{1100}'..='\u{11ff}'))
        }
        "俄语" | "俄文" | "russian" | "ru" => Some(|c| matches!(c, '\u{0400}'..='\u{052f}')),
        "阿拉伯语" | "arabic" | "ar" => Some(|c| matches!(c, '\u{0600}'..='\u{06ff}')),
        // Shared Latin alphabets cannot reliably distinguish English, French,
        // Spanish, etc. Their relevance remains the planner's responsibility.
        _ => None,
    }
}

impl LocalPlatform {
    /// The click authorizes bounded AI planning, never Campaign approval or source reads.
    /// Lost responses reconcile against this stable draft ID before any new model call.
    pub async fn prepare_discovery_draft(
        &self,
        input: PrepareDiscoveryDraftInput,
    ) -> Result<DiscoveryCampaign, DiscoveryDraftError> {
        validate_input(&input)?;
        let account = self
            .library_access_user()
            .await
            .map_err(|_| DiscoveryDraftError::AuthenticationRequired)?;
        let _permit = self
            .discovery_draft_gate
            .try_acquire()
            .map_err(|_| DiscoveryDraftError::Busy)?;
        *self
            .discovery_draft_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner()) =
            Some(DraftProgressState::new(input.id.clone(), account.clone()));
        let result = async {
            let creators = self
                .creators()
                .map_err(|_| DiscoveryDraftError::ReadUnavailable)?;
            match creators
                .get_discovery_campaign_async(input.id.clone())
                .await
            {
                Ok(campaign) => {
                    if !matches_request(&campaign, &input) {
                        return Err(DiscoveryDraftError::DraftConflict);
                    }
                    self.check_draft_account(&account).await?;
                    return Ok(campaign);
                }
                Err(CreatorRepositoryCallError::Owner(code)) if code == "Campaign not found" => {}
                Err(_) => return Err(DiscoveryDraftError::ReadUnavailable),
            }
            let sources: OpenCliDiscoverySources = self.opencli_setup_report().await.into();
            if sources.version.as_deref() != Some(&input.opencli_version)
                || !sources.daemon_running
                || !sources.extension_connected
                || !sources
                    .profiles
                    .iter()
                    .any(|p| p.context_id == input.profile_ref)
            {
                return Err(DiscoveryDraftError::DependencyUnavailable);
            }
            let plan = codex::plan_with_progress(
                &input.brief,
                &sources,
                &input.scope_overrides,
                &|phase| self.draft_phase(phase),
            )
            .await?;
            self.draft_phase(DraftPhase::ValidatingPlan);
            let draft = build_draft(&input, &sources, plan)?;
            self.check_draft_account(&account).await?;
            // A concurrent inbound peer can have created this ID while AI was running.
            // Creator's existing create owner atomically rejects different payloads.
            self.draft_phase(DraftPhase::SavingPlan);
            let campaign = creators
                .create_discovery_campaign_async(draft)
                .await
                .map_err(|_| DiscoveryDraftError::OutcomeUnknown)?;
            self.check_draft_account(&account)
                .await
                .map_err(|_| DiscoveryDraftError::OutcomeUnknown)?;
            if !matches_request(&campaign, &input) {
                return Err(DiscoveryDraftError::OutcomeUnknown);
            }
            Ok(campaign)
        }
        .await;
        self.draft_phase(if result.is_ok() {
            DraftPhase::Finished
        } else {
            DraftPhase::Failed
        });
        if let Err(error) = &result
            && let Some(state) = self
                .discovery_draft_progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
        {
            state.value.error_code = Some(error.code().into());
            if let DiscoveryDraftError::UnsupportedConditions(excerpts) = error {
                state.value.unsupported_conditions = excerpts.clone();
            }
        }
        result
    }

    async fn check_draft_account(&self, expected: &str) -> Result<(), DiscoveryDraftError> {
        match self.library_access_user().await {
            Ok(account) if account == expected => Ok(()),
            _ => Err(DiscoveryDraftError::AccountChanged),
        }
    }
}

#[cfg(test)]
mod tests;
