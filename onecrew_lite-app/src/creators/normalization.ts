import { asciiLower, channel, trim } from "./contracts.ts";
import type { z } from "zod";

export interface ContactLocatorInput { channel: z.infer<typeof channel>; kind: "email_address" | "handle" | "url" | "invite_link"; normalized_value: string; display_value: string }
const looksLikeUrl = (value: string): boolean => value.startsWith("http://") || value.startsWith("https://");
type Result<T> = { ok: true; value: T } | { ok: false; message: string };
function normalizeUrl(value: string): Result<string> {
  const url = URL.parse(looksLikeUrl(value) ? value : `https://${value}`);
  if (!url || !["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password) return { ok: false, message: `unsupported URL contact locator: ${value}` };
  url.hash = "";
  const retained = [...url.searchParams].filter(([key]) => !asciiLower(key).startsWith("utm_") && !["fbclid", "gclid"].includes(asciiLower(key)));
  url.search = "";
  for (const [key, value] of retained) url.searchParams.append(key, value);
  return { ok: true, value: url.toString() };
}
function handle(value: string): Result<string> {
  const url = looksLikeUrl(value) ? normalizeUrl(value) : null;
  if (url && !url.ok) return url;
  const candidate = url?.ok ? new URL(url.value).pathname.split("/").find(Boolean) ?? "" : value.replace(/^@+/u, "").replace(/^(?:t\.me\/)+/u, "").split(/[/?#]/u)[0]!;
  const normalized = trim(candidate.replace(/^@+/u, ""));
  if (!/^[A-Za-z0-9_.]+$/u.test(normalized)) return { ok: false, message: `invalid social handle contact locator: ${value}` };
  return { ok: true, value: normalized };
}
export function normalizeContact(channelValue: z.infer<typeof channel>, raw: string): ContactLocatorInput {
  const result = normalizeContactResult(channelValue, raw);
  if (!result.ok) throw new Error(result.message);
  return result.value;
}
export function normalizeContactResult(channelValue: z.infer<typeof channel>, raw: string): Result<ContactLocatorInput> {
  const value = trim(raw);
  if (channelValue === "email") {
    const normalized = asciiLower(trim(value.replace(/^(?:mailto:)+/u, "").split(/[?#]/u)[0]!));
    const parts = normalized.split("@");
    if (parts.length !== 2 || !parts[0] || !parts[1] || !parts[1].includes(".")) return { ok: false, message: `invalid email contact locator: ${value}` };
    return { ok: true, value: { channel: channelValue, kind: "email_address", normalized_value: normalized, display_value: normalized } };
  }
  if (channelValue === "wechat") {
    if (!value || Buffer.byteLength(value) > 128 || /[\p{White_Space}\p{Cc}]/u.test(value)) return { ok: false, message: "invalid WeChat contact locator" };
    return { ok: true, value: { channel: channelValue, kind: "handle", normalized_value: asciiLower(value), display_value: value } };
  }
  if (channelValue === "telegram" && looksLikeUrl(value)) {
    const normalized = normalizeUrl(value);
    if (!normalized.ok) return normalized;
    const url = new URL(normalized.value);
    if (["t.me", "telegram.me", "telegram.dog"].includes(asciiLower(url.hostname.replace(/^(?:www\.)+/u, "")))) {
      const segments = url.pathname.split("/").filter(Boolean);
      const first = segments[0];
      if (segments.length !== 1 || first?.startsWith("+") || ["joinchat", "c", "s", "share", "proxy"].includes(first!)) return { ok: true, value: { channel: channelValue, kind: first?.startsWith("+") || first === "joinchat" ? "invite_link" : "url", normalized_value: normalized.value, display_value: normalized.value } };
      const display = handle(first!);
      if (!display.ok) return display;
      return { ok: true, value: { channel: channelValue, kind: "handle", normalized_value: asciiLower(display.value), display_value: display.value } };
    }
  }
  if (["x", "instagram", "tiktok", "telegram"].includes(channelValue)) {
    const display = handle(value);
    if (!display.ok) return display;
    return { ok: true, value: { channel: channelValue, kind: "handle", normalized_value: asciiLower(display.value), display_value: display.value } };
  }
  const normalized = normalizeUrl(value);
  if (!normalized.ok) return normalized;
  return { ok: true, value: { channel: channelValue, kind: "url", normalized_value: normalized.value, display_value: normalized.value } };
}
