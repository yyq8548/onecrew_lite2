import type { App } from "@modelcontextprotocol/ext-apps";

export async function openWorkspace(app: App, destination: "workbench" | "library" | "both"): Promise<void> {
  if (!app.getHostCapabilities()?.message?.text) throw new Error("workspace_host_message_unavailable");
  const target = destination === "both" ? "工作台和当前计划的动态表格（没有计划时打开资料库）" : destination === "library" ? "资料库" : "工作台";
  const sent = await app.sendMessage({ role: "user", content: [{ type: "text", text: `我点击了 OneCrew 的打开按钮。请调用 onecrew_web 获取新的授权入口，在 Codex 内置浏览器打开${target}，复用已有对应标签页。保留状态卡；不要使用外部浏览器，不要开始搜索或发送邮件。打开后读取页面确认是否实际显示；如果仅排队或失败，请明确说明。` }] }, { timeout: 15_000 });
  if (sent.isError) throw new Error("workspace_open_request_rejected");
}
