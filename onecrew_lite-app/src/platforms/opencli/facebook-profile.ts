import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const reserved = new Set(["about", "ads", "business", "challenge", "checkpoint", "directory", "events", "friends", "gaming", "groups", "help", "home", "login", "login.php", "marketplace", "messages", "notifications", "pages", "photo", "photos", "privacy", "profile", "profile.php", "recover", "reel", "reels", "search", "settings", "share", "stories", "two_step_verification", "watch"]);
export const facebookIdValid = (value: string) => /^[1-9]\d{0,19}$/u.test(value);
export function facebookNumericContactSource(id: string, source: string): boolean { return ["about_contact_and_basic_info", "directory_contact_info", "directory_links"].some(tab => source === `https://www.facebook.com/profile.php?id=${id}&sk=${tab}`); }
export function facebookSourceValid(source: string) {
  const match = /^https:\/\/www\.facebook\.com\/([A-Za-z\d.]{1,100})\/?$/u.exec(source);
  return match !== null && !/^[\d.]+$/u.test(match[1]!) && !reserved.has(match[1]!.toLowerCase());
}
export function facebookNativeContactBase(source: string) {
  const match = /^(.*)\/(?:directory_contact_info|directory_links)$/u.exec(source);
  return match && !match[1]!.endsWith("/") && facebookSourceValid(match[1]!) ? match[1]! : null;
}
const text = z.string().refine(value => value.isWellFormed());
const label = text.refine(value => !!value.trim() && [...value].length <= 200 && !/\p{Cc}/u.test(value));
function input(valid: boolean) { if (!valid) throw new OpenCliError("opencli_invalid_input"); }
function one<T extends z.ZodType>(schema: T, value: unknown): z.output<T> { const parsed = z.array(schema).length(1).safeParse(value); if (!parsed.success) throw new OpenCliError("opencli_invalid_output"); return parsed.data[0]!; }
function followers(value: string) { const match = /^(0|[1-9]\d*|[1-9]\d{0,2}(?:,\d{3})+) followers?$/u.exec(value.trim().toLowerCase()); if (!match) return null; const count = Number(match[1]!.replace(/,/gu, "")); return Number.isSafeInteger(count) ? count : null; }
export function facebookIdentityPlan(source: string) {
  input(facebookSourceValid(source));
  const schema = z.strictObject({ adapter_version: z.literal("1.0.0"), source_url: z.literal(source), id: text.refine(facebookIdValid), display_name: label, profile_url: text.refine(value => facebookSourceValid(value) && value.replace(/\/+$/u, "").toLowerCase() === source.replace(/\/+$/u, "").toLowerCase()), evidence_source: z.literal("native_profile_header_response"), skip_reason: z.literal("followers_unavailable").optional() });
  return { site: "onecrew-facebook-profile", name: "identity", args: ["-f", "json", "--", source], addons: ["onecrew-browser-search"], maximumBytes: 4000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function facebookProfilePlan(id: string, source?: string) {
  input(facebookIdValid(id) && (source === undefined || facebookSourceValid(source)));
  const base = source?.replace(/\/+$/u, "");
  const schema = z.strictObject({ adapter_version: z.literal(source === undefined ? "1.0.0" : "2.0.0"), id: z.literal(id), display_name: label, follower_count: z.number().int().nonnegative().safe().nullable().optional().transform(value => value ?? null), follower_text: text.refine(value => [...value].length <= 100 && !/\p{Cc}/u.test(value)), profile_url: z.literal(`https://www.facebook.com/profile.php?id=${id}`), observed_profile_url: text.nullable().optional(), contact_links: z.array(text).default([]), biography: text.max(2000).optional(), bio_truncated: z.boolean().optional(), bio_status: z.enum(["complete", "truncated", "missing"]).optional(), evidence_source: z.literal(source === undefined ? "visible_numeric_profile_header" : "native_profile_header_response") }).refine(row => row.follower_count === followers(row.follower_text) && (base === undefined ? row.observed_profile_url == null && row.contact_links.length <= 2 && row.contact_links.every(link => facebookNumericContactSource(id, link)) : row.observed_profile_url === base && row.contact_links.length <= 2 && new Set(row.contact_links).size === row.contact_links.length && row.contact_links.every(link => facebookNativeContactBase(link) === base)));
  return { site: "onecrew-facebook-profile", name: source === undefined ? "read" : "native-read", args: ["-f", "json", "--", id, ...(source === undefined ? [] : [source])], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode(value: unknown) { const { observed_profile_url, contact_links, ...row } = one(schema, value); return { ...row, ...(observed_profile_url == null ? {} : { observed_profile_url }), contact_links }; } } satisfies ReviewedOpenCliRead<unknown>;
}
function contactLink(value: string) { const parsed = URL.parse(value); return Buffer.byteLength(value) <= 2048 && !/\p{Cc}/u.test(value) && parsed !== null && ["http:", "https:"].includes(parsed.protocol) && !!parsed.hostname && !parsed.username && !parsed.password; }
export function facebookContactPlan(id: string, native?: { readonly base: string; readonly source: string }, numericSource?: string) {
  input(facebookIdValid(id) && (native === undefined || facebookNativeContactBase(native.source) === native.base) && (numericSource === undefined || facebookNumericContactSource(id, numericSource)));
  const source = native?.source ?? numericSource ?? `https://www.facebook.com/profile.php?id=${id}&sk=about_contact_and_basic_info`;
  const schema = z.strictObject({ adapter_version: z.literal(native === undefined ? "1.0.0" : "2.0.0"), id: z.literal(id), source_url: z.literal(source), observed_profile_url: text.nullable().optional(), title: label, contact_text: text.refine(value => [...value].length <= 12000 && !value.includes("\0")), contact_links: z.array(text.refine(contactLink)).max(10), truncated: z.boolean() }).refine(row => native === undefined ? row.observed_profile_url == null : row.observed_profile_url === native.base);
  return { site: "onecrew-facebook-profile", name: native === undefined ? "contact" : "native-contact", args: ["-f", "json", "--", id, ...(native === undefined ? numericSource === undefined ? [] : [numericSource] : [native.base, native.source])], addons: ["onecrew-browser-search"], maximumBytes: 80000, decode(value: unknown) {
    const row = one(schema, value); const raw = `${row.contact_links.map(link => `${link === `https://www.facebook.com/messages/t/${id}/` ? "Messenger" : "Website"}: ${link}`).join("\n")}\n${row.contact_text}`; let excerpt = ""; let bytes = 0;
    for (const character of raw) { const length = Buffer.byteLength(character); if (bytes + length > 20000) break; bytes += length; excerpt += character; }
    return { requested_url: source, document_url: source, title: row.title, markdown_excerpt: excerpt, truncated: row.truncated || bytes < Buffer.byteLength(raw) };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
