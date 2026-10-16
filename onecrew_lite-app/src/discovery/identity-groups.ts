import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { buildDiscoveryLibrarySnapshot, discoveryMaterialDigest, type DiscoveryLibraryBasis } from "./library-material.ts";
import type { DiscoveryReviewBasis } from "./review-material.ts";
import { buildIdentityPair, identityAuthorPlatform, validateIdentityFindings } from "./identity-material.ts";
import { IDENTITY_REVIEW_POLICY, IDENTITY_REVOCATION_POLICY, identityRevokeInput, identitySubmitInput, identityReviewReceipt, type IdentityRelationship, type IdentityReviewReceipt } from "./identity-contracts.ts";
import { identityRevocationHash, identitySubmissionHash } from "./identity-review.ts";
import { identityComponents } from "./identity-relationships.ts";

export interface IdentityGroupReviewSource {
  decision_id: string; account: string; revision: number; kind: string; candidate_ids: [string, string]; material_hash: string; request_hash: string; input_json: string; receipt_json: string; revokes_decision_id: string | null; created_at: number | bigint; current_pair: readonly [DiscoveryReviewBasis, DiscoveryReviewBasis] | null;
}
export interface IdentityGroupsBasis { library: DiscoveryLibraryBasis; native_details: ReadonlyMap<string, readonly [name: string, url: string]>; reviews: readonly IdentityGroupReviewSource[] }
interface IdentityMember { candidate_id: string; platform: string; native_author_id: string; display_name: string; profile_url: string; verdict: string; reasons: string[]; sources: ReturnType<typeof buildDiscoveryLibrarySnapshot>["rows"] }
type ReviewState = "current" | "evidence_changed" | "revoked" | "previous_criteria";
const order = (left: string, right: string): number => Buffer.compare(Buffer.from(left), Buffer.from(right));
function ledgerParse<S extends z.ZodType>(schema: S, json: string): z.output<S> {
  let value: unknown;
  try { value = parseApplicationJson(json); }
  catch (cause) { throw new Error("identity_groups_ledger_invalid", { cause }); }
  const parsed = schema.safeParse(value);
  if (!parsed.success) throw new Error("identity_groups_ledger_invalid");
  return parsed.data;
}
function decodeLedger(source: IdentityGroupReviewSource, campaign: string): IdentityReviewReceipt {
  const receipt = ledgerParse(identityReviewReceipt, source.receipt_json);
  if (receipt.campaign_id !== campaign || receipt.decision_id !== source.decision_id || receipt.operator_account_id !== source.account || receipt.criteria_revision !== source.revision || receipt.kind !== source.kind || !isDeepStrictEqual(receipt.candidate_ids, source.candidate_ids) || receipt.material_hash !== source.material_hash || receipt.revokes_decision_id !== source.revokes_decision_id || BigInt(receipt.created_at) !== BigInt(source.created_at)) throw new Error("identity_groups_ledger_invalid");
  if (receipt.kind === "revoke") {
    const input = ledgerParse(identityRevokeInput, source.input_json);
    if (input.campaign_id !== campaign || input.expected_revision !== source.revision || input.decision_id !== source.decision_id || input.revokes_decision_id !== source.revokes_decision_id || receipt.policy_version !== IDENTITY_REVOCATION_POLICY || identityRevocationHash(source.account, input) !== source.request_hash) throw new Error("identity_groups_ledger_invalid");
  } else {
    const input = ledgerParse(identitySubmitInput, source.input_json);
    if (input.pair.campaign_id !== campaign || input.pair.expected_revision !== source.revision || input.decision_id !== source.decision_id || input.material_hash !== source.material_hash || input.decision !== receipt.kind || receipt.policy_version !== IDENTITY_REVIEW_POLICY || identitySubmissionHash(source.account, input) !== source.request_hash) throw new Error("identity_groups_ledger_invalid");
  }
  return receipt;
}
function currentReview(source: IdentityGroupReviewSource): boolean {
  if (source.current_pair === null) return false;
  const input = ledgerParse(identitySubmitInput, source.input_json);
  const pair = buildIdentityPair(input.pair, source.current_pair);
  if (pair.material_hash !== source.material_hash) return false;
  const original = ledgerParse(identityReviewReceipt, source.receipt_json);
  if (!isDeepStrictEqual(original.candidate_ids, pair.accounts.map(account => account.candidate_id)) || !isDeepStrictEqual(original.native_author_ids, pair.accounts.map(account => account.native_author_id)) || !isDeepStrictEqual(original.platforms, pair.accounts.map(identityAuthorPlatform))) throw new Error("identity_groups_ledger_invalid");
  validateIdentityFindings(input, pair);
  return true;
}
export function buildIdentityGroups(basis: IdentityGroupsBasis) {
  const library = buildDiscoveryLibrarySnapshot(basis.library), members = new Map<string, IdentityMember>();
  let unbound = 0;
  for (const row of library.rows) {
    if (row.candidate_id === null || row.native_author_id === null) { unbound++; continue; }
    const details = basis.native_details.get(row.candidate_id);
    if (!details) throw new Error("identity_groups_native_binding_unavailable");
    const platform = row.native_platform ?? row.hit.platform;
    let member = members.get(row.candidate_id);
    if (!member) {
      member = { candidate_id: row.candidate_id, platform, native_author_id: row.native_author_id, display_name: details[0], profile_url: details[1], verdict: row.verdict, reasons: row.reasons, sources: [] };
      members.set(row.candidate_id, member);
    }
    if (member.platform !== platform || member.native_author_id !== row.native_author_id || member.verdict !== row.verdict || !isDeepStrictEqual(member.reasons, row.reasons)) throw new Error("identity_groups_native_binding_conflict");
    member.sources.push(row);
  }
  const receipts = basis.reviews.map(source => decodeLedger(source, library.campaign_id));
  const byDecision = new Map(receipts.map(receipt => [receipt.decision_id, receipt]));
  const revoked = new Set<string>();
  for (const revocation of receipts.filter(receipt => receipt.kind === "revoke")) {
    const target = revocation.revokes_decision_id, original = target === null ? undefined : byDecision.get(target);
    if (target === null || original === undefined || original.kind === "revoke" || original.operator_account_id !== revocation.operator_account_id || !isDeepStrictEqual(original.candidate_ids, revocation.candidate_ids) || !isDeepStrictEqual(original.native_author_ids, revocation.native_author_ids) || !isDeepStrictEqual(original.platforms, revocation.platforms) || original.material_hash !== revocation.material_hash || revoked.has(target)) throw new Error("identity_groups_ledger_invalid");
    revoked.add(target);
  }
  const edges: IdentityRelationship[] = [];
  const reviews = receipts.map((receipt, index) => {
    let state: ReviewState;
    if (revoked.has(receipt.decision_id)) state = "revoked";
    else if (receipt.criteria_revision !== library.criteria_revision) state = "previous_criteria";
    else if (receipt.kind !== "revoke") {
      if (currentReview(basis.reviews[index]!) && receipt.candidate_ids.every(id => members.has(id))) {
        if (receipt.kind !== "insufficient") edges.push([receipt.candidate_ids[0], receipt.candidate_ids[1], receipt.kind]);
        state = "current";
      } else state = "evidence_changed";
    } else state = "current";
    return { receipt, state };
  });
  const components = identityComponents(edges);
  if (edges.some(([left, right, kind]) => kind === "different_authors" && (components.get(left) ?? left) === (components.get(right) ?? right))) throw new Error("identity_groups_relationship_conflict");
  const decisions = new Map<string, string[]>();
  for (const review of reviews) if (review.state === "current" && review.receipt.kind === "same_author") {
    const id = review.receipt.candidate_ids[0], root = components.get(id) ?? id;
    decisions.set(root, [...(decisions.get(root) ?? []), review.receipt.decision_id]);
  }
  const grouped = new Map<string, IdentityMember[]>();
  for (const id of [...members.keys()].sort(order)) { const root = components.get(id) ?? id; grouped.set(root, [...(grouped.get(root) ?? []), members.get(id)!]); }
  const groups = [...grouped].map(([root, group]) => {
    group.sort((left, right) => order(left.platform, right.platform) || order(left.native_author_id, right.native_author_id));
    return { group_id: discoveryMaterialDigest([1, group.map(member => [member.platform, member.native_author_id])]), has_same_author_review: group.length > 1, qualified_member_count: group.filter(member => member.verdict === "qualified").length, members: group, decision_ids: decisions.get(root) ?? [] };
  }).sort((left, right) => order(left.group_id, right.group_id));
  const criteria = z.record(z.string(), z.unknown()).parse(parseApplicationJson(basis.library.criteria_json));
  if (criteria.contact_preference === "public_email_first") {
    const email = (group: typeof groups[number]) => group.members.filter(member => member.verdict === "qualified").some(member => member.sources.some(source => source.contacts?.some(contact => contact.kind === "email")));
    groups.sort((left, right) => Number(left.qualified_member_count === 0) - Number(right.qualified_member_count === 0) || Number(!email(left)) - Number(!email(right)) || order(left.group_id, right.group_id));
  }
  const counts = { native_accounts: members.size, qualified_native_accounts: [...members.values()].filter(member => member.verdict === "qualified").length, identity_groups: groups.length, qualified_identity_groups: groups.filter(group => group.qualified_member_count > 0).length, qualified_native_only_groups: groups.filter(group => group.qualified_member_count > 0 && !group.has_same_author_review).length, unbound_source_hits: unbound, stale_identity_reviews: reviews.filter(review => review.state === "evidence_changed").length };
  return { campaign_id: library.campaign_id, criteria_revision: library.criteria_revision, snapshot_hash: discoveryMaterialDigest([1, library.snapshot_hash, groups, reviews, counts]), groups, reviews, counts };
}
