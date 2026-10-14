import { Button } from "@mcp-ui/components/ui/button";

/** One action, described rather than assembled by each card. */
type CardActionCopy = {
  label: string;
  disabled?: boolean;
  /** The unshortened name, when the visible label is clamped. */
  ariaLabel?: string;
  ariaExpanded?: boolean;
  ariaControls?: string;
  /** Bound a label whose length the server decides to two lines. */
  clamp?: boolean;
};

/** An action either calls back, or submits a form the card body owns.
 *
 * A panel that takes over the card body keeps its own validation: naming its
 * form here lets the one action bar submit that panel from outside it, so the
 * bar carries the panel's action without the card holding a second copy of the
 * panel's draft state.
 */
export type CardAction = CardActionCopy & (
  | { onClick: () => void; form?: never }
  | { form: string; onClick?: never }
);

/** The one action bar every card renders into the shared shell footer.
 *
 * Every Host's guidance says the same two things: at most two actions, placed
 * at the bottom of the card. Cards used to build this row by hand, so the same
 * three buttons arrived as `ghost`+`outline`+`sm` on one card and as plain
 * defaults on another, and the third one wrapped onto a second line that the
 * Host's height cap then clipped away.
 *
 * `cancel` is the way out rather than a third choice, so it sits left, away
 * from the two decisions on the right. The bar is not pinned: on web and
 * desktop the conversation composer can overlay an inline app's bottom edge,
 * so a floor-stuck bar is the least reachable place for a confirm button. A
 * card stays reachable by fitting the frame, not by sticking to it.
 */
export function ResultActions({ cancel, secondary, primary }: {
  cancel?: CardAction; secondary?: CardAction; primary?: CardAction;
}) {
  return <>
    {cancel && <ActionButton action={cancel} variant="ghost" className="mr-auto" />}
    {secondary && <ActionButton action={secondary} variant="outline" />}
    {primary && <ActionButton action={primary} />}
  </>;
}

function ActionButton({ action, variant, className }: {
  action: CardAction; variant?: "ghost" | "outline"; className?: string;
}) {
  return <Button
    type={action.form ? "submit" : "button"}
    form={action.form}
    variant={variant}
    className={className}
    disabled={action.disabled}
    aria-label={action.ariaLabel}
    aria-expanded={action.ariaExpanded}
    aria-controls={action.ariaControls}
    onClick={action.onClick}
  >
    {action.clamp ? <span className="line-clamp-2">{action.label}</span> : action.label}
  </Button>;
}
