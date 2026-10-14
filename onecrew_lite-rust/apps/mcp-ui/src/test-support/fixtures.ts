import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";

export const BINDING = {
  result_ref: `rr_${"a".repeat(32)}`,
  view: { identity: `rv_${"b".repeat(32)}`, kind: "confirmation" as const },
  origin: "mcp:full",
  fingerprint: `sha256:${"c".repeat(64)}`,
  revision: 2,
};

export const SNAPSHOT: ResultSnapshot = {
  locale: "zh-CN",
  binding: BINDING,
  presented: true,
  completed_sections_count: 0,
  batch: {
    outcomes: [{
      call_id: "call.creator",
      operation_id: "onecrew.creator.manual.create",
      disposition: { kind: "not_run", reason: "confirmation_required" },
      summary: "Creator creation requires confirmation.",
      resource_refs: [],
    }],
  },
  presentation: {
    kind: "simple_operation",
    title: "手动创建创作者",
    summary: "确认名称、平台和账号后再创建。",
    sections: [
      { kind: "fields", heading: "创作者信息", fields: [{ label: "名称", value: "Nora Beauty" }, { label: "平台", value: "X" }, { label: "账号", value: "@nora_beauty" }, { label: "备注", value: "美妆与护肤创作者，已查看公开主页。" }] },
    ],
  },
  pending_action: { kind: "confirm", prompt: "确认创建这位创作者？", label: "确认创建" },
  state: { kind: "available" },
  continuation: { state: "not_requested", continuation_revision: 1, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
};

export const QUESTIONS = [
  {
    id: "scope",
    selection_mode: "single" as const,
    header: "保存范围",
    prompt: "请选择保存范围",
    options: [
      { id: "email", label: "Email", description: "仅保存公开邮箱" },
      { id: "all", label: "全部可信路由", description: null },
    ],
  },
  {
    id: "notify",
    selection_mode: "single" as const,
    header: "通知",
    prompt: "是否继续任务？",
    options: [
      { id: "yes", label: "是", description: null },
      { id: "no", label: "否", description: null },
    ],
  },
];

export function choiceSnapshot(): ResultSnapshot {
  return {
    ...SNAPSHOT,
    binding: { ...BINDING, view: { ...BINDING.view, kind: "choice" } },
    presentation: { kind: "choice", title: "选择保存方式", summary: "请选择后继续", sections: [] },
    pending_action: { kind: "choice", questions: QUESTIONS },
  };
}

export function terminalSnapshot(): ResultSnapshot {
  return {
    ...SNAPSHOT,
    binding: { ...BINDING, revision: 3 },
    pending_action: null,
    state: { kind: "resolved", code: "completed", summary: "创作者已创建。" },
    continuation: { state: "pending", continuation_revision: 2, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
  };
}
