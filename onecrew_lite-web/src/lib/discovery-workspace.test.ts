import { describe, expect, it, vi } from "vitest";
import { createWorkspacePresence, discoveryStage } from "./discovery-workspace";
import type { DiscoveryCampaign } from "./opencli-discovery";

describe("workbench status ownership", () => {
  it("publishes skipped platforms with a paused selection without changing campaign authority", async () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const publish = createWorkspacePresence(send);
    await publish("phone", "paused", false, null, ["jike", "linkedin-salesnav"]);
    expect(send.mock.calls[0][0].excluded_platforms).toEqual(["jike", "linkedin-salesnav"]);
    expect(send.mock.calls[0][0].campaign_id).toBe("phone");
    expect(send.mock.calls[0][0].activate).toBe(false);
  });
  it("does not claim the runner is active after an uncertain request failed", () => {
    const campaign = { used_calls: 11, max_calls: 200, platform_runs: [{ status: "searching" }] } as DiscoveryCampaign;
    expect(discoveryStage(campaign, false, false, "error")).toBe("error");
    expect(discoveryStage(campaign, false, false, "paused")).toBe("paused");
  });
  it("keeps an in-flight read visible while pausing or using the last budget call", () => {
    const campaign = { used_calls: 5, max_calls: 5, platform_runs: [{ status: "searching" }] } as DiscoveryCampaign;
    expect(discoveryStage(campaign, true, true, "")).toBe("pausing");
    expect(discoveryStage(campaign, true, false, "")).toBe("searching");
    expect(discoveryStage(campaign, false, false, "")).toBe("searching");
    expect(discoveryStage({ ...campaign, platform_runs: [] }, false, false, "")).toBe("budget_exhausted");
    expect(discoveryStage(undefined, false, false, "search_exhausted")).toBe("needs_evidence");
    expect(discoveryStage(undefined, false, false, "target_reached")).toBe("finished");
  });
  it("serializes presentation changes and survives a failed heartbeat without sending authority", async () => {
    const send = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(undefined);
    const publish = createWorkspacePresence(send);
    const first = publish("old", "searching", true).catch(() => {});
    const second = publish("new", "paused", true);
    const third = publish(null, "paused", false);
    await Promise.all([first, second, third]);
    expect(send.mock.calls.map(([input]) => [input.sequence, input.campaign_id])).toEqual([[1, "old"], [2, "new"], [3, null]]);
    expect(Object.keys(send.mock.calls[0][0]).sort()).toEqual(["activate", "campaign_id", "sequence", "session_id", "stage"]);
  });
  it("does not attach another Campaign's stop diagnostic to a selection", async () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const publish = createWorkspacePresence(send);
    const stop = { campaign_id: "phone", revision: 1, used: 11, max: 200, recorded_at: 1000, reason: "page_closed" as const };
    await publish("water", "paused", true, stop);
    await publish("phone", "paused", true, stop);
    expect(send.mock.calls[0][0]).not.toHaveProperty("stop");
    expect(send.mock.calls[1][0].stop).toEqual({ reason: "page_closed", recorded_at: 1000 });
  });
});
