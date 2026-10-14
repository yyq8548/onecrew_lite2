import { useEffect, useState } from "react";
import type { App } from "@modelcontextprotocol/ext-apps";
import { OneCrewHeader } from "@mcp-ui/components/ui/onecrew-brand";
import { Card, CardHeader, CardTitle, CardDescription } from "@mcp-ui/components/ui/card";
import { translate, type CopyKey } from "@mcp-ui/lib/i18n";
import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { readCurrentResult } from "@mcp-ui/lib/result-client/client";
import { ResultClientError } from "@mcp-ui/lib/result-client/errors";

/** A result checkpoint is not a live Campaign progress feed. */
export function statusKey(snapshot: ResultSnapshot): CopyKey {
  if (snapshot.state.kind === "failed") return "stageFailed";
  if (snapshot.state.kind === "unknown") return "stageUnknown";
  if (snapshot.state.kind === "submitted") return snapshot.batch.outcomes.some(item => item.operation_id === "onecrew.discovery.advance")
    ? "stageSearching" : "stageProcessing";
  if (snapshot.state.kind === "resolved") {
    if (snapshot.state.code === "user_cancelled") return "statusCancelled";
    if (snapshot.state.code === "edit_requested") return "stageWorkbench";
    const rows = snapshot.presentation.sections.flatMap(section => section.kind === "creator_save_rows" ? section.rows : []);
    if (rows.some(row => row.status === "unknown")) return "stageUnknown";
    if (rows.some(row => row.status === "failed" || row.status === "not_run")) return "stageFailed";
    return "stageCompleted";
  }
  if (snapshot.pending_action) return "stageWorkbench";
  if (snapshot.batch.outcomes.some(item => item.disposition.kind === "failed")) return "stageFailed";
  if (snapshot.batch.outcomes.some(item => item.disposition.kind === "unknown" || item.disposition.kind === "not_run")) return "stageUnknown";
  return "stageCompleted";
}

export function StatusCardView({ snapshot, stage }: { snapshot: ResultSnapshot; stage: CopyKey }) {
  return <Card className="result-shell gap-0 overflow-hidden py-0 shadow-none" role="status" aria-live="polite">
    <OneCrewHeader />
    <CardHeader className="px-4 py-4">
      <CardTitle>{translate(snapshot.locale, stage)}</CardTitle>
      <CardDescription>{translate(snapshot.locale, "stageWorkbenchDetail")}</CardDescription>
    </CardHeader>
  </Card>;
}

/** Read only: no submission, choice, continuation, or authority mutation. */
export function StatusCard({ appClient, initial }: { appClient: App; initial: ResultSnapshot }) {
  const [snapshot, setSnapshot] = useState(initial);
  const [stage, setStage] = useState<CopyKey>("stageConnecting");
  useEffect(() => {
    let stopped = false;
    let inFlight = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let binding = initial.binding;
    const refresh = async () => {
      if (stopped || inFlight || document.visibilityState === "hidden") return;
      inFlight = true;
      try {
        const current = await readCurrentResult(appClient, binding);
        if (stopped) return;
        binding = current.binding;
        setSnapshot(current);
        setStage(statusKey(current));
        if (current.state.kind === "available" || current.state.kind === "submitted") {
          timer = setTimeout(() => void refresh(), 10000);
        }
      } catch (error) {
        if (!stopped) setStage(error instanceof ResultClientError && error.failure.kind === "server"
          && ["expired", "absent"].includes(error.failure.failure.code) ? "stageExpired" : "stageDisconnected");
      } finally { inFlight = false; }
    };
    const onVisible = () => { if (timer) clearTimeout(timer); void refresh(); };
    void refresh();
    window.addEventListener("focus", onVisible);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
      window.removeEventListener("focus", onVisible);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [appClient, initial]);
  return <StatusCardView snapshot={snapshot} stage={stage} />;
}
