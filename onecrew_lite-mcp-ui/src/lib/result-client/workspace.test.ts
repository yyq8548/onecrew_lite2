import { describe, expect, it } from "vitest";
import type { App } from "@modelcontextprotocol/ext-apps";
import { readWorkspaceActivity } from "./workspace";

const first = { campaign_id: "first", brief: "旧计划", stage: "paused", qualified: 0,
  pending: 1, target: 4, attempted: 1, total: 2, used: 3,
  login_required: 0, read_errors: 0, restricted: 0, stale: false };
const second = { ...first, campaign_id: "second", brief: "新计划", used: 0 };
function app(history: unknown): App {
  return { callServerTool: async () => ({ content: [], structuredContent: {
    startup: { phase: "ready" }, locale: "zh-CN", workspace_activity: first,
    workspace_activity_error: null, workspace_history: history,
  } }) } as unknown as App;
}
describe("service-owned task display history", () => {
  it("recovers a task missed while the card was unmounted even after selection returns", async () => {
    const result = await readWorkspaceActivity(app([first, second]));
    expect(result.activity?.campaign_id).toBe("first");
    expect(result.history.map(item => item.campaign_id)).toEqual(["first", "second"]);
  });
  it("rejects absent, duplicate and non-display history", async () => {
    for (const history of [undefined, [first, first], [null], [{ ...first, authority: "approved" }]]) {
      await expect(readWorkspaceActivity(app(history))).rejects.toThrow();
    }
  });
  it("accepts unlimited read statistics without changing explicit pause", async () => {
    const unlimited = { ...first, campaign_id: "unlimited", used: 10001 };
    const result = await readWorkspaceActivity(app([unlimited]));
    expect(result.history[0]).not.toHaveProperty("max");
    expect(result.history[0].used).toBe(10001);
    expect(result.history[0].stage).toBe("paused");
  });
});
