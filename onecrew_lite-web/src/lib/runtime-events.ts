/** A single SSE connection per webpage, with real unsubscribe and reconnect
 * invalidation so MCP changes cannot leave React Query caches stale. */
import { connectLocalRuntime } from "./local-runtime";

export type UnlistenFn = () => void;
type Listener = (event: { payload: unknown }) => void;
const listeners = new Map<string, Set<Listener>>();
let source: EventSource | null = null;

function emit(name: string, payload: unknown) {
  for (const listener of listeners.get(name) ?? []) listener({ payload });
}

function start() {
  if (source || listeners.size === 0) return;
  source = new EventSource("/api/events", { withCredentials: true });
  // Reconnect cannot replay missed facts; zero is the forced-refetch sentinel.
  source.onopen = () => emit("runtime://changed", { sequence: 0, scopes: ["all"] });
  source.onmessage = ({ data }) => {
    try {
      const value: unknown = JSON.parse(data);
      if (typeof value !== "object" || value === null || !("event" in value) || typeof value.event !== "string" || !("payload" in value)) return;
      emit(value.event, value.payload);
    } catch { /* Advisory malformed events are ignored; no payload logging. */ }
  };
}

export async function listen<T>(event: string, callback: (event: { payload: T }) => void): Promise<UnlistenFn> {
  await connectLocalRuntime();
  const listener: Listener = ({ payload }) => callback({ payload: payload as T });
  const subscribers = listeners.get(event) ?? new Set<Listener>();
  subscribers.add(listener);
  listeners.set(event, subscribers);
  start();
  return () => {
    subscribers.delete(listener);
    if (subscribers.size === 0) listeners.delete(event);
    if (listeners.size === 0) { source?.close(); source = null; }
  };
}
