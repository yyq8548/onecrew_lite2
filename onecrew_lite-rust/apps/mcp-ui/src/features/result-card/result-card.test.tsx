import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  AGENT_EDIT_FORM_ID,
  AgentEditActions,
  AgentEditRequest,
  LoadingShell,
  resultClientFailureCopy,
  ResultBusinessView,
  ResultCardView,
  resultStatusPresentation,
} from "@mcp-ui/features/result-card";
import { ResultShell } from "@mcp-ui/features/result-card/components/shell";
import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { AppLocaleProvider } from "@mcp-ui/lib/i18n";
import { confirmResult } from "@mcp-ui/lib/result-client";
import { appMock } from "@mcp-ui/test-support/app-mock";
import { DM_FAILURE, dmFailedSnapshot, dmReviewSnapshot } from "@mcp-ui/test-support/dm-failure-fixture";
import { creatorBatchSnapshot } from "@mcp-ui/test-support/creator-batch-fixture";
import { BINDING, SNAPSHOT, choiceSnapshot, terminalSnapshot } from "@mcp-ui/test-support/fixtures";

const APP_SOURCES = import.meta.glob([
  "/src/app/**/*.{ts,tsx}",
  "/src/components/**/*.{ts,tsx}",
  "/src/features/**/*.{ts,tsx}",
  "/src/lib/**/*.{ts,tsx}",
  "/src/widgets/operation-inspector/**/*.{ts,tsx}",
  "!/src/**/*.test.{ts,tsx}",
  "!/src/test-support/**",
], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("MCP App V2 business card composition", () => {
  const handlers = { onConfirm: vi.fn(), onSelect: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn() };

  it("renders one business shell with Confirm, Cancel, and accessible Agent edit entry", () => {
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot: SNAPSHOT, busy: false, ...handlers }));
    expect(html).toContain("data-presentation-kind=\"simple_operation\"");
    expect(html).toContain("确认创建");
    expect(html).toContain(">取消</button>");
    expect(html).toContain("修改需求");
    expect(html).not.toContain("参数（JSON）");
    expect(html).not.toContain("<dialog");
    expect(html).not.toContain("popover=");
    const editor = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "zh-CN", children: createElement(AgentEditRequest, {
      value: "把地区改成西班牙",
      busy: false,
      error: null,
      onChange: vi.fn(),
      onSubmit: vi.fn(),
    }) }));
    expect(editor).toContain("for=\"agent-edit-instruction\"");
    expect(editor).not.toContain("不要填写密码、验证码、令牌");
    expect(editor).not.toContain("UTF-8");
    expect(editor).not.toContain("旧卡");
    expect(editor).toContain("maxLength=\"2048\"");
    expect(editor).toContain("发送修改请求");
  });

  it("uses a card-shaped skeleton only before the initial snapshot is available", () => {
    const html = renderToStaticMarkup(createElement(LoadingShell, { label: "正在连接 OneCrew…" }));
    expect(html).toContain("role=\"status\"");
    expect(html).toContain("aria-label=\"正在连接 OneCrew…\"");
    expect(html).toContain("data-slot=\"skeleton\"");
    expect(html).not.toContain("reacher-loading-mark");
  });

  it("renders the initial snapshot while hydration keeps every action disabled", () => {
    const html = renderToStaticMarkup(createElement(ResultCardView, {
      snapshot: choiceSnapshot(),
      busy: false,
      syncing: true,
      ...handlers,
    }));
    expect(html).toContain("data-presentation-kind=\"choice\"");
    expect(html).toContain("aria-busy=\"true\"");
    expect(html).toContain("正在获取最新状态");
    expect(html.match(/role=\"radiogroup\"/g)?.length).toBe(1);
    expect(html.match(/role=\"radio\"[^>]*disabled/g)?.length).toBe(3);
    expect(html.match(/<button[^>]*disabled/g)?.length).toBeGreaterThanOrEqual(3);
  });

  it("renders only the current question, with custom input and no premature submit", () => {
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot: choiceSnapshot(), busy: false, ...handlers }));
    expect(html).toContain("保存范围");
    expect(html).toContain("全部可信路由");
    expect(html).toContain("role=\"radiogroup\"");
    expect(html).toContain("aria-labelledby=\"choice-question-heading\"");
    expect(html).toContain("人工输入");
    expect(html).not.toContain("role=\"dialog\"");
    expect(html).not.toContain("aria-modal");
    expect(html).not.toContain("UTF-8");
    expect(html).not.toContain("role=\"tablist\"");
    expect(html).not.toContain("预览答案");
    expect(html).toContain("data-choice-scroll=\"true\"");
    expect(html).not.toContain("是否继续通知 Agent？");
    expect(html).not.toContain("提交全部答案");
  });

  it("gives every card one action bar in the shell footer without changing what the actions do", () => {
    const confirm = renderToStaticMarkup(createElement(ResultCardView, { snapshot: SNAPSHOT, busy: false, ...handlers }));
    const choice = renderToStaticMarkup(createElement(ResultCardView, { snapshot: choiceSnapshot(), busy: false, ...handlers }));
    const batch = renderToStaticMarkup(createElement(ResultCardView, { snapshot: creatorBatchSnapshot(), busy: false, ...handlers, onConfirmCreatorSaveBatch: vi.fn() }));
    for (const html of [confirm, choice, batch]) {
      // One implementation: the wizard and the Creator review no longer carry
      // their own footer inside the scrolling body of the card.
      expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
      expect(html).not.toContain("data-choice-footer");
      expect(html).not.toContain("data-creator-batch-footer");
      expect(html.indexOf("data-result-actions")).toBeGreaterThan(html.indexOf("data-result-body"));
    }
    // The same stage labels, closed states, and once-only actions as before.
    expect(confirm).toContain("确认创建");
    expect(choice).toContain("下一题");
    expect(choice).not.toContain("提交全部答案");
    expect(batch).toContain("保存或更新所选达人");
    expect(renderToStaticMarkup(createElement(ResultCardView, { snapshot: terminalSnapshot(), busy: false, ...handlers }))).not.toContain("data-result-actions");
  });

  it("puts a panel's own actions into that one bar instead of a second row inside the panel", () => {
    const instruction = "把地区改成西班牙";
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "zh-CN", children: createElement(ResultShell, {
      snapshot: SNAPSHOT,
      footer: createElement(AgentEditActions, { value: instruction, busy: false, disabled: false, onClose: vi.fn() }),
      children: createElement("div", { id: "agent-edit-panel" }, createElement(AgentEditRequest, {
        formId: AGENT_EDIT_FORM_ID, value: instruction, busy: false, error: null, onChange: vi.fn(), onSubmit: vi.fn(),
      })),
    }) }));
    // Edit mode has two actions, and both are in the card's one action bar:
    // closing the panel no longer stands there alone while its send floats in
    // a row of its own inside the panel.
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
    const split = html.indexOf("data-result-actions");
    const [body, bar] = [html.slice(0, split), html.slice(split)];
    expect(bar).toContain("关闭编辑");
    expect(bar).toContain("发送修改请求");
    expect(bar).toContain("type=\"submit\"");
    expect(bar).toContain(`form="${AGENT_EDIT_FORM_ID}"`);
    // The bar submits the panel's own form, so the panel keeps its validation
    // and the card holds no second copy of the draft it is validating.
    expect(body).toContain(`id="${AGENT_EDIT_FORM_ID}"`);
    expect(body).not.toContain("发送修改请求");
    expect(body).not.toContain("type=\"submit\"");
    // Both cards that open this panel route through the one shared row. Editing
    // is a mode with actions of its own; reading a detail is not, and takes
    // none of this bar (see detail-swap.test.tsx).
    for (const path of ["/src/features/result-card/view.tsx", "/src/features/result-card/components/search-plan-card.tsx"]) {
      expect(APP_SOURCES[path]!, path).toContain("AgentEditActions");
    }
  });

  it("renders all fifteen closed business kinds without operation-ID routing", () => {
    const kinds = ["creator_save_batch_confirmation", "creator_save_batch_outcome", "creator_batch_confirmation", "creator_batch_outcome", "outreach_confirmation", "outreach_outcome", "email_confirmation", "email_outcome", "search_plan_editor", "outreach_draft", "record_preview", "simple_operation", "read_only", "choice", "batch_result"] as const;
    for (const kind of kinds) {
      const html = renderToStaticMarkup(createElement(ResultBusinessView, { projection: { ...SNAPSHOT.presentation, kind } }));
      expect(html).toContain("创作者信息");
      expect(html).not.toContain("onecrew.creator.manual.create");
    }
  });

  it("makes terminal, read-only, expired, failure, and unknown states actionless and distinct", () => {
    const states: ResultSnapshot[] = [
      { ...terminalSnapshot(), continuation: { state: "delivered", continuation_revision: 4, dispatch_started_at_ms: null, dispatch_not_after_ms: null } },
      { ...SNAPSHOT, binding: { ...BINDING, view: { ...BINDING.view, kind: "read_only" } }, pending_action: null },
      { ...terminalSnapshot(), state: { kind: "failed", code: "provider_failed", summary: "Provider read failed.", retryable: false } },
      { ...terminalSnapshot(), state: { kind: "unknown", code: "effect_unknown", summary: "The final effect is unknown." } },
    ];
    for (const snapshot of states) {
      const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
      expect(html).not.toContain("确认创建");
      expect(html).not.toContain("修改需求");
      expect(html).not.toContain(">取消</button>");
    }
    expect(resultClientFailureCopy({ kind: "server", failure: { code: "expired" } }, "zh-CN")).toMatchObject({ title: "状态已过期", code: "expired" });
    expect(resultClientFailureCopy({ kind: "server", failure: { code: "absent" } }, "zh-CN")).toMatchObject({ title: "状态已失效", code: "absent" });
    expect(resultClientFailureCopy({ kind: "invalid_response", reason: "snapshot_binding_rejected" }, "zh-CN")).toEqual({
      title: "卡片响应结构无效",
      detail: "响应未通过严格协议校验，操作保持禁用。",
      diagnostics: "snapshot 的 result binding 未通过格式校验。",
      code: "invalid_response.snapshot_binding_rejected",
    });
  });

  it("renders a confirmed native failure alongside its consumed, actionless result", async () => {
    const snapshot = dmFailedSnapshot();
    const { app, callServerTool } = appMock([{ content: [], structuredContent: {
      kind: "snapshot", snapshot, dm_failure: DM_FAILURE,
    } }]);
    const response = await confirmResult(app, dmReviewSnapshot().binding);
    const html = renderToStaticMarkup(createElement(ResultCardView, {
      snapshot: response.snapshot, dmFailure: response.dm_failure, busy: false, ...handlers,
    }));
    expect(response.snapshot).toEqual(snapshot);
    expect(response.snapshot.pending_action).toBeNull();
    expect(html).toContain("The confirmed message could not proceed.");
    expect(html).toContain("Earlier completed work is retained.");
    for (const value of ["Direct message diagnostic", "message_capability", "telegram", "mtproto", "FLOOD_WAIT_17", "420", DM_FAILURE.diagnostic.message]) expect(html).toContain(value);
    expect(html).not.toContain("Confirm message");
    expect(html).not.toContain(">Cancel</button>");
    expect(html).not.toContain("Check current state");
    expect(callServerTool).toHaveBeenCalledTimes(1);
  });

  it.each(["login 429 timeout", "sent successfully", "unknown native reason"])("keeps accepted readback state independent from diagnostic text: %s", async (message) => {
    const snapshot = dmFailedSnapshot();
    snapshot.state = { kind: "unknown", code: "outreach_remote_accepted", summary: "The message was accepted; readback is pending." };
    snapshot.batch.outcomes[0]!.disposition = { kind: "unknown", code: "outreach_remote_accepted" };
    const { app, callServerTool } = appMock([{ content: [], structuredContent: {
      kind: "snapshot", snapshot,
      dm_failure: { ...DM_FAILURE, stage: "message_readback", diagnostic: { ...DM_FAILURE.diagnostic, message } },
    } }]);
    const response = await confirmResult(app, dmReviewSnapshot().binding);
    const html = renderToStaticMarkup(createElement(ResultCardView, {
      snapshot: response.snapshot, dmFailure: response.dm_failure, busy: false, ...handlers,
    }));
    expect(response.snapshot).toEqual(snapshot);
    expect(resultStatusPresentation(response.snapshot)).toEqual(resultStatusPresentation(snapshot));
    expect(html).toContain("The message was accepted; readback is pending.");
    expect(html).toContain("message_readback");
    expect(html).toContain(message);
    expect(html).not.toContain("Confirm message");
    expect(html).not.toContain("recipient has read");
    expect(html).not.toContain("Retry");
    expect(callServerTool).toHaveBeenCalledTimes(1);
  });

  it("localizes diagnostic labels while retaining native data and absent codes", () => {
    const snapshot = dmFailedSnapshot();
    snapshot.locale = "zh-CN";
    const dmFailure = { ...DM_FAILURE, diagnostic: { ...DM_FAILURE.diagnostic, origin: "local" as const, code: null, rpc_code: null } };
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, dmFailure, busy: false, ...handlers }));
    for (const value of ["私信诊断", "阶段", "本地", "原生错误码", "未提供", "mtproto", "message_capability"]) expect(html).toContain(value);
    expect(html).not.toContain("HTTP 状态");
    expect(html).not.toContain("RPC 错误码");
    expect(renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }))).not.toContain("私信诊断");
  });

  it("renders cancelled, Agent-edit, and successful Resolved snapshots as exact distinct terminals", () => {
    const resolved = (code: string, summary: string): ResultSnapshot => ({
      ...terminalSnapshot(),
      state: { kind: "resolved", code, summary },
      continuation: { state: "delivered", continuation_revision: 4, dispatch_started_at_ms: 1_000, dispatch_not_after_ms: 2_000 },
    });
    const cancelled = resolved("user_cancelled", "The pending action was cancelled.");
    const edit = resolved("edit_requested", "The previous card was sealed for Agent editing.");
    const success = resolved("completed", "创作者已创建。");

    expect(resultStatusPresentation(cancelled)).toMatchObject({
      badge: { label: "已取消", variant: "outline" },
      terminal: { label: "已取消", tone: "neutral", icon: "cancelled" },
    });
    expect(resultStatusPresentation(edit)).toMatchObject({
      badge: { label: "修改请求已提交", variant: "outline" },
      terminal: { label: "修改请求已提交", tone: "neutral", icon: "edit" },
    });
    expect(resultStatusPresentation(success)).toMatchObject({
      badge: { label: "已完成", variant: "secondary" },
      terminal: { label: "已完成", tone: "success", icon: "success" },
    });

    const cancelledHtml = renderToStaticMarkup(createElement(ResultCardView, { snapshot: cancelled, busy: false, ...handlers }));
    const editHtml = renderToStaticMarkup(createElement(ResultCardView, { snapshot: edit, busy: false, ...handlers }));
    const successHtml = renderToStaticMarkup(createElement(ResultCardView, { snapshot: success, busy: false, ...handlers }));
    expect(cancelledHtml).toContain(">已取消<");
    expect(cancelledHtml).not.toContain("bg-success-background");
    expect(editHtml).toContain(">修改请求已提交<");
    expect(editHtml).not.toContain("bg-success-background");
    expect(successHtml).toContain(">已完成<");
    expect(successHtml).not.toContain("data-result-status");
    for (const html of [cancelledHtml, editHtml, successHtml]) {
      expect(html).not.toContain("确认创建");
      expect(html).not.toContain("修改需求");
      expect(html).not.toContain(">取消</button>");
    }
    expect(renderToStaticMarkup(createElement(ResultCardView, { snapshot: cancelled, busy: false, ...handlers }))).toBe(cancelledHtml);
    expect(renderToStaticMarkup(createElement(ResultCardView, { snapshot: edit, busy: false, ...handlers }))).toBe(editHtml);
  });

  it("uses official useApp only and removes V1 Plan, bridge, storage, and raw protocol UI", () => {
    const source = Object.values(APP_SOURCES).join("\n");
    expect(source).toContain("useApp");
    for (const forbidden of [
      "McpAppBridge",
      "postMessage",
      "localStorage",
      "CatalogGetResult",
      "CreatorDiscoveryPlan",
      "crm_catalog_get",
      "crm_operation_execute",
      "toolkit_guide",
      "playbook",
      "apps/desktop",
      "../desktop",
      "from \"@/",
      "from \"~/",
      "<dialog",
      "showModal",
      "popover=",
    ]) expect(source).not.toContain(forbidden);
  });
});
