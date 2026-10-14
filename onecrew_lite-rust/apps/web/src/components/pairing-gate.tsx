import type { ReactNode } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { OneCrewMark } from "@/components/brand";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  pairingGateView,
  type PairingGateAlertCopy,
  type PairingGateCopy,
  type PairingGatePhase,
  type PairingGatePrimaryAction,
  type PairingGatePrimaryLabel,
  type PairingGateProgress,
} from "@/lib/pairing-gate-state";

export type PairingGateProps = {
  phase: PairingGatePhase;
  onBegin: () => void;
  onReopen: () => void;
  onCancel: () => void;
  onRestart: () => void;
  authorizationUrl?: string;
  children: ReactNode;
};

const STATE_COPY_KEYS = {
  checking: { title: "states.checking.title", description: "states.checking.description" },
  idle: { title: "states.idle.title", description: "states.idle.description" },
  waiting: { title: "states.waiting.title", description: "states.waiting.description" },
  callback: { title: "states.callback.title", description: "states.callback.description" },
  enrolling: { title: "states.enrolling.title", description: "states.enrolling.description" },
  expired: { title: "states.expired.title", description: "states.expired.description" },
  canceled: { title: "states.canceled.title", description: "states.canceled.description" },
  invalidated: { title: "states.invalidated.title", description: "states.invalidated.description" },
  remoteUnavailable: { title: "states.remoteUnavailable.title", description: "states.remoteUnavailable.description" },
  rateLimited: { title: "states.rateLimited.title", description: "states.rateLimited.description" },
  deviceNameUnavailable: { title: "states.deviceNameUnavailable.title", description: "states.deviceNameUnavailable.description" },
  deviceNameInvalid: { title: "states.deviceNameInvalid.title", description: "states.deviceNameInvalid.description" },
  error: { title: "states.error.title", description: "states.error.description" },
} as const satisfies Record<PairingGateCopy, { title: string; description: string }>;

const PRIMARY_LABEL_KEYS = {
  begin: "actions.begin",
  reopen: "actions.reopen",
  restart: "actions.restart",
  rebind: "actions.rebind",
  retryNetwork: "actions.retryNetwork",
  retry: "actions.retry",
} as const satisfies Record<PairingGatePrimaryLabel, string>;

const PROGRESS_KEYS = {
  checking: "progress.checking",
  callback: "progress.callback",
  enrolling: "progress.enrolling",
} as const satisfies Record<PairingGateProgress, string>;

const ALERT_COPY_KEYS = {
  expired: { title: "alerts.expired.title", description: "alerts.expired.description" },
  canceled: { title: "alerts.canceled.title", description: "alerts.canceled.description" },
  invalidated: { title: "alerts.invalidated.title", description: "alerts.invalidated.description" },
  remoteUnavailable: { title: "alerts.remoteUnavailable.title", description: "alerts.remoteUnavailable.description" },
  rateLimited: { title: "alerts.rateLimited.title", description: "alerts.rateLimited.description" },
  deviceNameUnavailable: { title: "alerts.deviceNameUnavailable.title", description: "alerts.deviceNameUnavailable.description" },
  deviceNameInvalid: { title: "alerts.deviceNameInvalid.title", description: "alerts.deviceNameInvalid.description" },
  error: { title: "alerts.error.title", description: "alerts.error.description" },
} as const satisfies Record<PairingGateAlertCopy, { title: string; description: string }>;

export function PairingGate({
  phase,
  onBegin,
  onReopen,
  onCancel,
  onRestart,
  authorizationUrl,
  children,
}: PairingGateProps) {
  const { t } = useTranslation("pairing");
  const presentation = pairingGateView(phase);

  if (presentation.renderChildren) {
    return <>{children}</>;
  }

  if (!presentation.copy) return null;

  const copy = STATE_COPY_KEYS[presentation.copy];
  const progressLabel = presentation.progress
    ? t(PROGRESS_KEYS[presentation.progress])
    : null;
  const alertCopy = presentation.alert
    ? ALERT_COPY_KEYS[presentation.alert.copy]
    : null;
  const primaryLabel = presentation.primaryLabel
    ? t(PRIMARY_LABEL_KEYS[presentation.primaryLabel])
    : null;

  const primaryAction = actionHandler(
    presentation.primaryAction,
    onBegin,
    onReopen,
    onRestart,
  );

  return (
    <main
      className="grid min-h-svh w-full place-items-center overflow-y-auto bg-background px-5 py-16 text-foreground sm:px-8"
      aria-labelledby="pairing-gate-title"
    >
      <section className="flex w-full max-w-md flex-col items-center text-center">
        <div className="grid size-20 place-items-center rounded-3xl border bg-card text-foreground shadow-sm dark:bg-card/70">
          <OneCrewMark className="size-12" />
        </div>

        <div className="mt-10 min-w-0">
          <h1 id="pairing-gate-title" className="text-balance text-2xl font-semibold tracking-tight sm:text-3xl">
            {t(copy.title)}
          </h1>
          <p className="mx-auto mt-4 max-w-sm text-pretty text-sm leading-6 text-muted-foreground sm:text-base">
            {t(copy.description)}
          </p>
        </div>

        {progressLabel && (
          <div
            className="mt-9 flex min-h-10 items-center gap-3 rounded-full border border-[var(--brand-accent)]/40 bg-[var(--tone-progress-bg)] px-5 text-sm text-[var(--tone-progress-fg)]"
            role="status"
            aria-live="polite"
          >
            <Loader2 className="size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            <span>{progressLabel}</span>
          </div>
        )}

        {presentation.alert && alertCopy && (
          <Alert variant={presentation.alert.variant} className="mt-8 text-left">
            <AlertTriangle />
            <AlertTitle>{t(alertCopy.title)}</AlertTitle>
            <AlertDescription>{t(alertCopy.description)}</AlertDescription>
          </Alert>
        )}

        {(primaryAction || presentation.canCancel) && (
          <div className="mt-9 flex w-full flex-col items-stretch gap-2 sm:items-center">
            {primaryAction && primaryLabel && (
              <Button
                size="lg"
                className="w-full sm:min-w-64 sm:w-auto"
                asChild={presentation.primaryAction === "reopen" && Boolean(authorizationUrl)}
                onClick={presentation.primaryAction === "reopen" && authorizationUrl ? undefined : primaryAction}
              >
                {presentation.primaryAction === "reopen" && authorizationUrl
                  ? <a href={authorizationUrl} target="_blank" rel="noopener noreferrer">{primaryLabel}</a>
                  : primaryLabel}
              </Button>
            )}
            {presentation.canCancel && (
              <Button variant="ghost" size="sm" className="self-center" onClick={onCancel}>
                {t("actions.cancel")}
              </Button>
            )}
          </div>
        )}
      </section>
    </main>
  );
}

function actionHandler(
  action: PairingGatePrimaryAction,
  onBegin: () => void,
  onReopen: () => void,
  onRestart: () => void,
): (() => void) | null {
  switch (action) {
    case "begin":
      return onBegin;
    case "reopen":
      return onReopen;
    case "restart":
      return onRestart;
    case null:
      return null;
  }
}
