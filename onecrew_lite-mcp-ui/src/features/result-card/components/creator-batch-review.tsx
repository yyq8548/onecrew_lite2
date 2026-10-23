import { creatorSaveBlocked, creatorSaveRowSelectable } from "../creator-save-state";
import { ResultActions } from "./result-actions";
import { Checkbox } from "@mcp-ui/components/ui/checkbox";
import { Label } from "@mcp-ui/components/ui/label";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { creatorBatchSelections, type CreatorBatchDraft, type CreatorReviewRow } from "../creator-batch-state";
import type { CreatorSaveBatchReview } from "@mcp-ui/lib/result-contract";
import { CreatorReviewList } from "./creator-review-list";
import { CreatorSaveRecovery } from "./creator-save-recovery";

/** Review surface only. The owning result card holds the draft, supplies the
 * exact authority-bearing operation, and renders the one shared action bar. */
export function CreatorBatchReview({ rows, review, draft, busy, syncing = false, blocked = false, disabled, stale, onAcknowledge, onSelectAll, onSelect, onRoute }: {
  rows: CreatorReviewRow[]; review: CreatorSaveBatchReview | null; draft: CreatorBatchDraft; busy: boolean; syncing?: boolean; blocked?: boolean; disabled: boolean; stale: boolean;
  onAcknowledge: () => void; onSelectAll: (checked: boolean) => void;
  onSelect: (rowId: string, checked: boolean) => void; onRoute: (rowId: string, routeId: string, checked: boolean) => void;
}) {
  const { t } = useAppLocale();
  const selections = creatorBatchSelections(rows, draft);
  const selectableRows = rows.filter(row => creatorSaveRowSelectable(row.preview));
  const allSelected = selectableRows.length > 0 && selections.length === selectableRows.length;
  const summary = review?.summary;
  const { locale } = useAppLocale();
  const number = (value: number) => new Intl.NumberFormat(locale).format(value);
  return <div className="grid min-w-0 gap-3" data-creator-batch-review="true">
    {summary && <p role="status" data-creator-save-summary="true" className="text-xs text-muted-foreground">{t("creatorSaveSummary", { total: number(summary.total), eligibleNew: number(summary.eligible_new), eligibleUpdate: number(summary.eligible_update), existingSkipped: number(summary.existing_skipped), duplicateSkipped: number(summary.duplicate_skipped), blocked: number(summary.blocked), failed: number(summary.failed) })}</p>}
    <div className="flex flex-wrap items-center justify-between gap-2"><Label className="flex items-center gap-2 text-sm"><Checkbox className="rounded-sm" disabled={disabled || !selectableRows.length} checked={allSelected ? true : selections.length ? "indeterminate" : false} onCheckedChange={checked => onSelectAll(checked === true)} />{t("creatorSelectAll")}</Label><span role="status" className="text-xs tabular-nums text-muted-foreground">{t("creatorSelectedCount", { selected: selections.length, total: rows.length })}</span></div>
    {stale && <CreatorSaveRecovery stale busy={busy} syncing={syncing} blocked={blocked} onAcknowledge={onAcknowledge} />}
    <CreatorReviewList rows={rows} failures={review?.failures} selectedIds={draft.selectedIds} optionalIds={draft.optionalIds} disabled={disabled} onSelect={onSelect} onRoute={onRoute} />
  </div>;
}

/** The Creator review's actions, rendered in the shared card footer. */
export function CreatorBatchActions({ rows, draft, intent = "add_new", busy, disabled, onConfirm, onCancel }: {
  rows: CreatorReviewRow[]; draft: CreatorBatchDraft; intent?: CreatorSaveBatchReview["intent"]; busy: boolean; disabled: boolean;
  onConfirm: (selections: ReturnType<typeof creatorBatchSelections>) => void; onCancel: () => void;
}) {
  const { t } = useAppLocale();
  const selections = creatorBatchSelections(rows, draft);
  const conflictBlocked = rows.some(row => draft.selectedIds.includes(row.row_id) && creatorSaveBlocked(row.preview));
  // Confirm acts on every selected row, including the ones the bounded inline
  // list is not currently showing: what is displayed is a reading bound, never
  // the selection itself.
  return <ResultActions
    cancel={{ label: t("cancel"), onClick: onCancel, disabled }}
    primary={{
      label: busy ? t("processing") : intent === "save_or_update" ? t("creatorConfirmSaveOrUpdate") : t("creatorConfirmSelected"),
      disabled: disabled || conflictBlocked || selections.length === 0,
      onClick: () => { if (!disabled && !conflictBlocked && selections.length > 0) onConfirm(selections); },
    }}
  />;
}
