import { createHash } from "node:crypto";
import { Readable } from "node:stream";
import { isDeepStrictEqual } from "node:util";
import parser from "stream-chain/node/jsonl/parser.js";
import { z } from "zod";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import type { TableSyncTable } from "./table-sync-store.ts";

export const tableSyncPartBytes = 8 * 1024 * 1024;
type Pending = NonNullable<TableSyncTable["pending"]>;
const object = z.record(z.string(), z.unknown());
const nonempty = z.string().refine(value => !!value.trim());
const headerSchema = z.object({ kind: z.literal("header"), format: z.literal("onecrew.dynamic_table_capture"), format_version: z.literal(1), capture_kind: z.literal("full_sample"), version_id: z.uuid(), source_instance_id: z.uuid(), consent_id: z.uuid(), table_id: z.string().refine(value => Buffer.byteLength(value) > 0 && Buffer.byteLength(value) <= 128) }).passthrough();
const schemaSchema = z.object({ kind: z.literal("schema"), schema_id: z.string().min(1), fields: z.array(z.object({ id: z.string(), semantic_type: z.string() }).passthrough()).length(5) }).passthrough();
const rowSchema = z.object({ kind: z.literal("row.upsert"), row_id: z.string(), schema_id: z.string(), order: creatorU64,
  values: z.object({ name: z.object({ name: nonempty, identity: z.object({ platform_id: z.string().min(1) }).passthrough() }).passthrough(), category: z.unknown(), platform: z.unknown(), score: z.object({ level: z.enum(["excellent", "good", "medium"]), reason: nonempty }).passthrough(), contacts: z.string() }).strict(),
}).passthrough();

// 采用成熟 JSONL 解析器；原始整数通过 Node reviver 的 source 保留完整精度。
export async function validateTableSyncArchive(archive: string, expectedCount: bigint) {
  stringifyApplicationJson(archive);
  if (/(?:^|\n)[\t \r]*\n/u.test(archive)) throw new Error("table_sync_capture_invalid");
  const reviver = (_key: string, value: unknown, context?: { source?: string }) => typeof value === "number" && Number.isInteger(value) && !Number.isSafeInteger(value) && context?.source ? parseApplicationJson(context.source) : value;
  const parsed = Readable.from([archive]).pipe(parser.asStream({ reviver }));
  let header: z.infer<typeof headerSchema> | undefined, schema: z.infer<typeof schemaSchema> | undefined, count = 0n;
  const identifiers = new Set<string>();
  for await (const raw of parsed) {
    const item = raw as { key: number; value: unknown };
    stringifyApplicationJson(item.value);
    if (item.key === 0) { header = headerSchema.parse(item.value); continue; }
    if (item.key === 1) {
      schema = schemaSchema.parse(item.value);
      const expected = [["name", "people"], ["category", "string"], ["platform", "string"], ["score", "score"], ["contacts", "string"]];
      if (schema.fields.some((field, index) => field.id !== expected[index]![0] || field.semantic_type !== expected[index]![1])) throw new Error("table_sync_capture_invalid");
      continue;
    }
    const row = rowSchema.parse(item.value);
    if (!schema || row.schema_id !== schema.schema_id || BigInt(row.order) !== count || identifiers.has(row.row_id)) throw new Error("table_sync_capture_invalid");
    identifiers.add(row.row_id); count++;
  }
  if (!header || !schema || count !== expectedCount) throw new Error("table_sync_capture_invalid");
  return { header, schema, count };
}

export async function validateTableSyncPending(pending: Pending) {
  const archive = Buffer.from(pending.archive), sha256 = createHash("sha256").update(archive).digest("hex");
  if (sha256 !== pending.archive_sha256) throw new Error("table_sync_pending_integrity_failed");
  const result = await validateTableSyncArchive(pending.archive, BigInt(pending.record_count));
  for (const key of ["version_id", "consent_id", "table_id", "table_name", "capture_started_at", "capture_kind", "format", "format_version"]) if (!isDeepStrictEqual(result.header[key], pending.input[key])) throw new Error("table_sync_pending_integrity_failed");
  for (const [index, raw] of pending.parts.entries()) {
    if (index * tableSyncPartBytes >= archive.length) throw new Error("table_sync_pending_integrity_failed");
    const bytes = archive.subarray(index * tableSyncPartBytes, (index + 1) * tableSyncPartBytes), part = object.parse(raw);
    if (part.part_number !== index + 1 || BigInt(creatorU64.parse(part.bytes)) !== BigInt(bytes.length) || part.md5 !== createHash("md5").update(bytes).digest("hex") || typeof part.etag !== "string" || !part.etag) throw new Error("table_sync_pending_integrity_failed");
  }
  return result;
}

export async function validateTableSyncManifest(pending: Pending, raw: unknown): Promise<void> {
  const { header } = await validateTableSyncPending(pending), body = object.parse(raw), manifest = object.parse(body.manifest);
  if (body.status !== "committed" || body.version_id !== pending.input.version_id) throw new Error("table_sync_manifest_mismatch");
  for (const key of ["version_id", "consent_id", "local_source_id", "table_id", "table_name", "capture_seq", "capture_kind", "capture_started_at", "previous_version_id", "scopes", "format", "format_version"]) if (!isDeepStrictEqual(manifest[key], pending.input[key])) throw new Error("table_sync_manifest_mismatch");
  if (BigInt(creatorU64.parse(manifest.bytes)) !== BigInt(Buffer.byteLength(pending.archive)) || BigInt(creatorU64.parse(manifest.record_count)) !== BigInt(pending.record_count) || manifest.schema_count !== 1 || manifest.source_instance_id !== header.source_instance_id || manifest.capture_finished_at !== pending.finished || manifest.attempt_id !== pending.attempt || !isDeepStrictEqual(manifest.parts, pending.parts) || pending.parts.length !== Math.ceil(Buffer.byteLength(pending.archive) / tableSyncPartBytes) || typeof manifest.etag !== "string" || !manifest.etag) throw new Error("table_sync_manifest_mismatch");
}
