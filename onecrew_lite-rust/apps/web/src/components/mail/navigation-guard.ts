export type MailNavigationGuard = {
  requestLeave: (continueNavigation: () => void) => void;
};

/** Defers navigation until the mounted Mail workspace resolves its composer guard. */
export function requestGuardedMailNavigation(
  guard: MailNavigationGuard | null,
  continueNavigation: () => void,
): "guarded" | "continued" {
  if (!guard) {
    continueNavigation();
    return "continued";
  }
  guard.requestLeave(continueNavigation);
  return "guarded";
}
