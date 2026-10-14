/** Semantic badge tones — all color lives in the --tone-* CSS variables
 *  (theme.css, light + dark), so nothing hardcodes bg-emerald-100 dark:... . */
export const tone = {
  success: "bg-[var(--tone-success-bg)] text-[var(--tone-success-fg)]",
  info: "bg-[var(--tone-info-bg)] text-[var(--tone-info-fg)]",
  warning: "bg-[var(--tone-warning-bg)] text-[var(--tone-warning-fg)]",
  neutral: "bg-[var(--tone-neutral-bg)] text-[var(--tone-neutral-fg)]",
  muted: "bg-muted text-muted-foreground",
} as const;

/** The one inset-panel recipe. Two muted alphas exist app-wide: /20 for an
 *  inset panel, /50 for a hover state; anything else is a drift. */
export const panel = "rounded-lg border bg-muted/20";

/** Table and list cell text. The primary line inherits text-sm; every
 *  secondary line is one step down and muted. */
export const cellPrimary = "truncate font-medium";
export const cellMeta = "truncate text-xs text-muted-foreground";
