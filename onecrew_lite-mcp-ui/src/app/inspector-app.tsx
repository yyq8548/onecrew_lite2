import { useCallback, useEffect, useRef, useState } from "react";
import { useApp } from "@modelcontextprotocol/ext-apps/react";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";

import { LoadingShell } from "@mcp-ui/features/result-card";
import { StatusCard } from "@mcp-ui/features/status-card";
import { parseWorkspaceStatus, WorkspaceStatusCard, type WorkspaceStatus } from "@mcp-ui/features/workspace-status";
import { ResultFailureShell } from "@mcp-ui/features/result-card/components/shell";
import { parseInitialResult, type ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { applyHostContext } from "./host-theme";
import { contextForDisplayMode, HostFrameProvider, mergeHostContext, readHostFrame, UNSTATED_HOST_FRAME, type HostContext, type HostDimensionsByMode, type HostFrame } from "./host-frame";
import { translate, type CopyKey } from "@mcp-ui/lib/i18n";

type BootstrapState =
  | { kind: "workspace"; snapshot: WorkspaceStatus }
  | { kind: "waiting" }
  | { kind: "ready"; snapshot: ResultSnapshot }
  | { kind: "missing" }
  | { kind: "invalid" };

/** Remember the complete merged dimensions, not the raw partial patch. */
export function rememberHostDimensions(
  previous: HostContext | undefined,
  patch: HostContext | undefined,
  dimensionsByMode: HostDimensionsByMode,
): HostDimensionsByMode {
  if (!patch?.containerDimensions) return dimensionsByMode;
  const merged = mergeHostContext(previous, patch);
  const mode = merged?.displayMode ?? "inline";
  const dimensions = merged?.containerDimensions;
  return dimensions ? { ...dimensionsByMode, [mode]: { ...dimensions } } : dimensionsByMode;
}

export function InspectorApp() {
  const [bootstrap, setBootstrap] = useState<BootstrapState>({ kind: "waiting" });
  // The Host states the frame it gives this App — its size, its display mode,
  // its platform. Keeping it in state means a Host that changes any of them
  // mid-session re-renders the card against the new frame.
  const [frame, setFrame] = useState<HostFrame>(UNSTATED_HOST_FRAME);
  const hostContext = useRef<HostContext>(undefined);
  // A mode-only notification carries no dimensions. Keep the last dimensions
  // stated for each mode so fullscreen cannot inherit inline's stale budget,
  // and an inline return can recover its own budget without inventing one.
  const dimensionsByMode = useRef<HostDimensionsByMode>({});
  const t = (key: CopyKey) => translate(bootstrap.kind === "ready" ? bootstrap.snapshot.locale : "en-US", key);
  const adoptHostContext = useCallback((patch: HostContext | undefined) => {
    const previous = hostContext.current;
    const merged = mergeHostContext(previous, patch);
    const mode = merged?.displayMode ?? "inline";
    const remembered = rememberHostDimensions(previous, patch, dimensionsByMode.current);
    dimensionsByMode.current = remembered;
    const current = contextForDisplayMode(merged, mode, remembered);
    hostContext.current = current;
    applyHostContext(current);
    setFrame(readHostFrame(current));
  }, []);
  const { app, isConnected, error } = useApp({
    appInfo: {
      name: "onecrew-result-card",
      title: "onecrew_lite",
      version: __MCP_UI_VERSION__,
    },
    // Declaring fullscreen is what makes the escape hatch exist: a Host may
    // only move this App to a mode it said it supports. Inline stays the
    // default — fullscreen is asked for, never assumed.
    capabilities: { availableDisplayModes: ["inline", "fullscreen"] },
    autoResize: true,
    strict: true,
    onAppCreated: (created) => {
      created.ontoolresult = (payload: CallToolResult) => {
        if (payload._meta?.["onecrew/workspace"] !== undefined) {
          const workspace = parseWorkspaceStatus(payload);
          setBootstrap(workspace ? { kind: "workspace", snapshot: workspace } : { kind: "invalid" });
          return;
        }
        const parsed = parseInitialResult(payload);
        setBootstrap(parsed.status === "valid"
          ? { kind: "ready", snapshot: parsed.snapshot }
          : { kind: parsed.status });
      };
      created.onhostcontextchanged = (context) => {
        // The SDK notification is a partial context update. Replacing the
        // previous value here would erase the Host's stated height whenever a
        // later theme or locale notification omitted dimensions.
        adoptHostContext(context);
      };
      created.onteardown = async () => ({});
    },
  });

  useEffect(() => {
    if (!app || !isConnected) return;
    const context = app.getHostContext();
    adoptHostContext(context);
  }, [app, isConnected, adoptHostContext]);

  // The Host has final say over its own UI and answers with the mode it
  // actually set, which may not be the one requested. That answer is the only
  // thing recorded here; an optimistic local "now fullscreen" would render a
  // layout the Host never granted.
  const requestDisplayMode = useCallback((mode: "inline" | "fullscreen" | "pip") => {
    if (!app) return;
    void app.requestDisplayMode({ mode })
      .then((result) => {
        // Keep the accepted mode in the merge base. Otherwise a later partial
        // theme/locale notification would merge into the old inline context
        // and undo the Host's actual display-mode response.
        const merged = contextForDisplayMode(hostContext.current, result.mode, dimensionsByMode.current);
        hostContext.current = merged;
        setFrame(current => merged ? readHostFrame(merged) : { ...current, displayMode: result.mode });
      })
      .catch(() => { /* A refused request leaves the card exactly as it is. */ });
  }, [app]);

  if (error) {
    return (
      <ResultFailureShell
        title={t("bootFailedTitle")}
        detail={t("bootFailedDetail")}
        code="app_connection_failed"
      />
    );
  }
  if (!app || !isConnected || bootstrap.kind === "waiting") {
    return <LoadingShell label={t("loading")} />;
  }
  if (bootstrap.kind === "missing") {
    return (
      <ResultFailureShell
        title={t("missingTitle")}
        detail={t("missingDetail")}
        code="result_snapshot_missing"
      />
    );
  }
  if (bootstrap.kind === "invalid") {
    return (
      <ResultFailureShell
        title={t("snapshotInvalidTitle")}
        detail={t("snapshotInvalidDetail")}
        code="result_snapshot_invalid"
      />
    );
  }
  return <HostFrameProvider frame={frame} onRequestDisplayMode={requestDisplayMode}>
    {bootstrap.kind === "workspace"
      ? <WorkspaceStatusCard key={bootstrap.snapshot.viewId} appClient={app} initial={bootstrap.snapshot} />
      : <StatusCard key={bootstrap.snapshot.binding.result_ref} appClient={app} initial={bootstrap.snapshot} />}
  </HostFrameProvider>;
}
