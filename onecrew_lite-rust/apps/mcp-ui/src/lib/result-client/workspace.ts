import type { App } from "@modelcontextprotocol/ext-apps";
import { parseWorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";
export async function readWorkspaceActivity(app: App) {
  const reply = await app.callServerTool({ name: "onecrew_status", arguments: {} });
  const data = reply.structuredContent;
  if (reply.isError || !data || typeof data.startup !== "object" || !data.startup
    || !("phase" in data.startup) || !["inspecting", "upgrading", "ready", "failed"].includes(data.startup.phase as string)
    || (data.locale !== "zh-CN" && data.locale !== "en-US")
    || (data.workspace_activity_error !== null && data.workspace_activity_error !== "workspace_activity_unavailable")) throw new Error("workspace status unavailable");
  if (!Array.isArray(data.workspace_history)) throw new Error("workspace history unavailable");
  const seen = new Set<string>();
  const history = data.workspace_history.map(value => {
    const activity = parseWorkspaceActivity(value);
    if (!activity || seen.has(activity.campaign_id)) throw new Error("workspace history invalid");
    seen.add(activity.campaign_id);
    return activity;
  });
  return { phase: data.startup.phase as string, locale: data.locale as "zh-CN" | "en-US", history, activity: parseWorkspaceActivity(data.workspace_activity), error: data.workspace_activity_error as string | null };
}
