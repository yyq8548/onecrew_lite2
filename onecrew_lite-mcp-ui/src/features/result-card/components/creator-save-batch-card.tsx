import { useState } from "react";
import { Button } from "@mcp-ui/components/ui/button";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import type { CreatorSaveBatchSelection, ResultSnapshot } from "@mcp-ui/lib/result-contract";
import type { ContinuationPauseReason, ResultClientFailure } from "@mcp-ui/lib/result-client";
import { continuationStatusCopy } from "../presentation";
import { initialCreatorBatchDraft, rebaseCreatorBatchDraft, setCreatorBatchRoute, setCreatorBatchSelected } from "../creator-batch-state";
import { creatorSaveRowSelectable } from "../creator-save-state";
import { CreatorBatchActions, CreatorBatchReview } from "./creator-batch-review";
import { ResultShell, TerminalSummary } from "./shell";
import { CompactResult } from "./compact-result";
import { CompletedResultSections } from "./completed-sections";

export function CreatorSaveBatchCard({ snapshot, busy, syncing = false, onConfirmCreatorSaveBatch, onRetryCreatorSaveBatch, onCancel, onRefreshSearchPlan, revisionFailure, revisionBlocked = false, continuationIssue }: {
  onRetryCreatorSaveBatch?: (rowIds: string[]) => void;
  snapshot: ResultSnapshot; busy: boolean; syncing?: boolean; onConfirmCreatorSaveBatch?: (selections: CreatorSaveBatchSelection[]) => void; onCancel: () => void;
  onRefreshSearchPlan?: () => void; revisionFailure?: ResultClientFailure | null; revisionBlocked?: boolean; continuationIssue?: ContinuationPauseReason | null;
}) {
  const { t } = useAppLocale();
  const review = snapshot.state.kind === "available" && snapshot.pending_action?.kind === "creator_save_batch" ? snapshot.pending_action.review : null;
  const rows = review?.rows ?? [];
  // The card owns the review draft so the one shared footer can act on it.
  const [draft, setDraft] = useState(() => initialCreatorBatchDraft(rows));
  const [reviewRevision, setReviewRevision] = useState(snapshot.binding.revision);
  const stale = reviewRevision !== snapshot.binding.revision;
  const blocked = revisionBlocked || !onConfirmCreatorSaveBatch;
  const disabled = busy || syncing || blocked || stale;
  const outcomes = snapshot.presentation.sections.flatMap(section => section.kind === "creator_save_rows" ? section.rows : []);
  const continuation = continuationStatusCopy(snapshot.continuation.state, snapshot.locale, continuationIssue);
  const retryIds = outcomes.some(row => row.status === "unknown") ? [] : outcomes.filter(row => row.status === "failed" && row.failure_code === "temporary_write_failure").map(row => row.row_id);
  const footer = review
    ? <CreatorBatchActions rows={rows} draft={draft} intent={review.intent} busy={busy} disabled={disabled} onConfirm={selections => onConfirmCreatorSaveBatch?.(selections)} onCancel={onCancel} />
    : retryIds.length > 0 && onRetryCreatorSaveBatch ? <Button disabled={busy || syncing || revisionBlocked} onClick={() => onRetryCreatorSaveBatch(retryIds)}>{busy ? (t("creatorRetrying")) : (t("creatorRetryFailedCount", { count: retryIds.length }))}</Button> : undefined;
  return <ResultShell snapshot={snapshot} syncing={syncing} footer={footer}>
    <TerminalSummary snapshot={snapshot} />
    {review && <CompletedResultSections snapshot={snapshot} />}
    {review ? <CreatorBatchReview rows={rows} review={review} draft={draft} busy={busy} syncing={syncing} blocked={blocked} disabled={disabled} stale={stale}
      onAcknowledge={() => { setDraft(current => rebaseCreatorBatchDraft(rows, current)); setReviewRevision(snapshot.binding.revision); }}
      onSelectAll={checked => setDraft(current => ({ ...current, selectedIds: checked ? rows.filter(row => creatorSaveRowSelectable(row.preview)).map(row => row.row_id) : [] }))}
      onSelect={(rowId, checked) => setDraft(current => setCreatorBatchSelected(current, rowId, checked, rows))}
      onRoute={(rowId, routeId, checked) => setDraft(current => setCreatorBatchRoute(current, rowId, routeId, checked, rows))} /> : <CompactResult snapshot={snapshot} />}
    {revisionFailure && <div role="alert" className="grid gap-2 rounded-lg border border-destructive/30 p-2"><p className="text-xs">{t("creatorSaveFailure")}</p>{revisionBlocked && onRefreshSearchPlan && <Button type="button" size="sm" variant="outline" disabled={busy} onClick={onRefreshSearchPlan}>{t("searchPlanRefresh")}</Button>}</div>}
    {syncing && <p role="status" className="text-xs text-muted-foreground">{t("syncing")}</p>}
    {continuation && <p className="text-xs text-muted-foreground" aria-live="polite">{continuation}</p>}
  </ResultShell>;
}
