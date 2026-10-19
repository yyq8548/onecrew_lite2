import type { ReactNode } from "react";
import { OneCrewHeader } from "@mcp-ui/components/ui/onecrew-brand";
import { useCompactWidth, useHostFrame } from "@mcp-ui/app/host-frame";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@mcp-ui/components/ui/card";
import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { resultStatusPresentation } from "../presentation";
import { CardText, ResultDetailContent, ResultTextDetailsProvider } from "./text-details";

/** The card, and the one action bar that belongs to it.
 *
 * The bar carries the card's own actions in every state, because it acts on the
 * whole selection rather than on whatever the body is currently showing.
 * Swapping the body bounds reading only: a detail that replaces the body draws
 * its own way back, so a reader who opens a message's internals or one row's
 * routes still has Confirm and Cancel in front of them. Standing between the
 * reader and the decision is the one thing this bar must never do.
 *
 * A mode with actions of its own — the Agent edit panel — is a different thing
 * from reading: the card passes that panel's actions as its `footer`, so they
 * arrive here as the card's actions rather than as a second row under the
 * panel. */
export function ResultShell({ snapshot, children, footer, syncing = false, titleOverride, identity }: {
  snapshot: ResultSnapshot; children: ReactNode; footer?: ReactNode; syncing?: boolean; titleOverride?: string; identity?: ReactNode;
}) {
  const { t } = useAppLocale();
  const [compact, compactRef] = useCompactWidth();
  // The mode the Host granted, not one this App assumed: fullscreen is the
  // only mode in which the card may own a scrollport.
  const { displayMode } = useHostFrame();
  const status = resultStatusPresentation(snapshot);
  const kind = snapshot.presentation.kind;
  const title = titleOverride ?? (kind === "read_only" ? snapshot.presentation.title : status.terminal?.label ?? (kind === "search_plan_editor" ? t("kindSearchPlan") : kind === "creator_save_batch_confirmation" ? t("kindCreatorSave") : snapshot.presentation.title));
  const processing = snapshot.state.kind === "submitted";
  const processingDetail = t(kind === "creator_save_batch_confirmation" ? "statusSaving" : "statusSubmitted");
  const statusDetail = identity ? null : processing ? processingDetail : status.terminal?.detail !== title ? status.terminal?.detail : null;
  return <ResultTextDetailsProvider revision={snapshot.binding.revision}><Card ref={compactRef} data-presentation-kind={kind} data-display-mode={displayMode ?? undefined} data-compact={compact || undefined} aria-busy={syncing || undefined} className="result-shell gap-0 overflow-hidden py-0 shadow-none">
    <OneCrewHeader />
    <CardHeader className="result-shell-header flex flex-col gap-3 px-4 pb-2 pt-4">
      {status.terminal ? <div className="processing-status" role="status" aria-live="polite">
        <div className="processing-status-copy"><CardTitle>{title}</CardTitle>{identity}{statusDetail && <p>{statusDetail}</p>}</div>
      </div> : <div className="flex w-full min-w-0 items-start justify-between gap-3">
        <CardTitle className="flex min-w-0 items-start gap-2 text-base leading-snug">
          <CardText text={title} label={t("cardTitle")} lines={2} />
        </CardTitle>
      </div>}
    </CardHeader>
    <CardContent data-result-scroll="true" className="result-shell-content grid gap-3 px-4 pb-4 pt-1"><ResultDetailContent><div data-result-body="true" className="grid min-w-0 gap-3">{children}</div></ResultDetailContent></CardContent>
    {footer && <CardFooter data-result-actions="true" className="result-shell-footer flex flex-wrap justify-end gap-2 border-t px-4 py-3">{footer}</CardFooter>}
  </Card></ResultTextDetailsProvider>;
}

export function ResultFailureShell({ title, detail, code, diagnostics }: { title: string; detail: string; code: string; diagnostics?: string }) {
  const { t } = useAppLocale();
  return <Card role="alert" className="result-shell gap-0 overflow-hidden py-0 shadow-none"><OneCrewHeader /><CardHeader className="result-shell-header flex flex-col gap-3 px-4 py-4">
    <div className="processing-status"><div className="processing-status-copy">
      <CardTitle className="text-base">{title}</CardTitle>
      <CardDescription className="whitespace-pre-wrap leading-5 [overflow-wrap:anywhere]">{detail}</CardDescription>
    </div></div>
  </CardHeader></Card>;
}

/** Shared header owns the terminal title and explanation across all cards. */
export function TerminalSummary({ snapshot: _snapshot }: { snapshot: ResultSnapshot }) {
  return null;
}
