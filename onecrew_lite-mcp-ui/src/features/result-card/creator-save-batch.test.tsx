import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ResultCardView } from "./view";
import { resultStatusPresentation } from "./presentation";
import { CreatorBatchActions } from "./components/creator-batch-review";
import { CreatorBatchReview } from "./components/creator-batch-review";
import { creatorBatchSelections, initialCreatorBatchDraft } from "./creator-batch-state";
import { creatorBatchOutcomeRows, creatorBatchResultSnapshot, creatorBatchSnapshot } from "@mcp-ui/test-support/creator-batch-fixture";
import { appMock } from "@mcp-ui/test-support/app-mock";
import { BINDING } from "@mcp-ui/test-support/fixtures";
import { confirmCreatorSaveBatch } from "@mcp-ui/lib/result-client";
import { formatContinuationText } from "@mcp-ui/lib/result-client/continuation-text";
import { parseResultSnapshot, parseResultStateOutput, resultSnapshotDiagnostics, type ClaimedResultContinuation } from "@mcp-ui/lib/result-contract";

const handlers = { onConfirm: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn(), onSelect: vi.fn(), onConfirmCreatorSaveBatch: vi.fn() };

describe("Creator batch card", () => {
  it("rejects summary categories that contradict the owner rows", () => {
    const snapshot = creatorBatchSnapshot(2);
    const action = snapshot.pending_action;
    if (action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const summary = { ...action.review.summary, eligible_new: 1, eligible_update: 1 };
    expect(parseResultSnapshot({ ...snapshot, pending_action: { ...action, review: { ...action.review, summary } } })).toBeNull();
  });
  it.each(["duplicate", "over_limit"])("rejects a %s failure-row union", (mode) => {
    const snapshot = creatorBatchSnapshot(mode === "over_limit" ? 50 : 1);
    const action = snapshot.pending_action;
    if (action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const failure = { row_id: mode === "duplicate" ? action.review.rows[0]!.row_id : "failed-extra", display_name: null, platform: "x" as const, handle: null, profile_url: null, reason: "preparation_failed" as const };
    const review = { ...action.review, failures: [failure], summary: { ...action.review.summary, total: action.review.summary.total + 1, failed: 1 } };
    expect(parseResultSnapshot({ ...snapshot, pending_action: { ...action, review } })).toBeNull();
  });
  it("retains every selection when static rendering has no Host geometry", () => {
    const snapshot = creatorBatchSnapshot(50);
    expect(parseResultSnapshot(snapshot)).not.toBeNull();
    if (snapshot.pending_action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const rows = snapshot.pending_action.review.rows;
    // What the card shows is a reading bound; the selection is untouched by it,
    // so confirming still saves every selected row, shown or not.
    expect(creatorBatchSelections(rows, initialCreatorBatchDraft(rows))).toHaveLength(50);
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
    // Static rendering deliberately shows the intrinsic list; the browser
    // Host lane is the evidence for the bounded reading window.
    expect((html.match(/data-creator-row=/g) ?? []).length).toBe(50);
    expect(html).toContain("50");
    expect(html).toContain("保存或更新所选达人");
  });
  it("defaults selection and select-all to owner-eligible rows only", () => {
    const snapshot = creatorBatchSnapshot(3);
    if (snapshot.pending_action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    snapshot.pending_action.review.rows[1]!.preview.status = "existing_skipped";
    snapshot.pending_action.review.rows[1]!.preview.reason = "existing_creator";
    const draft = initialCreatorBatchDraft(snapshot.pending_action.review.rows);
    expect(draft.selectedIds).toEqual(["save.creator.1", "save.creator.3"]);
    expect(creatorBatchSelections(snapshot.pending_action.review.rows, draft).map(row => row.row_id)).toEqual(["save.creator.1", "save.creator.3"]);
  });
  it("accepts a full business batch of result rows and rejects one more", () => {
    expect(parseResultSnapshot(creatorBatchResultSnapshot(Array.from({ length: 50 }, () => "saved")))).not.toBeNull();
    expect(parseResultSnapshot(creatorBatchResultSnapshot(Array.from({ length: 51 }, () => "saved")))).toBeNull();
  });
  it("keeps every real saved resource while retaining the generic 64-ref boundary", () => {
    const snapshot = creatorBatchResultSnapshot(Array.from({ length: 50 }, () => "saved"));
    const refs = snapshot.batch.outcomes.flatMap(outcome => outcome.resource_refs);
    expect(refs).toHaveLength(50);
    snapshot.batch.outcomes = [{ ...snapshot.batch.outcomes[0]!, resource_refs: refs }];
    expect(parseResultSnapshot(snapshot)).not.toBeNull();
    // A full batch yields at most one ref per row, so it never reaches the
    // generic ceiling that ordinary results are still held to.
    const many = Array.from({ length: 65 }, (_unused, index) => ({ ...refs[index % refs.length]!, id: `creator-generic-${index}` }));
    const ordinary = structuredClone(snapshot);
    ordinary.presentation = { kind: "record_preview", title: "Records", summary: null, sections: [] };
    ordinary.completed_sections_count = 0;
    ordinary.batch.outcomes[0]!.resource_refs = many;
    expect(parseResultSnapshot(ordinary)).toBeNull();
    ordinary.batch.outcomes[0]!.resource_refs = many.slice(0, 64);
    expect(parseResultSnapshot(ordinary)).not.toBeNull();
  });
  it("uses the save projection budget for a full batch of prepared detail sections", () => {
    const snapshot = creatorBatchSnapshot(50);
    snapshot.presentation.sections = Array.from({ length: 50 }, (_, index) => ({ kind: "fields" as const, heading: `Creator ${index}`, fields: [{ label: "Profile", value: "Public profile description ".repeat(64) }] }));
    expect(new TextEncoder().encode(JSON.stringify(snapshot.presentation)).length).toBeGreaterThan(64 * 1024);
    expect(resultSnapshotDiagnostics(snapshot)).toEqual([]);
    expect(parseResultSnapshot(snapshot)).not.toBeNull();
  });
  it("renders five selectable creators and one batch save action", () => {
    const snapshot = creatorBatchSnapshot(); expect(parseResultSnapshot(snapshot)).toEqual(snapshot);
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
    expect(html.match(/data-creator-row=/g)).toHaveLength(5);
    expect(html.match(/role="checkbox"/g)).toHaveLength(6);
    expect(html.match(/保存或更新所选达人/g)).toHaveLength(1);
    expect(html).not.toContain("Creator 已保存到 CRM");
  });
  it("disables the only save action for an empty selection", () => {
    // The review's actions are the card's shared footer now, so the empty
    // selection reaches exactly the component the shell footer renders.
    const html = renderToStaticMarkup(createElement(CreatorBatchActions, { rows: [], draft: initialCreatorBatchDraft([]), busy: false, disabled: false, onConfirm() {}, onCancel() {} }));
    expect(html).toMatch(/<button[^>]*disabled[^>]*>Add selected creators<\/button>/);
  });

  it("labels a save_or_update confirmation as an update-capable action", () => {
    const snapshot = creatorBatchSnapshot(1);
    if (snapshot.pending_action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const html = renderToStaticMarkup(createElement(CreatorBatchActions, {
      rows: snapshot.pending_action.review.rows,
      draft: initialCreatorBatchDraft(snapshot.pending_action.review.rows),
      intent: "save_or_update",
      busy: false,
      disabled: false,
      onConfirm() {},
      onCancel() {},
    }));
    expect(html).toContain("Save or update selected creators");
    expect(html).not.toContain("Add selected creators");
  });

  it("disables select-all when the owner reports no eligible rows", () => {
    const snapshot = creatorBatchSnapshot(2);
    if (snapshot.pending_action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const rows = snapshot.pending_action.review.rows.map(row => ({
      ...row,
      preview: { ...row.preview, status: "existing_skipped" as const, reason: "existing_creator" as const },
    }));
    const review = { ...snapshot.pending_action.review, rows };
    const html = renderToStaticMarkup(createElement(CreatorBatchReview, {
      rows,
      review,
      draft: initialCreatorBatchDraft(rows),
      busy: false,
      disabled: false,
      stale: false,
      onAcknowledge() {},
      onSelectAll() {},
      onSelect() {},
      onRoute() {},
    }));
    const firstCheckbox = html.match(/<button[^>]*role="checkbox"[^>]*>/)?.[0] ?? "";
    expect(firstCheckbox).toContain("disabled");
  });

  it("shows typed preparation failures without inventing a profile or selection", () => {
    const base = creatorBatchSnapshot(1);
    if (base.pending_action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const failure = { row_id: "save.creator.failed.1", display_name: null, platform: "x" as const, handle: null, profile_url: null, reason: "preparation_failed" as const };
    const snapshot = { ...base, pending_action: { ...base.pending_action, review: { ...base.pending_action.review, failures: [failure] } } };
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
    expect(html).toContain('data-creator-failure-row="save.creator.failed.1"');
    expect(html).toContain(failure.row_id);
    expect(html).toContain("此项无法准备保存");
    expect(html).toContain("未命名创作者");
    const failureRow = html.match(/<li data-creator-failure-row="save\.creator\.failed\.1"[\s\S]*?<\/li>/)?.[0] ?? "";
    expect(failureRow).not.toContain("https://");
    expect(html.match(/role="checkbox"/g)).toHaveLength(2);
  });

  it("keeps preparation failures readable without SSR pagination claims", () => {
    const base = creatorBatchSnapshot(1);
    if (base.pending_action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const failures = Array.from({ length: 49 }, (_, index) => ({
      row_id: `save.creator.failed.${index + 1}`,
      display_name: null,
      platform: "x" as const,
      handle: null,
      profile_url: null,
      reason: "preparation_failed" as const,
    }));
    const snapshot = { ...base, pending_action: { ...base.pending_action, review: { ...base.pending_action.review, failures } } };
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
    expect(html.match(/data-creator-failure-row=/g)).toHaveLength(49);
    expect(html).not.toContain('data-creator-list-pages="true"');
    const failureRow = html.match(/<li data-creator-failure-row="save\.creator\.failed\.1"[\s\S]*?<\/li>/)?.[0] ?? "";
    expect(failureRow).not.toContain("https://");
  });
  it("sends one typed private confirmation with all selected row and route identities", async () => {
    const pending = creatorBatchSnapshot(); if (pending.pending_action?.kind !== "creator_save_batch") throw new Error("fixture");
    const selections = creatorBatchSelections(pending.pending_action.review.rows, initialCreatorBatchDraft(pending.pending_action.review.rows));
    const result = creatorBatchResultSnapshot(["saved", "saved", "saved", "saved", "saved"]);
    const { app, callServerTool } = appMock([{ content: [], structuredContent: { kind: "snapshot", snapshot: result } }]);
    expect(await confirmCreatorSaveBatch(app, pending.binding, selections)).toEqual(result);
    expect(callServerTool).toHaveBeenCalledTimes(1);
    // The five rows save inside this one request, so it must outlive the
    // host's own 60s default.
    expect(callServerTool).toHaveBeenCalledWith(
      { name: "onecrew_result_state", arguments: { action: "confirm_creator_save_batch", binding: pending.binding, selections } },
      { timeout: 196_000 },
    );
  });
  it("retains every partial/unknown/unselected row without reporting all-success", () => {
    const result = creatorBatchResultSnapshot(); expect(parseResultSnapshot(result)).toEqual(result);
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot: result, busy: false, ...handlers }));
    for (const label of ["已保存", "失败", "结果未知", "未执行", "未选择"]) expect(html).toContain(label);
    expect(html).not.toContain("添加所选达人");
    expect(resultStatusPresentation(result).terminal?.tone).toBe("warning");
    const partial = creatorBatchResultSnapshot(["saved", "failed", "not_selected"]);
    expect(resultStatusPresentation(partial).badge.label).toBe("部分完成");
    const success = creatorBatchResultSnapshot(["saved", "not_selected"]);
    expect(resultStatusPresentation(success).badge.label).toBe("已保存 1 位");
  });
  it("rejects duplicate rows, a fake saved ID, and extra heading on the exact rows section", () => {
    const pending = creatorBatchSnapshot(); if (pending.pending_action?.kind !== "creator_save_batch") throw new Error("fixture");
    pending.pending_action.review.rows[1]!.row_id = pending.pending_action.review.rows[0]!.row_id;
    expect(parseResultSnapshot(pending)).toBeNull();
    const result = creatorBatchResultSnapshot(); const section = result.presentation.sections[0]; if (section?.kind !== "creator_save_rows") throw new Error("fixture");
    section.rows[1]!.creator_id = "invented-creator"; expect(parseResultSnapshot(result)).toBeNull();
    const extra = creatorBatchResultSnapshot(); Object.assign(extra.presentation.sections[0]!, { heading: null }); expect(parseResultSnapshot(extra)).toBeNull();
  });
  it("parses the follow-up fields of each row and leaves display fields on the card", () => {
    const rows = creatorBatchOutcomeRows();
    const outcomes = rows.map(({ row_id, status, operation, reason, creator_id, failure_code }) => ({ row_id, status, operation, reason, creator_id, failure_code }));
    const claimed: ClaimedResultContinuation = { binding: BINDING, continuation_revision: 3, lease_id: `rcl_${"d".repeat(32)}`, dispatch_started_at_ms: 10, dispatch_not_after_ms: 130, request: { message: "达人保存完成：已保存 1 个，失败 1 个。不要重复已完成或结果不确定的保存。", requires_model_context: true, context: { kind: "creator_save_batch", summary: { total: outcomes.length, added: 1, updated: 0, existing_skipped: 0, duplicate_skipped: 0, not_selected: 1, failed: 2, unknown: 1 }, outcomes } } };
    expect(parseResultStateOutput({ kind: "claimed_continuation", claimed })).toEqual({ kind: "claimed_continuation", claimed });
    // A Host that drops null values still yields the same outcomes.
    const withoutNulls = outcomes.map(row => Object.fromEntries(Object.entries(row).filter(([, value]) => value !== null)));
    expect(parseResultStateOutput({ kind: "claimed_continuation", claimed: { ...claimed, request: { ...claimed.request, context: { ...claimed.request.context, outcomes: withoutNulls } } } })).toEqual({ kind: "claimed_continuation", claimed });
    const text = formatContinuationText(claimed, "zh-CN");
    for (const row of outcomes) { expect(text).toContain(`- row_id: "${row.row_id}"`); expect(text).toContain(`- status: "${row.status}"`); }
    expect(text).toContain('- creator_id: "creator-saved-1"'); expect(text).toContain("- creator_id: null"); expect(text).toContain('- failure_code: "save_failed"');
    for (const field of ["confirmed_route_ids", "avatar_url", "profile_url", "display_name", "handle", "platform"]) expect(text).not.toContain(field);
    expect(text).not.toContain(claimed.lease_id); expect(text).not.toContain('"outcomes":');
    // The wide card row is not a continuation outcome, and a saved row needs its Creator ID.
    const withOutcomes = (changed: typeof outcomes) => ({ kind: "claimed_continuation", claimed: { ...claimed, request: { ...claimed.request, context: { ...claimed.request.context, outcomes: changed } } } });
    expect(parseResultStateOutput(withOutcomes(rows))).toBeNull();
    expect(parseResultStateOutput(withOutcomes(outcomes.map((row, index) => index === 0 ? { ...row, creator_id: null } : row)))).toBeNull();
    expect(parseResultStateOutput(withOutcomes(outcomes.map((row, index) => index === 1 ? { ...row, row_id: outcomes[0]!.row_id } : row)))).toBeNull();
    expect(parseResultStateOutput(withOutcomes(outcomes.map((row, index) => index === 1 ? { ...row, reason: "existing_creator" } : row)))).toBeNull();
  });
});
