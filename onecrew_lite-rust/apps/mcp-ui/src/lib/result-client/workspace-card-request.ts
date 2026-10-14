import type { App } from "@modelcontextprotocol/ext-apps";
import { parseCardRequest } from "@mcp-ui/lib/result-contract/workspace-card-request";

/** Only the service authors messages. A rejected/uncertain delivery is not retried. */
export async function requestWorkspaceCard(app: App, viewId: string, manual = false, baseline?: string): Promise<"none" | "delivered" | "unavailable" | "retryable"> {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(viewId)
    || !app.getHostCapabilities()?.message?.text) return "retryable";
  let message: string;
  try {
    const reply = await app.callServerTool({ name: "onecrew_workspace_card_request", arguments: { view_id: viewId, manual, baseline_campaign_id: baseline ?? null } });
    if (reply.isError) return "retryable";
    const data = parseCardRequest(reply.structuredContent);
    if (data.kind === "none") return "none";
    message = data.message;
  } catch { return "retryable"; }
  try {
    const sent = await app.sendMessage({ role: "user", content: [{ type: "text", text: message }] });
    return sent.isError ? "unavailable" : "delivered";
  } catch { return "unavailable"; }
}
