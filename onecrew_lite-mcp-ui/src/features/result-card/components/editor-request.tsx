import { Button } from "@mcp-ui/components/ui/button";
import { Label } from "@mcp-ui/components/ui/label";
import { Textarea } from "@mcp-ui/components/ui/textarea";
import { ResultActions } from "./result-actions";

import { normalizeEditInstruction } from "@mcp-ui/lib/result-contract";
import { useAppLocale } from "@mcp-ui/lib/i18n";

/** The panel the card body swaps in, and the form its action bar submits.
 *
 * Only one edit panel is open on a card at a time, so one pair of ids names
 * both the region the close control points at and the form the bar submits. */
export const AGENT_EDIT_PANEL_ID = "agent-edit-panel";
export const AGENT_EDIT_FORM_ID = "agent-edit-form";

/** The edit panel's own actions, contributed to the card's one action bar.
 *
 * The panel takes over the card body, so it draws no button row of its own:
 * closing it and sending it are the two actions of that surface, and both
 * belong in the one bar the shell already renders. The send reaches the panel
 * through `form=`, so the panel keeps its own validation and the card never
 * holds a second copy of the draft it is validating. */
export function AgentEditActions({ value, busy, disabled, searchPlan = false, initialSearch = false, onClose }: {
  value: string;
  busy: boolean;
  disabled: boolean;
  searchPlan?: boolean;
  initialSearch?: boolean;
  onClose: () => void;
}) {
  const { t } = useAppLocale();
  return <ResultActions
    secondary={{ label: t("closeEdit"), onClick: onClose, disabled, ariaExpanded: true, ariaControls: AGENT_EDIT_PANEL_ID }}
    primary={{
      label: busy ? t("sending") : t(searchPlan ? initialSearch ? "planInitialSubmit" : "planAssistSubmit" : "sendEdit"),
      form: AGENT_EDIT_FORM_ID,
      disabled: disabled || normalizeEditInstruction(value) == null,
    }}
  />;
}

export function AgentEditRequest({ value, busy, error, onChange, onSubmit, searchPlan = false, initialSearch = false, hideLabel = false, formId }: {
  formId?: string;
  searchPlan?: boolean;
  initialSearch?: boolean;
  hideLabel?: boolean;
  value: string;
  busy: boolean;
  error: string | null;
  onChange: (value: string) => void;
  onSubmit: (instruction: string) => void;
}) {
  const normalized = normalizeEditInstruction(value);
  const { t } = useAppLocale();
  const validationError = error || (value.trim() && normalized == null ? t("customInvalid") : null);
  // Help and error text is content, not an action: it stays in the panel body
  // whether or not the panel keeps a button beside it.
  const help = searchPlan
    ? <p id={validationError ? "agent-edit-error" : "agent-edit-help"} role={validationError ? "alert" : undefined} className={validationError ? "text-xs text-destructive" : "text-xs text-muted-foreground"}>{validationError || t(initialSearch ? "planInitialHelp" : "planAssistHelp")}</p>
    : null;
  return (
    <form id={formId} onSubmit={event => { event.preventDefault(); if (!busy && normalized) onSubmit(normalized); }} aria-labelledby="agent-edit-title" className={searchPlan ? "plan-assist" : "grid gap-3"}>
      <div className="grid gap-2">
        <Label className={hideLabel ? "sr-only" : undefined} id="agent-edit-title" htmlFor="agent-edit-instruction">{t(searchPlan ? initialSearch ? "planInitialLabel" : "planAssistLabel" : "editLabel")}</Label>
        <Textarea
          id="agent-edit-instruction"
          className="field-sizing-fixed h-28 min-h-28 resize-none"
          style={searchPlan ? {height:72,minHeight:72} : undefined}
          value={value}
          disabled={busy}
          maxLength={2_048}
          rows={3}
          aria-invalid={validationError != null}
          aria-describedby={validationError ? "agent-edit-error" : searchPlan ? "agent-edit-help" : undefined}
          placeholder={t(searchPlan ? initialSearch ? "planInitialPlaceholder" : "planAssistPlaceholder" : "editDescription")}
          onChange={(event) => onChange(event.currentTarget.value)}
        />
        {!searchPlan && validationError && <p id="agent-edit-error" role="alert" className="text-xs text-destructive">{validationError}</p>}
      </div>
      {/* Named from outside, this panel is submitted by the card's one action
          bar, so it draws no row of its own. The standalone panel — a card is
          not what surrounds it — keeps the row it always had. */}
      {formId ? help : <div className={searchPlan ? "plan-assist-footer" : "flex flex-wrap justify-end gap-2"}>
        {help}
        <Button type="button" disabled={busy || normalized == null} onClick={() => normalized && onSubmit(normalized)}>
          {busy ? t("sending") : t(searchPlan ? initialSearch ? "planInitialSubmit" : "planAssistSubmit" : "sendEdit")}
        </Button>
      </div>}
    </form>
  );
}
