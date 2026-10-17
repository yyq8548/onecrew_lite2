import type { App } from "@modelcontextprotocol/ext-apps";

export async function openWorkspace(app: App, destination: "workbench" | "library"): Promise<void> {
  const reply = await app.callServerTool({ name: "onecrew_workspace_open", arguments: { destination } });
  const data = reply.structuredContent;
  if (reply.isError || !data || typeof data.url !== "string" || typeof data.instance_id !== "string") throw new Error("workspace_open_unavailable");
  const url = new URL(data.url);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !url.port || url.username || url.password || url.search
    || url.pathname !== (destination === "library" ? "/library" : "/") || !/^#onecrew_ticket=[A-Za-z0-9_-]{43}$/u.test(url.hash)) throw new Error("workspace_open_invalid");
  const opened = await app.openLink({ url: url.href });
  if (opened.isError) throw new Error("workspace_open_rejected");
}
