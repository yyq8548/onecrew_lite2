import { useState } from "react";
import { Button } from "@mcp-ui/components/ui/button";
import { Label } from "@mcp-ui/components/ui/label";
import { TagInput } from "@mcp-ui/components/ui/tag-input";
import { Textarea } from "@mcp-ui/components/ui/textarea";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { SEARCH_PLAN_KEYWORD_MAX_BYTES, SEARCH_PLAN_KEYWORD_MAX_COUNT, type ResultBinding, type ResultSnapshot, type SearchPlanEditorDraft, type SearchPlanPatch } from "@mcp-ui/lib/result-contract";
import type { ResultClientFailure, ContinuationPauseReason } from "@mcp-ui/lib/result-client";
import { startSearchPlanEdit, rebaseSearchPlanEdit, searchPlanEditPatch, searchPlanEditStale, searchPlanFieldsError, utf8Length, type SearchPlanEdit, type SearchPlanFields } from "@mcp-ui/features/result-card/search-plan-state";
import { continuationStatusCopy } from "@mcp-ui/features/result-card/presentation";
import { ResultActions } from "@mcp-ui/features/result-card/components/result-actions";
import { ResultShell, TerminalSummary } from "@mcp-ui/features/result-card/components/shell";
import { AGENT_EDIT_FORM_ID, AGENT_EDIT_PANEL_ID, AgentEditActions, AgentEditRequest } from "@mcp-ui/features/result-card/components/editor-request";
import { SearchPlanPreview, SearchPlanProjectionView } from "@mcp-ui/features/result-card/components/search-plan-preview";

// The editable draft is description and keywords only (the typed
// SearchPlanPatch accepts nothing else). Operation-bound scopes are listed
// read-only beside the fields so the user confirms what will run.
export function SearchPlanEditFields({ edit, draft, localError, pendingKeyword, busy, onChange, onPendingKeyword }: {
  edit: SearchPlanEdit; draft: SearchPlanEditorDraft; localError: "description" | "keywords" | null; pendingKeyword: boolean; busy: boolean;
  onChange: (fields: SearchPlanFields) => void; onPendingKeyword: (pending: boolean) => void;
}) {
  const { t } = useAppLocale();
  return <>
    <div className="grid gap-2">
      <Label htmlFor="search-plan-description">{t("searchPlanDescription")}</Label>
      <Textarea id="search-plan-description" className="field-sizing-fixed h-20 min-h-20 resize-y" value={edit.fields.description} disabled={busy} aria-invalid={localError === "description"}
        onChange={(event) => onChange({ ...edit.fields, description: event.currentTarget.value })} />
    </div>
    <div><TagInput showHelp={false} value={edit.fields.keywords} onChange={(keywords) => onChange({ ...edit.fields, keywords })} onPendingInputChange={onPendingKeyword} maxItems={SEARCH_PLAN_KEYWORD_MAX_COUNT} maxLength={SEARCH_PLAN_KEYWORD_MAX_BYTES} lengthOf={utf8Length} disabled={busy} copy={{
      label: t("searchPlanKeywords"), help: t("searchPlanKeywordHelp"), placeholder: t("searchPlanKeywordPlaceholder"), add: t("searchPlanAddKeyword"), remove: (keyword) => t("searchPlanRemoveKeyword", { keyword }),
      previous: t("previousPage"), next: t("nextPage"), page: (current, total) => t("page", { current, total }), duplicateError: t("keywordDuplicate"), countError: t("searchPlanKeywordCountError"), lengthError: t("searchPlanKeywordLengthError"),
    }} /></div>
    <section className="grid min-w-0 gap-1.5" aria-label={t("searchPlanScope")}>
      <p className="text-sm text-muted-foreground">{t("searchPlanScopeReadOnlyHelp")}</p>
      <SearchPlanPreview draft={draft} scopeOnly />
    </section>
    {localError && <p role="alert" className="text-sm text-destructive">{t(localError === "description" ? "searchPlanDescriptionError" : edit.fields.keywords.length > SEARCH_PLAN_KEYWORD_MAX_COUNT ? "searchPlanKeywordCountError" : "searchPlanKeywordsError")}</p>}
    {pendingKeyword && <p role="status" className="text-sm text-muted-foreground">{t("searchPlanPendingKeyword")}</p>}
  </>;
}

export function SearchPlanCard({ snapshot, busy, syncing = false, revisionFailure = null, revisionBlocked = false, continuationIssue, onReviseSearchPlan, onRefreshSearchPlan, onConfirm, onCancel, onRequestEdit }: {
  snapshot: ResultSnapshot; busy: boolean; syncing?: boolean; revisionFailure?: ResultClientFailure | null; revisionBlocked?: boolean;
  onReviseSearchPlan?: (binding: ResultBinding, patch: SearchPlanPatch) => Promise<boolean>;
  onRefreshSearchPlan?: () => void;
  continuationIssue?: ContinuationPauseReason | null;
  onConfirm: () => void; onCancel: () => void; onRequestEdit: (instruction: string) => void;
}) {
  const { t } = useAppLocale();
  const [edit, setEdit] = useState<SearchPlanEdit | null>(null);
  const [pendingKeyword, setPendingKeyword] = useState(false);
  const [agentEdit, setAgentEdit] = useState(false);
  const [instruction, setInstruction] = useState("");
  const pending = snapshot.pending_action?.kind === "search_plan" ? snapshot.pending_action : null;
  const available = snapshot.state.kind === "available" && pending != null;
  const disabled = busy || syncing || revisionBlocked;
  const stale = edit != null && pending != null && searchPlanEditStale(edit, snapshot.binding, pending.draft_digest);
  const patch = edit ? searchPlanEditPatch(edit) : {};
  const dirty = Object.keys(patch).length > 0;
  const localError = edit ? searchPlanFieldsError(edit.fields) : null;
  const continuation = continuationStatusCopy(snapshot.continuation.state, snapshot.locale, continuationIssue);
  const failureExpired = revisionFailure?.kind === "server" && ["expired", "absent", "invalid_state"].includes(revisionFailure.failure.code);
  const failureRejected = revisionFailure?.kind === "server" && revisionFailure.failure.code === "invalid_input";
  const failureTooLarge = revisionFailure?.kind === "server" && revisionFailure.failure.code === "search_plan_payload_too_large";
  const discard = () => { setEdit(null); setPendingKeyword(false); };

  const footer = available ? agentEdit ? <AgentEditActions searchPlan value={instruction} busy={busy} disabled={disabled} onClose={() => setAgentEdit(false)} />
    : edit ? <ResultActions
      cancel={{ label: t("searchPlanDiscard"), onClick: discard, disabled: busy }}
      primary={{
        label: busy ? t("searchPlanUpdating") : t("searchPlanUpdate"),
        disabled: disabled || stale || !dirty || !!localError || pendingKeyword || !onReviseSearchPlan,
        onClick: () => { void (async () => {
          if (!onReviseSearchPlan || disabled || stale || !dirty || localError || pendingKeyword) return;
          if (await onReviseSearchPlan(edit.binding, patch)) discard();
        })(); },
      }}
    /> : <ResultActions
      cancel={{ label: t("cancel"), onClick: onCancel, disabled }}
      secondary={{ label: t("searchPlanEdit"), disabled: disabled || !onReviseSearchPlan, onClick: () => setEdit(startSearchPlanEdit(pending!.draft, snapshot.binding, pending!.draft_digest)) }}
      primary={{ label: t("searchPlanConfirm"), onClick: onConfirm, disabled }}
    /> : undefined;

  return <ResultShell snapshot={snapshot} titleOverride={edit || agentEdit ? t("searchPlanEdit") : undefined} footer={footer} syncing={syncing}>
    {available && pending ? agentEdit ? <div id={AGENT_EDIT_PANEL_ID}><AgentEditRequest searchPlan formId={AGENT_EDIT_FORM_ID} value={instruction} busy={disabled} error={null} onChange={setInstruction} onSubmit={onRequestEdit} /></div>
      : edit ? <section className="grid min-w-0 gap-3" aria-label={t("searchPlanEdit")}>
        <SearchPlanEditFields edit={edit} draft={pending.draft} localError={localError} pendingKeyword={pendingKeyword} busy={busy} onChange={(fields) => setEdit({ ...edit, fields })} onPendingKeyword={setPendingKeyword} />
        {stale && <div className="grid gap-2 rounded-xl border bg-muted/40 p-3">
          <p role="alert" className="text-sm">{t("searchPlanConflict")}</p>
          <Button type="button" variant="outline" size="sm" className="h-auto whitespace-normal" disabled={disabled} onClick={() => setEdit(rebaseSearchPlanEdit(edit, pending.draft, snapshot.binding, pending.draft_digest))}>{t("searchPlanRebase")}</Button>
        </div>}
      </section> : <>
        <section aria-label={t("searchPlanScope")}><h2 className="text-sm font-medium">{t("searchPlanScope")}</h2><SearchPlanPreview key={`${snapshot.binding.revision}:${pending.draft_digest}`} draft={pending.draft} /></section>
        {/* The assist panel replaces the preview rather than unfolding under
            it: this Host keeps its own frame height, so an unfolding panel put
            its own textarea and the action bar past the frame's edge. */}
        <div className="border-t pt-4">
          <Button type="button" variant="outline" size="sm" className="h-auto whitespace-normal" disabled={disabled} onClick={() => setAgentEdit(true)}>{t("planAssistLabel")}</Button>
        </div>
      </> : <SearchPlanProjectionView projection={snapshot.presentation} />}
    {revisionFailure && available && (!stale || revisionBlocked || failureTooLarge) && <div role="alert" className="grid gap-2 rounded-xl border border-destructive/30 bg-destructive/10 p-3 text-sm">
      <p>{t(failureExpired ? "searchPlanExpired" : failureTooLarge ? "searchPlanTooLargeDetail" : failureRejected ? "searchPlanRevisionRejected" : "searchPlanRevisionFailed")}</p>
      {revisionBlocked && onRefreshSearchPlan && <Button type="button" variant="outline" size="sm" disabled={busy} onClick={onRefreshSearchPlan}>{t("searchPlanRefresh")}</Button>}
    </div>}
    {syncing && <p role="status" className="text-sm text-muted-foreground">{t("syncing")}</p>}
    <TerminalSummary snapshot={snapshot} />
    {continuation && <p className="text-sm text-muted-foreground" aria-live="polite">{continuation}</p>}
  </ResultShell>;
}
