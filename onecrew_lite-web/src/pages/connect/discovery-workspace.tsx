import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { createWorkspacePresence, type WorkspaceStage } from "@/lib/discovery-workspace";
import type { DiscoveryRunCheckpoint } from "@/lib/discovery-run-checkpoint";

/** Publish selection and runner stage; MCP reads counts from the shared owner. */
const NO_EXCLUSIONS: string[] = [];
export function DiscoveryWorkspacePresence({ campaignId, stage, stop, lastStop, excludedPlatforms = NO_EXCLUSIONS }: { campaignId: string | null; stage: WorkspaceStage; stop?: DiscoveryRunCheckpoint | null; lastStop?: () => DiscoveryRunCheckpoint | null; excludedPlatforms?: readonly string[] }) {
  const { t } = useTranslation("shell");
  const publisher = useRef<ReturnType<typeof createWorkspacePresence> | null>(null);
  const publishedCampaign = useRef<string | null | undefined>(undefined);
  const [failed, setFailed] = useState(false);
  const lastStopRef = useRef(lastStop);
  lastStopRef.current = lastStop;
  const excludedRef = useRef(excludedPlatforms);
  excludedRef.current = excludedPlatforms;
  useEffect(() => {
    publisher.current ??= createWorkspacePresence();
    let closed = false;
    const send = async (activate: boolean) => {
      try { await publisher.current!(campaignId, stage, activate, stop, excludedPlatforms); if (!closed) setFailed(false); }
      catch { if (!closed) setFailed(true); }
    };
    const changed = publishedCampaign.current !== campaignId;
    publishedCampaign.current = campaignId;
    void send(changed && document.visibilityState !== "hidden");
    const focus = () => { if (document.visibilityState !== "hidden") void send(true); };
    // A running hidden tab still owns its execution. This heartbeat only
    // refreshes a display lease and never reads a provider or continues a run.
    const timer = window.setInterval(() => void send(false), 15_000);
    window.addEventListener("focus", focus);
    document.addEventListener("visibilitychange", focus);
    return () => { closed = true; window.clearInterval(timer); window.removeEventListener("focus", focus); document.removeEventListener("visibilitychange", focus); };
  }, [campaignId, stage, stop, excludedPlatforms]);
  // Browser cleanup relinquishes only the display lease. The host owns search execution.
  return failed ? <p role="status" className="text-sm text-muted-foreground">{t("opencliPilot.cardSyncFailed")}</p> : null;
}
