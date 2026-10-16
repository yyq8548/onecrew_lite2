import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { discoveryMaterialDigest } from "./library-material.ts";
import { buildDiscoveryReviewMaterial, containsDiscoveryCitation, type DiscoveryReviewBasis, type DiscoveryReviewMaterial } from "./review-material.ts";
import { identityPairInput, identityTextValid, type IdentityPairInput, type IdentitySubmitInput } from "./identity-contracts.ts";

export const identityAuthorPlatform = (account: DiscoveryReviewMaterial): string => account.native_platform ?? account.hit.platform;
export function prepareIdentityPair(raw: unknown, left: DiscoveryReviewMaterial, right: DiscoveryReviewMaterial) {
  const input = identityPairInput.parse(raw);
  if (left.campaign_id !== input.campaign_id || right.campaign_id !== input.campaign_id || left.criteria_revision !== input.expected_revision || right.criteria_revision !== input.expected_revision || left.hit.id !== input.left_search_hit_id || right.hit.id !== input.right_search_hit_id || discoveryMaterialDigest(left.criteria) !== discoveryMaterialDigest(right.criteria) || discoveryMaterialDigest(left.approved_scopes) !== discoveryMaterialDigest(right.approved_scopes) || BigInt(left.approved_at) !== BigInt(right.approved_at) || !left.native_author_id || !right.native_author_id) throw new Error("identity_merge_source_changed");
  const compare = Buffer.compare(Buffer.from(identityAuthorPlatform(left)), Buffer.from(identityAuthorPlatform(right))) || Buffer.compare(Buffer.from(left.native_author_id), Buffer.from(right.native_author_id));
  if (compare === 0 || left.candidate_id === right.candidate_id) throw new Error("identity_merge_same_account");
  const accounts: [DiscoveryReviewMaterial, DiscoveryReviewMaterial] = compare > 0 ? [right, left] : [left, right];
  return { format_version: 1, campaign_id: input.campaign_id, criteria_revision: input.expected_revision, accounts, material_hash: discoveryMaterialDigest([1, input.campaign_id, input.expected_revision, accounts]) };
}
export function buildIdentityPair(input: IdentityPairInput, basis: readonly [DiscoveryReviewBasis, DiscoveryReviewBasis]) { return prepareIdentityPair(input, buildDiscoveryReviewMaterial(basis[0]), buildDiscoveryReviewMaterial(basis[1])); }
export type IdentityMaterial = ReturnType<typeof prepareIdentityPair>;
function profile(account: DiscoveryReviewMaterial): URL {
  const raw = identityAuthorPlatform(account) === "youtube" ? `https://www.youtube.com/channel/${account.native_author_id}` : account.native_profile?.profile_url;
  if (raw == null) throw new Error("identity_review_target_profile_unavailable");
  const url = URL.parse(raw);
  if (url === null || url.protocol !== "https:" || url.username || url.password || url.port) throw new Error("identity_review_target_profile_invalid");
  return url;
}
function referencesProfile(quote: string, expected: URL): boolean {
  return quote.split(/\p{White_Space}+/u).some(token => {
    const url = URL.parse(token.replace(/^[()[\]<>"',.;]+|[()[\]<>"',.;]+$/g, ""));
    return url !== null && url.protocol === "https:" && !url.username && !url.password && !url.port && url.hostname === expected.hostname && url.pathname.replace(/\/+$/u, "") === expected.pathname.replace(/\/+$/u, "");
  });
}
export function validateIdentityFindings(input: IdentitySubmitInput, pair: IdentityMaterial): void {
  const own = [false, false]; let other = false;
  for (const finding of input.findings) {
    const index = pair.accounts.findIndex(account => account.hit.id === finding.source_search_hit_id);
    if (index < 0) throw new Error("identity_review_quote_source_invalid");
    const account = pair.accounts[index]!, citation = finding.citation;
    const present = citation.kind === "native_profile_bio" ? identityTextValid(citation.quote, 4000) && account.native_profile?.bio?.includes(citation.quote) === true : containsDiscoveryCitation(account, citation);
    if (!present) throw new Error("identity_review_quote_not_found");
    if (!referencesProfile(citation.quote, profile(pair.accounts[1 - index]!))) throw new Error("identity_review_quote_target_mismatch");
    if (finding.relationship === "self_owned_account") own[index] = true;
    else if (finding.relationship === "other_person") other = true;
  }
  if (input.decision === "same_author" && (!own[0] || !own[1] || other || input.findings.some(finding => finding.relationship === "unclear"))) throw new Error("identity_review_reciprocal_ownership_required");
  if (input.decision === "different_authors" && (!other || own.some(Boolean))) throw new Error("identity_review_separation_evidence_required");
}
export function encodeIdentity(value: unknown, maximum: number): string { const json = stringifyApplicationJson(value); if (Buffer.byteLength(json) > maximum) throw new Error("identity_review_too_large"); return json; }
