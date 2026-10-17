import { expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { App } from "@modelcontextprotocol/ext-apps";
import { WorkspaceStatusCard } from "./workspace-status";
import { parseWorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";
import { readWorkspaceActivity } from "@mcp-ui/lib/result-client/workspace";
import { WorkspaceStatusView } from "./workspace-status-view";

it("keeps execution distinct from display disconnect and validates the host timestamp", () => {
  const activity = { campaign_id: "keyboard", brief: "键盘", stage: "searching" as const,
    qualified: 2, pending: 1, target: 5, attempted: 2, total: 4, used: 7, max: 100,
    login_required: 0, read_errors: 0, restricted: 0, stale: false, updated_at: 1000 };
  expect(parseWorkspaceActivity(activity)?.updated_at).toBe(1000);
  const stopped = { ...activity, read_errors: 3, current_read_stops: 0, read_operation_stops: [{ platform: "bilibili", profile_ref: "dedicated", operation: "profile", code: "opencli_invalid_output" }] };
  expect(parseWorkspaceActivity(stopped)).toEqual(stopped);
  expect(() => parseWorkspaceActivity({ ...stopped, read_operation_stops: [{}] })).toThrow();
  for (const timestamp of [0, -1, null, "now", NaN])
    expect(() => parseWorkspaceActivity({ ...activity, updated_at: timestamp })).toThrow();
  const html = renderToStaticMarkup(<WorkspaceStatusView phase="disconnected" activity={activity} error={null}
    libraryError={null} locale="zh-CN" updatedAt={2000} />);
  expect(html).toContain("状态更新已中断");
  expect(html).toContain("上次更新");
  expect(html).toContain("2/5");
  expect(html).not.toContain("已暂停");
});

it("shows the workbench Campaign progress instead of a generic connected message", () => {
  const initial = { kind: "workspace" as const, url: "http://127.0.0.1:22124", library_url: "http://127.0.0.1:8091/library", library_error: null, locale: "zh-CN" as const, startup: "ready" as const,
    workspace_activity: { campaign_id: "water", brief: "我要卖水杯\nContent language: 中文\nContent category: 水杯", stage: "searching" as const, qualified: 12, pending: 3, target: 100, attempted: 5, total: 5, used: 21, max: 500, login_required: 1, read_errors: 1, restricted: 0, stale: false }, workspace_activity_error: null };
  const html = renderToStaticMarkup(<WorkspaceStatusCard appClient={{} as App} initial={initial} />);
  expect(html).toContain("我要卖水杯");
  expect(html).toContain("12/100");
  expect(html).toContain("正在搜索");
  expect(html).toContain("累计读取失败");
  expect(html).not.toContain("Content language");
  expect(html).not.toContain("21/500");
  expect(html).toContain("打开管理端");
  expect(html).not.toMatch(/<(input|select|textarea)\b/);
});

it("refreshes the locale from the same service status as progress", async () => {
  const callServerTool = vi.fn(async () => ({ structuredContent: { locale: "zh-CN", startup: { phase: "ready" }, workspace_history: [], workspace_activity: null, workspace_activity_error: null } }));
  expect(await readWorkspaceActivity({ callServerTool } as unknown as App)).toMatchObject({ locale: "zh-CN", activity: null });
  expect(callServerTool).toHaveBeenCalledExactlyOnceWith({ name: "onecrew_status", arguments: {} });
});

it("keeps scoring and table sync failures distinct from provider execution", () => {
  const activity = { campaign_id: "keyboard", brief: "键盘", stage: "searching" as const,
    qualified: 2, pending: 3, target: 5, attempted: 2, total: 4, used: 7, max: 100,
    login_required: 0, read_errors: 0, restricted: 0, stale: false,
    library_sync_pending: true, deferred_scores: 3, failure_phase: "score" as const };
  expect(parseWorkspaceActivity(activity)).toEqual(activity);
  for (const invalid of [{ deferred_scores: -1 }, { library_sync_pending: "ready" }, { failure_phase: "unknown" }])
    expect(() => parseWorkspaceActivity({ ...activity, ...invalid })).toThrow();
  const html = renderToStaticMarkup(<WorkspaceStatusView phase="ready" activity={activity} error={null}
    libraryError={null} locale="zh-CN" updatedAt={1000} />);
  expect(html).toContain("正在搜索");
  expect(html).toContain("结果待同步");
  expect(html).toContain("后台评分待续：3");
  expect(html).toContain("评分");
  expect(html).not.toMatch(/<(button|input|select|textarea)\b/);
});

it("projects scoring diagnostics and table recovery without leaking arbitrary error text", () => {
  const activity = { campaign_id: "keyboard", brief: "键盘", stage: "error" as const,
    qualified: 2, pending: 1, target: 5, attempted: 2, total: 4, used: 7, max: 100,
    login_required: 0, read_errors: 0, restricted: 0, stale: false,
    failure_phase: "score" as const, failure_code: "review_submit_outcome_unknown",
    library_sync_error: "library_source_stale", library_sync_pending: true, library_synced_at: 1000 };
  expect(parseWorkspaceActivity(activity)).toEqual(activity);
  for (const invalid of [{ failure_code: "secret http://private.invalid" }, { library_sync_error: "Token=private" }, { library_synced_at: -1 }])
    expect(() => parseWorkspaceActivity({ ...activity, ...invalid })).toThrow();
  const html = renderToStaticMarkup(<WorkspaceStatusView phase="ready" activity={activity} error={null}
    libraryError={null} locale="zh-CN" updatedAt={1000} />);
  expect(html).toContain("评分中断");
  expect(html).toContain("review_submit_outcome_unknown");
  expect(html).not.toContain("表格已同步");
  expect(html).not.toContain("读取结果未确认");
});

it("shows a typed stop diagnostic without accepting arbitrary error text or actions", () => {
  const activity = { campaign_id: "phone", brief: "卖手机架", stage: "error" as const, qualified: 0, pending: 0, target: 30, attempted: 11, total: 57, used: 11, max: 200, login_required: 0, read_errors: 5, restricted: 0, stale: false,
    stop: { reason: "request_failed" as const, recorded_at: 1000 } };
  expect(parseWorkspaceActivity(activity)).toEqual(activity);
  for (const stop of [{ reason: "password=secret", recorded_at: 1000 }, { ...activity.stop, authority: "approved" }, { ...activity.stop, recorded_at: 0 }, null]) {
    expect(() => parseWorkspaceActivity({ ...activity, stop })).toThrow();
  }
  const html = renderToStaticMarkup(<WorkspaceStatusCard appClient={{} as App} initial={{ kind: "workspace", url: "http://127.0.0.1:22124", library_url: "http://127.0.0.1:8091/library", library_error: null, locale: "zh-CN", startup: "ready", workspace_activity: activity, workspace_activity_error: null }} />);
  expect(html).toContain("卖手机架");
  expect(html).toContain("执行请求结果未确认");
  expect(html).not.toContain("水杯");
  expect(html).toContain("打开管理端");
  expect(html).not.toMatch(/<(input|select|textarea)\b/);
});
