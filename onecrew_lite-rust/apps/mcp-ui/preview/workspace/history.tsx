import { useRef, useState } from "react";
import type { App } from "@modelcontextprotocol/ext-apps";
import { WorkspaceStatusCard, retainWorkspaceActivity, type WorkspaceStatus } from "@mcp-ui/features/workspace-status";
import type { WorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";

const first: WorkspaceActivity = {
  campaign_id: "history-first", brief: "验收任务一：洗发水", stage: "paused",
  qualified: 1, pending: 2, target: 4, attempted: 2, total: 2, used: 3, max: 20,
  login_required: 0, read_errors: 0, restricted: 0, stale: false,
};
const second: WorkspaceActivity = { ...first, campaign_id: "history-second", brief: "验收任务二：水杯", used: 1 };
const initial: WorkspaceStatus = {
  kind: "workspace", url: "http://127.0.0.1:22124", library_url: "http://127.0.0.1:8091/library",
  library_error: null, locale: "zh-CN", startup: "ready", workspace_activity: first,
  workspace_activity_error: null, viewId: "00000000-0000-4000-8000-000000000001",
};

/** Exercises the production controller; this mock never calls a service or starts a task. */
export function HistoryPreview() {
  const current = useRef(first);
  const serverHistory = useRef([first]);
  const [generation, setGeneration] = useState(0);
  const [mounted, setMounted] = useState(true);
  const [viewId, setViewId] = useState(initial.viewId);
  const [selected, setSelected] = useState(first.brief);
  const [app] = useState(() => ({
    callServerTool: async () => ({ content: [], structuredContent: {
      startup: { phase: "ready" }, locale: "zh-CN",
      workspace_activity: current.current, workspace_activity_error: null,
      workspace_history: serverHistory.current,
    } }),
  }) as unknown as App);
  function select(activity: WorkspaceActivity) {
    current.current = activity;
    serverHistory.current = retainWorkspaceActivity(serverHistory.current, activity);
    setSelected(activity.brief);
    window.dispatchEvent(new Event("focus"));
  }
  return <section className="space-y-4" aria-label="任务历史控制器验收">
    <p>模拟宿主；无业务请求。当前输入：{selected}</p>
    <div className="flex flex-wrap gap-2">
      <button onClick={() => select(first)}>恢复任务一</button>
      <button onClick={() => select(second)}>切换任务二</button>
      <button onClick={() => select({ ...second, used: 2 })}>更新任务二</button>
      <button onClick={() => setGeneration(value => value + 1)}>重载同一视图</button>
      <button onClick={() => setMounted(false)}>卸载卡片</button>
      <button onClick={() => { setViewId(crypto.randomUUID()); setMounted(true); }}>挂载新视图</button>
    </div>
    {mounted && <WorkspaceStatusCard key={`${viewId}:${generation}`} appClient={app} initial={{ ...initial, viewId }} />}
  </section>;
}
