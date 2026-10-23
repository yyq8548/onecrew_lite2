import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { assertValidatedCreatorConnection } from "../storage/creator-schema.ts";
import { observeDiscoveryReviewContacts } from "./contact-repository.ts";
import { discoveryMaterialDigest } from "./library-material.ts";
import { loadDiscoveryReviewBasis } from "./review-basis.ts";
import { discoveryReviewReceipt, discoveryReviewRequestHash, prepareDiscoveryReview, submitDiscoveryReviewInput, type DiscoveryReviewReceipt, type PreparedDiscoveryReview } from "./review-submission.ts";

const bounded = (maximum: number) => z.string().refine(value => value.isWellFormed() && !!value.trim() && Buffer.byteLength(value) <= maximum);
export type DiscoveryReviewPreparation = { state: "committed"; receipt: DiscoveryReviewReceipt } | { state: "prepared"; prepared: PreparedDiscoveryReview };
export class DiscoveryReviewRepository {
  readonly #database: DatabaseSync;
  readonly #issued = new WeakMap<PreparedDiscoveryReview, string>();
  constructor(database: DatabaseSync) { assertValidatedCreatorConnection(database); this.#database = database; }
  #transaction<T>(write: boolean, body: () => T): T {
    this.#database.exec(write ? "BEGIN IMMEDIATE" : "BEGIN DEFERRED"); let committed = false;
    try { const value = body(); this.#database.exec("COMMIT"); committed = true; return value; }
    finally { if (!committed) this.#database.exec("ROLLBACK"); }
  }
  #find(account: string, campaign: string, request: string) {
    const raw = this.#database.prepare("SELECT request_hash,assessment_json FROM discovery_evidence_reviews WHERE operator_account_id=?1 AND discovery_campaign_id=?2 AND request_id=?3").get(account, campaign, request);
    return raw ? z.object({ request_hash: z.string(), assessment_json: z.string() }).parse(raw) : null;
  }
  #decode(stored: { request_hash: string; assessment_json: string }, expectedHash?: string): DiscoveryReviewReceipt {
    if (expectedHash !== undefined && stored.request_hash !== expectedHash) throw new Error("review_idempotency_conflict");
    return discoveryReviewReceipt.parse(parseApplicationJson(Buffer.from(stored.assessment_json)));
  }
  get(account: string, campaign: string, request: string): DiscoveryReviewReceipt | null {
    bounded(256).parse(account); bounded(128).parse(campaign); bounded(128).parse(request);
    return this.#transaction(false, () => { const stored = this.#find(account, campaign, request); return stored ? this.#decode(stored) : null; });
  }
  prepareSubmission(account: string, raw: unknown): DiscoveryReviewPreparation {
    const input = submitDiscoveryReviewInput.parse(raw), hash = discoveryReviewRequestHash(account, input);
    const read = this.#transaction(false, () => {
      const stored = this.#find(account, input.campaign_id, input.request_id);
      return stored ? { state: "committed" as const, receipt: this.#decode(stored, hash) } : { state: "basis" as const, basis: loadDiscoveryReviewBasis(this.#database, input.campaign_id, input.expected_revision, input.search_hit_id) };
    });
    if (read.state === "committed") return read;
    const prepared = prepareDiscoveryReview(account, input, read.basis);
    this.#issued.set(prepared, discoveryMaterialDigest(prepared));
    return { state: "prepared", prepared };
  }
  commitPrepared(prepared: PreparedDiscoveryReview): DiscoveryReviewReceipt {
    const fingerprint = this.#issued.get(prepared);
    if (fingerprint === undefined || fingerprint !== discoveryMaterialDigest(prepared)) throw new Error("review_preparation_not_owned_or_changed");
    return this.#transaction(true, () => {
      const receipt = prepared.receipt, stored = this.#find(receipt.operator_account_id, receipt.campaign_id, receipt.request_id);
      if (stored) return this.#decode(stored, prepared.request_hash);
      if (!isDeepStrictEqual(loadDiscoveryReviewBasis(this.#database, prepared.basis.campaign_id, prepared.basis.revision, prepared.basis.hit.id), prepared.basis)) throw new Error("review_material_changed");
      const inserted = this.#database.prepare(`INSERT INTO discovery_evidence_reviews
        (id,operator_account_id,discovery_campaign_id,criteria_revision,search_hit_id,candidate_id,native_author_id,profile_ref,
         request_id,request_hash,material_hash,material_json,policy_version,reviewer_json,findings_json,verdict,assessment_json,created_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)`).run(receipt.review_id, receipt.operator_account_id, receipt.campaign_id, receipt.criteria_revision, receipt.search_hit_id, receipt.candidate_id, receipt.native_author_id, receipt.profile_ref, receipt.request_id, prepared.request_hash, receipt.material_hash, prepared.material_json, receipt.policy_version, prepared.reviewer_json, prepared.findings_json, receipt.verdict, prepared.receipt_json, receipt.created_at);
      if (inserted.changes !== 1) throw new Error("review_ledger_write_failed");
      const changed = this.#database.prepare(`UPDATE discovery_candidates SET verdict=?1,evidence_json=?2,updated_at=?3
        WHERE id=?4 AND discovery_campaign_id=?5 AND criteria_revision=?6 AND source_platform=?9
          AND platform_account_id=?7 AND source_profile_ref=?8`).run(receipt.verdict, prepared.evidence_json, receipt.created_at, receipt.candidate_id, receipt.campaign_id, receipt.criteria_revision, receipt.native_author_id, receipt.profile_ref, prepared.basis.hit.platform);
      if (changed.changes !== 1) throw new Error("review_material_changed");
      observeDiscoveryReviewContacts(this.#database, prepared);
      return this.#decode({ request_hash: prepared.request_hash, assessment_json: prepared.receipt_json }, prepared.request_hash);
    });
  }
  submit(account: string, raw: unknown): DiscoveryReviewReceipt { const value = this.prepareSubmission(account, raw); return value.state === "committed" ? value.receipt : this.commitPrepared(value.prepared); }
}
