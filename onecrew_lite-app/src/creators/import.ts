import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { integer, parseJson, text } from "./contracts.ts";
import { applyImport } from "./import-apply.ts";
import { changesGraph, loadImportBasis, planFingerprint, planImport, emptyImpact } from "./import-plan.ts";
import { encodeBundle, historicalDigests, readStagedBundle, validateBundle, type TransferBundle } from "./transfer-contracts.ts";

export const importOperationInput = z.object({ operation_id: text }).strict();
export const importFileInput = z.object({ file_name: text, bytes: z.instanceof(Uint8Array) }).strict();
export const prepareAgain = "choose the Creator bundle again and prepare a fresh import";
const impactSchema = z.object(Object.fromEntries(Object.keys(emptyImpact()).map(key => [key, integer])));
const operationSchema = z.object({ id: text, content_sha256: text, bundle_json: text.nullable(), plan_fingerprint: text, impact_json: text, status: z.enum(["prepared", "completed"]), result_json: text.nullable(), expires_at: integer });
function receipt(row: Record<string, unknown>) { return { content_sha256: text.parse(row.content_sha256), format_version: integer.parse(row.format_version), creator_count: integer.parse(row.creator_count), impact: impactSchema.parse(parseJson(text.parse(row.impact_json))), imported_at: integer.parse(row.imported_at) }; }
function complete(db: DatabaseSync, id: string, result: unknown, at: number) { const changed = db.prepare("UPDATE creator_import_operations SET bundle_json=NULL,status='completed',result_json=?,updated_at=? WHERE id=? AND status='prepared'").run(JSON.stringify(result), at, id).changes; if (changed !== 1) throw new Error("Creator import operation is no longer prepared"); }
function load(db: DatabaseSync, id: string) { const row = db.prepare("SELECT * FROM creator_import_operations WHERE id=?").get(id); if (!row) throw new Error("Creator import operation not found"); return operationSchema.parse(row); }
function terminal(result: string | null) { if (result === null) throw new Error("Completed Creator import has no result"); const decoded = parseJson(result); if (!decoded || typeof decoded !== "object" || Array.isArray(decoded) || !["imported", "unchanged", "stale", "expired", "cancelled"].includes(String(decoded.status))) throw new Error("Stored Creator import result invalid"); return decoded; }
export function prepareImport(db: DatabaseSync, input: TransferBundle) {
  const bundle = validateBundle(input), at = Math.floor(Date.now() / 1000);
  db.prepare("UPDATE creator_import_operations SET bundle_json=NULL,status='completed',result_json=json_object('status','expired','operation_id',id,'next_action',?),updated_at=? WHERE status='prepared' AND expires_at<=?").run(prepareAgain, at, at);
  const digests = JSON.stringify([bundle.content_sha256, ...historicalDigests(bundle)]);
  const imported = db.prepare("SELECT * FROM creator_import_receipts WHERE content_sha256 IN (SELECT value FROM json_each(?)) ORDER BY imported_at DESC LIMIT 1").get(digests);
  if (imported) return { status: "already_imported" as const, receipt: receipt(imported) };
  const existing = db.prepare("SELECT * FROM creator_import_operations WHERE content_sha256 IN (SELECT value FROM json_each(?)) AND status='prepared' AND expires_at>? ORDER BY created_at DESC LIMIT 1").get(digests, at);
  if (existing) { const op = operationSchema.parse(existing); return { status: "confirmation_required" as const, operation_id: op.id, content_sha256: op.content_sha256, impact: impactSchema.parse(parseJson(op.impact_json)), expires_at: op.expires_at }; }
  const basis = loadImportBasis(db, bundle), planned = planImport(basis, bundle);
  if (planned.conflicts.length) return { status: "blocked" as const, content_sha256: bundle.content_sha256, conflict_count: planned.conflicts.length, conflicts: planned.conflicts.slice(0, 50) };
  if (!planned.plan) throw new Error("Creator import planner returned no plan");
  const operation_id = randomUUID(), expires_at = at + 1800, impact = planned.plan.impact;
  db.prepare("INSERT INTO creator_import_operations (id,content_sha256,bundle_json,plan_fingerprint,impact_json,status,result_json,expires_at,created_at,updated_at) VALUES (?,?,?,?,?,'prepared',NULL,?,?,?)").run(operation_id, bundle.content_sha256, encodeBundle(bundle), planFingerprint(planned.plan, basis), JSON.stringify(impact), expires_at, at, at);
  return { status: "confirmation_required" as const, operation_id, content_sha256: bundle.content_sha256, impact, expires_at };
}
export function cancelImport(db: DatabaseSync, input: unknown) {
  const { operation_id } = importOperationInput.parse(input), op = load(db, operation_id);
  if (op.status === "completed") return terminal(op.result_json);
  const result = { status: "cancelled" as const, operation_id }; complete(db, operation_id, result, Math.floor(Date.now() / 1000)); return result;
}
export function confirmImport(db: DatabaseSync, input: unknown): { result: ReturnType<typeof terminal>; changed: boolean } {
  const { operation_id } = importOperationInput.parse(input), op = load(db, operation_id), at = Math.floor(Date.now() / 1000);
  if (op.status === "completed") return { result: terminal(op.result_json), changed: false };
  if (op.expires_at <= at) { const result = { status: "expired", operation_id, next_action: prepareAgain }; complete(db, operation_id, result, at); return { result, changed: false }; }
  const imported = db.prepare("SELECT * FROM creator_import_receipts WHERE content_sha256=?").get(op.content_sha256);
  if (imported) { const current = receipt(imported), result = { status: "unchanged", operation_id, receipt: current }; complete(db, operation_id, result, at); return { result, changed: false }; }
  if (op.bundle_json === null) throw new Error("Prepared Creator import has no bundle");
  const bundle = readStagedBundle(op.bundle_json), basis = loadImportBasis(db, bundle), planned = planImport(basis, bundle), plan = planned.plan;
  if (bundle.content_sha256 !== op.content_sha256 || planned.conflicts.length || !plan || planFingerprint(plan, basis) !== op.plan_fingerprint || JSON.stringify(plan.impact) !== op.impact_json) { const result = { status: "stale", operation_id, next_action: prepareAgain }; complete(db, operation_id, result, at); return { result, changed: false }; }
  applyImport(db, bundle, plan, at);
  const current = { content_sha256: bundle.content_sha256, format_version: bundle.format_version, creator_count: bundle.creator_count, impact: plan.impact, imported_at: at }, changed = changesGraph(plan.impact);
  db.prepare("INSERT INTO creator_import_receipts (content_sha256,format,format_version,profile,snapshot_kind,creator_count,impact_json,source_id,imported_at) VALUES (?,?,?,'portable_public','full',?,?,NULL,?)").run(bundle.content_sha256, bundle.format, bundle.format_version, bundle.creator_count, JSON.stringify(plan.impact), at);
  const result = { status: changed ? "imported" : "unchanged", operation_id, receipt: current }; complete(db, operation_id, result, at); return { result, changed };
}
