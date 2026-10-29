import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { App } from "@modelcontextprotocol/ext-apps";
import { parseWorkspaceStatus, retainWorkspaceActivity, mergeWorkspaceHistory, historyForWorkspaceView, WorkspaceStatusCard, type WorkspaceStatus } from "./workspace-status";
import type { WorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";
import { WorkspaceStatusView } from "./workspace-status-view";
const snapshot: WorkspaceStatus = { kind: "workspace", url: "http://127.0.0.1:22124", library_url: "http://127.0.0.1:8091/library", library_error: null, locale: "zh-CN", startup: "ready", workspace_activity: null, workspace_activity_error: null };
function parse(value: unknown) { return parseWorkspaceStatus({ content: [], _meta: { "onecrew/workspace": value } }); }
describe("workspace bootstrap contract", () => {
  it("distinguishes completed recommendations, contact collection and a user pause", () => {
    const activity: WorkspaceActivity = { campaign_id: "yt-ai-agent-094r2-20261028", brief: "YouTube AI Agent", stage: "verifying", qualified: 30, pending: 0, target: 30, attempted: 1, total: 1, used: 101, login_required: 0, read_errors: 1, restricted: 0, stale: false, pipeline: { active_pending: 0, queued_scores: 0, running_scores: 0, discovery_held: false, contact_pending: true, recommendations_complete: true } };
    const render = (value: WorkspaceActivity) => renderToStaticMarkup(<WorkspaceStatusView phase="ready" activity={value} error={null} libraryError={null} locale="zh-CN" updatedAt={null} />);
    expect(render(activity)).toContain("推荐已完成，正在补充公开联系方式");
    expect(render({ ...activity, pipeline: { ...activity.pipeline!, recommendations_complete: false } })).toContain("正在核实");
    const paused = render({ ...activity, stage: "paused" });
    expect(paused).toContain("已暂停");
    expect(paused).not.toContain("正在补充公开联系方式");
  });
  it("starts a newly mounted card at its plan without duplicating older history", () => {
    const first = { campaign_id: "first" } as WorkspaceActivity;
    const second = { campaign_id: "second" } as WorkspaceActivity;
    expect(historyForWorkspaceView([first, second], "second")).toEqual([second]);
    expect(historyForWorkspaceView([first, second], "first")).toEqual([first, second]);
    expect(historyForWorkspaceView([first, second])).toEqual([first, second]);
  });
  it("places older server tasks above the current task when a new card missed their transitions", () => {
    const first = { campaign_id: "first", used: 2 } as WorkspaceActivity;
    const second = { campaign_id: "second", used: 1 } as WorkspaceActivity;
    expect(mergeWorkspaceHistory([second], [first, second], second)).toEqual([first, second]);
    expect(mergeWorkspaceHistory([first, second], [first, second], { ...first, used: 4 })).toEqual([{ ...first, used: 4 }, second]);
  });
  it("preserves old tasks in creation order and updates only the matching task", () => {
    const first = { campaign_id: "first", used: 2 } as WorkspaceActivity;
    const second = { campaign_id: "second", used: 1 } as WorkspaceActivity;
    const history = retainWorkspaceActivity([first], second);
    expect(retainWorkspaceActivity(history, { ...second, used: 3 })).toEqual([first, { ...second, used: 3 }]);
    expect(retainWorkspaceActivity(history, null)).toBe(history);
    expect(retainWorkspaceActivity(history, { ...first, used: 4 })).toEqual([{ ...first, used: 4 }, second]);
  });
  it("shows the stage and exceptions without customer controls", () => {
    const render = (initial: WorkspaceStatus) => renderToStaticMarkup(<WorkspaceStatusCard appClient={{} as App} initial={initial} />);
    const ready = render(snapshot);
    expect(ready).toContain("onecrew_lite 本地服务已连接");
    expect(ready).toContain("请同时打开工作台和动态表格");
    expect(ready).toContain("打开管理端");
    expect(ready).toContain("打开资料库");
    expect(ready).not.toMatch(/<(input|select|textarea)\b/);
    expect(render({ ...snapshot, library_url: null, library_error: "workspace_library_not_packaged" })).toContain("动态表格未连接");
  });
  it("accepts verified local entries without a business result or authority", () => {
    expect(parse(snapshot)).toEqual(snapshot);
    expect(parse({ ...snapshot, library_url: null, library_error: "workspace_library_not_packaged" })).not.toBeNull();
  });
  it("rejects redirects, extra authority, ambiguous failures and unsupported phases", () => {
    for (const patch of [
      { url: "https://example.com" }, { library_url: "http://127.0.0.1:8091/login" },
      { library_url: "http://secret@127.0.0.1:8091/library" },
      { authority: "approved" }, { startup: "searching" }, { library_url: null },
      { library_error: "untrusted <script>" }, { locale: "fr-FR" },
    ]) expect(parse({ ...snapshot, ...patch })).toBeNull();
  });
});
