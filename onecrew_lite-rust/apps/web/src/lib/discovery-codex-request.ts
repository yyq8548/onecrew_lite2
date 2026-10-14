import type { DiscoveryCampaign } from "./opencli-discovery";

export type DiscoveryCodexRequest = {
  id: string;
  brief: string;
  profileRef: string;
  version: string;
  platforms: string[];
  targetCount: number | null;
  maxCalls: number | null;
  minFollowersOverride?: number | null;
  scopeOverrides?: { platform: string; keywords: string[] }[];
  startAfterPrepare?: boolean;
  criteriaOverrides?: { region: string | null; language: string | null; category: string | null; requires_ai_tool_demo: boolean; requires_public_tool_url: boolean };
};

/** Only closed codes are shown; never render CLI output or a raw rejected value. */
export function discoveryDraftFailureKey(error: unknown) {
  const code = error instanceof Error ? error.message : error;
  switch (code) {
    case "discovery_draft_input_invalid": return "aiInvalidInput";
    case "discovery_draft_plan_invalid": return "aiInvalidPlan";
    case "discovery_draft_intent_invalid": return "aiInvalidIntent";
    case "discovery_draft_scope_invalid": return "aiInvalidScope";
    case "discovery_draft_coverage_incomplete": return "aiIncompleteCoverage";
    case "discovery_draft_codex_unavailable": return "aiCodexUnavailable";
    case "discovery_draft_codex_login_required": return "aiCodexLoginRequired";
    case "discovery_draft_browser_unavailable": return "aiBrowserUnavailable";
    case "discovery_draft_login_required":
    case "discovery_draft_account_changed": return "aiLoginRequired";
    case "discovery_draft_busy": return "aiBusy";
    case "discovery_draft_codex_timeout": return "aiTimeout";
    case "discovery_draft_conditions_unsupported": return "aiUnsupported";
    case "discovery_draft_conflict": return "aiConflict";
    case "discovery_draft_outcome_unknown": return "aiUncertain";
    default: return "aiFailed";
  }
}

export function matchesDiscoveryCodexRequest(campaign: DiscoveryCampaign, request: DiscoveryCodexRequest): boolean {
  return campaign.id === request.id && campaign.phase === "draft"
    && campaign.brief === request.brief && campaign.opencli_version === request.version
    && campaign.scopes.length > 0 && campaign.used_calls === 0
    && (request.targetCount === null || campaign.target_count === request.targetCount)
    && (request.maxCalls === null || campaign.max_calls === request.maxCalls)
    && (request.minFollowersOverride == null || campaign.criteria.min_subscribers === request.minFollowersOverride)
    && (!request.criteriaOverrides || Object.entries(request.criteriaOverrides).every(([key, value]) => value === null || campaign.criteria[key] === value))
    && (!request.scopeOverrides || request.scopeOverrides.every(requested => campaign.scopes.some(scope => scope.platform === requested.platform && (JSON.stringify(scope.keywords) === JSON.stringify(requested.keywords)
      || (typeof campaign.criteria.language === "string" && !!campaign.criteria.language.trim() && !/-(tag|node|subreddit)$/.test(requested.platform)
        && scope.keywords.length > 0 && scope.keywords.length <= requested.keywords.length)))))
    && campaign.scopes.every(scope => scope.profile_ref === request.profileRef && request.platforms.includes(scope.platform));
}

export function canStartPreparedDraft(pending: { id: string; profileRef: string } | null, campaign: DiscoveryCampaign | undefined, selectedId: string | null, profileRef: string): boolean {
  return !!pending && !!campaign && selectedId === pending.id && campaign.id === pending.id
    && pending.profileRef === profileRef && campaign.phase === "draft" && campaign.used_calls === 0
    && campaign.scopes.length > 0 && campaign.scopes.every(scope => scope.profile_ref === profileRef);
}
