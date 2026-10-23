import { createHash } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { channel, integer, manualCreatorInput, parseJson, stage, tags, text, trim, type ManualCreatorInput } from "./contracts.ts";
import { contactRouteRecordSchema } from "./detail.ts";
import { normalizeContactResult } from "./normalization.ts";

const itemWire = z.object({ display_name: text, tags: z.array(text).nullish(), notes: text.nullish(), contact_routes: z.array(z.object({ channel, value: text, source_url: text.nullish() }).strict()).max(16).default([]) }).strict();
export const batchInput = z.object({ idempotency_key: text, validation: z.enum(["strict", "permissive"]), items: z.array(itemWire).min(1).max(50) }).strict();
export type CreatorBatchInput = z.infer<typeof batchInput>;
export const manualCreatorResult = z.object({ creator_id: text, display_name: text, tags, stage, created_at: integer, contact_routes: z.array(contactRouteRecordSchema) }).strict();
export const batchResult = z.object({ outcomes: z.array(z.discriminatedUnion("kind", [z.object({ kind: z.literal("accepted"), input_index: integer, creator: manualCreatorResult }).strict(), z.object({ kind: z.literal("rejected"), input_index: integer, message: text }).strict()])).min(1).max(50), replayed: z.boolean() }).strict();
export type CreatorBatchResult = z.infer<typeof batchResult>;
export function creatorBatchWireInput(input: CreatorBatchInput) {
  return { idempotency_key: input.idempotency_key, validation: input.validation, items: input.items.map(item => ({ display_name: item.display_name, ...(item.tags == null ? {} : { tags: item.tags }), notes: item.notes ?? null, contact_routes: item.contact_routes.map(route => ({ channel: route.channel, value: route.value, source_url: route.source_url ?? null })) })) };
}
export function batchFingerprint(input: CreatorBatchInput): string {
  const { items } = creatorBatchWireInput(input);
  if (Buffer.byteLength(JSON.stringify(items)) > 1048576) throw new Error("Creator batch payload exceeds 1048576 bytes");
  return createHash("sha256").update(JSON.stringify([input.validation, items])).digest("hex");
}
function bounded(value: string, bytes: number): string { let result = ""; for (const character of value) { if (Buffer.byteLength(result + character) > bytes) break; result += character; } return result; }
export function creatorBatchPreview(input: unknown) {
  const parsed = batchInput.parse(input); if (!trim(parsed.idempotency_key) || Buffer.byteLength(parsed.idempotency_key) > 128) throw new Error("idempotency_key must contain 1-128 characters");
  return { kind: "creator_batch_confirmation", title: "Review Creator batch", summary: "Confirm the exact Creator records listed below before they are created.", sections: [{ kind: "fields", heading: null, fields: [{ label: "Validation", value: parsed.validation }, { label: "Creators", value: String(parsed.items.length) }] }, { kind: "items", heading: "Creators to create", items: parsed.items.map(item => ({ title: bounded(trim(item.display_name), 256) || "(creator has no name)", description: bounded(trim(item.contact_routes.map(route => `${route.channel}: ${trim(route.value)}`).join(", ")), 512) || "no initial contact routes" })) }] };
}
export function preflightBatch(db: DatabaseSync, input: CreatorBatchInput): ({ ok: true; item: ManualCreatorInput } | { ok: false; message: string })[] {
  const seen = new Set<string>();
  return input.items.map(item => {
    const parsed = manualCreatorInput.safeParse(item);
    if (!parsed.success) return { ok: false, message: parsed.error.issues.map(issue => issue.message).join("; ") };
    let message: string | undefined; const locators: ReturnType<typeof normalizeContactResult>[] = [];
    for (const route of parsed.data.contact_routes) {
      const normalized = normalizeContactResult(route.channel, route.value); if (!normalized.ok) { message = normalized.message; break; }
      const key = JSON.stringify([normalized.value.channel, normalized.value.kind, normalized.value.normalized_value]);
      if (seen.has(key)) { message = "duplicate contact route in Creator batch"; break; }
      seen.add(key); locators.push(normalized);
    }
    if (message) return { ok: false, message };
    for (const locator of locators) { if (!locator.ok) throw new Error("Validated contact locator missing"); const v = locator.value; if (db.prepare("SELECT 1 FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id WHERE locator.channel=? AND locator.kind=? AND locator.normalized_value=? LIMIT 1").get(v.channel, v.kind, v.normalized_value)) return { ok: false, message: "contact route already belongs to another Creator" }; }
    return { ok: true, item: parsed.data };
  });
}
export function batchExisting(db: DatabaseSync, key: string, fingerprint: string): CreatorBatchResult | null {
  const row = db.prepare("SELECT * FROM idempotency_records WHERE key=?").get(key); if (!row) return null;
  if (row.action !== "creator_batch_create" || row.request_fingerprint !== fingerprint) throw new Error("idempotency key was already used for a different request");
  if (row.state === "failed") throw new Error(`Creator batch previously failed: ${String(row.result_json)}`);
  if (row.state !== "completed") throw new Error("idempotency action is already in progress");
  const stored = batchResult.parse(parseJson(text.parse(row.result_json)));
  return { outcomes: stored.outcomes, replayed: true };
}
