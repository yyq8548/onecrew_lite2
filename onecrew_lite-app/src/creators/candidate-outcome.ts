import { creatorSaveOutcome, creatorSaveResultSummary, type CreatorSaveOutcome, type CreatorSaveReview, type CreatorSaveSelection } from "../results/creator-save-contracts.ts";
import { z } from "zod";
import { creatorSavePresentationProjection, executedCallOutcome, terminalResult, resultResourceRef } from "../results/contracts.ts";
import type { OwnerCallCompletion } from "../results/grants.ts";
import type { OwnerReviewContext } from "../results/owner-context.ts";
import type { CandidateBatchResult } from "./candidate-execute.ts";
import type { CandidateAccount } from "./candidate-contracts.ts";
type ResultResourceRef = z.infer<typeof resultResourceRef>;

export function creatorCandidateOutcomeText(rows: readonly CreatorSaveOutcome[], locale: "en-US" | "zh-CN"): { title: string; message: string } {
  const counts = { saved: 0, failed: 0, unknown: 0, not_run: 0, not_selected: 0 };
  for (const row of rows) counts[row.operation === "added" || row.operation === "updated" ? "saved" : row.operation === "existing_skipped" || row.operation === "duplicate_skipped" ? "not_run" : row.operation]++;
  const zh = locale === "zh-CN", templates = zh ? ["已保存 {n} 个", "失败 {n} 个", "结果不确定 {n} 个", "未执行 {n} 个", "未选择 {n} 个"] : ["{n} saved", "{n} failed", "{n} uncertain", "{n} not run", "{n} not selected"];
  return { title: zh ? "达人保存结果" : "Creator save results", message: (zh ? "达人保存完成：" : "Creator save finished: ") + Object.values(counts).flatMap((count, index) => count ? [templates[index]!.replace("{n}", String(count))] : []).join(zh ? "，" : ", ") + (zh ? "。不要重复已完成或结果不确定的保存。" : ". Do not repeat completed or uncertain saves.") };
}
export function candidateReviewSummary(review: CreatorSaveReview) { return { summary: structuredClone(review.summary), rows: review.rows.map(row => ({ row_id: row.row_id, status: row.preview.status, reason: row.preview.reason })), failures: review.failures.map(row => ({ row_id: row.row_id, reason: row.reason })) }; }
function unselected(preview: CreatorSaveReview["rows"][number]["preview"]): Pick<CreatorSaveOutcome, "operation" | "reason"> {
  if (preview.status === "existing_skipped") return { operation: "existing_skipped", reason: "existing_creator" };
  if (preview.status === "duplicate_skipped") return { operation: "duplicate_skipped", reason: "duplicate_selection" };
  if (preview.status === "eligible_new" || preview.status === "eligible_update") return { operation: "not_selected", reason: "user_not_selected" };
  return { operation: "failed", reason: preview.reason ?? "preparation_failed" };
}
export function completeCreatorCandidateSave(review: CreatorSaveReview, selections: readonly CreatorSaveSelection[], result: CandidateBatchResult | null, context: OwnerReviewContext, locale: "en-US" | "zh-CN", noop = false, heldAccounts?: ReadonlyMap<string, CandidateAccount>): OwnerCallCompletion {
  const rows: CreatorSaveOutcome[] = [], resources = new Map<string, ResultResourceRef>();
  for (const row of review.rows) {
    const preview = row.preview, chosen = selections.find(selection => selection.row_id === row.row_id), saved = result?.outcomes.find(outcome => outcome.row_id === row.row_id);
    let outcome: Pick<CreatorSaveOutcome, "status" | "operation" | "reason" | "creator_id" | "failure_code">;
    if (!chosen) { const status = unselected(preview); if (noop && status.operation === "not_selected") throw new Error("Actionable Creator save rows cannot become a no-op result"); outcome = { status: noop || status.operation !== "not_selected" ? "not_run" : "not_selected", ...status, creator_id: null, failure_code: null }; }
    else if (saved?.status === "saved") {
      outcome = { status: "saved", operation: saved.created ? "added" : "updated", reason: null, creator_id: saved.creator.creator.id, failure_code: null };
      resources.set(saved.creator.creator.id, { kind: "creator", id: saved.creator.creator.id, label: saved.creator.creator.display_name });
    } else if (saved?.status === "failed") outcome = { status: "failed", operation: "failed", reason: "row_failed", creator_id: null, failure_code: "creator_save_row_failed" };
    else if (saved?.status === "skipped") outcome = { status: "not_run", operation: saved.reason === "existing_creator" ? "existing_skipped" : "duplicate_skipped", reason: saved.reason === "existing_creator" ? "existing_creator" : "duplicate_selection", creator_id: null, failure_code: null };
    else outcome = { status: "unknown", operation: "unknown", reason: "commit_unknown", creator_id: null, failure_code: result === null ? "creator_save_commit_unknown" : "creator_save_readback_unavailable" };
    const account = noop ? null : heldAccounts?.get(row.row_id);
    rows.push(creatorSaveOutcome.parse({ row_id: row.row_id, display_name: account?.display_name ?? preview.display_name, platform: preview.platform, handle: account ? account.handle : preview.handle, profile_url: account ? account.profile_url : preview.profile_url, avatar_url: noop ? preview.avatar_url : null, ...outcome, confirmed_route_ids: chosen?.confirmed_route_ids ?? [] }));
  }
  for (const failure of review.failures) rows.push(creatorSaveOutcome.parse({ row_id: failure.row_id, display_name: failure.display_name, platform: failure.platform, handle: failure.handle, profile_url: failure.profile_url, avatar_url: null, status: "not_run", operation: "failed", reason: "preparation_failed", creator_id: null, failure_code: "creator_save_preparation_failed", confirmed_route_ids: [] }));
  const summary = creatorSaveResultSummary.parse({ total: rows.length, added: 0, updated: 0, existing_skipped: 0, duplicate_skipped: 0, not_selected: 0, failed: 0, unknown: 0 }); for (const row of rows) summary[row.operation]++;
  const unknown = summary.unknown > 0 || noop && summary.failed > 0, failed = summary.failed > 0;
  const { title, message } = creatorCandidateOutcomeText(rows, locale), code = noop ? unknown ? "creator_save_noop_unknown" : failed ? "creator_save_noop_blocked" : "creator_save_noop" : unknown ? "creator_save_commit_unknown" : "creator_save_completed";
  const terminal = terminalResult.parse({ kind: unknown ? "unknown" : noop && failed ? "failed" : "resolved", code, summary: message, ...(noop && failed && !unknown ? { retryable: false } : {}) });
  const outcome = executedCallOutcome.parse({ call_id: context.call_id, operation_id: context.operation_id, disposition: unknown ? { kind: "unknown", code } : failed ? { kind: "failed", code: noop ? code : "creator_save_partial_failure" } : { kind: "succeeded" }, summary: message, resource_refs: [...resources].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([, value]) => value) });
  const continuation = noop ? null : { message, requires_model_context: true, context: { kind: "creator_save_batch", summary, outcomes: rows.map(({ row_id, status, operation, reason, creator_id, failure_code }) => ({ row_id, status, operation, reason, creator_id, failure_code })) } };
  return { output: { rows, summary }, continue_batch: !unknown && !failed, outcome, presentation: creatorSavePresentationProjection.parse({ kind: "creator_save_batch_outcome", title, summary: message, sections: [{ kind: "creator_save_rows", rows }] }), terminal, continuation };
}
