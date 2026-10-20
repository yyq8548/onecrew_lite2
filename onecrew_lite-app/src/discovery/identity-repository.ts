import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { loadDiscoveryReviewBasis } from "./review-basis.ts";
import type { DiscoveryReviewBasis } from "./review-material.ts";
import { buildIdentityPair } from "./identity-material.ts";
import { identityGetInput, identityGroupsInput, identityPairInput, identityRevokeInput, identitySubmitInput, identityTextValid, type IdentityPairInput, type IdentityRelationship } from "./identity-contracts.ts";
import { decodeIdentityReceipt, identityRevocationHash, identitySubmissionHash, prepareIdentityReview, prepareIdentityRevocation, type PreparedIdentityReview, type PreparedIdentityRevocation, type StoredIdentityReview } from "./identity-review.ts";
import { validateIdentityAddition } from "./identity-relationships.ts";
import { loadDiscoveryLibraryBasis } from "./library-basis.ts";
import { loadIdentityGroupsBasis } from "./identity-groups-basis.ts";
import { buildIdentityGroups } from "./identity-groups.ts";

const stored = z.object({ request_hash: z.string(), receipt_json: z.string() });
export function loadIdentityPairBasis(db: DatabaseSync, input: IdentityPairInput): [DiscoveryReviewBasis, DiscoveryReviewBasis] {
  if (!db.isTransaction) throw new Error("identity_pair_requires_snapshot");
  return [loadDiscoveryReviewBasis(db, input.campaign_id, input.expected_revision, input.left_search_hit_id), loadDiscoveryReviewBasis(db, input.campaign_id, input.expected_revision, input.right_search_hit_id)];
}
export function identityTransaction<T>(db: DatabaseSync, write: boolean, operation: () => T): T {
  db.exec(write ? "BEGIN IMMEDIATE" : "BEGIN DEFERRED");
  try { const result = operation(); db.exec("COMMIT"); return result; }
  catch (error) {
    if (db.isTransaction) {
      try { db.exec("ROLLBACK"); }
      catch (rollback) { throw new AggregateError([error, rollback], "Identity operation and rollback failed"); }
    }
    throw error;
  }
}
export class DiscoveryIdentityRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #find(campaign: string, decision: string): StoredIdentityReview | null { const row = this.#db.prepare("SELECT request_hash,receipt_json FROM discovery_identity_reviews WHERE discovery_campaign_id=? AND decision_id=?").get(campaign, decision); return row === undefined ? null : stored.parse(row); }
  material(raw: unknown) { const input = identityPairInput.parse(raw); return identityTransaction(this.#db, false, () => buildIdentityPair(input, loadIdentityPairBasis(this.#db, input))); }
  groups(raw: unknown) {
    const input = identityGroupsInput.parse(raw);
    return identityTransaction(this.#db, false, () => buildIdentityGroups(loadIdentityGroupsBasis(this.#db, loadDiscoveryLibraryBasis(this.#db, input.campaign_id, input.expected_revision))));
  }
  get(account: string, raw: unknown) {
    const input = identityGetInput.parse(raw);
    if (!identityTextValid(account, 256)) throw new Error("identity_review_input_invalid");
    const record = this.#find(input.campaign_id, input.decision_id);
    return record === null ? null : decodeIdentityReceipt(record, account);
  }
  submit(account: string, raw: unknown) {
    const input = identitySubmitInput.parse(raw), hash = identitySubmissionHash(account, input);
    const source = identityTransaction(this.#db, false, () => {
      const record = this.#find(input.pair.campaign_id, input.decision_id);
      return record === null ? { kind: "basis" as const, basis: loadIdentityPairBasis(this.#db, input.pair) } : { kind: "committed" as const, record };
    });
    if (source.kind === "committed") return decodeIdentityReceipt(source.record, account, hash);
    const prepared = prepareIdentityReview(account, input, hash, source.basis);
    return decodeIdentityReceipt(this.#commitReview(prepared), account, hash);
  }
  revoke(account: string, raw: unknown) {
    const input = identityRevokeInput.parse(raw), hash = identityRevocationHash(account, input);
    const source = identityTransaction(this.#db, false, () => {
      const record = this.#find(input.campaign_id, input.decision_id);
      if (record !== null) return { kind: "committed" as const, record };
      const original = this.#find(input.campaign_id, input.revokes_decision_id);
      if (original === null) throw new Error("identity_review_not_found");
      const material = z.object({ material_json: z.string() }).parse(this.#db.prepare("SELECT material_json FROM discovery_identity_reviews WHERE discovery_campaign_id=? AND decision_id=?").get(input.campaign_id, input.revokes_decision_id));
      return { kind: "original" as const, record: original, material: material.material_json };
    });
    if (source.kind === "committed") return decodeIdentityReceipt(source.record, account, hash);
    const prepared = prepareIdentityRevocation(account, input, hash, source.record, source.material);
    return decodeIdentityReceipt(this.#commitRevocation(prepared), account, hash);
  }
  #insert(prepared: PreparedIdentityReview | PreparedIdentityRevocation): StoredIdentityReview {
    const receipt = prepared.receipt;
    this.#db.prepare("INSERT INTO discovery_identity_reviews (discovery_campaign_id,decision_id,operator_account_id,criteria_revision,kind,left_candidate_id,right_candidate_id,material_hash,request_hash,input_json,material_json,receipt_json,revokes_decision_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(receipt.campaign_id, receipt.decision_id, receipt.operator_account_id, receipt.criteria_revision, receipt.kind, receipt.candidate_ids[0], receipt.candidate_ids[1], receipt.material_hash, prepared.request_hash, prepared.input_json, prepared.material_json, prepared.receipt_json, receipt.revokes_decision_id, receipt.created_at);
    return { request_hash: prepared.request_hash, receipt_json: prepared.receipt_json };
  }
  #commitReview(prepared: PreparedIdentityReview): StoredIdentityReview {
    return identityTransaction(this.#db, true, () => {
      const receipt = prepared.receipt, existing = this.#find(receipt.campaign_id, receipt.decision_id);
      if (existing !== null) { if (existing.request_hash !== prepared.request_hash) throw new Error("identity_review_idempotency_conflict"); return existing; }
      const pair = { campaign_id: receipt.campaign_id, expected_revision: receipt.criteria_revision, left_search_hit_id: prepared.basis[0].hit.id, right_search_hit_id: prepared.basis[1].hit.id };
      if (!isDeepStrictEqual(loadIdentityPairBasis(this.#db, pair), prepared.basis)) throw new Error("identity_review_material_changed");
      const rows = this.#db.prepare("SELECT r.left_candidate_id,r.right_candidate_id,r.kind FROM discovery_identity_reviews r WHERE r.discovery_campaign_id=? AND r.criteria_revision=? AND r.kind IN ('same_author','different_authors') AND NOT EXISTS(SELECT 1 FROM discovery_identity_reviews x WHERE x.discovery_campaign_id=r.discovery_campaign_id AND x.kind='revoke' AND x.revokes_decision_id=r.decision_id) LIMIT 10001").all(receipt.campaign_id, receipt.criteria_revision);
      if (rows.length > 10_000) throw new Error("identity_review_ledger_too_large");
      const edges: IdentityRelationship[] = rows.map(row => { const edge = z.object({ left_candidate_id: z.string(), right_candidate_id: z.string(), kind: z.string() }).parse(row); return [edge.left_candidate_id, edge.right_candidate_id, edge.kind]; });
      validateIdentityAddition(edges, receipt.kind, receipt.candidate_ids);
      return this.#insert(prepared);
    });
  }
  #commitRevocation(prepared: PreparedIdentityRevocation): StoredIdentityReview {
    return identityTransaction(this.#db, true, () => {
      const receipt = prepared.receipt, existing = this.#find(receipt.campaign_id, receipt.decision_id);
      if (existing !== null) { if (existing.request_hash !== prepared.request_hash) throw new Error("identity_review_idempotency_conflict"); return existing; }
      const row = this.#db.prepare("SELECT current_revision,phase FROM discovery_campaigns WHERE id=?").get(receipt.campaign_id);
      if (row === undefined) throw new Error("campaign_not_found");
      const current = z.object({ current_revision: z.number().int().min(1).max(4_294_967_295), phase: z.string() }).parse(row);
      if (current.current_revision !== receipt.criteria_revision) throw new Error("identity_review_revision_changed");
      if (current.phase !== "active") throw new Error("identity_review_campaign_inactive");
      const target = receipt.revokes_decision_id;
      if (target === null) throw new Error("identity_review_input_invalid");
      const original = this.#find(receipt.campaign_id, target);
      if (original === null) throw new Error("identity_review_not_found");
      if (original.request_hash !== prepared.original_request_hash || original.receipt_json !== prepared.original_receipt_json) throw new Error("identity_review_original_changed");
      if (this.#db.prepare("SELECT 1 FROM discovery_identity_reviews WHERE discovery_campaign_id=? AND kind='revoke' AND revokes_decision_id=? LIMIT 1").get(receipt.campaign_id, target) !== undefined) throw new Error("identity_review_already_revoked");
      return this.#insert(prepared);
    });
  }
}
