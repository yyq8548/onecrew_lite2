/** Authenticated, same-origin browser transport. Readiness comes from a real
 * service handshake, never from a synthetic native-framework marker. */
import { parseApplicationJson, stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { bootstrapCommands } from "@onecrew/runtime-contracts/bootstrap";
import { withBrowserSessionLock } from "@onecrew/runtime-contracts/browser-session-lock";

type Session = { csrf: string; version: string; transport: "local_http" } & (
  { protocol_version?: 1; scope?: "bootstrap" } | { protocol_version: 2; accountId: string; epoch: string }
);
type Envelope<T> = { ok: true; result: T } | { ok: false; error: unknown };

let session: Session | null = null;
let connecting: Promise<Session> | null = null;
let upgrading: Promise<Session> | null = null;
export type RuntimeConnectionState = "connecting" | "connected" | "unreachable" | "session_required" | "account_changed";
let connectionState: RuntimeConnectionState = "connecting";
let generation = 0;
const bootstrapPaths = new Set(bootstrapCommands.map(name => `/api/commands/${name}`));
const availabilityListeners = new Set<() => void>();

export function isRuntimeAvailable(): boolean { return session !== null; }
export function runtimeConnectionState(): RuntimeConnectionState { return connectionState; }
export function hasUnifiedAccountSession(): boolean { return session?.protocol_version === 2; }
export function unifiedAccountSessionKey(): string | null { return session?.protocol_version === 2 ? JSON.stringify([session.accountId, session.epoch]) : null; }
export function subscribeRuntimeAvailability(listener: () => void): () => void {
  availabilityListeners.add(listener);
  return () => { availabilityListeners.delete(listener); };
}

function changed() { for (const listener of availabilityListeners) listener(); }

function invalidateSession(reason: RuntimeConnectionState = "session_required", expected?: Session) {
  if (expected && session !== expected) return;
  generation++;
  session = null;
  connecting = null;
  upgrading = null;
  connectionState = reason;
  changed();
}

function unreachable(expected?: Session) {
  if (expected && session !== expected) return;
  connectionState = "unreachable";
  changed();
}

async function readHandshake(): Promise<Session> {
  const response = await fetch("/api/session", { credentials: "same-origin", cache: "no-store", signal: AbortSignal.timeout(10_000) });
  if (response.status === 401) throw new Error("web_session_required");
  if (response.status === 409) throw new Error("access.account_changed");
  if (!response.ok) throw new Error("local_runtime_handshake_failed");
  const value: unknown = parseApplicationJson(new Uint8Array(await response.arrayBuffer()));
  if (typeof value !== "object" || value === null || !("ok" in value) || value.ok !== true || !("result" in value)) throw new Error("invalid_local_runtime_handshake");
  const result = value.result as Partial<Session>;
  if (typeof result.csrf !== "string" || typeof result.version !== "string" || result.transport !== "local_http") throw new Error("invalid_local_runtime_handshake");
  if (result.protocol_version !== undefined && result.protocol_version !== 1 && result.protocol_version !== 2) throw new Error("unsupported_local_runtime_protocol");
  if ("scope" in result && (result.scope !== "bootstrap" || result.protocol_version !== 1)) throw new Error("invalid_local_runtime_handshake");
  if (result.protocol_version === 2 && (typeof result.accountId !== "string" || !result.accountId || typeof result.epoch !== "string" || !result.epoch)) throw new Error("invalid_local_runtime_handshake");
  return result as Session;
}

export async function connectLocalRuntime(refresh = false): Promise<Session> {
  if (session && connectionState === "connected" && !refresh) return session;
  if (!connecting) {
    const revision = generation;
    const flight = withBrowserSessionLock(async () => {
      if (typeof window !== "undefined") {
        const url = new URL(window.location.href);
        const fragment = new URLSearchParams(url.hash.slice(1));
        const tickets = fragment.getAll("onecrew_ticket");
        if (tickets.length > 1) throw new Error("invalid_workspace_ticket");
        const ticket = tickets[0] ?? null;
        if (ticket !== null) {
          fragment.delete("onecrew_ticket");
          url.hash = fragment.toString();
          window.history.replaceState(window.history.state, "", url);
          const exchanged = await fetch("/api/local/session", {
            method: "POST", credentials: "same-origin", cache: "no-store",
            signal: AbortSignal.timeout(10_000),
            headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ticket }),
          });
          if (!exchanged.ok) throw new Error("web_session_required");
        }
      }
      const next = await readHandshake();
      if (revision !== generation) throw new Error("access.account_changed");
      if (session?.protocol_version === 2 && (next.protocol_version !== 2 || next.accountId !== session.accountId || next.epoch !== session.epoch)) {
        invalidateSession("account_changed");
        throw new Error("access.account_changed");
      }
      session = next;
      connectionState = "connected";
      changed();
      return session;
    }).catch((error: unknown) => {
      if (revision === generation) {
        if (error instanceof Error && error.message === "web_session_required") invalidateSession();
        else if (error instanceof Error && error.message === "access.account_changed") invalidateSession("account_changed");
        else unreachable();
      }
      throw error;
    });
    connecting = flight;
    void flight.then(() => { if (connecting === flight) connecting = null; }, () => { if (connecting === flight) connecting = null; });
  }
  return connecting;
}

export async function runtimeRequest<T>(path: string, body: BodyInit, headers: Record<string, string> = {}): Promise<T> {
  let current = await connectLocalRuntime();
  if (current.protocol_version === 1 && current.scope === "bootstrap" && !bootstrapPaths.has(path)) {
    if (!upgrading) {
      const flight = connectLocalRuntime(true);
      upgrading = flight;
      void flight.then(() => { if (upgrading === flight) upgrading = null; }, () => { if (upgrading === flight) upgrading = null; });
    }
    current = await upgrading;
    if (current.protocol_version !== 2) throw new Error("access.login_required");
  }
  let response: Response;
  try {
    response = await fetch(path, {
      method: "POST", credentials: "same-origin", cache: "no-store", signal: AbortSignal.timeout(path === "/api/commands/onecrew_discovery_draft_prepare" ? 600_000 : 120_000),
      headers: { ...headers, "X-OneCrew-CSRF": current.csrf, ...(current.protocol_version === 2 ? { "X-OneCrew-Account-Id": current.accountId, "X-OneCrew-Auth-Epoch": current.epoch } : {}) }, body,
    });
  } catch (error) {
    // 单个请求超过期限不代表主服务断线；连接状态由握手确认。
    if (!(error instanceof DOMException && error.name === "TimeoutError")) unreachable(current);
    throw error;
  }
  const envelope = parseApplicationJson(new Uint8Array(await response.arrayBuffer())) as Envelope<T>;
  const error = !envelope.ok ? envelope.error : undefined;
  const code = typeof error === "string" ? error : typeof error === "object" && error !== null && "code" in error ? error.code : undefined;
  if (response.status === 401 && (code === "local.invalid_session" || code === "local.unauthorized" || code === undefined)) {
    invalidateSession("session_required", current);
    throw new Error("web_session_required");
  }
  if (response.status === 409 && code === "access.account_changed") { invalidateSession("account_changed", current); throw new Error("access.account_changed"); }
  if (!envelope.ok) throw envelope.error;
  if (!response.ok) throw new Error("local_runtime_request_failed");
  return envelope.result;
}

export async function invoke<T>(command: string, arguments_: Record<string, unknown> = {}): Promise<T> {
  if (!/^[a-z][a-z0-9_]*$/.test(command)) throw new Error("invalid_command_name");
  return runtimeRequest<T>(`/api/commands/${command}`, stringifyApplicationJson(arguments_), { "Content-Type": "application/json" });
}
