import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { identitySubmitInput, identityU32 } from "./identity-contracts.ts";
import type { IdentityGroupsBasis, IdentityGroupReviewSource } from "./identity-groups.ts";
import type { DiscoveryLibraryBasis } from "./library-material.ts";
import type { DiscoveryReviewBasis } from "./review-material.ts";
import { loadDiscoveryReviewBasis } from "./review-basis.ts";

const MAX_RECORDS = 10_000;
const MAX_BYTES = 32 * 1024 * 1024;
const bytes = (value: unknown) => Buffer.byteLength(stringifyApplicationJson(value));
function sourceBytes(pair: readonly [DiscoveryReviewBasis, DiscoveryReviewBasis]): number {
  return pair.reduce((size, basis) => {
    const profile = basis.native_profile === null ? null : { ...(basis.native_profile.web_text != null ? { web_text: basis.native_profile.web_text } : {}), profile_url: basis.native_profile.profile_url, observed_at: basis.native_profile.observed_at, ...(basis.native_profile.bio != null ? { bio: basis.native_profile.bio } : {}), ...(basis.native_profile.self_declared_location != null ? { self_declared_location: basis.native_profile.self_declared_location } : {}) };
    return size + bytes([basis.campaign_id, basis.revision, basis.criteria_json, basis.scopes_json, basis.hit, basis.candidate_id, basis.native_author_id, profile, basis.evidence_json, basis.transcript, basis.source_description, basis.native_content, basis.pages]);
  }, 0);
}
export function loadIdentityGroupsBasis(database: DatabaseSync, library: DiscoveryLibraryBasis): IdentityGroupsBasis {
  if (!database.isTransaction) throw new Error("identity_groups_requires_snapshot");
  const campaign = library.campaign_id, revision = library.revision;
  const rows = library.rows.map(row => ({ hit: row.hit, candidate: row.candidate === null ? null : { id: row.candidate.id, ...(row.candidate.followers != null ? { followers: row.candidate.followers } : {}), ...(row.candidate.native_platform != null ? { native_platform: row.candidate.native_platform } : {}), native_author_id: row.candidate.native_author_id, verdict: row.candidate.verdict, evidence_json: row.candidate.evidence_json, identity_receipt: row.candidate.identity_receipt, manual_approved_at: row.candidate.manual_approved_at, ...(row.candidate.contacts.length ? { contacts: row.candidate.contacts } : {}) } }));
  let totalBytes = Buffer.byteLength(library.criteria_json) + Buffer.byteLength(library.scopes_json) + bytes(rows);
  if (totalBytes > MAX_BYTES) throw new Error("identity_groups_snapshot_too_large");
  const native_details = new Map<string, readonly [string, string]>();
  for (const raw of database.prepare("SELECT id,display_name,profile_url FROM discovery_candidates WHERE discovery_campaign_id=? AND criteria_revision=? ORDER BY id LIMIT 10001").all(campaign, revision)) {
    const row = z.object({ id: z.string(), display_name: z.string(), profile_url: z.string() }).parse(raw);
    totalBytes += Buffer.byteLength(row.id) + Buffer.byteLength(row.display_name) + Buffer.byteLength(row.profile_url);
    if (native_details.size === MAX_RECORDS || totalBytes > MAX_BYTES) throw new Error("identity_groups_snapshot_too_large");
    native_details.set(row.id, [row.display_name, row.profile_url]);
  }
  const statement = database.prepare("SELECT r.decision_id,r.operator_account_id,r.criteria_revision,r.kind,r.left_candidate_id,r.right_candidate_id,r.material_hash,r.request_hash,r.input_json,r.receipt_json,r.revokes_decision_id,r.created_at,EXISTS(SELECT 1 FROM discovery_identity_reviews x WHERE x.discovery_campaign_id=r.discovery_campaign_id AND x.kind='revoke' AND x.revokes_decision_id=r.decision_id) AS revoked FROM discovery_identity_reviews r WHERE r.discovery_campaign_id=? ORDER BY r.decision_id LIMIT 10001");
  statement.setReadBigInts(true);
  const reviews: IdentityGroupReviewSource[] = [];
  for (const raw of statement.all(campaign)) {
    const row = z.object({ decision_id: z.string(), operator_account_id: z.string(), criteria_revision: identityU32, kind: z.string(), left_candidate_id: z.string(), right_candidate_id: z.string(), material_hash: z.string(), request_hash: z.string(), input_json: z.string(), receipt_json: z.string(), revokes_decision_id: z.string().nullable(), created_at: z.union([z.number().int().safe(), z.bigint()]), revoked: z.number().int().min(0).max(1) }).parse(Object.fromEntries(Object.entries(raw).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value])));
    totalBytes += Buffer.byteLength(row.decision_id) + Buffer.byteLength(row.operator_account_id) + Buffer.byteLength(row.input_json) + Buffer.byteLength(row.receipt_json);
    if (reviews.length === MAX_RECORDS || totalBytes > MAX_BYTES) throw new Error("identity_groups_snapshot_too_large");
    const source: IdentityGroupReviewSource = { decision_id: row.decision_id, account: row.operator_account_id, revision: row.criteria_revision, kind: row.kind, candidate_ids: [row.left_candidate_id, row.right_candidate_id], material_hash: row.material_hash, request_hash: row.request_hash, input_json: row.input_json, receipt_json: row.receipt_json, revokes_decision_id: row.revokes_decision_id, created_at: row.created_at, current_pair: null };
    if (row.revoked === 0 && source.revision === revision && ["same_author", "different_authors", "insufficient"].includes(source.kind)) {
      const input = identitySubmitInput.parse(parseApplicationJson(source.input_json));
      if (input.pair.campaign_id !== campaign || input.pair.expected_revision !== revision) throw new Error("identity_groups_ledger_invalid");
      let pair: [DiscoveryReviewBasis, DiscoveryReviewBasis] | null = null;
      try { pair = [loadDiscoveryReviewBasis(database, campaign, revision, input.pair.left_search_hit_id), loadDiscoveryReviewBasis(database, campaign, revision, input.pair.right_search_hit_id)]; }
      catch (error) {
        if (!(error instanceof Error) || !["review_source_unavailable", "review_material_author_binding_unavailable", "review_native_profile_binding_unavailable", "review_native_content_binding_unavailable"].includes(error.message)) throw error;
      }
      if (pair !== null) {
        totalBytes += sourceBytes(pair);
        if (totalBytes > MAX_BYTES) throw new Error("identity_groups_snapshot_too_large");
        source.current_pair = pair;
      }
    }
    reviews.push(source);
  }
  return { library, native_details, reviews };
}
