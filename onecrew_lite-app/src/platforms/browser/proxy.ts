import { isIP } from "node:net";
import { Server } from "proxy-chain";
import { createSession } from "wreq-js";
import { z } from "zod";
import { parseApplicationJson } from "../../contracts/application-json.ts";

const text = z.string().refine(value => value.isWellFormed() && !/\p{Cc}/u.test(value));
export const proxyConfigSchema = z.strictObject({ kind: z.enum(["http", "https", "socks5"]), host: text.min(1).max(253).refine(value => !/[\s/@?#\\]/u.test(value)), port: z.number().int().min(1).max(65535), username: text.nullish(), password: text.nullish() });
export type ProxyConfig = z.infer<typeof proxyConfigSchema>;
export function proxyUrl(value: ProxyConfig): string {
  const proxy = proxyConfigSchema.parse(value), host = isIP(proxy.host) === 6 && !proxy.host.startsWith("[") ? `[${proxy.host}]` : proxy.host;
  const url = new URL(`${proxy.kind}://${host}:${proxy.port}`);
  if (proxy.username) { url.username = proxy.username; url.password = proxy.password ?? ""; }
  return url.href;
}
export async function createBrowserProxy(value: ProxyConfig) {
  const upstream = proxyUrl(value), server = new Server({ host: "127.0.0.1", port: 0, verbose: false, prepareRequestFunction: () => ({ upstreamProxyUrl: upstream }) });
  await server.listen();
  const address = server.server.address();
  if (!address || typeof address === "string" || address.address !== "127.0.0.1") { await server.close(true); throw new Error("browser.proxy_loopback_required"); }
  let closing: Promise<void> | undefined;
  return Object.freeze({ host: "127.0.0.1", port: address.port, close(): Promise<void> { closing ??= server.close(true); return closing; }, toJSON(): never { throw new Error("browser.proxy_not_serializable"); } });
}
const endpoints = ["https://ipinfo.io/json", "http://ip-api.com/json", "https://api.ipify.org?format=json"] as const;
function language(country: string): string | null {
  const region = country.trim().toUpperCase(); if (!/^[A-Z]{2}$/u.test(region)) return null;
  const value = new Intl.Locale(`und-${region}`).maximize().language; if (value === "und") return null;
  return `${value}-${region},${value};q=0.9${value === "en" ? "" : ",en;q=0.8"}`;
}
export async function probeProxy(value: ProxyConfig, signal: AbortSignal) {
  const proxy = proxyConfigSchema.parse(value); signal.throwIfAborted();
  const session = await createSession({ browser: "chrome_149", os: process.platform === "win32" ? "windows" : "macos", proxy: proxyUrl(proxy) });
  const failures: string[] = [];
  try {
    for (const endpoint of endpoints) {
      const result = await (async () => {
        signal.throwIfAborted();
        const response = await session.fetch(endpoint, { method: "GET", redirect: "error", signal: AbortSignal.any([signal, AbortSignal.timeout(12_000)]) });
        if (!response.ok || !response.body) throw new Error("proxy.probe_http_rejected");
        const reader = response.body.getReader(), chunks: Uint8Array[] = []; let size = 0;
        try { for (;;) { const item = await reader.read(); if (item.done) break; size += item.value.length; if (size > 64 * 1024) throw new Error("proxy.probe_body_too_large"); chunks.push(item.value); } }
        finally { await reader.cancel(); reader.releaseLock(); }
        const parsed = z.record(z.string(), z.unknown()).parse(parseApplicationJson(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks))));
        const field = (...names: string[]) => names.map(name => parsed[name]).find(item => typeof item === "string" && item.length > 0) as string | undefined;
        const ip = field("ip", "query"); if (!ip || !isIP(ip)) throw new Error("proxy.probe_ip_invalid");
        const country = field("country", "country_iso", "countryCode") ?? "", timezone = field("timezone", "time_zone") ?? null;
        if (timezone) new Intl.DateTimeFormat("en", { timeZone: timezone }).format();
        return { ip, country, timezone, accept_language: language(country), endpoint: `${proxy.host}:${proxy.port}` };
      })().then(result => ({ result }), () => ({ failed: true as const }));
      signal.throwIfAborted();
      if ("result" in result) return result.result;
      failures.push(new URL(endpoint).hostname);
    }
    throw new Error(`proxy.probe_failed:${failures.join(",")}`);
  } finally { await session.close(); }
}
