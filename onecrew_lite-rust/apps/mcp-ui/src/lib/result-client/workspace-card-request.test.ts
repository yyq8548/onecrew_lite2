import type { App } from "@modelcontextprotocol/ext-apps";
import { describe, it, expect, vi } from "vitest";
import { requestWorkspaceCard } from "./workspace-card-request";
const id = "11111111-1111-4111-8111-111111111111";
function client(data: unknown, supported = true) {
  return { getHostCapabilities: () => supported ? { message: { text: {} } } : {},
    callServerTool: vi.fn().mockResolvedValue({ structuredContent: data }),
    sendMessage: vi.fn().mockResolvedValue({}) };
}
describe("workspace card host notification", () => {
  it("can recover host support before any message was attempted", async () => {
    const app = client({ kind: "message", message: "owner message" }, false);
    expect(await requestWorkspaceCard(app as unknown as App, id)).toBe("retryable");
    app.getHostCapabilities = () => ({ message: { text: {} } });
    expect(await requestWorkspaceCard(app as unknown as App, id, true, id)).toBe("delivered");
    expect(app.callServerTool).toHaveBeenCalledWith({ name: "onecrew_workspace_card_request", arguments: { view_id: id, manual: true, baseline_campaign_id: id } });
    expect(app.sendMessage).toHaveBeenCalledTimes(1);
  });
  it("forwards only the server-authored message through the SDK", async () => {
    const app = client({ kind: "message", message: "owner message" });
    expect(await requestWorkspaceCard(app as unknown as App, id)).toBe("delivered");
    expect(app.sendMessage).toHaveBeenCalledWith({ role: "user", content: [{ type: "text", text: "owner message" }] });
  });
  it("does not claim without host support or a valid view", async () => {
    const app = client({ kind: "message", message: "owner message" }, false);
    expect(await requestWorkspaceCard(app as unknown as App, id)).toBe("retryable");
    expect(app.callServerTool).not.toHaveBeenCalled();
  });
  it("rejects malformed contracts and stays silent on no event", async () => {
    for (const data of [{ kind: "message", message: "x", approve: true }, { kind: "message", message: "" }, null, { kind: "none" }]) {
      const app = client(data);
      expect(await requestWorkspaceCard(app as unknown as App, id)).toBe(data?.kind === "none" ? "none" : "retryable");
      expect(app.sendMessage).not.toHaveBeenCalled();
    }
  });
  it("does not retry a refused or uncertain host delivery", async () => {
    const app = client({ kind: "message", message: "owner message" });
    app.sendMessage.mockRejectedValue(new Error("disconnected"));
    expect(await requestWorkspaceCard(app as unknown as App, id)).toBe("unavailable");
    expect(app.sendMessage).toHaveBeenCalledTimes(1);
  });
});
