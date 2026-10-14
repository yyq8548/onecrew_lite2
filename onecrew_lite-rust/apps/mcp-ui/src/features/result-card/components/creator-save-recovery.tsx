import { Button } from "@mcp-ui/components/ui/button";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import type { ResultClientFailure } from "@mcp-ui/lib/result-client";

export function CreatorSaveRecovery({ stale, busy, syncing, blocked, failure, onAcknowledge, onRefresh }: {
  stale: boolean; busy: boolean; syncing: boolean; blocked: boolean;
  failure?: ResultClientFailure | null;
  onAcknowledge: () => void;
  onRefresh?: () => void;
}) {
  const { t } = useAppLocale();
  return <>
    {stale && <div className="grid gap-2 rounded-lg border bg-muted/40 p-3"><p role="alert" className="text-sm">{t("creatorReviewChanged")}</p><Button type="button" variant="outline" size="sm" disabled={busy || syncing || blocked} onClick={onAcknowledge}>{t("creatorReviewRefresh")}</Button></div>}
    {failure && <div role="alert" className="grid gap-2 rounded-lg border border-destructive/30 p-3"><p className="text-sm">{t("creatorSaveFailure")}</p>{blocked && onRefresh && <Button type="button" variant="outline" size="sm" disabled={busy} onClick={onRefresh}>{t("searchPlanRefresh")}</Button>}</div>}
  </>;
}
