import { createSession } from "wreq-js";
import { isInteger, isSafeNumber, parse } from "lossless-json";

export const X_BROWSER_PROFILE = "chrome_149" as const;
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
export interface TrustedNativeReadIdentity {
  readonly headers: Readonly<Record<string, string>>;
  readonly browser: "chrome_149" | "okhttp_5";
  readonly allowPath: (url: URL) => boolean;
  readonly observeResponseHeaders?: (headers: Readonly<Pick<Headers, "get" | "getSetCookie">>) => void;
  readonly classifyHttpFailure?: (status: number, body: string, url: URL) => Error | Promise<Error>;
}
export class NativeReadTransportError extends Error {
  readonly status: number | null;
  constructor(code: "request_failed" | "http_rejected" | "body_unavailable" | "response_too_large" | "response_invalid", status: number | null = null) { super(`native_read_${code}`); this.status = status; }
}

export async function createReadSession(allowedOrigins: readonly string[], proxy?: string, identity?: TrustedNativeReadIdentity) {
  if (allowedOrigins.length === 0) throw new Error("Native transport requires explicit allowed origins");
  const origins = new Set(allowedOrigins.map((origin) => {
    const url = new URL(origin);
    if (url.protocol !== "https:" || url.username || url.password || url.href !== `${url.origin}/`) {
      throw new Error("Native transport origin must be an HTTPS origin without credentials");
    }
    return url.origin;
  }));
  if (process.platform !== "win32" && process.platform !== "darwin") {
    throw new Error("Native transport target is not supported by this migration");
  }
  if (proxy !== undefined) {
    const parsed = new URL(proxy);
    if (!["http:", "https:", "socks5:", "socks5h:"].includes(parsed.protocol)) {
      throw new Error("Unsupported native transport proxy protocol");
    }
  }
  const headers = identity === undefined ? undefined : Object.freeze({ ...identity.headers });
  const allowPath = identity?.allowPath;
  if (headers && Object.entries(headers).some(([name, value]) => !/^[!#$%&'*+.^_`|~A-Za-z\d-]+$/u.test(name) || ["host", "connection", "content-length", "transfer-encoding"].includes(name.toLowerCase()) || !value.isWellFormed() || /\p{Cc}/u.test(value))) throw new Error("Native transport session headers are invalid");
  const session = await createSession({
    browser: identity?.browser ?? X_BROWSER_PROFILE,
    os: process.platform === "win32" ? "windows" : "macos",
    ...(proxy === undefined ? {} : { proxy }),
  });
  let closed = false;
  const getText = async (url: URL, signal: AbortSignal): Promise<string> => {
      if (closed) throw new Error("Native transport session is closed");
      if (!origins.has(url.origin) || url.username || url.password) throw new Error("Native transport origin is not allowed");
      if (allowPath && !allowPath(url)) throw new Error("Native transport path is not allowed");
      signal.throwIfAborted();
      const response = await session.fetch(url.href, {
        method: "GET", redirect: "error", signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)]),
        ...(headers === undefined ? {} : { headers }),
      }).then(value => value, () => { signal.throwIfAborted(); throw new NativeReadTransportError("request_failed"); });
      identity?.observeResponseHeaders?.(response.headers);
      if (!response.ok && !identity?.classifyHttpFailure) throw new NativeReadTransportError("http_rejected", response.status);
      if (!response.body) throw new NativeReadTransportError("body_unavailable");
      const reader = response.body.getReader();
      const chunks: Uint8Array[] = [];
      let bytes = 0;
      try {
        for (;;) {
          const next = await reader.read();
          if (next.done) break;
          bytes += next.value.byteLength;
          if (bytes > MAX_RESPONSE_BYTES) throw new NativeReadTransportError("response_too_large");
          chunks.push(next.value);
        }
      } finally {
        await reader.cancel();
        reader.releaseLock();
      }
      const body = await Promise.resolve().then(() => new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks))).then(value => value, () => { throw new NativeReadTransportError("response_invalid"); });
      if (!response.ok) throw await identity!.classifyHttpFailure!(response.status, body, url);
      return body;
  };
  return {
    getText,
    async getJson(url: URL, signal: AbortSignal): Promise<unknown> {
      const body = await getText(url, signal);
      return Promise.resolve().then(() => parse(body, (key, value) => {
        if (!key.isWellFormed() || typeof value === "string" && !value.isWellFormed()) throw new Error("Native transport JSON contains invalid Unicode");
        return value;
      }, { parseNumber: raw => {
        if (isInteger(raw)) return isSafeNumber(raw) ? Number(raw) : BigInt(raw);
        const value = Number(raw); if (!Number.isFinite(value)) throw new Error("Native transport JSON number is invalid"); return value;
      } })).then(value => value, () => { throw new NativeReadTransportError("response_invalid"); });
    },
    async close(): Promise<void> {
      if (closed) return;
      closed = true;
      await session.close();
    },
  };
}
