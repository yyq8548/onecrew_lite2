export function discoveryProviderFailureKey(code: string) {
  const messages = { provider_request_failed: "sourceRequestFailed", provider_response_http_error: "sourceHttpFailed", provider_endpoint_not_found: "sourceEndpointMissing", provider_response_redirected: "sourceRedirected", provider_response_invalid_json: "sourceInvalidJson", provider_response_shape_invalid: "sourceInvalidShape", provider_source_unavailable: "sourceUnavailable", opencli_invalid_output: "sourceInvalidShape", opencli_timeout: "sourceTimeout", opencli_auth_required: "sourceAuthRequired", opencli_risk_control: "sourceRiskStopped" } as const;
  return `opencliPilot.${messages[code as keyof typeof messages] ?? "technicalStop"}` as const;
}
export function discoveryExecutionFailureKey(code?: string) {
  switch (code) {
    case "discovery_draft_codex_login_required": return "queryCodexLoginRequired";
    case "discovery_draft_codex_unavailable": return "queryCodexUnavailable";
    case "discovery_draft_codex_timeout": return "queryCodexTimeout";
    case "discovery_draft_plan_invalid": return "queryPlanInvalid";
    case "discovery_draft_intent_invalid":
    case "discovery_draft_scope_invalid": return "queryPlanInvalid";
    case "query_round_authentication_required":
    case "discovery_draft_login_required":
    case "discovery_draft_account_changed": return "queryAccountRequired";
    case "query_round_campaign_changed": return "queryCampaignChanged";
    case "discovery_draft_codex_failed":
    case "query_expansion_failed": return "queryExpansionFailed";
    default: return undefined;
  }
}
