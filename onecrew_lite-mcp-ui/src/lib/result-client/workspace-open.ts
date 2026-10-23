import type { App } from "@modelcontextprotocol/ext-apps";

export async function openWorkspace(app: App, destination: "workbench" | "library" | "both"): Promise<void> {
  if (!app.getHostCapabilities()?.message?.text) throw new Error("workspace_host_message_unavailable");
  const target = destination === "both" ? "工作台和当前计划的动态表格（没有计划时打开资料库）" : destination === "library" ? "资料库" : "工作台";
  const sent = await app.sendMessage({ role: "user", content: [{ type: "text", text: `我点击了 OneCrew 的打开按钮。请调用 onecrew_web 获取新的授权入口，在 Codex 内置浏览器打开${target}，复用已有对应标签页。明确显示右侧浏览器面板（visibility.set(true)），选中目标标签并读取实际内容；同时打开两个页面时，依次检查后将工作台标签置于前台，保留表格标签。markDeliverable() 仅保留标签，显示完成须确认面板可见、目标标签已选中且页面可读。保留状态卡；不要使用外部浏览器，不要开始搜索或发送邮件。工具不可用、仅排队或读取失败时，请明确说明尚未显示。` }] }, { timeout: 15_000 });
  if (sent.isError) throw new Error("workspace_open_request_rejected");
}
