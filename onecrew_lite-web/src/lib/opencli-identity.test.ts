import { beforeEach, describe, expect, it, vi } from "vitest";
import { getDiscoveryIdentity, runDiscoveryCampaign, type DiscoveryCampaign } from "./opencli-discovery";
import { invoke } from "./local-runtime";

vi.mock("./local-runtime", () => ({ invoke: vi.fn() }));
const receipt = {
  claim: { campaign_id: "c", expected_revision: 1, search_hit_id: "h", profile_ref: "dedicated",
    author_handle: "creator", source_url: "https://x.com/i/status/123", post_id: "123", call_number: 2 },
  status: "completed", account_id: "987", observed_source_url: "https://x.com/creator/status/123", stop_code: null, captured_at: 100,
};
const campaign: DiscoveryCampaign = { id: "c", revision: 1, phase: "active", brief: "AI tools", opencli_version: "1.8.8",
  criteria: { min_followers: 10000 }, target_count: 30, used_calls: 1,
  scopes: [{ platform: "twitter", profile_ref: "dedicated", keywords: ["AI tools"] }],
  platform_runs: [{ platform: "twitter", profile_ref: "dedicated", status: "completed", calls_used: 1, stop_code: null }],
};
const result = { campaign: { ...campaign, used_calls: 2 }, counts: { qualified: 0, pending: 0, rejected: 0 },
  step: { kind: "read_identity", search_hit_id: "h" }, identity: receipt, profile: null, transcript: null, content: null };

describe("saved post identity contracts", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("uses only the local receipt read and preserves exact native IDs", async () => {
    vi.mocked(invoke).mockResolvedValue(receipt);
    expect(await getDiscoveryIdentity("c", "h")).toEqual(receipt);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_identity_get", { input: { campaign_id: "c", search_hit_id: "h" } });
  });

  it("rejects another Campaign, hit, source, inconsistent state or lossy ID", async () => {
    for (const invalid of [
      { ...receipt, claim: { ...receipt.claim, campaign_id: "other" } },
      { ...receipt, claim: { ...receipt.claim, search_hit_id: "other" } },
      { ...receipt, observed_source_url: "https://x.com/other/status/123" },
      { ...receipt, account_id: 987 }, { ...receipt, account_id: "0987" },
      { ...receipt, status: "failed", stop_code: "opencli_auth_required" },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce(invalid);
      await expect(getDiscoveryIdentity("c", "h")).rejects.toThrow();
    }
  });

  it("continues after one budgeted identity step, without treating identity as qualification", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(result)
      .mockResolvedValueOnce({ ...result, step: { kind: "needs_evidence" }, identity: null });
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(campaign, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(onStep.mock.calls[0][0].counts.qualified).toBe(0);
  });

  it("stops before another action if the step receipt is absent or belongs to another hit", async () => {
    for (const invalid of [{ ...result, identity: null }, { ...result, step: { kind: "read_identity", search_hit_id: "wrong" } }]) {
      vi.mocked(invoke).mockReset().mockResolvedValueOnce(invalid);
      await expect(runDiscoveryCampaign(campaign, new AbortController().signal, vi.fn())).rejects.toThrow();
      expect(invoke).toHaveBeenCalledTimes(1);
    }
  });
});
