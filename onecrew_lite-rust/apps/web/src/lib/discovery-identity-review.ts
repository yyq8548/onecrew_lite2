import { invoke } from "./local-runtime";
import type { DiscoveryIdentityMergeMaterial, DiscoveryIdentityMergeMaterialInput, DiscoveryIdentityReviewReceipt, GetDiscoveryIdentityReviewInput, SubmitDiscoveryIdentityReviewInput, RevokeDiscoveryIdentityReviewInput } from "./generated/discovery-contracts";
import { validateDiscoveryIdentityMergeMaterial } from "./generated/discovery-contracts/validators/DiscoveryIdentityMergeMaterial.js";
import { validateDiscoveryIdentityReviewReceipt } from "./generated/discovery-contracts/validators/DiscoveryIdentityReviewReceipt.js";

export async function getIdentityReviewMaterial(input: DiscoveryIdentityMergeMaterialInput): Promise<DiscoveryIdentityMergeMaterial> {
  const raw = await invoke<unknown>("onecrew_discovery_identity_material_get", { input });
  if (!validateDiscoveryIdentityMergeMaterial(raw) || raw.format_version !== 1
    || raw.campaign_id !== input.campaign_id || raw.criteria_revision !== input.expected_revision
    || raw.accounts.length !== 2 || !/^[a-f0-9]{64}$/.test(raw.material_hash))
    throw new Error("identity_review_material_invalid");
  const ids = new Set([input.left_search_hit_id, input.right_search_hit_id]);
  if (ids.size !== 2 || raw.accounts.some(account => account.campaign_id !== input.campaign_id
      || account.criteria_revision !== input.expected_revision || account.hit.campaign_id !== input.campaign_id
      || !ids.delete(account.hit.id) || !account.native_author_id)
    || raw.accounts[0].candidate_id === raw.accounts[1].candidate_id
    || (raw.accounts[0].hit.platform === raw.accounts[1].hit.platform
      && raw.accounts[0].native_author_id === raw.accounts[1].native_author_id))
    throw new Error("identity_review_material_invalid");
  return raw;
}

export async function getIdentityReviewReceipt(input: GetDiscoveryIdentityReviewInput): Promise<DiscoveryIdentityReviewReceipt | null> {
  const raw = await invoke<unknown>("onecrew_discovery_identity_review_get", { input });
  if (raw === null) return null;
  if (!validateDiscoveryIdentityReviewReceipt(raw) || raw.campaign_id !== input.campaign_id
    || raw.decision_id !== input.decision_id) throw new Error("identity_review_receipt_invalid");
  return raw;
}

// Never turn a failed write into success based only on a receipt ID: a receipt
// does not contain the entire submitted rationale/findings. The UI retains the
// exact request and offers explicit readback before any further write.
export async function submitIdentityReview(input: SubmitDiscoveryIdentityReviewInput): Promise<DiscoveryIdentityReviewReceipt> {
  const raw = await invoke<unknown>("onecrew_discovery_identity_review_submit", { input });
  if (!validateDiscoveryIdentityReviewReceipt(raw) || raw.campaign_id !== input.pair.campaign_id
    || raw.criteria_revision !== input.pair.expected_revision || raw.decision_id !== input.decision_id
    || raw.material_hash !== input.material_hash || raw.kind !== input.decision || raw.revokes_decision_id !== null)
    throw new Error("identity_review_receipt_invalid");
  return raw;
}

export async function revokeIdentityReview(input: RevokeDiscoveryIdentityReviewInput): Promise<DiscoveryIdentityReviewReceipt> {
  const raw = await invoke<unknown>("onecrew_discovery_identity_review_revoke", { input });
  if (!validateDiscoveryIdentityReviewReceipt(raw) || raw.campaign_id !== input.campaign_id
    || raw.criteria_revision !== input.expected_revision || raw.decision_id !== input.decision_id
    || raw.kind !== "revoke" || raw.revokes_decision_id !== input.revokes_decision_id)
    throw new Error("identity_review_receipt_invalid");
  return raw;
}

export type IdentityQuoteOption = { key: string; text: string; citation: SubmitDiscoveryIdentityReviewInput["findings"][number]["citation"] };
type IdentityReviewDraft = {
  decisionId: string; decision: SubmitDiscoveryIdentityReviewInput["decision"]; label: string; rationale: string;
  quotes: Record<string, string>;
  relationships: Record<string, SubmitDiscoveryIdentityReviewInput["findings"][number]["relationship"]>;
};
export function prepareHumanIdentityReview(material: DiscoveryIdentityMergeMaterial, draft: IdentityReviewDraft): SubmitDiscoveryIdentityReviewInput {
  return prepareIdentityReview(material, { ...draft, reviewerKind: "human" });
}
export function prepareIdentityReview(material: DiscoveryIdentityMergeMaterial,
  draft: IdentityReviewDraft & { reviewerKind: SubmitDiscoveryIdentityReviewInput["reviewer"]["kind"] },
): SubmitDiscoveryIdentityReviewInput {
  return {
    pair: { campaign_id: material.campaign_id, expected_revision: material.criteria_revision,
      left_search_hit_id: material.accounts[0].hit.id, right_search_hit_id: material.accounts[1].hit.id },
    decision_id: draft.decisionId, material_hash: material.material_hash, decision: draft.decision,
    reviewer: { kind: draft.reviewerKind, label: draft.label.trim() }, rationale: draft.rationale.trim(),
    findings: material.accounts.flatMap(account => {
      const option = identityQuoteOptions(account).find(item => item.key === draft.quotes[account.hit.id]);
      return option ? [{ source_search_hit_id: account.hit.id, citation: option.citation,
        relationship: draft.relationships[account.hit.id] ?? "unclear" }] : [];
    }),
  };
}

export function identityQuoteOptions(account: DiscoveryIdentityMergeMaterial["accounts"][number]): IdentityQuoteOption[] {
  const options: IdentityQuoteOption[] = [];
  if (account.native_profile?.bio) options.push({ key: "bio", text: account.native_profile.bio,
    citation: { kind: "native_profile_bio", quote: account.native_profile.bio } });
  if (account.video_description) options.push({ key: "description", text: account.video_description,
    citation: { kind: "description", quote: account.video_description } });
  if (account.transcript?.status === "completed" && !account.transcript.stop_code)
    account.transcript.segments.forEach((segment, index) => options.push({ key: `transcript:${index}`, text: segment.text,
      citation: { kind: "transcript", segment_index: index, quote: segment.text } }));
  const content = account.native_content;
  if (content?.status === "completed" && !content.stop_code && content.native_content && typeof content.native_content === "object") {
    const native = content.native_content;
    const text = "text" in native ? native.text : "description" in native ? native.description : null;
    if (typeof text === "string" && text) options.push({ key: "native", text, citation: { kind: "native_content", quote: text } });
  }
  return options;
}
