import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { cancelOutreach, confirmOutreach, prepareOutreach, readOutreachResult, readOutreachStatus,
  recordedOutreach, recordedOutreachStatus, type CreatorOutreachRecord, type OutreachRecordIds,
  type OutreachReviewState } from "@/lib/outreach-review";
import type { OwnerReviewSnapshot } from "@/lib/owner-review";
import { ProviderErrorAlert } from "@/components/provider-error-alert";
import { diagnosticText, presentDmFailure, type DmFailure } from "@/lib/provider-error";

export function OutreachActionReview({ open, review, onOpenChange, onSnapshot, onConfirmationAttempted, onRecorded, onRecordedStatus, onRejected, onCanceled }: {
  open: boolean; review: OutreachReviewState | null;
  onOpenChange: (open: boolean) => void;
  onSnapshot: (snapshot: OwnerReviewSnapshot) => void;
  onConfirmationAttempted: () => void;
  onRecorded: (record: CreatorOutreachRecord) => Promise<void>;
  onRecordedStatus: (record: OutreachRecordIds) => Promise<void>;
  onRejected: (guidance: string) => void;
  onCanceled: () => void;
}) {
  const { t } = useTranslation("inbox");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [diagnostic, setDiagnostic] = useState<DmFailure | null>(null);
  const [guidance, setGuidance] = useState<string | null>(null);
  const [confirmationBlocked, setConfirmationBlocked] = useState(false);
  useEffect(() => { setError(null); setDiagnostic(null); setGuidance(null); setConfirmationBlocked(false); }, [review?.snapshot.binding.result_ref]);
  if (!review) return null;
  const { snapshot, intent } = review;
  const pending = snapshot.state.kind === "available" && snapshot.pending_action?.kind === "confirm" ? snapshot.pending_action : null;

  async function confirm() {
    if (!pending || busy || confirmationBlocked || !review) return;
    setBusy(true); setError(null); setDiagnostic(null); setConfirmationBlocked(true); onConfirmationAttempted();
    try {
      const result = await confirmOutreach(review.snapshot);
      onSnapshot(result.snapshot);
      const completion = result.completion;
      if (completion && completion.idempotency_key !== intent.input.idempotency_key) {
        setError({ code: "confirmation_identity_mismatch", message: t("outreach.review.unverified") }); return;
      }
      if (result.failure) {
        setError(completion
          ? { code: "confirmation_response_conflict", message: t("outreach.review.unverified") }
          : result.failure);
        return;
      }
      setDiagnostic(completion?.failure ?? null);
      const recorded = recordedOutreach(completion);
      if (recorded) { await onRecorded(recorded); return; }
      if (completion?.phase === "rejected") {
        const evidence = completion.failure ? presentDmFailure(completion.failure) : null;
        onRejected(evidence
          ? [completion.guidance, diagnosticText(evidence)].join("\n")
          : completion.guidance);
        return;
      }
      setGuidance(completion?.guidance ?? t("outreach.review.unverified"));
    } catch (failure) { setError(failure); setGuidance(t("outreach.review.unverified")); }
    finally { setBusy(false); }
  }

  async function checkStatus() {
    if (busy || !review) return;
    setBusy(true); setError(null);
    // A process-local result may expire while its durable action remains. Read
    // both owners, without turning a missing result into permission to resend.
    try {
      const current = await readOutreachResult(review.snapshot);
      onSnapshot(current);
      if (current.state.kind === "available") setConfirmationBlocked(false);
    } catch { /* Durable status is authoritative below. */ }
    try {
      const status = await readOutreachStatus(intent);
      const record = recordedOutreachStatus(status);
      if (record) { await onRecordedStatus(record); return; }
      if (status.kind === "current" && status.state.phase === "refused") {
        onRejected(t("outreach.review.rejected")); return;
      }
      setGuidance(t("outreach.review.unsettled"));
    } catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }

  async function resume() {
    if (busy) return;
    setBusy(true); setError(null);
    try { onSnapshot(await prepareOutreach(intent)); }
    catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }

  async function cancel() {
    if (busy || !pending || !review) return;
    setBusy(true); setError(null);
    try { onSnapshot(await cancelOutreach(review.snapshot)); onCanceled(); }
    catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }

  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-xl">
      <DialogHeader><DialogTitle>{snapshot.presentation.title}</DialogTitle><DialogDescription>{t("outreach.review.description")}</DialogDescription></DialogHeader>
      <div className="min-h-0 space-y-4 overflow-y-auto">
        {snapshot.presentation.summary && <p className="whitespace-pre-wrap text-sm">{snapshot.presentation.summary}</p>}
        {snapshot.presentation.sections.map((section, index) => {
          if (section.kind === "fields") return <section key={index} className="space-y-2">{section.heading && <h3 className="text-sm font-medium">{section.heading}</h3>}<dl className="space-y-2">{section.fields.map((field, fieldIndex) => <div key={fieldIndex} className="rounded-md border p-2"><dt className="text-xs text-muted-foreground">{field.label}</dt><dd className="break-words whitespace-pre-wrap text-sm">{field.value}</dd></div>)}</dl></section>;
          if (section.kind === "notice") return <p key={index} className="whitespace-pre-wrap text-sm" role={section.tone === "critical" ? "alert" : undefined}>{section.body}</p>;
          if (section.kind === "items") return <section key={index}>{section.heading && <h3 className="text-sm font-medium">{section.heading}</h3>}<ul>{section.items.map((item, itemIndex) => <li key={itemIndex}><p className="text-sm">{item.title}</p>{item.description && <p className="whitespace-pre-wrap text-xs text-muted-foreground">{item.description}</p>}</li>)}</ul></section>;
          return null;
        })}
        {pending && <p className="whitespace-pre-wrap text-sm">{pending.prompt}</p>}
        {"summary" in snapshot.state && <p className="whitespace-pre-wrap text-sm" role="status">{snapshot.state.summary}</p>}
        {guidance && <p className="whitespace-pre-wrap text-sm" role="status">{guidance}</p>}
        {diagnostic && <ProviderErrorAlert failure={diagnostic} scope={t("outreach.review.diagnostic")} variant="warning" />}
        {error !== null && <ProviderErrorAlert error={error} scope={t("outreach.review.diagnostic")} />}
        {review.confirmationAttempted && <p className="text-xs text-muted-foreground">{t("outreach.review.sameIntent")}</p>}
      </div>
      <DialogFooter>
        <Button variant="outline" onClick={() => onOpenChange(false)}>{t("outreach.review.close")}</Button>
        {pending && <Button variant="outline" disabled={busy} onClick={() => void cancel()}>{t("outreach.review.cancelPrepared")}</Button>}
        {review.confirmationAttempted && <Button variant="outline" disabled={busy} onClick={() => void checkStatus()}>{t("outreach.review.checkStatus")}</Button>}
        {(!pending || confirmationBlocked) && <Button variant="outline" disabled={busy} onClick={() => void resume()}>{t("outreach.review.resume")}</Button>}
        {pending && <Button disabled={busy || confirmationBlocked} onClick={() => void confirm()}>{busy && <Loader2 className="animate-spin" />}{pending.label}</Button>}
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
