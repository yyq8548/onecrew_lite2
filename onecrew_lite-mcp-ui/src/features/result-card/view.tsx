import { ResultStatus } from "@mcp-ui/components/ui/result-status";
import { OneCrewHeader } from "@mcp-ui/components/ui/onecrew-brand";
import { CompactResult } from "./components/compact-result";
import { useReducer, useState } from "react";
import { displayLabel } from "@mcp-ui/lib/display-label";
import { emailPreview } from "./email-preview";
import { AppLocaleProvider, useAppLocale } from "@mcp-ui/lib/i18n";

import { Button } from "@mcp-ui/components/ui/button";
import { Card, CardContent, CardHeader } from "@mcp-ui/components/ui/card";
import { Skeleton } from "@mcp-ui/components/ui/skeleton";
import {
  type ResultChoiceAnswer,
  type CreatorSaveBatchSelection,
  type ResultSnapshot,
  type ResultDmFailure,
  type ResultBinding,
  type SearchPlanPatch,
} from "@mcp-ui/lib/result-contract";
import type { ResultClientFailure, ContinuationPauseReason } from "@mcp-ui/lib/result-client";
import { continuationStatusCopy } from "./presentation";
import { initialChoiceState, reduceChoice, type ChoiceState, type ChoiceAction as ChoiceDraftAction } from "./choice-state";
import { ChoiceAction, ChoiceActions, SubmittedChoice } from "./components/choice-action";
import { AGENT_EDIT_FORM_ID, AGENT_EDIT_PANEL_ID, AgentEditActions, AgentEditRequest } from "./components/editor-request";
import { ResultBusinessView } from "./components/result-views";
import { ResultActions } from "./components/result-actions";
import { ResultShell, TerminalSummary } from "./components/shell";
import { SearchPlanCard } from "@mcp-ui/features/result-card/components/search-plan-card";

import { CreatorSaveBatchCard } from "./components/creator-save-batch-card";
import { CompletedResultSections } from "./components/completed-sections";
import { DmFailureNotice } from "./components/dm-failure-notice";
import { isRetiredSendingResult } from "./retired-sending";

type ResultCardProps = {
  snapshot: ResultSnapshot;
  busy: boolean;
  syncing?: boolean;
  onConfirm: () => void;
  onSelect: (answers: ResultChoiceAnswer[]) => void;
  onCancel: () => void;
  onRequestEdit: (instruction: string) => void;
  onReviseSearchPlan?: (binding: ResultBinding, patch: SearchPlanPatch) => Promise<boolean>;
  onRetryCreatorSaveBatch?: (rowIds: string[]) => void;
  onConfirmCreatorSaveBatch?: (selections: CreatorSaveBatchSelection[]) => void;
  onRefreshSearchPlan?: () => void;
  revisionFailure?: ResultClientFailure | null;
  revisionBlocked?: boolean;
  recoveryFailure?: ResultClientFailure | null;
  dmFailure?: ResultDmFailure | null;
  onRecover?: () => void;
  continuationIssue?: ContinuationPauseReason | null;
};

export function ResultCardView(props: ResultCardProps) {
  if (isRetiredSendingResult(props.snapshot)) {
    return <AppLocaleProvider locale={props.snapshot.locale}>
      {props.dmFailure && <DmFailureNotice failure={props.dmFailure} />}
      <ResultCardBody {...props} snapshot={{
        ...props.snapshot,
        pending_action: null,
        binding: { ...props.snapshot.binding, view: { ...props.snapshot.binding.view, kind: "read_only" } },
      }} />
    </AppLocaleProvider>;
  }
  return <AppLocaleProvider locale={props.snapshot.locale}>
    {props.recoveryFailure && <RecoveryNotice busy={props.busy} onRecover={props.onRecover} />}
    {props.dmFailure && <DmFailureNotice failure={props.dmFailure} />}
    {props.snapshot.presentation.kind === "creator_save_batch_confirmation" || props.snapshot.presentation.kind === "creator_save_batch_outcome"
      ? <CreatorSaveBatchCard key={props.snapshot.binding.result_ref} {...props} /> : props.snapshot.presentation.kind === "search_plan_editor"
      ? <SearchPlanCard key={props.snapshot.binding.result_ref} {...props} />
      : <ResultCardBody key={`${props.snapshot.binding.result_ref}:${props.snapshot.binding.fingerprint}`} {...props} />}
  </AppLocaleProvider>;
}


function RecoveryNotice({ busy, onRecover }: { busy: boolean; onRecover?: () => void }) {
  const { t } = useAppLocale();
  return <div role="alert" className="mb-3 grid gap-12 rounded-lg border border-destructive/30 p-3"><div className="flex items-start gap-2"><span className="mt-0.5 inline-flex" aria-hidden="true"><ResultStatus tone="unknown" iconOnly label={t("statusUnknown")} /></span><p className="min-w-0 text-sm leading-6">{t("recoveryNotice")}</p></div>{onRecover && <Button type="button" variant="outline" size="sm" disabled={busy} onClick={onRecover}>{t("checkCurrentState")}</Button>}</div>;
}

function ResultCardBody({ snapshot, busy, syncing = false, revisionBlocked = false, continuationIssue, onConfirm, onSelect, onCancel, onRequestEdit }: ResultCardProps) {
  const { t, locale } = useAppLocale();
  const [editing, setEditing] = useState(false);
  const [instruction, setInstruction] = useState("");
  const [editError, setEditError] = useState<string | null>(null);
  const questions = snapshot.pending_action?.kind === "choice" ? snapshot.pending_action.questions : [];
  // The wizard draft lives with the card so its actions can sit in the one
  // shared footer; the wizard body stays mounted while the Agent edit panel is
  // open, exactly as before.
  const [choice, dispatchChoice] = useReducer(
    (current: ChoiceState, action: ChoiceDraftAction) => reduceChoice(current, action, questions),
    undefined,
    initialChoiceState,
  );
  const available = snapshot.state.kind === "available";
  const canAct = available && snapshot.binding.view.kind !== "read_only" && snapshot.pending_action != null;
  const actionDisabled = busy || syncing || revisionBlocked;
  const choosing = available && snapshot.pending_action?.kind === "choice";
  const showProjection = !(available && snapshot.pending_action?.kind === "choice");
  const currentProjection = available ? { ...snapshot.presentation, sections: snapshot.presentation.sections.slice(snapshot.completed_sections_count) } : snapshot.presentation;

  const footer = canAct && editing ? (
    <AgentEditActions value={instruction} busy={busy} disabled={actionDisabled} onClose={() => setEditing(false)} />
  ) : canAct && snapshot.pending_action?.kind === "confirm" ? (
    <ResultActions
      cancel={{ label: t("cancel"), onClick: onCancel, disabled: actionDisabled }}
      secondary={{ label: t("edit"), onClick: () => setEditing(true), disabled: actionDisabled, ariaExpanded: false, ariaControls: "agent-edit-panel" }}
      primary={{
        label: busy ? t("processing") : displayLabel(snapshot.pending_action.label, locale),
        onClick: onConfirm,
        disabled: actionDisabled,
        ariaLabel: snapshot.pending_action.label,
        clamp: true,
      }}
    />
  ) : choosing ? (
    <ChoiceActions questions={questions} state={choice} disabled={actionDisabled || !canAct} dispatch={dispatchChoice} onCancel={onCancel} onSubmit={onSelect} />
  ) : undefined;

  // A prepared message names itself. The operation title comes from the
  // capability catalog, which carries one fixed English string per operation
  // and no locale, so the card that shows a Chinese message kept an English
  // heading above it. Recognizing the message is the same test the body uses.
  const preparedMessage = available && emailPreview(snapshot.presentation) != null;

  return (
    <ResultShell snapshot={snapshot} titleOverride={preparedMessage ? t("kindEmailSend") : undefined} footer={footer} syncing={syncing}>
      {available && <CompletedResultSections snapshot={snapshot} />}
      {/* The edit panel replaces the projection rather than stacking under it.
          This Host ignores the size the App reports and keeps its own frame
          height, so a panel appended below the preview pushed its own textarea
          and the action bar out of the frame. The wizard below stays mounted
          but hidden, so its draft survives a trip through the panel. */}
      {editing && canAct ? (
        <div id={AGENT_EDIT_PANEL_ID}>
          <AgentEditRequest
            formId={AGENT_EDIT_FORM_ID}
            value={instruction}
            busy={busy}
            error={editError}
            onChange={(value) => {
              setInstruction(value);
              setEditError(null);
            }}
            onSubmit={(normalized) => {
              if (!normalized) {
                setEditError(t("editRequired"));
                return;
              }
              onRequestEdit(normalized);
            }}
          />
        </div>
      ) : showProjection && (snapshot.presentation.kind === "choice"
        ? <SubmittedChoice projection={currentProjection} submitted={snapshot.state.kind === "resolved" && snapshot.state.code === "choice_selected"} /> : snapshot.state.kind !== "available" && snapshot.presentation.kind !== "read_only" ? <CompactResult snapshot={snapshot} /> : <ResultBusinessView projection={currentProjection} />)}
      {choosing && (
        <div hidden={editing}><ChoiceAction
          questions={questions}
          state={choice}
          dispatch={dispatchChoice}
          disabled={actionDisabled || !canAct}
        /></div>
      )}
      {syncing && (
        <p className="rounded-xl border bg-muted/40 px-3 py-2 text-sm text-muted-foreground" role="status">
          {t("syncing")}
        </p>
      )}
      <TerminalSummary snapshot={snapshot} />
      <ContinuationStatus snapshot={snapshot} reason={continuationIssue} />
    </ResultShell>
  );
}

export function LoadingShell({ label }: { label: string }) {
  return (
    <Card className="gap-0 overflow-hidden py-0 shadow-sm" role="status" aria-label={label}>
      <OneCrewHeader /><div className="result-shell-header"><p className="text-sm text-muted-foreground">{label}</p></div><span className="sr-only">{label}</span>
      <CardHeader aria-hidden="true" className="grid grid-cols-[auto_1fr_auto] items-start gap-3 border-b px-4 py-4">
        <Skeleton className="size-10 rounded-xl" />
        <div className="grid gap-2 pt-0.5">
          <Skeleton className="h-3 w-14" />
          <Skeleton className="h-4 w-36 max-w-full" />
          <Skeleton className="h-3 w-52 max-w-[80%]" />
        </div>
        <Skeleton className="h-5 w-12 rounded-full" />
      </CardHeader>
      <CardContent aria-hidden="true" className="grid gap-3 px-4 py-4">
        <Skeleton className="h-3 w-24" />
        <Skeleton className="h-14 w-full rounded-xl" />
        <Skeleton className="h-16 w-full rounded-xl" />
      </CardContent>
    </Card>
  );
}

function ContinuationStatus({ snapshot, reason }: { snapshot: ResultSnapshot; reason?: ContinuationPauseReason | null }) {
  const copy = continuationStatusCopy(snapshot.continuation.state, snapshot.locale, reason);
  return copy
    ? <p className="rounded-xl border bg-muted/40 px-3 py-2 text-sm text-muted-foreground" aria-live="polite">{copy}</p>
    : null;
}
