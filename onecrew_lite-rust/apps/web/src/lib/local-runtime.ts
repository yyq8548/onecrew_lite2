/** Authenticated, same-origin browser transport. Readiness comes from a real
 * service handshake, never from a synthetic native-framework marker. */
import { parseApplicationJson, stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { bootstrapCommands } from "@onecrew/runtime-contracts/bootstrap";

type Session = { csrf: string; version: string; transport: "local_http" } & (
  { protocol_version?: 1; scope?: "bootstrap" } | { protocol_version: 2; accountId: string; epoch: string }
);
type Envelope<T> = { ok: true; result: T } | { ok: false; error: unknown };

let session: Session | null = null;
let connecting: Promise<Session> | null = null;
let upgrading: Promise<Session> | null = null;
const bootstrapPaths = new Set(bootstrapCommands.map(name => `/api/commands/${name}`));
const availabilityListeners = new Set<() => void>();

export function isRuntimeAvailable(): boolean { return session !== null; }
export function hasUnifiedAccountSession(): boolean { return session?.protocol_version === 2; }
export function unifiedAccountSessionKey(): string | null { return session?.protocol_version === 2 ? JSON.stringify([session.accountId, session.epoch]) : null; }
export function subscribeRuntimeAvailability(listener: () => void): () => void {
  availabilityListeners.add(listener);
  return () => { availabilityListeners.delete(listener); };
}

function changed() { for (const listener of availabilityListeners) listener(); }

function invalidateSession() {
  session = null;
  connecting = null;
  upgrading = null;
  changed();
}

async function readHandshake(): Promise<Session> {
  const response = await fetch("/api/session", { credentials: "same-origin", cache: "no-store" });
  const value: unknown = parseApplicationJson(new Uint8Array(await response.arrayBuffer()));
  if (!response.ok || typeof value !== "object" || value === null || !("ok" in value) || value.ok !== true || !("result" in value)) throw new Error("web_session_required");
  const result = value.result as Partial<Session>;
  if (typeof result.csrf !== "string" || typeof result.version !== "string" || result.transport !== "local_http") throw new Error("invalid_local_runtime_handshake");
  if (result.protocol_version !== undefined && result.protocol_version !== 1 && result.protocol_version !== 2) throw new Error("unsupported_local_runtime_protocol");
  if ("scope" in result && (result.scope !== "bootstrap" || result.protocol_version !== 1)) throw new Error("invalid_local_runtime_handshake");
  if (result.protocol_version === 2 && (typeof result.accountId !== "string" || !result.accountId || typeof result.epoch !== "string" || !result.epoch)) throw new Error("invalid_local_runtime_handshake");
  return result as Session;
}

export async function connectLocalRuntime(): Promise<Session> {
  if (session) return session;
  if (!connecting) {
    connecting = (async () => {
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
            headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ticket }),
          });
          if (!exchanged.ok) throw new Error("web_session_required");
        }
      }
      session = await readHandshake();
      changed();
      return session;
    })().catch((error) => { invalidateSession(); throw error; });
  }
  return connecting;
}

export async function runtimeRequest<T>(path: string, body: BodyInit, headers: Record<string, string> = {}): Promise<T> {
  let current = await connectLocalRuntime();
  if (current.protocol_version === 1 && current.scope === "bootstrap" && !bootstrapPaths.has(path)) {
    if (!upgrading) {
      const previous = current;
      const flight = readHandshake().then(next => {
        if (session !== previous) throw new Error("access.account_changed");
        session = next;
        changed();
        return next;
      });
      upgrading = flight;
      void flight.then(() => { if (upgrading === flight) upgrading = null; }, () => { if (upgrading === flight) upgrading = null; });
    }
    current = await upgrading;
    if (current.protocol_version !== 2) throw new Error("access.login_required");
  }
  let response: Response;
  try {
    response = await fetch(path, {
      method: "POST", credentials: "same-origin", cache: "no-store",
      headers: { ...headers, "X-OneCrew-CSRF": current.csrf, ...(current.protocol_version === 2 ? { "X-OneCrew-Account-Id": current.accountId, "X-OneCrew-Auth-Epoch": current.epoch } : {}) }, body,
    });
  } catch (error) { invalidateSession(); throw error; }
  if (response.status === 401 || response.status === 403) {
    invalidateSession();
    throw new Error("web_session_required");
  }
  if (response.status === 409) { invalidateSession(); throw new Error("access.account_changed"); }
  if (!response.ok) throw new Error("local_runtime_request_failed");
  const envelope = parseApplicationJson(new Uint8Array(await response.arrayBuffer())) as Envelope<T>;
  if (!envelope.ok) throw envelope.error;
  return envelope.result;
}

export async function invoke<T>(command: string, arguments_: Record<string, unknown> = {}): Promise<T> {
  if (!/^[a-z][a-z0-9_]*$/.test(command)) throw new Error("invalid_command_name");
  return runtimeRequest<T>(`/api/commands/${command}`, stringifyApplicationJson(arguments_), { "Content-Type": "application/json" });
}
