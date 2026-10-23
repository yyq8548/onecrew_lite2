import { createHash } from "node:crypto";
import { z } from "zod";
import { channel, confidence, integer, locatorKind, platform, sourceKind, stage, tags, text, trim, parseJson } from "./contracts.ts";

export const transferProfile = z.object({ handle: text.nullable(), display_name: text.nullable(), profile_url: text.nullable(), avatar_url: text.nullable() }).strict();
export const transferFollowers = z.partialRecord(platform, integer);
export const transferAccount = z.object({ capture_order: integer.max(4294967295), platform, target_kind: z.enum(["user", "bot", "channel", "supergroup", "group", "creator", "business"]), external_id: text, link_state: z.enum(["claimed", "confirmed", "rejected"]), confidence, source_kind: sourceKind, latest_profile: transferProfile.nullable() }).strict();
export const transferContact = z.object({ save_order: integer.max(4294967295), channel, locator_kind: locatorKind, normalized_value: text, display_value: text, confidence, source_kinds: z.array(sourceKind) }).strict();
export const transferCreator = z.object({ semantic_sha256: text, record_key: text, display_name: text, tags, notes: text.nullish(), score: integer.max(100).nullable(), follower_counts: transferFollowers, pinned: z.boolean(), relationship: z.object({ stage }).strict(), accounts: z.array(transferAccount).max(64), contact_routes: z.array(transferContact) }).strict();
export type TransferCreator = z.infer<typeof transferCreator>;
export type TransferAccount = z.infer<typeof transferAccount>;
export type TransferContact = z.infer<typeof transferContact>;
export const transferBundle = z.object({ format: z.literal("reacher.creator_bundle"), format_version: z.union([z.literal(1), z.literal(2), z.literal(3), z.literal(4)]), profile: z.literal("portable_public"), snapshot_kind: z.literal("full"), creator_count: integer.max(10000), content_sha256: text, creators: z.array(transferCreator).max(10000) }).strict();
export type TransferBundle = z.infer<typeof transferBundle>;
const platformOrder = platform.options;
const targetOrder = transferAccount.shape.target_kind.options;
export const compareText = (a: string, b: string) => Buffer.compare(Buffer.from(a), Buffer.from(b));
export function compareAccounts(a: TransferAccount, b: TransferAccount) { return a.capture_order - b.capture_order || platformOrder.indexOf(a.platform) - platformOrder.indexOf(b.platform) || targetOrder.indexOf(a.target_kind) - targetOrder.indexOf(b.target_kind) || compareText(a.external_id, b.external_id); }
export function compareContacts(a: TransferContact, b: TransferContact) { return a.save_order - b.save_order || compareText(a.channel, b.channel) || compareText(a.locator_kind, b.locator_kind) || compareText(a.normalized_value, b.normalized_value); }
export const digest = (value: unknown) => `sha256:${createHash("sha256").update(JSON.stringify(value)).digest("hex")}`;
export function canonicalCreator(creator: TransferCreator, version: number): unknown[] {
  const projection: unknown[] = [creator.display_name, creator.score, platformOrder.filter(p => creator.follower_counts[p] !== undefined).map(p => [p, creator.follower_counts[p]]), creator.pinned, creator.relationship.stage,
    creator.accounts.map(a => [a.capture_order, a.platform, a.target_kind, a.external_id, a.link_state, a.confidence, a.source_kind, a.latest_profile === null ? null : [a.latest_profile.handle, a.latest_profile.display_name, a.latest_profile.profile_url, a.latest_profile.avatar_url]]),
    creator.contact_routes.map(r => [r.save_order, r.channel, r.locator_kind, r.normalized_value, r.display_value, r.confidence, r.source_kinds])];
  if (version >= 3) projection.push(creator.tags);
  if (version >= 4) projection.push(creator.notes ?? null);
  return projection;
}
function contentDigest(creators: TransferCreator[], version: number) { return digest(["reacher.creator_bundle", version, "portable_public", "full", creators.length, creators.map(c => [c.record_key, c.semantic_sha256])]); }
function required(value: string, max: number, field: string) { if (!trim(value) || Buffer.byteLength(value) > max) throw new Error(`Creator bundle ${field} is empty or exceeds ${max} bytes`); }
function publicUrl(value: string) { required(value, 2048, "URL"); const url = new URL(value); if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error("Creator bundle URL must use HTTP(S) without userinfo"); }
export function validateBundle(input: unknown): TransferBundle {
  const bundle = transferBundle.parse(input);
  for (const creator of bundle.creators) { creator.follower_counts = Object.fromEntries(platformOrder.filter(p => creator.follower_counts[p] !== undefined).map(p => [p, creator.follower_counts[p]])); if (creator.notes == null) delete creator.notes; }
  if (bundle.creator_count !== bundle.creators.length) throw new Error("Creator bundle count does not match creators");
  const accountFacts = new Map<string, string>(), confirmed = new Map<string, string>(), locators = new Map<string, string>();
  const occurrences = new Map<string, { canonical: unknown[]; actual: string[] }>();
  for (const [index, c] of bundle.creators.entries()) {
    if (index > 0 && compareText(bundle.creators[index - 1]!.record_key, c.record_key) >= 0) throw new Error("Creator records must be strictly sorted");
    required(c.display_name, 512, "display_name");
    if (c.notes != null) required(c.notes, 32000, "notes");
    if (bundle.format_version < 4 && (c.notes != null || c.contact_routes.some(r => r.channel === "wechat"))) throw new Error("Historical Creator bundle cannot contain notes or WeChat");
    if (bundle.format_version < 3 && c.tags.length) throw new Error("Historical Creator bundle cannot contain tags");
    if (bundle.format_version === 1 && (c.follower_counts.tiktok !== undefined || c.accounts.some(a => a.platform === "tiktok") || c.contact_routes.some(r => r.channel === "tiktok"))) throw new Error("Creator bundle v1 cannot contain TikTok facts");
    if (JSON.stringify(tags.parse(c.tags)) !== JSON.stringify(c.tags)) throw new Error("Creator tags must already be normalized");
    const accountKeys = new Set<string>(), contactKeys = new Set<string>();
    for (const [i, a] of c.accounts.entries()) {
      if (i > 0 && compareAccounts(c.accounts[i - 1]!, a) >= 0) throw new Error("Creator accounts must be strictly sorted");
      required(a.external_id, 512, "external_id");
      if (!/^[\x21-\x7e]+$/.test(a.external_id) || /["\\]/.test(a.external_id)) throw new Error("External ID must be visible ASCII");
      if (a.platform === "tiktok" && !/^tto:[A-Za-z0-9_-]+$/.test(a.external_id)) throw new Error("TikTok transfer identity must be tto:<creator_id>");
      if (a.latest_profile) {
        for (const value of [a.latest_profile.handle, a.latest_profile.display_name]) if (value !== null) required(value, 512, "profile text");
        for (const value of [a.latest_profile.profile_url, a.latest_profile.avatar_url]) if (value !== null) publicUrl(value);
      }
      const key = JSON.stringify([a.platform, a.target_kind, a.external_id]);
      const facts = JSON.stringify([a.capture_order, a.platform, a.target_kind, a.external_id, a.source_kind, a.latest_profile]);
      if (accountKeys.has(key) || accountFacts.has(key) && accountFacts.get(key) !== facts) throw new Error("Creator bundle has conflicting account facts");
      accountKeys.add(key); accountFacts.set(key, facts);
      if (a.link_state === "confirmed") { if (confirmed.has(key) && confirmed.get(key) !== c.record_key) throw new Error("Account confirmed for multiple Creators"); confirmed.set(key, c.record_key); }
    }
    for (const [i, r] of c.contact_routes.entries()) {
      if (i > 0 && compareContacts(c.contact_routes[i - 1]!, r) >= 0) throw new Error("Creator contacts must be strictly sorted");
      required(r.normalized_value, 2048, "normalized_value"); required(r.display_value, 2048, "display_value");
      const key = JSON.stringify([r.channel, r.locator_kind, r.normalized_value]);
      if (contactKeys.has(key) || locators.has(key) && locators.get(key) !== c.record_key) throw new Error("Creator bundle shares or duplicates contact locator");
      contactKeys.add(key); locators.set(key, c.record_key);
    }
    const canonical = canonicalCreator(c, bundle.format_version);
    if (digest(canonical) !== c.semantic_sha256) throw new Error("Creator semantic digest does not match facts");
    const group = occurrences.get(c.semantic_sha256) ?? { canonical, actual: [] }; group.actual.push(c.record_key); occurrences.set(c.semantic_sha256, group);
  }
  for (const group of occurrences.values()) {
    const expected = group.actual.map((_, occurrence) => digest(["reacher.creator_record", group.canonical, occurrence])).sort(compareText);
    if (JSON.stringify(expected) !== JSON.stringify(group.actual.sort(compareText))) throw new Error("Creator record key does not match canonical occurrence");
  }
  if (bundle.content_sha256 !== contentDigest(bundle.creators, bundle.format_version)) throw new Error("Creator content digest does not match facts");
  if (Buffer.byteLength(JSON.stringify(bundle)) + 1 > 32 * 1024 * 1024) throw new Error("Creator bundle exceeds 32 MiB");
  return bundle;
}
export function buildBundle(input: TransferCreator[], version?: 1 | 2 | 3 | 4): TransferBundle {
  const creators = structuredClone(input);
  const selected = version ?? (creators.some(c => c.notes != null || c.contact_routes.some(r => r.channel === "wechat")) ? 4 : 3);
  const occurrences = new Map<string, number>();
  for (const c of creators) {
    c.accounts.sort(compareAccounts); c.contact_routes.sort(compareContacts);
    const canonical = canonicalCreator(c, selected);
    c.semantic_sha256 = digest(canonical);
    const occurrence = occurrences.get(c.semantic_sha256) ?? 0;
    c.record_key = digest(["reacher.creator_record", canonical, occurrence]); occurrences.set(c.semantic_sha256, occurrence + 1);
  }
  creators.sort((a, b) => compareText(a.record_key, b.record_key));
  const result = { format: "reacher.creator_bundle" as const, format_version: selected, profile: "portable_public" as const, snapshot_kind: "full" as const, creator_count: creators.length, content_sha256: contentDigest(creators, selected), creators };
  return validateBundle(result);
}
export function encodeBundle(bundle: TransferBundle): string {
  return `${JSON.stringify({ ...bundle, creators: bundle.creators.map(c => ({ semantic_sha256: c.semantic_sha256, record_key: c.record_key, display_name: c.display_name, ...(bundle.format_version >= 3 ? { tags: c.tags } : {}), ...(c.notes == null ? {} : { notes: c.notes }), score: c.score, follower_counts: Object.fromEntries(platformOrder.filter(p => c.follower_counts[p] !== undefined).map(p => [p, c.follower_counts[p]])), pinned: c.pinned, relationship: c.relationship, accounts: c.accounts, contact_routes: c.contact_routes })) })}\n`;
}
export function readStagedBundle(raw: string): TransferBundle {
  const input = parseJson(raw);
  if (!input || typeof input !== "object" || Array.isArray(input) || !Array.isArray(input.creators)) throw new Error("Invalid staged Creator bundle");
  const version = integer.parse(input.format_version);
  const records = input.creators.map(c => { if (!c || typeof c !== "object" || Array.isArray(c)) throw new Error("Invalid staged Creator"); if (version < 3 && "tags" in c || version < 4 && "notes" in c) throw new Error("Historical Creator bundle contains unreleased fields"); return { ...c, ...(version < 3 ? { tags: [] } : {}) }; });
  const bundle = validateBundle({ ...input, creators: records });
  if (encodeBundle(bundle) !== raw) throw new Error("Creator bundle artifact is not in canonical byte form");
  return bundle;
}
export function historicalDigests(bundle: TransferBundle): string[] {
  if (bundle.creators.some(c => c.notes != null || c.contact_routes.some(r => r.channel === "wechat"))) return [];
  const digests: string[] = [];
  if (bundle.format_version === 4) digests.push(buildBundle(bundle.creators, 3).content_sha256);
  if (bundle.creators.some(c => c.tags.length > 0)) return digests;
  digests.push(buildBundle(bundle.creators, 2).content_sha256);
  if (!bundle.creators.some(c => c.follower_counts.tiktok !== undefined || c.accounts.some(a => a.platform === "tiktok") || c.contact_routes.some(r => r.channel === "tiktok"))) digests.push(buildBundle(bundle.creators, 1).content_sha256);
  return digests;
}
