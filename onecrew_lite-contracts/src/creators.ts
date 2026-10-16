import { z } from "zod";
import { parseStoredJson } from "./stored-json.ts";
import { isLosslessNumber, isSafeNumber, parse } from "lossless-json";
import { assertStrictJson, isWellFormedUnicode } from "./json.ts";

export const text = z.string().refine(isWellFormedUnicode, "Invalid Unicode");
export const integer = z.number().int().nonnegative().safe();
export const platform = z.enum(["x", "instagram", "telegram", "youtube", "tiktok"]);
export const stage = z.enum(["prospect", "engaged", "collaborating", "archived"]);
export const channel = z.enum(["email", "wechat", "x", "instagram", "telegram", "youtube", "tiktok", "website", "link_hub"]);
export const locatorKind = z.enum(["email_address", "handle", "url", "invite_link", "channel_id"]);
export const confidence = z.enum(["exact", "asserted", "possible"]);
export const sourceKind = z.enum(["initial_platform_account", "profile_bio", "profile_website", "profile_location", "video_description", "channel_description", "link_hub", "manual_business_email", "community_administrator", "manual_entry", "contact_route_resolution", "creator_import"]);
export const trim = (value: string): string => value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
export const asciiLower = (value: string): string => value.replace(/[A-Z]/g, character => character.toLowerCase());
export function parseJson(value: string) { assertStrictJson(value); return z.json().parse(parse(value, (key, value) => { if (!isWellFormedUnicode(key) || typeof value === "string" && !isWellFormedUnicode(value)) throw new Error("Stored JSON contains invalid Unicode"); return value; }, { parseNumber: raw => { if (!isSafeNumber(raw)) throw new Error("Stored JSON number cannot be represented safely"); return Number(raw); } })); }
const nonempty = text.transform(trim).refine(value => value.length > 0, "Value cannot be empty when provided");
export const tags = z.array(text.refine(value => !/\p{Cc}/u.test(value), "Creator tags cannot contain control characters").transform(trim).refine(value => [...value].length >= 1 && [...value].length <= 12, "Creator tags must contain 1 to 12 characters")).max(3).refine(values => new Set(values).size === values.length, "Creator tags must be unique");
export const creatorU64 = z.union([integer, z.bigint().nonnegative().max(18446744073709551615n), z.custom<import("lossless-json").LosslessNumber>(isLosslessNumber)]).transform((value, context): number | bigint => {
  const raw = value.toString(); if (!/^[0-9]+$/.test(raw) || BigInt(raw) > 18446744073709551615n) { context.addIssue({ code: "custom", message: "Value must be an unsigned 64-bit integer" }); return z.NEVER; }
  const number = BigInt(raw); return number <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(number) : number;
});
export const followers = z.partialRecord(platform, creatorU64);
export const manualCreatorInput = z.object({ display_name: nonempty, tags: tags.nullish(), notes: nonempty.nullish(), contact_routes: z.array(z.object({ channel, value: nonempty, source_url: nonempty.nullish() }).strict()).max(16).default([]) }).strict();
export const updateCreatorInput = z.object({ creator_id: text, display_name: nonempty.nullish(), tags: tags.nullish(), notes: nonempty.nullish(), score: integer.max(100).nullish(), assessment: nonempty.nullish(), follower_counts: followers.nullish() }).strict().refine(input => input.display_name != null || input.tags != null || input.notes != null || input.score != null || input.assessment != null || Object.keys(input.follower_counts ?? {}).length > 0, "creator update requires at least one field to change");
const range = z.object({ from: integer.nullish(), to_exclusive: integer.nullish() }).strict().refine(value => value.from == null || value.to_exclusive == null || value.from < value.to_exclusive, "creator date range is invalid");
export const listCreatorsInput = z.object({
  limit: integer.max(4294967295).nullish(), cursor: text.nullish(), query: text.nullish(),
  stages: z.array(stage).default([]), primary_platforms: z.array(platform).default([]),
  last_touch_range: range.nullish(), updated_range: range.nullish(),
  sorting: z.array(z.object({ field: z.enum(["display_name", "followers", "score", "last_touch_at", "updated_at", "created_at"]), direction: z.enum(["asc", "desc"]) }).strict()).max(6).default([]).refine(values => new Set(values.map(value => value.field)).size === values.length, "creator sorting is invalid"),
  include_facets: z.boolean().default(false), group: z.discriminatedUnion("kind", [z.object({ kind: z.literal("uncategorized") }).strict(), z.object({ kind: z.literal("group"), group_id: text }).strict()]).nullish(), include_archived: z.boolean().default(false),
}).strict();
export type ListCreatorsInput = z.infer<typeof listCreatorsInput>;
export type ManualCreatorInput = z.infer<typeof manualCreatorInput>;
export const stageCreatorInput = z.object({ creator_id: text, stage }).strict();
export const pinCreatorInput = z.object({ creator_id: text, pinned: z.boolean() }).strict();
export const timestamps = { created_at: integer, updated_at: integer };
export const creatorRow = z.object({ id: text, display_name: text, notes: text.nullable(), score: integer.max(100).nullable(), assessment: text.nullable(), follower_counts_json: text, tags_json: text, archived_at: integer.nullable(), pinned_at: integer.nullable(), ...timestamps });
export const sourceRow = z.object({ id: text, kind: sourceKind, platform: platform.nullable(), source_url: text.nullable(), external_ref: text.nullable(), account_ref_id: text.nullable(), metadata_json: text, observed_at: integer });
export function sourceOutput(value: unknown) { const { metadata_json, ...row } = sourceRow.parse(value); return { ...row, metadata: parseStoredJson(metadata_json) }; }
export function creatorOutput(value: unknown) { const { tags_json, follower_counts_json, ...row } = creatorRow.parse(value); return { ...row, tags: tags.parse(parseStoredJson(tags_json)), follower_counts: followers.parse(parseStoredJson(follower_counts_json)) }; }
