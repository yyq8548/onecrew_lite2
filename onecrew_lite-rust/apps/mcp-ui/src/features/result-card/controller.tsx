import { useEffect, useRef, useState } from "react";
import type { App } from "@modelcontextprotocol/ext-apps/react";

import type { ResultBinding, ResultDmFailure, ResultSnapshot, ResultSnapshotOutput, SearchPlanPatch } from "@mcp-ui/lib/result-contract";
import {
  cancelResult,
  confirmResult,
  confirmCreatorSaveBatch,
  dispatchPendingContinuation,
  readCurrentResult,
  requestAgentEdit,
  resultRefreshDelay,
  ResultClientError,
  selectResult,
  type ResultClientFailure,
  type ContinuationPauseReason,
} from "@mcp-ui/lib/result-client";
import { resultClientFailureCopy, reviewRevisionAfterSnapshot } from "./presentation";
import { ResultCardView } from "./view";
import { ResultFailureShell } from "./components/shell";
import { reviseSearchPlanWithReadback } from "@mcp-ui/features/result-card/search-plan-revision";

export function ResultCard({ appClient, initial }: { appClient: App; initial: ResultSnapshot }) {
  const [snapshot, setSnapshot] = useState(initial);
  const [reviewRevision, setReviewRevision] = useState(initial.binding.revision);
  const [hydrated, setHydrated] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<ResultClientFailure | null>(null);
  const [revisionFailure, setRevisionFailure] = useState<ResultClientFailure | null>(null);
  const [revisionBlocked, setRevisionBlocked] = useState(false);
  const [recoveryFailure, setRecoveryFailure] = useState<ResultClientFailure | null>(null);
  const [dmFailure, setDmFailure] = useState<ResultDmFailure | null>(null);
  const [continuationIssue, setContinuationIssue] = useState<ContinuationPauseReason | null>(null);
  const [pollTick, setPollTick] = useState(0);
  const hydrationAttempts = useRef(new Set<string>());
  const submittedPolls = useRef(0);
  const continuationAttempts = useRef(new Set<string>());
  const mounted = useRef(true);
  const inFlight = useRef(false);
  const latestSnapshot = useRef(snapshot);
  latestSnapshot.current = snapshot;

  function adoptSnapshot(next: ResultSnapshot, confirmed = false) {
    const previous = latestSnapshot.current;
    if (next.binding.result_ref === previous.binding.result_ref &&
      (next.binding.revision < previous.binding.revision || next.continuation.continuation_revision < previous.continuation.continuation_revision)) return false;
    latestSnapshot.current = next;
    setReviewRevision(current => reviewRevisionAfterSnapshot(current, previous, next, confirmed));
    setSnapshot(next);
    return true;
  }

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  // Returning from OneCrew's language switcher refreshes display preferences
  // without re-minting authority or discarding the wizard's local draft.
  useEffect(() => {
    let active = true;
    let reading = false;
    const refresh = async () => {
      if (reading || inFlight.current || document.visibilityState === "hidden") return;
      reading = true;
      try {
        const current = await readCurrentResult(appClient, latestSnapshot.current.binding);
        const previous = latestSnapshot.current;
        if (active && current.binding.result_ref === previous.binding.result_ref &&
          current.binding.revision >= previous.binding.revision &&
          current.continuation.continuation_revision >= previous.continuation.continuation_revision) adoptSnapshot(current);
      } catch { /* A preference refresh never changes a known result into a guessed outcome. */ }
      finally { reading = false; }
    };
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => {
      active = false;
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", refresh);
    };
  }, [appClient]);

  useEffect(() => {
    const key = `${initial.binding.result_ref}:${initial.binding.revision}`;
    if (hydrationAttempts.current.has(key)) return;
    hydrationAttempts.current.add(key);
    void readCurrentResult(appClient, initial.binding)
      .then((current) => {
        if (!mounted.current) return;
        adoptSnapshot(current);
        setHydrated(true);
      })
      .catch((error) => {
        if (!mounted.current) return;
        setFailure(clientFailure(error));
        setHydrated(true);
      });
  }, [appClient, initial.binding]);

  // Each pending continuation revision is dispatched once, whichever path
  // sees it first: the user action's own chain or a hydrated remount.
  function startContinuation(pending: ResultSnapshot) {
    if (pending.continuation.state !== "pending") return;
    const key = `${pending.binding.result_ref}:${pending.continuation.continuation_revision}`;
    if (continuationAttempts.current.has(key)) return;
    continuationAttempts.current.add(key);
    void dispatchPendingContinuation(appClient, pending, (reason) => { if (mounted.current) setContinuationIssue(reason); })
      .then((current) => { if (mounted.current) adoptSnapshot(current); })
      .catch(async (error) => {
        if (!mounted.current) return;
        const currentFailure = clientFailure(error);
        if (currentFailure.kind === "server" && [
          "invalid_state",
          "revision_conflict",
          "continuation_revision_conflict",
          "continuation_lease_mismatch",
        ].includes(currentFailure.failure.code)) {
          try {
            const current = await readCurrentResult(appClient, pending.binding);
            if (mounted.current) adoptSnapshot(current);
            return;
          } catch (refreshError) {
            if (mounted.current) setRecoveryFailure(clientFailure(refreshError));
            return;
          }
        }
        if (mounted.current) setRecoveryFailure(currentFailure);
      });
  }

  useEffect(() => {
    if (!hydrated || failure || recoveryFailure) return;
    startContinuation(snapshot);
  }, [appClient, failure, recoveryFailure, hydrated, snapshot]);

  // The server settles a result without telling anyone: a dispatch lease
  // expires on its own clock, and a submitted owner write runs inside the
  // App's own request, so a lost response leaves this card holding the last
  // pre-terminal snapshot. Re-read on the delay the snapshot itself implies.
  useEffect(() => {
    if (!hydrated || failure || recoveryFailure) return;
    const submitted = snapshot.state.kind === "submitted";
    const delay = resultRefreshDelay(snapshot, Date.now(), submittedPolls.current);
    if (delay == null) {
      // Out of attempts while still unsettled: stop claiming progress and
      // offer the state check instead of an outcome this card cannot confirm.
      if (submitted) setRecoveryFailure({ kind: "transport" });
      return;
    }
    let active = true;
    const timer = window.setTimeout(() => {
      void readCurrentResult(appClient, snapshot.binding)
        .then((current) => {
          if (!active) return;
          if (submitted) submittedPolls.current += 1;
          adoptSnapshot(current);
          setPollTick((tick) => tick + 1);
        })
        .catch((error) => {
          if (!active) return;
          // Polling reads work the server already accepted; one failed read
          // must not turn it into a reported outcome. Keep the budget moving
          // so an unreachable service still reaches the state check.
          if (!submitted) { setRecoveryFailure(clientFailure(error)); return; }
          submittedPolls.current += 1;
          setPollTick((tick) => tick + 1);
        });
    }, delay);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [appClient, failure, recoveryFailure, hydrated, snapshot, pollTick]);

  async function transition(action: () => Promise<ResultSnapshot | ResultSnapshotOutput>, confirmsReview = false) {
    if (!hydrated || inFlight.current || revisionBlocked || latestSnapshot.current.state.kind !== "available") return;
    inFlight.current = true;
    submittedPolls.current = 0;
    setBusy(true);
    setFailure(null);
    setRevisionFailure(null);
    setDmFailure(null);
    try {
      const response = await action();
      const next = "snapshot" in response ? response.snapshot : response;
      if (!adoptSnapshot(next, confirmsReview)) return;
      // Owner diagnostics accompany an already-consumed review. Apply the
      // authoritative result before displaying evidence from its DM stage.
      if ("snapshot" in response) setDmFailure(response.dm_failure ?? null);
      // Continue in the click's own async chain: waiting for a re-render
      // could outlast the Host's user-gesture window for the follow-up.
      if (!recoveryFailure) startContinuation(next);
    } catch (error) {
      if (["search_plan_editor", "creator_save_batch_confirmation"].includes(snapshot.presentation.kind)) {
        setRevisionFailure(clientFailure(error));
        setRevisionBlocked(true);
        try {
          const current = await readCurrentResult(appClient, snapshot.binding);
          adoptSnapshot(current);
          setRevisionBlocked(false);
          if (current.state.kind !== "available") setRevisionFailure(null);
        } catch { /* Keep the draft visible, with all authority-bearing actions blocked. */ }
      } else {
        setRecoveryFailure(clientFailure(error));
        setRevisionBlocked(true);
        try {
          const current = await readCurrentResult(appClient, snapshot.binding);
          adoptSnapshot(current);
          setRevisionBlocked(false);
          setRecoveryFailure(null);
        } catch { /* Preserve input, but do not replay an uncertain operation. */ }
      }
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }

  async function revisePlan(binding: ResultBinding, patch: SearchPlanPatch): Promise<boolean> {
    if (!hydrated || inFlight.current || latestSnapshot.current.state.kind !== "available") return false;
    inFlight.current = true;
    setBusy(true);
    setRevisionFailure(null);
    try {
      const outcome = await reviseSearchPlanWithReadback(appClient, binding, patch);
      if (!mounted.current) return false;
      if (outcome.kind === "updated") {
        adoptSnapshot(outcome.snapshot);
        setRevisionBlocked(false);
        return true;
      }
      setRevisionFailure(outcome.failure);
      setRevisionBlocked(outcome.current == null);
      if (outcome.current) {
        adoptSnapshot(outcome.current);
        if (outcome.current.state.kind !== "available") setRevisionFailure(null);
      }
      return false;
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(false);
    }
  }

  async function refreshSearchPlan() {
    if (!hydrated || inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    try {
      const current = await readCurrentResult(appClient, latestSnapshot.current.binding);
      if (!mounted.current) return;
      submittedPolls.current = 0;
      adoptSnapshot(current);
      setRevisionBlocked(false);
      setRevisionFailure(null);
      setRecoveryFailure(null);
    } catch (error) {
      if (mounted.current) { setRevisionFailure(clientFailure(error)); setRecoveryFailure(clientFailure(error)); setRevisionBlocked(true); }
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(false);
    }
  }

  if (failure) {
    const copy = resultClientFailureCopy(failure, snapshot.locale);
    return <ResultFailureShell title={copy.title} detail={copy.detail} code={copy.code} diagnostics={copy.diagnostics} />;
  }
  return (
    <ResultCardView
      key={`${snapshot.binding.result_ref}:${reviewRevision}`}
      snapshot={snapshot}
      busy={busy}
      syncing={!hydrated}
      onConfirm={() => void transition(() => confirmResult(appClient, snapshot.binding), true)}
      onSelect={(answers) => void transition(() => selectResult(appClient, snapshot.binding, answers), true)}
      onCancel={() => void transition(() => cancelResult(appClient, snapshot.binding))}
      onRequestEdit={(instruction) => void transition(() => requestAgentEdit(appClient, snapshot.binding, instruction))}
      onReviseSearchPlan={revisePlan}
      onConfirmCreatorSaveBatch={(selections) => void transition(() => confirmCreatorSaveBatch(appClient, snapshot.binding, selections), true)}
      onRefreshSearchPlan={() => void refreshSearchPlan()}
      revisionFailure={revisionFailure}
      revisionBlocked={revisionBlocked}
      recoveryFailure={recoveryFailure}
      dmFailure={dmFailure}
      onRecover={() => void refreshSearchPlan()}
      continuationIssue={continuationIssue}
    />
  );
}

function clientFailure(error: unknown): ResultClientFailure {
  return error instanceof ResultClientError ? error.failure : { kind: "transport" };
}
