import { ResultStatus } from "@mcp-ui/components/ui/result-status";
import { useAppLocale, type CopyKey } from "@mcp-ui/lib/i18n";
import type { CreatorSaveBatchOutcome } from "@mcp-ui/lib/result-contract";
import { CreatorIdentity } from "./creator-review-list";

const OPERATION_COPY: Record<CreatorSaveBatchOutcome["operation"], CopyKey> = {
  added: "creatorSaveAdded", updated: "creatorSaveUpdated", existing_skipped: "creatorSaveExistingSkipped",
  duplicate_skipped: "creatorSaveDuplicateSkipped", not_selected: "creatorBatchSkipped", failed: "statusFailed", unknown: "statusUnknown",
};
const REASON_COPY: Record<NonNullable<CreatorSaveBatchOutcome["reason"]>, CopyKey> = {
  existing_creator: "creatorSaveReasonExistingCreator", duplicate_selection: "creatorSaveReasonDuplicateSelection",
  identity_ambiguous: "creatorSaveReasonIdentityAmbiguous", required_contact_route_conflict: "creatorSaveReasonRequiredContactRouteConflict",
  conflicting_update: "creatorSaveReasonConflictingUpdate", preparation_failed: "creatorSaveReasonPreparationFailed",
  user_not_selected: "creatorBatchSkipped", row_failed: "statusFailed", commit_unknown: "statusUnknown",
};

/** Every outcome at once. A card fits its content rather than paging it: a
 * saved-and-failed split the reader has to click through is a worse answer
 * than the whole short list. */
export function CreatorSaveOutcomeRows({ rows }: { rows: CreatorSaveBatchOutcome[] }) {
  const { t } = useAppLocale();
  return <ul data-creator-outcomes="true" className="divide-y rounded-lg border">{rows.map(row => <li key={row.row_id} className="grid gap-2 px-3 py-3"><div className="flex min-w-0 items-center justify-between gap-2"><CreatorIdentity preview={row} /><span className="flex min-w-0 items-center justify-end gap-2">{row.reason && <span className="text-xs text-muted-foreground">{t(REASON_COPY[row.reason])}</span>}{row.status === "failed" && !row.reason && <span className="text-xs text-muted-foreground">{row.failure_code === "temporary_write_failure" ? (t("creatorTemporaryWriteFailure")) : (t("creatorWriteFailure"))}</span>}<ResultStatus tone={row.operation === "added" || row.operation === "updated" ? "success" : row.operation === "failed" ? "failure" : row.operation === "unknown" ? "unknown" : "neutral"} label={t(OPERATION_COPY[row.operation])} iconOnly={row.operation === "added" || row.operation === "updated" || row.operation === "failed"} /></span></div></li>)}</ul>;
}
