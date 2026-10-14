/** Authenticated, same-origin browser transport. Readiness comes from a real
 * service handshake, never from a synthetic native-framework marker. */
type Session = { csrf: string; version: string; transport: "local_http" };
type Envelope<T> = { ok: true; result: T } | { ok: false; error: unknown };

let session: Session | null = null;
let connecting: Promise<Session> | null = null;
const availabilityListeners = new Set<() => void>();

export function isRuntimeAvailable(): boolean { return session !== null; }
export function subscribeRuntimeAvailability(listener: () => void): () => void {
  availabilityListeners.add(listener);
  return () => { availabilityListeners.delete(listener); };
}

function changed() { for (const listener of availabilityListeners) listener(); }

function invalidateSession() {
  session = null;
  connecting = null;
  changed();
}

export async function connectLocalRuntime(): Promise<Session> {
  if (session) return session;
  if (!connecting) {
    connecting = (async () => {
      const response = await fetch("/api/session", { credentials: "same-origin", cache: "no-store" });
      const value: unknown = await response.json();
      if (!response.ok || typeof value !== "object" || value === null || !("ok" in value) || value.ok !== true || !("result" in value)) {
        throw new Error("web_session_required");
      }
      const result = value.result as Partial<Session>;
      if (typeof result.csrf !== "string" || typeof result.version !== "string" || result.transport !== "local_http") {
        throw new Error("invalid_local_runtime_handshake");
      }
      session = result as Session;
      changed();
      return session;
    })().catch((error) => { invalidateSession(); throw error; });
  }
  return connecting;
}

export async function runtimeRequest<T>(path: string, body: BodyInit, headers: Record<string, string> = {}): Promise<T> {
  const current = await connectLocalRuntime();
  let response: Response;
  try {
    response = await fetch(path, {
      method: "POST", credentials: "same-origin", cache: "no-store",
      headers: { ...headers, "X-OneCrew-CSRF": current.csrf }, body,
    });
  } catch (error) { invalidateSession(); throw error; }
  if (response.status === 401 || response.status === 403) {
    invalidateSession();
    throw new Error("web_session_required");
  }
  if (!response.ok) throw new Error("local_runtime_request_failed");
  const envelope = await response.json() as Envelope<T>;
  if (!envelope.ok) throw envelope.error;
  return envelope.result;
}

export async function invoke<T>(command: string, arguments_: Record<string, unknown> = {}): Promise<T> {
  if (!/^[a-z][a-z0-9_]*$/.test(command)) throw new Error("invalid_command_name");
  return runtimeRequest<T>(`/api/commands/${command}`, JSON.stringify(arguments_), { "Content-Type": "application/json" });
}
