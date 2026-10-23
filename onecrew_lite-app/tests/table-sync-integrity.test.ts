import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { recommendationCapture } from "../../onecrew_lite-library/dist/modules/tables/recommendation-capture.js";
import type { DiscoveryCreatorView, TableDto } from "../../onecrew_lite-library/dist/modules/tables/data.js";
import { validateTableSyncArchive, validateTableSyncPending, validateTableSyncManifest } from "../src/sync/table-sync-integrity.ts";
import type { TableSyncTable } from "../src/sync/table-sync-store.ts";

// 复用原 Library creator-export 合同数据，实际调用其生产 capture 编码器。
const table: TableDto = { id: "table", name: "测试", columns: [{ id: "score", name: "Match evaluation", data_type: "score" }, { id: "category", name: "Content category", data_type: "string" }] };
const material: DiscoveryCreatorView = {
  state: "current", projection: { table_id: "table", campaign_id: "campaign", projection_format: 1, criteria_revision: 1, source_revision: 1, source_hash: "a".repeat(64), identity_revision: 1, identity_hash: "b".repeat(64), source_rows: [{ search_hit_id: "hit", candidate_id: "author", native_author_id: "123", evidence_version: "c".repeat(64) }], groups: [{ group_id: "d".repeat(64), has_same_author_review: false, decision_ids: [], members: [{ candidate_id: "author", platform: "twitter", native_author_id: "123", display_name: "作者", profile_url: "https://x.com/author", source_hit_ids: ["hit"] }] }] },
  sources: [{ search_hit_id: "hit", verdict: "qualified", reasons: [], contacts: [{ kind: "email", original_value: "business@example.com", normalized_value: "business@example.com", native_author_id: "123", source_url: "https://x.com/author", source_excerpt: "business@example.com", captured_at: 1, attribution: "author_published" }], row: { id: "row", values: { score: { level: "medium", reason: "已通过" }, category: "科技" } } }],
};
const metadata = { version_id: "d9a351fd-94f5-48b4-ae80-df503d41f10b", source_instance_id: "cd7d4b88-d3c6-47ab-b901-e4eaf164bb4e", consent_id: "5ec2446c-0b80-4b7c-a5c5-503c077390de", capture_started_at: "2026-10-09T00:00:00.000Z" };

test("actual Library capture JSONL preserves all rows and rejects changed schema, order, identity and blank events", async () => {
  const capture = recommendationCapture(table, material, metadata), archive = capture.bytes.toString("utf8");
  const result = await validateTableSyncArchive(archive, 1n);
  assert.equal(result.header.version_id, metadata.version_id);
  assert.equal(result.count, 1n);
  await assert.rejects(validateTableSyncArchive(archive, 0n), /capture_invalid/);
  await assert.rejects(validateTableSyncArchive(archive.replace('"order":0', '"order":18446744073709551615'), 1n), /capture_invalid/);
  await assert.rejects(validateTableSyncArchive(archive.replace('"semantic_type":"people"', '"semantic_type":"string"'), 1n), /capture_invalid/);
  await assert.rejects(validateTableSyncArchive(archive.replace('"platform_id":"123"', '"platform_id":""'), 1n));
  await assert.rejects(validateTableSyncArchive(archive.replace("\n", "\n\n"), 1n), /capture_invalid/);
  const contactless = recommendationCapture(table, { ...material, sources: [{ ...material.sources[0]!, contacts: [] }] }, metadata);
  assert.equal((await validateTableSyncArchive(contactless.bytes.toString(), 1n)).count, 1n);
  await assert.rejects(validateTableSyncArchive(contactless.bytes.toString().replace('"contacts":""', '"contacts":null'), 1n));
  await assert.rejects(validateTableSyncArchive(contactless.bytes.toString().replace(',"contacts":""', ''), 1n));
  const empty = recommendationCapture(table, { ...material, sources: [{ ...material.sources[0]!, verdict: "rejected" }] }, metadata);
  assert.equal((await validateTableSyncArchive(empty.bytes.toString(), 0n)).count, 0n);
});

test("pending capture and manifest validation bind exact bytes, input metadata and all part checksums", async () => {
  const capture = recommendationCapture(table, material, metadata), archive = capture.bytes.toString("utf8"), attempt = "11111111-1111-4111-8111-111111111111";
  const input = { version_id: metadata.version_id, consent_id: metadata.consent_id, local_source_id: metadata.source_instance_id, table_id: table.id, table_name: table.name, capture_seq: "18446744073709551615", capture_kind: "full_sample", capture_started_at: metadata.capture_started_at, previous_version_id: null, scopes: ["dynamic_table_records", "dynamic_table_schema"], format: "onecrew.dynamic_table_capture", format_version: 1 };
  const parts = [{ part_number: 1, bytes: capture.bytes.length, md5: createHash("md5").update(capture.bytes).digest("hex"), etag: "contract-part" }];
  const pending: NonNullable<TableSyncTable["pending"]> = { input, archive, archive_sha256: createHash("sha256").update(capture.bytes).digest("hex"), content_hash: capture.content_hash, record_count: capture.record_count, finished: metadata.capture_started_at, parts, attempt };
  await validateTableSyncPending(pending);
  await assert.rejects(validateTableSyncPending({ ...pending, archive: `${archive} ` }), /integrity_failed/);
  await assert.rejects(validateTableSyncPending({ ...pending, input: { ...input, consent_id: attempt } }), /integrity_failed/);
  await assert.rejects(validateTableSyncPending({ ...pending, parts: [{ ...parts[0], md5: "0".repeat(32) }] }), /integrity_failed/);
  const manifest = { ...input, source_instance_id: metadata.source_instance_id, bytes: capture.bytes.length, record_count: capture.record_count, schema_count: 1, capture_finished_at: metadata.capture_started_at, attempt_id: attempt, parts, etag: "contract-manifest" };
  await validateTableSyncManifest(pending, { status: "committed", version_id: metadata.version_id, manifest });
  for (const changed of [{ bytes: capture.bytes.length + 1 }, { record_count: 0 }, { capture_seq: "1" }, { source_instance_id: attempt }, { attempt_id: metadata.version_id }, { parts: [] }]) await assert.rejects(validateTableSyncManifest(pending, { status: "committed", version_id: metadata.version_id, manifest: { ...manifest, ...changed } }), /manifest_mismatch/);
});
