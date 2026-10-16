import { describe, expect, it } from "vitest";
import { discoveryExecutionFailureKey, discoveryProviderFailureKey } from "./discovery-execution-error";

it.each([
  ["provider_response_http_error", "sourceHttpFailed"],
  ["provider_endpoint_not_found", "sourceEndpointMissing"],
  ["provider_response_redirected", "sourceRedirected"],
  ["provider_response_invalid_json", "sourceInvalidJson"],
  ["provider_response_shape_invalid", "sourceInvalidShape"],
  ["provider_source_unavailable", "sourceUnavailable"],
  ["opencli_auth_required", "sourceAuthRequired"],
  ["opencli_risk_control", "sourceRiskStopped"],
])("renders a readable provider stop for %s", (code, message) => {
  expect(discoveryProviderFailureKey(code)).toBe(`opencliPilot.${message}`);
});

it.each([
  ["creator_repository_overloaded", "repositoryBusy"],
  ["creator_repository_campaign_missing", "repositoryCampaignMissing"],
  ["creator_repository_outcome_unknown", "repositoryReadFailed"],
  ["discovery_draft_scope_invalid", "queryPlanInvalid"],
])("explains the exact execution failure %s", (code, message) => {
  expect(discoveryExecutionFailureKey(code)).toBe(message);
});

describe("搜索执行错误说明", () => {
  it("按实际错误码区分登录、启动、超时和计划校验", () => {
    expect(discoveryExecutionFailureKey("discovery_draft_codex_login_required")).toBe("queryCodexLoginRequired");
    expect(discoveryExecutionFailureKey("discovery_draft_codex_unavailable")).toBe("queryCodexUnavailable");
    expect(discoveryExecutionFailureKey("discovery_draft_codex_timeout")).toBe("queryCodexTimeout");
    expect(discoveryExecutionFailureKey("discovery_draft_plan_invalid")).toBe("queryPlanInvalid");
    expect(discoveryExecutionFailureKey("query_round_authentication_required")).toBe("queryAccountRequired");
    expect(discoveryExecutionFailureKey("query_round_campaign_changed")).toBe("queryCampaignChanged");
    expect(discoveryExecutionFailureKey(undefined)).toBeUndefined();
  });
});
