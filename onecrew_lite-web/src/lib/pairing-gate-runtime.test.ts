import { afterEach, describe, expect, it, vi } from "vitest";

import type { LoginProjection } from "@/lib/generated/creator-sync-contracts";
import { pairingGatePhase, pairingOwnerKey } from "@/lib/pairing-gate-state";
import {
  createPairingPollFallback,
  PAIRING_MAX_POLL_ATTEMPTS,
  PAIRING_POLL_INTERVAL_MS,
  PAIRING_POLL_WINDOW_MS,
  pairingGateActionHandlers,
  pairingOwnerTransition,
} from "./pairing-gate-runtime";

function projection(
  state: LoginProjection["status"]["state"],
): LoginProjection {
  return {
    browser_opened: false,
    callback_received: false,
    issue: null,
    flow_id: state === "pairing_pending" ? "flow-1" : null,
    status: {
      action: null,
      credential_id: state === "active" ? "credential-1" : null,
      installation_id: state === "active" ? "installation-1" : null,
      issue: null,
      state,
    },
  };
}

describe("PairingGate runtime convergence", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("routes restart directly through begin without canceling first", () => {
    const active = projection("active");
    const begin = vi.fn().mockResolvedValue(active);
    const reopen = vi.fn().mockResolvedValue(active);
    const cancel = vi.fn().mockResolvedValue(active);
    const run = vi.fn((action: () => Promise<LoginProjection>) => {
      void action();
    });
    const handlers = pairingGateActionHandlers(run, { begin, reopen, cancel });

    handlers.onRestart();

    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith(begin);
    expect(begin).toHaveBeenCalledOnce();
    expect(reopen).not.toHaveBeenCalled();
    expect(cancel).not.toHaveBeenCalled();
  });

  it("keeps polling beyond the old 90-second limit and through a five-minute budget", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const refresh = vi.fn().mockResolvedValue(undefined);
    const onExhausted = vi.fn();
    const fallback = createPairingPollFallback({ refresh, onExhausted });

    expect(PAIRING_POLL_INTERVAL_MS).toBe(1_500);
    expect(PAIRING_POLL_WINDOW_MS).toBe(300_000);
    expect(PAIRING_MAX_POLL_ATTEMPTS).toBe(200);

    fallback.setPhase("waiting");
    await vi.advanceTimersByTimeAsync(91_500);
    expect(refresh).toHaveBeenCalledTimes(61);
    expect(onExhausted).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(PAIRING_POLL_INTERVAL_MS);
    expect(refresh).toHaveBeenCalledTimes(62);
    expect(onExhausted).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(
      PAIRING_POLL_WINDOW_MS - 93_000 - PAIRING_POLL_INTERVAL_MS,
    );
    expect(refresh).toHaveBeenCalledTimes(199);
    expect(onExhausted).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(PAIRING_POLL_INTERVAL_MS);
    expect(refresh).toHaveBeenCalledTimes(200);
    expect(onExhausted).toHaveBeenCalledOnce();
    fallback.stop();
  });

  it("accepts an active callback event after ninety seconds but before the deadline", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const refresh = vi.fn().mockResolvedValue(undefined);
    const onExhausted = vi.fn();
    const fallback = createPairingPollFallback({ refresh, onExhausted });

    fallback.setPhase("waiting");
    await vi.advanceTimersByTimeAsync(120_000);
    expect(refresh).toHaveBeenCalledTimes(80);
    expect(onExhausted).not.toHaveBeenCalled();

    const activeProjection = projection("active");
    const activeOwner = pairingOwnerKey(activeProjection);
    const activating = pairingOwnerTransition(
      null,
      pairingGatePhase(activeProjection),
      activeOwner,
    );
    fallback.setPhase("active");
    expect(activating.nextReadyOwner).toBe(
      "creator-sync:installation-1:credential-1",
    );

    await vi.advanceTimersByTimeAsync(PAIRING_POLL_WINDOW_MS);
    expect(refresh).toHaveBeenCalledTimes(80);
    expect(onExhausted).not.toHaveBeenCalled();
    fallback.stop();
  });

  it("gates event activation until cache clear and unmounts before nonactive clear", () => {
    const activeProjection = projection("active");
    const activeOwner = pairingOwnerKey(activeProjection);
    const activating = pairingOwnerTransition(
      null,
      pairingGatePhase(activeProjection),
      activeOwner,
    );
    expect(activating).toEqual({
      nextReadyOwner: "creator-sync:installation-1:credential-1",
      renderOwner: null,
      shouldClearQueries: true,
    });

    const activated = pairingOwnerTransition(
      activating.nextReadyOwner,
      pairingGatePhase(activeProjection),
      activeOwner,
    );
    expect(activated.renderOwner).toBe("creator-sync:installation-1:credential-1");
    expect(activated.shouldClearQueries).toBe(false);

    const nonactiveProjection = projection("pairing_required");
    const deactivating = pairingOwnerTransition(
      activated.renderOwner,
      pairingGatePhase(nonactiveProjection),
      pairingOwnerKey(nonactiveProjection),
    );
    expect(deactivating.renderOwner).toBeNull();
    expect(deactivating.shouldClearQueries).toBe(true);
    expect(deactivating.nextReadyOwner).toBeNull();

    const restarted = projection("pairing_pending");
    expect(pairingGatePhase(restarted)).toBe("waiting");
    expect(
      pairingOwnerTransition(
        deactivating.nextReadyOwner,
        pairingGatePhase(restarted),
        pairingOwnerKey(restarted),
      ),
    ).toEqual({
      nextReadyOwner: null,
      renderOwner: null,
      shouldClearQueries: false,
    });
  });
});
