import { createRoot } from "react-dom/client";
import { useState } from "react";
import { WorkspaceStatusView } from "../../src/features/workspace-status-view";
import "../../src/styles/theme.css";
import { HistoryPreview } from "./history";

function Preview() {
  const [state, setState] = useState("searching");
  const [locale, setLocale] = useState<"zh-CN" | "en-US">("zh-CN");
  return <main className="mx-auto max-w-sm space-y-4 p-4">
    <p className="text-sm text-muted-foreground">状态卡验收 · 模拟数据，不执行搜索</p>
    <HistoryPreview />
    <div className="flex gap-2 text-sm">
      <select aria-label="状态" value={state} onChange={e => setState(e.target.value)}>
        {["searching", "verifying", "pausing", "stale", "disconnected"].map(s => <option key={s}>{s}</option>)}
      </select>
      <select aria-label="语言" value={locale} onChange={e => setLocale(e.target.value as typeof locale)}><option>zh-CN</option><option>en-US</option></select>
    </div>
    <WorkspaceStatusView phase={state === "disconnected" ? "disconnected" : "ready"} error={null} libraryError={null} locale={locale} updatedAt={1_790_887_000_000}
      activity={{ campaign_id: "preview", brief: "我要卖水杯，给我找点达人帮我带货", stage: state === "pausing" ? "pausing" : state === "verifying" ? "verifying" : "searching", qualified: 12, pending: 3, target: 100, attempted: 5, total: 5, used: 21, max: 500, login_required: 1, read_errors: 1, restricted: 0, stale: state === "stale" }} />
  </main>;
}
createRoot(document.getElementById("root")!).render(<Preview />);
