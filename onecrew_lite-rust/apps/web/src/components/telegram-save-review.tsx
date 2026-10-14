import type { OwnerReviewSnapshot } from "@/lib/owner-review";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { scoreGrade, type ScoreGrade } from "@/lib/score-grade";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  confirmTelegramAdministratorSave, creatorSaveReviewBlocked, creatorSaveReviewRows, creatorSaveSelections,
  readCreatorSaveResult, readSavedTelegramCreator, savedTelegramCreatorId,
} from "@/lib/telegram-candidate";
import type { CreatorRecord } from "@/lib/creator";

const TELEGRAM_SCORE_GRADE_KEYS: Record<ScoreGrade, "outreach.telegram.reviewGradeExcellent" | "outreach.telegram.reviewGradeGood" | "outreach.telegram.reviewGradeMedium" | "outreach.telegram.reviewGradeLow" | "outreach.telegram.reviewGradeUnrated"> = {
  excellent: "outreach.telegram.reviewGradeExcellent",
  good: "outreach.telegram.reviewGradeGood",
  medium: "outreach.telegram.reviewGradeMedium",
  low: "outreach.telegram.reviewGradeLow",
  unrated: "outreach.telegram.reviewGradeUnrated",
};

export function TelegramSaveReview({ snapshot, onSnapshot, onClose, onSaved }: {
  snapshot: OwnerReviewSnapshot | null;
  onSnapshot: (snapshot: OwnerReviewSnapshot) => void;
  onClose: () => void;
  onSaved: (record: CreatorRecord) => void;
}) {
  const { t, i18n } = useTranslation("inbox");
  const [selectedRoutes, setSelectedRoutes] = useState<Record<string, string[]>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [needsReceipt, setNeedsReceipt] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [requestedPage, setRequestedPage] = useState(0);
  const activeResult = useRef(snapshot?.binding.result_ref);
  activeResult.current = snapshot?.binding.result_ref;
  useEffect(() => {
    setSelectedRoutes({}); setError(null); setNeedsReceipt(false); setRequestedPage(0);
  }, [snapshot?.binding.result_ref]);
  useEffect(() => {
    if (!snapshot) return;
    // Expiry is display-only; the runtime is the authority on submission.
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, [snapshot?.binding.result_ref]);
  if (!snapshot) return null;
  const rows = creatorSaveReviewRows(snapshot);
  // Page presentation only; selections still cover every reviewed row.
  const pageCount = Math.max(1, Math.ceil(rows.length / 5));
  const page = Math.min(requestedPage, pageCount - 1);
  const selections = creatorSaveSelections(rows, selectedRoutes);
  const expired = rows.some(({ preview }) => preview.expires_at * 1000 <= now);
  const blocked = creatorSaveReviewBlocked(rows);
  const canConfirm = rows.length > 0 && !expired && !blocked && !needsReceipt && selections.every((row) => row.confirmed_route_ids.length > 0);

  async function finish(result: OwnerReviewSnapshot) {
    if (activeResult.current !== result.binding.result_ref) return;
    onSnapshot(result);
    const creatorId = savedTelegramCreatorId(result);
    if (creatorId) {
      const creator = await readSavedTelegramCreator(creatorId);
      if (activeResult.current === result.binding.result_ref) onSaved(creator);
    }
  }

  async function submit() {
    if (!snapshot || !canConfirm || busy) return;
    setBusy(true); setError(null); setNeedsReceipt(true);
    try {
      const receipt = await confirmTelegramAdministratorSave(snapshot, selections);
      // Only a terminal receipt plus Creator readback can complete this dialog.
      await finish(await readCreatorSaveResult(receipt));
      setNeedsReceipt(false);
    } catch (failure) {
      setError(String(failure));
      // A lost HTTP response is not evidence that the owner did not commit.
      // Keep the exact result binding and offer read-only recovery.
    } finally { setBusy(false); }
  }

  async function recover() {
    if (!snapshot || busy) return;
    setBusy(true); setError(null);
    try {
      await finish(await readCreatorSaveResult(snapshot));
      setNeedsReceipt(false);
    } catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  }

  const receiptState = snapshot.state.kind !== "available";
  return <Dialog open onOpenChange={(open) => { if (!open && !busy) onClose(); }}>
    <DialogContent className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-xl" onInteractOutside={(event) => { if (busy) event.preventDefault(); }} onEscapeKeyDown={(event) => { if (busy) event.preventDefault(); }}>
      <DialogHeader><DialogTitle>{t("outreach.telegram.reviewTitle")}</DialogTitle><DialogDescription>{t("outreach.telegram.reviewDescription")}</DialogDescription></DialogHeader>
      <div className="min-h-0 space-y-4 overflow-y-auto">
        {rows.slice(page * 5, (page + 1) * 5).map(({ row_id, preview }) => <section key={row_id} className="space-y-3 rounded-lg border p-3">
          <div><h3 className="font-medium">{preview.display_name ?? preview.handle ?? preview.platform}</h3><p className="text-sm text-muted-foreground">{preview.handle ? `@${preview.handle}` : preview.platform}</p></div>
          <p className="text-xs text-muted-foreground">{preview.identity_creator_ids.length ? t("outreach.telegram.reviewExisting") : t("outreach.telegram.reviewNew")}</p>
          {preview.notes && <p className="whitespace-pre-wrap text-sm">{preview.notes}</p>}
          {preview.assessment && <p className="whitespace-pre-wrap text-sm">{preview.assessment}</p>}
          <p className="text-sm">{t("outreach.telegram.reviewScore", { grade: t(TELEGRAM_SCORE_GRADE_KEYS[scoreGrade(preview.score)]) })}</p>
          <div className="space-y-2"><p className="text-sm font-medium">{t("outreach.telegram.reviewRoutes")}</p>{preview.routes.map((route) => <label key={route.route_id} className="flex items-start gap-2 rounded-md border p-2">
            <Checkbox checked={route.collision.status === "none" && (route.required || (selectedRoutes[row_id] ?? []).includes(route.route_id))} disabled={route.required || route.collision.status !== "none" || busy}
              onCheckedChange={(checked) => setSelectedRoutes((current) => ({ ...current, [row_id]: checked === true ? [...(current[row_id] ?? []), route.route_id] : (current[row_id] ?? []).filter((id) => id !== route.route_id) }))} />
            <span className="min-w-0 flex-1 text-sm"><span className="block break-all">{route.display_value}</span><span className="block text-xs text-muted-foreground">{route.channel}{route.required ? ` · ${t("outreach.telegram.reviewRequired")}` : ""}</span>
              {route.collision.status !== "none" && <span className="block text-xs text-destructive">{t("outreach.telegram.reviewCollision")}</span>}
            </span>
          </label>)}</div>
          <p className="text-xs text-muted-foreground">{t("outreach.telegram.reviewExpires", { date: new Intl.DateTimeFormat(i18n.resolvedLanguage ?? i18n.language, { dateStyle: "short", timeStyle: "medium" }).format(new Date(preview.expires_at * 1000)) })}</p>
        </section>)}
        {pageCount > 1 && <nav aria-label={t("outreach.telegram.reviewPages")} className="flex items-center justify-between gap-2">
          <Button variant="outline" size="sm" disabled={busy || page === 0} onClick={() => setRequestedPage(page - 1)}>{t("outreach.telegram.reviewPrevious")}</Button>
          <span className="text-xs">{t("outreach.telegram.reviewPage", { page: page + 1, total: pageCount })}</span>
          <Button variant="outline" size="sm" disabled={busy || page + 1 >= pageCount} onClick={() => setRequestedPage(page + 1)}>{t("outreach.telegram.reviewNext")}</Button>
        </nav>}
        {expired && <p role="alert" className="text-sm text-destructive">{t("outreach.telegram.reviewExpired")}</p>}
        {blocked && <p role="alert" className="text-sm text-destructive">{t("outreach.telegram.reviewBlocked")}</p>}
        {receiptState && <p role="status" className="text-sm">{"summary" in snapshot.state ? snapshot.state.summary : t("outreach.telegram.reviewPending")}</p>}
        {error && <p role="alert" className="break-words text-sm text-destructive">{error}</p>}
        {needsReceipt && <p className="text-sm text-muted-foreground">{t("outreach.telegram.reviewRecover")}</p>}
      </div>
      <DialogFooter>
        <Button variant="outline" disabled={busy} onClick={onClose}>{t("outreach.telegram.reviewClose")}</Button>
        {(needsReceipt || receiptState) && <Button variant="outline" disabled={busy} onClick={() => void recover()}>{busy && <Loader2 className="animate-spin" />}{t("outreach.telegram.reviewReceipt")}</Button>}
        {rows.length > 0 && <Button disabled={busy || !canConfirm} onClick={() => void submit()}>{busy && <Loader2 className="animate-spin" />}{t("outreach.telegram.reviewConfirm")}</Button>}
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
