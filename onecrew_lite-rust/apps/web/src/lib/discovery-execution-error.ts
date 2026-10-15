export function discoveryExecutionFailureKey(code?: string) {
  switch (code) {
    case "discovery_draft_codex_login_required": return "queryCodexLoginRequired";
    case "discovery_draft_codex_unavailable": return "queryCodexUnavailable";
    case "discovery_draft_codex_timeout": return "queryCodexTimeout";
    case "discovery_draft_plan_invalid": return "queryPlanInvalid";
    case "discovery_draft_intent_invalid":
    case "discovery_draft_scope_invalid": return "queryPlanInvalid";
    case "creator_repository_overloaded":
    case "creator_repository_read_pool_unavailable":
    case "creator_repository_locked": return "repositoryBusy";
    case "creator_repository_campaign_missing": return "repositoryCampaignMissing";
    case "creator_repository_closed":
    case "creator_repository_cancelled": return "repositoryStopped";
    case "creator_repository_read_failed":
    case "creator_repository_outcome_unknown": return "repositoryReadFailed";
    case "query_round_authentication_required":
    case "discovery_draft_login_required":
    case "discovery_draft_account_changed": return "queryAccountRequired";
    case "query_round_campaign_changed": return "queryCampaignChanged";
    case "discovery_draft_codex_failed":
    case "query_expansion_failed": return "queryExpansionFailed";
    default: return undefined;
  }
}
