import type { LoginProjection } from "@/lib/generated/creator-sync-contracts";
import type { PairingGatePhase } from "@/lib/pairing-gate-state";

export const PAIRING_POLL_INTERVAL_MS = 1_500;
export const PAIRING_POLL_WINDOW_MS = 5 * 60 * 1_000;
export const PAIRING_MAX_POLL_ATTEMPTS = Math.ceil(
  PAIRING_POLL_WINDOW_MS / PAIRING_POLL_INTERVAL_MS,
);

const POLLING_PHASES = new Set<PairingGatePhase>(["waiting", "callback", "enrolling"]);

export type PairingOwnerTransition = {
  nextReadyOwner: string | null;
  renderOwner: string | null;
  shouldClearQueries: boolean;
};

export type PairingAction = () => Promise<LoginProjection>;

export type PairingActions = {
  begin: PairingAction;
  reopen: PairingAction;
  cancel: PairingAction;
};

export function pairingGateActionHandlers(
  run: (action: PairingAction) => void,
  actions: PairingActions,
) {
  return {
    onBegin: () => run(actions.begin),
    onReopen: () => run(actions.reopen),
    onCancel: () => run(actions.cancel),
    onRestart: () => run(actions.begin),
  };
}

export function pairingOwnerTransition(
  readyOwner: string | null,
  phase: PairingGatePhase,
  ownerKey: string | null,
): PairingOwnerTransition {
  const nextReadyOwner = phase === "active" ? ownerKey : null;
  return {
    nextReadyOwner,
    renderOwner:
      phase === "active" && ownerKey !== null && readyOwner === ownerKey
        ? ownerKey
        : null,
    shouldClearQueries: nextReadyOwner !== readyOwner,
  };
}

type PairingPollClock = {
  now: () => number;
  setTimer: (callback: () => void, delayMs: number) => number;
  clearTimer: (timer: number) => void;
};

type PairingPollFallbackOptions = {
  refresh: () => Promise<void>;
  onExhausted: () => void;
  clock?: PairingPollClock;
};

export type PairingPollFallback = {
  setPhase: (phase: PairingGatePhase) => void;
  stop: () => void;
};

const SYSTEM_POLL_CLOCK: PairingPollClock = {
  now: () => Date.now(),
  setTimer: (callback, delayMs) => globalThis.setTimeout(callback, delayMs) as unknown as number,
  clearTimer: (timer) => globalThis.clearTimeout(timer),
};

export function createPairingPollFallback({
  refresh,
  onExhausted,
  clock = SYSTEM_POLL_CLOCK,
}: PairingPollFallbackOptions): PairingPollFallback {
  let phase: PairingGatePhase = "checking";
  let timer: number | null = null;
  let startedAt: number | null = null;
  let attempts = 0;
  let refreshing = false;
  let exhausted = false;
  let stopped = false;

  const clearScheduledTimer = () => {
    if (timer === null) return;
    clock.clearTimer(timer);
    timer = null;
  };

  const schedule = () => {
    if (
      stopped
      || exhausted
      || refreshing
      || timer !== null
      || startedAt === null
      || !POLLING_PHASES.has(phase)
    ) return;

    const elapsedMs = Math.max(0, clock.now() - startedAt);
    if (attempts >= PAIRING_MAX_POLL_ATTEMPTS || elapsedMs >= PAIRING_POLL_WINDOW_MS) {
      exhausted = true;
      onExhausted();
      return;
    }

    const delayMs = Math.min(
      PAIRING_POLL_INTERVAL_MS,
      PAIRING_POLL_WINDOW_MS - elapsedMs,
    );
    timer = clock.setTimer(() => {
      timer = null;
      if (stopped || exhausted || !POLLING_PHASES.has(phase)) return;
      attempts += 1;
      refreshing = true;
      void refresh().then(
        () => {
          refreshing = false;
          schedule();
        },
        () => {
          refreshing = false;
          schedule();
        },
      );
    }, delayMs);
  };

  return {
    setPhase(nextPhase) {
      const wasPolling = POLLING_PHASES.has(phase);
      const willPoll = POLLING_PHASES.has(nextPhase);
      phase = nextPhase;

      if (!willPoll) {
        clearScheduledTimer();
        startedAt = null;
        attempts = 0;
        exhausted = false;
        return;
      }

      if (!wasPolling) {
        startedAt = clock.now();
        attempts = 0;
        exhausted = false;
      }
      schedule();
    },
    stop() {
      stopped = true;
      clearScheduledTimer();
    },
  };
}
