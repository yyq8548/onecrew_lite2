import { z } from "zod";
import { creatorU64, trim } from "./contracts.ts";
import { parseApplicationJson, stringifyApplicationJson } from "@onecrew/runtime-contracts/json";

export const historyU64 = creatorU64.transform(value => BigInt(value));
export const historyChannel = z.enum(["email", "wechat", "x", "instagram", "telegram", "youtube", "tiktok", "website", "link_hub"]);
export const historyIntent = z.enum(["interested", "question", "negotiating", "declined", "irrelevant", "other"]);
const optionalText = z.string().nullable().optional().transform(value => value ?? null);
const limit = z.number().int().min(0).max(4294967295).nullable().optional().transform(value => value ?? 25);
export const historyConversationListInput = z.strictObject({ channel: historyChannel.nullable().optional().transform(value => value ?? null), cursor: optionalText, limit, query: optionalText, status: z.enum(["waiting_reply", "replied"]).nullable().optional().transform(value => value ?? null) });
export const historyTriageListInput = z.strictObject({ cursor: optionalText, limit, include_acknowledged: z.boolean().default(false) });
export const historyTriageRecordInput = z.strictObject({ email_id: optionalText, message_id: optionalText, intent: historyIntent, summary: z.string(), suggested_reply: optionalText });
export const historyTriage = z.object({ id: z.string(), channel: z.enum(["email", "dm"]), email_id: z.string().optional(), message_id: z.string().optional(), intent: historyIntent, summary: z.string(), suggested_reply: z.string().optional(), created_at: historyU64, acknowledged_at: historyU64.optional() });
export function historyExcerpt(value: string): string {
  const normalized = value.split(/[\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]+/u).filter(Boolean).join(" ");
  const characters = Array.from(normalized); return characters.length > 160 ? `${characters.slice(0, 160).join("")}…` : normalized;
}
export function historyCursor(raw: string | null, context: string): readonly [bigint | null, string | null] {
  if (raw === null) return [null, null];
  const encoded = trim(raw);
  if (!/^[A-Za-z0-9_-]+$/u.test(encoded)) throw new Error("page cursor is not valid for this query");
  const bytes = Buffer.from(encoded, "base64url");
  if (bytes.toString("base64url") !== encoded) throw new Error("page cursor is not valid for this query");
  const envelope = z.object({ v: z.union([z.literal(1), z.literal(1n)]), context: z.literal(context), position: z.string() }).parse(parseApplicationJson(bytes.toString("utf8")));
  const separator = envelope.position.indexOf("|"), at = envelope.position.slice(0, separator), id = envelope.position.slice(separator + 1);
  if (separator < 0 || id.includes("|") || !/^\+?[0-9]+$/u.test(at)) throw new Error("page cursor is not valid for this query");
  return [historyU64.parse(BigInt(at)), id];
}
export function historyPage<T>(rows: T[], limit: number, context: string, position: (row: T) => readonly [bigint, string]) {
  const more = rows.length > limit, items = rows.slice(0, limit), last = items.at(-1);
  if (!more || !last) return { items };
  const [at, id] = position(last);
  return { items, next_cursor: Buffer.from(stringifyApplicationJson({ v: 1, context, position: `${at}|${id}` })).toString("base64url") };
}
