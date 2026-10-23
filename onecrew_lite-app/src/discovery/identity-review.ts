import { parseApplicationJson } from "../contracts/application-json.ts";
import { discoveryMaterialDigest } from "./library-material.ts";
import { type DiscoveryReviewBasis } from "./review-material.ts";
import { buildIdentityPair, encodeIdentity, identityAuthorPlatform, validateIdentityFindings } from "./identity-material.ts";
import { IDENTITY_REVIEW_POLICY, IDENTITY_REVOCATION_POLICY, identityReviewReceipt, identityTextValid, type IdentityReviewReceipt, type IdentitySubmitInput, type IdentityRevokeInput } from "./identity-contracts.ts";

export interface StoredIdentityReview { request_hash: string; receipt_json: string }
export interface PreparedIdentityReview { basis: readonly [DiscoveryReviewBasis, DiscoveryReviewBasis]; request_hash: string; input_json: string; material_json: string; receipt_json: string; receipt: IdentityReviewReceipt }
export interface PreparedIdentityRevocation { original_request_hash: string; original_receipt_json: string; request_hash: string; input_json: string; material_json: string; receipt_json: string; receipt: IdentityReviewReceipt }
function clock(): number { const value = Math.floor(Date.now() / 1000); if (!Number.isSafeInteger(value) || value < 0) throw new Error("identity_review_clock_invalid"); return value; }
function trustedAccount(account: string): void { if (!identityTextValid(account, 256) || /\p{Cc}/u.test(account)) throw new Error("identity_review_input_invalid"); }
function reviewerInput(account: string, input: IdentitySubmitInput | IdentityRevokeInput): void {
  trustedAccount(account);
  if (!identityTextValid(input.reviewer.label, 256) || !identityTextValid(input.rationale, 4000)) throw new Error("identity_review_input_invalid");
  encodeIdentity(input, 65_536);
}
export function identitySubmissionHash(account: string, input: IdentitySubmitInput): string { reviewerInput(account, input); return discoveryMaterialDigest([account, IDENTITY_REVIEW_POLICY, input]); }
export function identityRevocationHash(account: string, input: IdentityRevokeInput): string { reviewerInput(account, input); return discoveryMaterialDigest([account, IDENTITY_REVOCATION_POLICY, input]); }
export function decodeIdentityReceipt(stored: StoredIdentityReview, account: string, hash?: string): IdentityReviewReceipt {
  if (hash !== undefined && hash !== stored.request_hash) throw new Error("identity_review_idempotency_conflict");
  let value: unknown;
  try { value = parseApplicationJson(stored.receipt_json); }
  catch (cause) { throw new Error("identity_review_receipt_invalid", { cause }); }
  const parsed = identityReviewReceipt.safeParse(value);
  if (!parsed.success) throw new Error("identity_review_receipt_invalid");
  if (parsed.data.operator_account_id !== account) throw new Error("identity_review_account_mismatch");
  return parsed.data;
}
export function prepareIdentityReview(account: string, input: IdentitySubmitInput, hash: string, basis: readonly [DiscoveryReviewBasis, DiscoveryReviewBasis]): PreparedIdentityReview {
  const pair = buildIdentityPair(input.pair, basis);
  if (pair.material_hash !== input.material_hash) throw new Error("identity_review_material_changed");
  validateIdentityFindings(input, pair);
  const receipt: IdentityReviewReceipt = {
    campaign_id: input.pair.campaign_id, criteria_revision: input.pair.expected_revision, decision_id: input.decision_id,
    operator_account_id: account, policy_version: IDENTITY_REVIEW_POLICY, kind: input.decision, revokes_decision_id: null,
    candidate_ids: [pair.accounts[0].candidate_id, pair.accounts[1].candidate_id], native_author_ids: [pair.accounts[0].native_author_id, pair.accounts[1].native_author_id],
    platforms: [identityAuthorPlatform(pair.accounts[0]), identityAuthorPlatform(pair.accounts[1])], material_hash: pair.material_hash, created_at: clock(),
  };
  return { basis, request_hash: hash, input_json: encodeIdentity(input, 65_536), material_json: encodeIdentity(pair, 3_145_728), receipt_json: encodeIdentity(receipt, 65_536), receipt };
}
export function prepareIdentityRevocation(account: string, input: IdentityRevokeInput, hash: string, stored: StoredIdentityReview, material_json: string): PreparedIdentityRevocation {
  const original = decodeIdentityReceipt(stored, account);
  if (original.campaign_id !== input.campaign_id || original.decision_id !== input.revokes_decision_id) throw new Error("identity_review_receipt_invalid");
  if (original.kind === "revoke") throw new Error("identity_review_cannot_revoke_revocation");
  const receipt: IdentityReviewReceipt = { ...original, criteria_revision: input.expected_revision, decision_id: input.decision_id, policy_version: IDENTITY_REVOCATION_POLICY, kind: "revoke", revokes_decision_id: input.revokes_decision_id, created_at: clock() };
  return { original_request_hash: stored.request_hash, original_receipt_json: stored.receipt_json, request_hash: hash, input_json: encodeIdentity(input, 65_536), material_json, receipt_json: encodeIdentity(receipt, 65_536), receipt };
}
