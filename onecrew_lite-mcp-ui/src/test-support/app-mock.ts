import type { App } from "@modelcontextprotocol/ext-apps/react";
import { vi } from "vitest";

import { SNAPSHOT } from "./fixtures";

type HostContext = NonNullable<ReturnType<App["getHostContext"]>>;

export function appMock(responses: unknown[] = [], initialHostContext: HostContext = { toolInfo: { tool: { name: "onecrew_result_present" } } } as HostContext) {
  const callServerTool = vi.fn(async (_request: { name: string; arguments?: Record<string, unknown> }) => responses.shift() ?? { content: [], structuredContent: { kind: "snapshot", snapshot: SNAPSHOT } });
  const updateModelContext = vi.fn(async () => ({}));
  const sendMessage = vi.fn(async () => ({}));
  const getHostCapabilities = vi.fn(() => ({ updateModelContext: { text: {}, structuredContent: {} }, message: { text: {} } }));
  let hostContext = { ...initialHostContext };
  let onHostContextChanged: ((context: HostContext) => void) | undefined;
  const getHostContext = vi.fn(() => hostContext);
  const requestDisplayMode = vi.fn(async ({ mode }: { mode: "inline" | "fullscreen" | "pip" }) => ({ mode }));
  const app = {
    callServerTool, updateModelContext, sendMessage, getHostCapabilities, getHostContext, requestDisplayMode,
    get onhostcontextchanged() { return onHostContextChanged; },
    set onhostcontextchanged(callback: ((context: HostContext) => void) | undefined) { onHostContextChanged = callback; },
  } as unknown as App;
  const notifyHostContext = (patch: HostContext) => {
    hostContext = { ...hostContext, ...patch };
    app.onhostcontextchanged?.(patch);
  };
  return {
    app,
    callServerTool,
    updateModelContext,
    sendMessage,
    getHostCapabilities,
    requestDisplayMode,
    getHostContext,
    notifyHostContext,
  };
}
