import { createHash } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { candidateJson, type CandidateIntent } from "./candidate-contracts.ts";
import { candidateAdministrator, validateCandidateSource } from "./candidate-provenance.ts";
import { insertCandidateCreator, persistSelectedCandidate } from "./candidate-persistence.ts";
import { activeCreator, resolveIdentity } from "./contacts.ts";
import { creatorDetail } from "./detail.ts";
import { text, trim } from "./contracts.ts";
import type { SelectedCandidateBatch, SelectedCandidateRow } from "./candidate-selection.ts";
import { candidateSelectedPayload } from "./candidate-selection.ts";

export type CandidateRowOutcome = { status: "saved"; row_id: string; creator: ReturnType<typeof creatorDetail>; created: boolean; outreach_context: null | { creator_id: string; contact_route_id: string; operator_account_ref_id: string } } | { status: "skipped"; row_id: string; reason: "existing_creator" | "duplicate_selection" } | { status: "failed"; row_id: string; error: string };
export interface CandidateBatchResult { outcomes: CandidateRowOutcome[] }
export class CandidateCommitUnknownError extends AggregateError {}
const receiptAction = "creator_save_batch_v1";
function saveRow(db: DatabaseSync, row: SelectedCandidateRow, intent: CandidateIntent, batchTags: Map<string, string[]>): CandidateRowOutcome {
  const input = row.input, source = validateCandidateSource(db, input); if (!source.ok) throw new Error(source.message);
  const identity = resolveIdentity(db, { candidate: input.account }), administrator = candidateAdministrator(input.provenance);
  if (identity.status === "possible_match" || identity.status === "conflicting_matches") throw new Error("resolve ambiguous Creator identity before saving");
  if (identity.status === "exact_existing_creator" && administrator?.source.kind === "creator_contact_route" && administrator.source.source_creator_id === identity.creator_id) throw new Error("a community administrator cannot be merged into the source Creator");
  if (identity.status === "exact_existing_creator" && intent === "add_new") return { status: "skipped", row_id: input.row_id, reason: "existing_creator" };
  const created = identity.status === "new", creator = identity.status === "new" ? insertCandidateCreator(db, input) : text.parse(identity.creator_id);
  if (!created) activeCreator(db, creator);
  return { status: "saved", row_id: input.row_id, created, ...persistSelectedCandidate(db, creator, row, batchTags) };
}
export function executeCandidateRows(db: DatabaseSync, prepared: SelectedCandidateBatch, executionKey: string, decodeReceipt: (value: unknown) => CandidateBatchResult): { result: CandidateBatchResult; changed: boolean } {
  if (!trim(executionKey) || Buffer.byteLength(executionKey) > 256) throw new Error("Creator save execution key must be bounded and nonempty");
  const fingerprint = createHash("sha256").update(candidateJson([receiptAction, candidateSelectedPayload(prepared)])).digest("hex");
  const loadReceipt = (): CandidateBatchResult | null => {
    const row = db.prepare("SELECT action,request_fingerprint,state,result_json FROM idempotency_records WHERE key=?").get(executionKey); if (!row) return null;
    if (row.action !== receiptAction || row.request_fingerprint !== fingerprint || row.state !== "completed") throw new Error("Creator save execution key conflicts with another payload or action");
    return decodeReceipt(parseStoredJson(text.parse(row.result_json)));
  };
  db.exec("BEGIN IMMEDIATE"); let committed = false;
  try {
    const existing = loadReceipt(); if (existing) { db.exec("COMMIT"); committed = true; return { result: existing, changed: false }; }
    const outcomes: CandidateRowOutcome[] = []; let batchTags = new Map<string, string[]>();
    for (const row of prepared.rows) {
      db.exec("SAVEPOINT creator_save_row"); const savedTags = structuredClone(batchTags);
      let outcome: CandidateRowOutcome;
      try { outcome = saveRow(db, row, prepared.intent, batchTags); }
      catch (error) {
        try { db.exec("ROLLBACK TO SAVEPOINT creator_save_row; RELEASE SAVEPOINT creator_save_row"); }
        catch (rollback) { throw new AggregateError([error, rollback], "Creator save row rollback failed"); }
        batchTags = savedTags;
        if (!(error instanceof Error)) throw error;
        outcomes.push({ status: "failed", row_id: row.input.row_id, error: error.message });
        continue;
      }
      db.exec("RELEASE SAVEPOINT creator_save_row"); outcomes.push(outcome);
    }
    for (const outcome of outcomes) if (outcome.status === "saved") outcome.creator = creatorDetail(db, outcome.creator.creator.id);
    const result = { outcomes }, at = Math.floor(Date.now() / 1000);
    db.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,result_json,created_at,updated_at) VALUES(?,?,?,'completed',?,?,?)").run(executionKey, receiptAction, fingerprint, candidateJson(result), at, at);
    try { db.exec("COMMIT"); committed = true; }
    catch (error) {
      try { if (db.isTransaction) db.exec("ROLLBACK"); db.exec("BEGIN DEFERRED"); const receipt = loadReceipt(); db.exec("COMMIT"); if (receipt) { committed = true; return { result: receipt, changed: true }; } }
      catch (reconcile) { throw new CandidateCommitUnknownError([error, reconcile], "Creator save commit outcome is unknown; receipt reconciliation failed"); }
      throw new CandidateCommitUnknownError([error], "Creator save commit outcome is unknown; retry the same execution key to reconcile its receipt");
    }
    return { result, changed: outcomes.some(row => row.status === "saved") };
  } finally { if (!committed && db.isTransaction) db.exec("ROLLBACK"); }
}
