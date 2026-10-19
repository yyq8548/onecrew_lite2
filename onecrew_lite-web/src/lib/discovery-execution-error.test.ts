import { describe, expect, it } from "vitest";
import { discoveryExecutionFailureKey } from "./discovery-execution-error";

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
