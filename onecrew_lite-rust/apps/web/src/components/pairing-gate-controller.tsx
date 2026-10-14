import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { PairingGate } from "@/components/pairing-gate";
import {
  beginCreatorSyncPairing,
  cancelCreatorSyncPairing,
  readCreatorSyncPairingStatus,
  reopenCreatorSyncPairing,
  subscribeCreatorSyncPairingStatus,
  pairingAuthorizationUrl,
} from "@/lib/creator-sync-pairing";
import type { LoginProjection } from "@/lib/generated/creator-sync-contracts";
import { pairingGatePhase, pairingOwnerKey, type PairingGatePhase } from "@/lib/pairing-gate-state";
import {
  createPairingPollFallback,
  pairingGateActionHandlers,
  pairingOwnerTransition,
} from "@/lib/pairing-gate-runtime";

const PAIRING_ACTIONS = {
  begin: beginCreatorSyncPairing,
  reopen: reopenCreatorSyncPairing,
  cancel: cancelCreatorSyncPairing,
};

export function PairingGateController({
  children,
}: {
  children: (ownerKey: string) => ReactNode;
}) {
  const queryClient = useQueryClient();
  const [projection, setProjection] = useState<LoginProjection | null>(null);
  const [failed, setFailed] = useState(false);
  const [actionPending, setActionPending] = useState(false);
  const [readyOwner, setReadyOwner] = useState<string | null>(null);
  const eventEpoch = useRef(0);
  const actionInFlight = useRef(false);
  const pollFallback = useRef<ReturnType<typeof createPairingPollFallback> | null>(null);

  const refresh = useCallback(async () => {
    const observedEventEpoch = eventEpoch.current;
    try {
      const nextProjection = await readCreatorSyncPairingStatus();
      if (observedEventEpoch !== eventEpoch.current) return;
      setProjection(nextProjection);
      setFailed(false);
    } catch {
      if (observedEventEpoch !== eventEpoch.current) return;
      setFailed(true);
    }
  }, []);

  useEffect(() => {
    let active = true;
    const subscription = subscribeCreatorSyncPairingStatus((nextProjection) => {
      if (!active) return;
      eventEpoch.current += 1;
      setProjection(nextProjection);
      setFailed(false);
      setActionPending(false);
    });
    // Event delivery is the primary convergence path. A registration failure
    // stays non-fatal because the initial read and bounded fallback remain.
    // The initial read starts only after registration settles, so a completed
    // browser callback cannot disappear between the read and the listener.
    void subscription.then(
      () => { if (active) void refresh(); },
      () => { if (active) void refresh(); },
    );
    return () => {
      active = false;
      eventEpoch.current += 1;
      void subscription.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [refresh]);

  const mappedPhase = projection ? pairingGatePhase(projection) : "checking";
  const phase: PairingGatePhase = failed ? "error" : actionPending ? "checking" : mappedPhase;

  useEffect(() => {
    const fallback = createPairingPollFallback({
      refresh,
      onExhausted: () => setFailed(true),
    });
    pollFallback.current = fallback;
    return () => {
      fallback.stop();
      if (pollFallback.current === fallback) pollFallback.current = null;
    };
  }, [refresh]);

  useEffect(() => {
    pollFallback.current?.setPhase(phase);
  }, [phase]);

  const ownerKey = useMemo(() => projection ? pairingOwnerKey(projection) : null, [projection]);
  const ownerTransition = useMemo(
    () => pairingOwnerTransition(readyOwner, phase, ownerKey),
    [ownerKey, phase, readyOwner],
  );
  useEffect(() => {
    if (!ownerTransition.shouldClearQueries) return;
    queryClient.clear();
    setReadyOwner(ownerTransition.nextReadyOwner);
  }, [ownerTransition, queryClient]);

  const run = useCallback(async (action: () => Promise<LoginProjection>) => {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    const observedEventEpoch = eventEpoch.current;
    setActionPending(true);
    setFailed(false);
    try {
      const nextProjection = await action();
      if (observedEventEpoch === eventEpoch.current) setProjection(nextProjection);
    } catch {
      if (observedEventEpoch === eventEpoch.current) setFailed(true);
    } finally {
      actionInFlight.current = false;
      if (observedEventEpoch === eventEpoch.current) setActionPending(false);
    }
  }, []);

  const actionHandlers = useMemo(
    () => pairingGateActionHandlers((action) => void run(action), PAIRING_ACTIONS),
    [run],
  );

  return (
    <PairingGate
      phase={phase === "active" && ownerTransition.renderOwner === null ? "checking" : phase}
      authorizationUrl={pairingAuthorizationUrl(projection?.flow_id)}
      {...actionHandlers}
    >
      {ownerTransition.renderOwner ? children(ownerTransition.renderOwner) : null}
    </PairingGate>
  );
}
