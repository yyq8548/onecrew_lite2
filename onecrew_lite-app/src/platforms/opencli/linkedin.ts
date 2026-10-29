import { z } from "zod";
import { nativeContentObservations } from "./observations.ts";
import { OpenCliError } from "./errors.ts";
import { publicWebUrl } from "./public-url.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const text = z.string().refine(value => value.isWellFormed());
const bounded = (maximum: number) => text.refine(value => Buffer.byteLength(value) <= maximum && !value.includes("\0"));
export function normalizeLinkedinHandle(value: string): string {
  const decoded = decodeURIComponent(value);
  if (!/^[\p{L}\p{N}][\p{L}\p{N}\p{M}_-]{0,127}$/u.test(decoded) || ["me", "self"].includes(decoded.toLowerCase())) throw new OpenCliError("opencli_invalid_input");
  return encodeURIComponent(decoded);
}
const accountValid = (value: string) => /^ACo[A-Za-z\d_-]{16,125}$/u.test(value);
const publicLink = text.refine(value => publicWebUrl(value, true) !== null);
const label = text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 512 && !/\p{Cc}/u.test(value));
const numeric = /^[1-9]\d{0,19}$/u;
function target(handle: string, account?: string) { const normalized = normalizeLinkedinHandle(handle); if (account !== undefined && !accountValid(account)) throw new OpenCliError("opencli_invalid_input"); return normalized; }
function one<T extends z.ZodType>(schema: T, value: unknown): z.output<T> { const parsed = z.array(schema).length(1).safeParse(value); if (!parsed.success) throw new OpenCliError("opencli_invalid_output"); return parsed.data[0]!; }
function followers(value: string) {
  const match = /^(0|[1-9]\d*|[1-9]\d{0,2}(?:,\d{3})+) followers?$/u.exec(value.trim().toLowerCase());
  if (!match) return null; const count = Number(match[1]!.replace(/,/gu, "")); return Number.isSafeInteger(count) ? count : null;
}
export const linkedinConnectionObservation = z.strictObject({ raw: bounded(100), lower: z.number().int().nonnegative().safe().nullable(), upper: z.number().int().nonnegative().safe().nullable() }).nullable();
export function linkedinConnections(raw: string) {
  raw = bounded(100).parse(raw).trim();
  if (!raw) return null;
  const match = /^(0|[1-9]\d*|[1-9]\d{0,2}(?:,\d{3})+)(\+)?\s*(?:connections?|位好友|位联系人|位人脉)$/iu.exec(raw);
  if (!match) return { raw, lower: null, upper: null };
  const lower = Number(match[1]!.replaceAll(",", ""));
  if (!Number.isSafeInteger(lower)) throw new OpenCliError("opencli_invalid_output");
  return { raw, lower, upper: match[2] ? null : lower };
}
function experienceSchema(handle: string, account: z.ZodType<string>) {
  return z.strictObject({ author_handle: z.literal(handle), account_id: account, profile_url: z.literal(`https://www.linkedin.com/in/${handle}/`),
    evidence_source: z.literal("visible_current_experience"), section_found: z.boolean(), completeness: z.enum(["complete", "unknown"]).optional(),
    entries: z.array(z.strictObject({ text: bounded(4000).refine(value => !!value.trim() && /(?:\bPresent\b|至今|现在|現在)/iu.test(value)) })).max(3)
  }).refine(value => value.section_found || value.entries.length === 0);
}
export function linkedinProfilePlan(handle: string) {
  handle = target(handle); const source = `https://www.linkedin.com/in/${handle}/`;
  const schema = z.strictObject({ adapter_version: z.literal("1.0.0"), author_handle: z.literal(handle), account_id: text.refine(accountValid), author_label: label, profile_url: z.literal(source), follower_count: z.number().int().nonnegative().safe().nullable().optional().transform(value => value ?? null), follower_text: bounded(100), connections: linkedinConnectionObservation.optional(), headline: bounded(2000), bio: bounded(8000), public_links: z.array(publicLink).max(10), contact_links: z.array(z.literal(`${source}overlay/contact-info/`)).max(1), evidence_source: z.literal("visible_profile_header"), bio_truncated: z.boolean(), bio_status: z.enum(["absent", "loading", "complete", "truncated", "unknown"]).optional(), current_experience: experienceSchema(handle, text.refine(accountValid)).optional() }).refine(row => {
    if (row.current_experience && row.current_experience.account_id !== row.account_id) return false;
    const connection = row.connections;
    if (connection == null) return row.follower_count === followers(row.follower_text);
    const expected = linkedinConnections(connection.raw);
    return row.follower_count === followers(row.follower_text) && expected !== null && expected.lower === connection.lower && expected.upper === connection.upper && expected.raw === connection.raw;
  });
  return { site: "onecrew-linkedin-reader", name: "profile", args: ["-f", "json", "--", handle], addons: ["onecrew-browser-search"], foregroundBrowser: true, maximumBytes: 48000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function linkedinContentPlan(handle: string, account: string) {
  handle = target(handle, account);
  const schema = z.strictObject({ account_id: z.literal(account), author_handle: z.literal(handle), profile_url: z.literal(`https://www.linkedin.com/in/${handle}/`), post_id: text.regex(numeric), source_url: text, text: bounded(16000).refine(value => !!value.trim()), text_truncated: z.boolean(), observations: nativeContentObservations.optional(), evidence_source: z.literal("visible_author_activity") }).refine(row => row.source_url === `https://www.linkedin.com/feed/update/urn:li:activity:${row.post_id}/`);
  return { site: "onecrew-linkedin-reader", name: "content", args: ["-f", "json", "--", handle, account], addons: ["onecrew-browser-search"], foregroundBrowser: true, maximumBytes: 24000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function linkedinExperiencePlan(handle: string, account: string) {
  handle = target(handle, account);
  const schema = experienceSchema(handle, z.literal(account));
  return { site: "onecrew-linkedin-reader", name: "experience", args: ["-f", "json", "--", handle, account], addons: ["onecrew-browser-search"], foregroundBrowser: true, maximumBytes: 16000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function linkedinContactPlan(handle: string, account: string) {
  handle = target(handle, account); const source = `https://www.linkedin.com/in/${handle}/overlay/contact-info/`;
  const schema = z.strictObject({ adapter_version: z.literal("1.0.0"), author_handle: z.literal(handle), account_id: z.literal(account), source_url: z.literal(source), title: label.refine(value => [...value].length <= 200), contact_text: bounded(8000), contact_links: z.array(publicLink).max(10), truncated: z.boolean(), evidence_source: z.literal("visible_contact_dialog") });
  return { site: "onecrew-linkedin-reader", name: "contact", args: ["-f", "json", "--", handle, account], addons: ["onecrew-browser-search"], foregroundBrowser: true, maximumBytes: 32000, decode(value: unknown) {
    const row = one(schema, value); const excerpt = `${row.contact_links.map(link => `Website: ${link}`).join("\n")}\n${row.contact_text}`;
    if (Buffer.byteLength(excerpt) > 20000) throw new OpenCliError("opencli_output_too_large");
    return { requested_url: source, document_url: source, title: row.title, markdown_excerpt: excerpt, truncated: row.truncated };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function linkedinCommentsPlan(handle: string, account: string, source: string) {
  handle = target(handle, account); const match = /^https:\/\/www\.linkedin\.com\/feed\/update\/urn:li:activity:([1-9]\d{0,19})\/$/u.exec(source);
  if (!match) throw new OpenCliError("opencli_invalid_input");
  const schema = z.strictObject({ author_handle: z.literal(handle), account_id: z.literal(account), source_url: z.literal(source), comments: z.array(z.strictObject({ id: text.regex(numeric), parent_post_id: z.literal(match[1]!), text: bounded(2000) })).max(5), evidence_source: z.literal("visible_same_post_comments") }).refine(row => new Set(row.comments.map(comment => comment.id)).size === row.comments.length);
  return { site: "onecrew-linkedin-reader", name: "comments", args: ["-f", "json", "--", handle, account, source], addons: ["onecrew-browser-search"], foregroundBrowser: true, maximumBytes: 16000, decode: (value: unknown) => one(schema, value).comments } satisfies ReviewedOpenCliRead<unknown>;
}
