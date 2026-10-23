import { useEffect, useRef, useState } from "react";
import { Button } from "@mcp-ui/components/ui/button";
import type { App } from "@modelcontextprotocol/ext-apps";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { parseWorkspaceActivity, type WorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";
import { readWorkspaceActivity } from "@mcp-ui/lib/result-client/workspace";
import { requestWorkspaceCard } from "@mcp-ui/lib/result-client/workspace-card-request";
import { openWorkspace } from "@mcp-ui/lib/result-client/workspace-open";
import { WorkspaceStatusView } from "./workspace-status-view";
import { readWorkspaceHistory, writeWorkspaceHistory, validWorkspaceViewId } from "./workspace-history";
import type { LocalServiceStatus } from "@onecrew/runtime-contracts/status";

export type WorkspaceStatus = {
  kind: "workspace"; url: string; library_url: string | null; library_error: string | null;
  locale: "en-US" | "zh-CN"; startup: "inspecting" | "upgrading" | "ready" | "failed";
  workspace_activity: WorkspaceActivity | null; workspace_activity_error: string | null;
  viewId?: string;
};
const phases = ["inspecting", "upgrading", "ready", "failed"] as const;
/** Presentation history only. Switching plans must not replace a previous task's card. */
export function retainWorkspaceActivity(history: WorkspaceActivity[], next: WorkspaceActivity | null): WorkspaceActivity[] {
  if (!next) return history;
  const index = history.findIndex(item => item.campaign_id === next.campaign_id);
  return index < 0 ? [...history, next] : history.map((item, at) => at === index ? next : item);
}
export function mergeWorkspaceHistory(previous: WorkspaceActivity[], server: WorkspaceActivity[], current: WorkspaceActivity | null): WorkspaceActivity[] {
  // A remounted card can bootstrap with the newest task. The service owns the
  // order of tasks missed by that view; don't append older tasks below it.
  const ids = new Set(server.map(item => item.campaign_id));
  return retainWorkspaceActivity([...previous.filter(item => !ids.has(item.campaign_id)), ...server], current);
}
/** A fresh card starts at its initial plan; older cards retain earlier history. */
export function historyForWorkspaceView(server: WorkspaceActivity[], initialCampaign?: string): WorkspaceActivity[] {
  const index = initialCampaign ? server.findIndex(item => item.campaign_id === initialCampaign) : -1;
  return index >= 0 ? server.slice(index) : server;
}
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function localUrl(value: unknown, library: boolean): value is string {
  if (typeof value !== "string") return false;
  try {
    const url = new URL(value);
    return url.protocol === "http:" && url.hostname === "127.0.0.1" && Number(url.port) > 0
      && !url.username && !url.password && !url.search && !url.hash
      && url.pathname === (library ? "/library" : "/");
  } catch { return false; }
}
/** Explicit bootstrap contract, separate from result checkpoints and authority. */
export function parseWorkspaceStatus(payload: CallToolResult): WorkspaceStatus | null {
  const value = payload._meta?.["onecrew/workspace"];
  if (!record(value) || Object.keys(value).sort().join() !== "kind,library_error,library_url,locale,startup,url,workspace_activity,workspace_activity_error"
    || value.kind !== "workspace" || !localUrl(value.url, false)
    || (value.locale !== "zh-CN" && value.locale !== "en-US")
    || !phases.includes(value.startup as WorkspaceStatus["startup"])
    || (value.library_url !== null && !localUrl(value.library_url, true))
    || (value.library_error !== null && (typeof value.library_error !== "string" || !/^workspace_[a-z_]+$/.test(value.library_error)))
    || (value.library_url === null) !== (value.library_error !== null)
    || (value.workspace_activity_error !== null && value.workspace_activity_error !== "workspace_activity_unavailable")) return null;
  try { parseWorkspaceActivity(value.workspace_activity); } catch { return null; }
  return validWorkspaceViewId(payload._meta?.viewUUID)
    ? { ...value, viewId: payload._meta.viewUUID } as WorkspaceStatus
    : value as WorkspaceStatus;
}

// 状态卡只提供页面入口；业务选择和确认仍在工作台完成。
export function WorkspaceStatusCard({ appClient, initial }: { appClient: App; initial: WorkspaceStatus }) {
  const openingRef = useRef(false);
  const [opening, setOpening] = useState(false);
  const [openFailed, setOpenFailed] = useState(false);
  const [openRequested, setOpenRequested] = useState(false);
  const open = async (destination: "workbench" | "library" | "both") => {
    if (openingRef.current) return;
    openingRef.current = true;
    setOpening(true);
    setOpenFailed(false);
    setOpenRequested(false);
    await openWorkspace(appClient, destination).then(
      () => { openingRef.current = false; setOpening(false); setOpenRequested(true); },
      () => { openingRef.current = false; setOpening(false); setOpenFailed(true); },
    );
  };
  const [notificationFailed, setNotificationFailed] = useState(false);
  const [sending, setSending] = useState(false);
  const [phase, setPhase] = useState<string>(initial.startup);
  const [activity, setActivity] = useState(initial.workspace_activity);
  const [history, setHistory] = useState<WorkspaceActivity[]>(() => {
    let saved: WorkspaceActivity[] = [];
    try {
      if (initial.viewId && typeof window !== "undefined") saved = readWorkspaceHistory(window.sessionStorage, initial.viewId);
    } catch { /* Host storage is optional; current server snapshot still renders. */ }
    // Replayed tool output can predate the last live poll in this same view.
    return saved.some(item => item.campaign_id === initial.workspace_activity?.campaign_id)
      ? saved : retainWorkspaceActivity(saved, initial.workspace_activity);
  });
  const [error, setError] = useState(initial.workspace_activity_error);
  const [locale, setLocale] = useState(initial.locale);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const [connections, setConnections] = useState<LocalServiceStatus["workspace_connections"]>();
  const [account, setAccount] = useState<string | null>(null);
  useEffect(() => {
    try {
      if (initial.viewId) writeWorkspaceHistory(window.sessionStorage, initial.viewId, history);
    } catch { /* Host storage is optional. */ }
  }, [initial.viewId, history]);
  useEffect(() => {
    let stopped = false;
    let busy = false;
    let deliveryUnavailable = false;
    let retryAt = 0;
    const refresh = async () => {
      if (busy || stopped || document.visibilityState === "hidden") return;
      busy = true;
      try {
        const status = await readWorkspaceActivity(appClient);
        if (!stopped) {
          setPhase(status.phase); setError(status.error); setLocale(status.locale);
          setConnections(status.connections); setAccount(status.account);
          if (!status.error) {
            setActivity(status.activity);
            setHistory(previous => mergeWorkspaceHistory(previous,
              historyForWorkspaceView(status.history, initial.workspace_activity?.campaign_id), status.activity));
            setUpdatedAt(Date.now());
            if (!deliveryUnavailable && Date.now() >= retryAt && initial.viewId) {
              const outcome = await requestWorkspaceCard(appClient, initial.viewId, false, initial.workspace_activity?.campaign_id);
              if (outcome === "unavailable") deliveryUnavailable = true;
              if (outcome === "retryable") retryAt = Date.now() + 30_000;
              if (outcome === "delivered") setNotificationFailed(false);
              else if (outcome === "unavailable" || outcome === "retryable") setNotificationFailed(true);
            }
          }
        }
      } catch { if (!stopped) { setPhase("disconnected"); setConnections(undefined); setAccount(null); } }
      finally { busy = false; }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 3000);
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => {
      stopped = true; clearInterval(timer);
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", refresh);
    };
  }, [appClient, initial.viewId]);
  return <div className="space-y-4">
    <div className="flex flex-wrap gap-2">
      {initial.library_url && <Button disabled={opening} onClick={() => { void open("both"); }}>{locale === "zh-CN" ? "同时打开工作台和动态表格" : "Open workbench and results table"}</Button>}
      <Button disabled={opening} onClick={() => { void open("workbench"); }}>{locale === "zh-CN" ? "打开管理端" : "Open workbench"}</Button>
      {initial.library_url && <Button disabled={opening} onClick={() => { void open("library"); }}>{locale === "zh-CN" ? "打开资料库" : "Open Library"}</Button>}
    </div>
    <div role="status" className="space-y-1 text-sm">
      <p>{locale === "zh-CN" ? "云端账号：" : "Cloud account: "}{account ?? (locale === "zh-CN" ? "待核实" : "Unverified")}</p>
      <p>{locale === "zh-CN" ? "浏览器：" : "Browser: "}{connections?.browser === "ready" ? (locale === "zh-CN" ? "已授权身份已连接" : "Authorized profile connected") : (locale === "zh-CN" ? "未连接或待核实，请在工作台恢复" : "Disconnected or unverified; recover in the workbench")}</p>
      <p>{locale === "zh-CN" ? "动态表格：" : "Results table: "}{connections?.library === "ready" ? (locale === "zh-CN" ? "当前账号数据可读取" : "Current account data readable") : (locale === "zh-CN" ? "未连接或待核实" : "Disconnected or unverified")}</p>
    </div>
    {opening && <p role="status">{locale === "zh-CN" ? "正在请求 Codex 打开页面…" : "Requesting Codex to open the page…"}</p>}
    {openRequested && <p role="status">{locale === "zh-CN" ? "已请求 Codex 打开页面，等待对话中的页面检查结果。" : "Requested Codex to open the page. Await the page verification in the conversation."}</p>}
    {openFailed && <p role="alert">{locale === "zh-CN" ? "宿主未接受打开请求。请在对话中输入“打开 OneCrew 工作台和资料库”。" : "The host did not accept the request. Ask Codex to open the OneCrew workbench and Library in the conversation."}</p>}
    {history.map(item => <WorkspaceStatusView key={item.campaign_id} phase={item.campaign_id === activity?.campaign_id ? phase : "ready"}
      activity={item} error={item.campaign_id === activity?.campaign_id ? error : null} updatedAt={null}
      historical={item.campaign_id !== activity?.campaign_id}
      libraryError={item.campaign_id === activity?.campaign_id ? initial.library_error : null} locale={locale} />)}
    {!activity && <WorkspaceStatusView phase={phase} activity={null} error={error} updatedAt={updatedAt} libraryError={initial.library_error} locale={locale} />}
    {initial.viewId && <div className="space-y-2">
      {notificationFailed && <p role="status">{locale === "zh-CN" ? "当前客户端未确认接收工作台通知，自动交接尚未确认。任务和进度已保留；如果对话尚未处理任务，可在当前对话中要求继续。已启动的模型处理无需逐条通知。" : "This client has not confirmed the workbench notification, so automatic handoff is unconfirmed. Tasks and progress are saved. If the conversation has not started processing, ask it to continue. An active model loop does not need a notification for every task."}</p>}
      <Button disabled={sending} onClick={async () => {
        setSending(true);
        try {
          const outcome = await requestWorkspaceCard(appClient, initial.viewId!, true, initial.workspace_activity?.campaign_id);
          setNotificationFailed(outcome !== "delivered");
        } finally { setSending(false); }
      }}>{locale === "zh-CN" ? "显示最新状态" : "Show latest status"}</Button>
    </div>}
  </div>;
}
