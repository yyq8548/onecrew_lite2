import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "./local-runtime";
import { reviewFixture, xReviewFixture } from "./discovery-review.fixture";
import { getIdentityReviewMaterial, getIdentityReviewReceipt, identityQuoteOptions, prepareHumanIdentityReview, prepareIdentityReview, revokeIdentityReview, submitIdentityReview } from "./discovery-identity-review";
import type { SubmitDiscoveryIdentityReviewInput } from "./generated/discovery-contracts";
vi.mock("./local-runtime", () => ({ invoke: vi.fn() }));
const pair = { campaign_id: "campaign", expected_revision: 1, left_search_hit_id: "left", right_search_hit_id: "right" };
function material() {
  const left = reviewFixture(), right = xReviewFixture();
  left.hit.id = "left";
  right.hit.id = "right";
  right.candidate_id = "other-candidate";
  return { format_version: 1, campaign_id: "campaign", criteria_revision: 1,
    accounts: [left, right], material_hash: "a".repeat(64) };
}
const submission: SubmitDiscoveryIdentityReviewInput = { pair, material_hash: "a".repeat(64), decision_id: "decision",
  reviewer: { kind: "human", label: "reviewer" }, decision: "insufficient", rationale: "Missing reciprocal links", findings: [] };
const receipt = () => ({ campaign_id: "campaign", criteria_revision: 1, decision_id: "decision", operator_account_id: "account",
  policy_version: "reciprocal-native-account-review-v1", kind: "insufficient", revokes_decision_id: null,
  candidate_ids: ["candidate", "other-candidate"], native_author_ids: ["channel", "987654321"], platforms: ["youtube", "twitter"],
  material_hash: "a".repeat(64), created_at: 1 });

describe("source-bound identity review client", () => {
  it("records an Agent review as Agent rather than impersonating a human", () => {
    const request = prepareIdentityReview(material(), { decisionId: "agent-decision", decision: "insufficient",
      reviewerKind: "agent", label: " Codex ", rationale: "Missing identity proof", quotes: {}, relationships: {} });
    expect(request.reviewer).toEqual({ kind: "agent", label: "Codex" });
    expect(request.findings).toEqual([]);
  });
  it("freezes canonical evidence and keeps exact request values independent from draft edits", () => {
    const evidence = material(); evidence.accounts.reverse();
    const draft = { decisionId: "stable-id", decision: "insufficient" as const, label: " Human ", rationale: " Need evidence ",
      quotes: { right: "bio", left: "made-up-quote" }, relationships: {} };
    const request = prepareHumanIdentityReview(evidence, draft);
    draft.label = "Changed"; draft.quotes.right = "native";
    expect(request.pair.left_search_hit_id).toBe("right");
    expect(request.reviewer).toEqual({ kind: "human", label: "Human" });
    expect(request.decision_id).toBe("stable-id");
    expect(request.findings).toHaveLength(1);
    expect(request.findings[0]).toMatchObject({ source_search_hit_id: "right", relationship: "unclear", citation: { kind: "native_profile_bio" } });
    expect(request.material_hash).toBe(evidence.material_hash);
  });
  beforeEach(() => vi.resetAllMocks());
  it("accepts the owner's canonical pair ordering", async () => {
    const raw = material(); raw.accounts.reverse();
    vi.mocked(invoke).mockResolvedValue(raw);
    expect(await getIdentityReviewMaterial(pair)).toEqual(raw);
  });
  it.each(["campaign", "revision", "duplicate-hit", "missing-hit", "same-candidate", "same-account"])("rejects an incorrectly bound pair: %s", async kind => {
    const raw = material();
    if (kind === "campaign") raw.accounts[0].campaign_id = "other";
    if (kind === "revision") raw.accounts[1].criteria_revision = 2;
    if (kind === "duplicate-hit") raw.accounts[1].hit.id = "left";
    if (kind === "missing-hit") raw.accounts[1].hit.id = "missing";
    if (kind === "same-candidate") raw.accounts[1].candidate_id = raw.accounts[0].candidate_id;
    if (kind === "same-account") { raw.accounts[1].hit.platform = raw.accounts[0].hit.platform; raw.accounts[1].native_author_id = raw.accounts[0].native_author_id; }
    vi.mocked(invoke).mockResolvedValue(raw);
    await expect(getIdentityReviewMaterial(pair)).rejects.toThrow("identity_review_material_invalid");
  });
  it("returns explicit absence and rejects another decision's receipt", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(null).mockResolvedValueOnce({ ...receipt(), decision_id: "other" });
    expect(await getIdentityReviewReceipt({ campaign_id: "campaign", decision_id: "decision" })).toBeNull();
    await expect(getIdentityReviewReceipt({ campaign_id: "campaign", decision_id: "decision" })).rejects.toThrow("identity_review_receipt_invalid");
  });
  it("never retries or declares success after a failed write", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("response lost"));
    await expect(submitIdentityReview(submission)).rejects.toThrow("response lost");
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it.each([{ kind: "same_author" }, { material_hash: "b".repeat(64) }, { criteria_revision: 2 }, { revokes_decision_id: "other" }])("rejects mismatched submit receipts: %j", async patch => {
    vi.mocked(invoke).mockResolvedValue({ ...receipt(), ...patch });
    await expect(submitIdentityReview(submission)).rejects.toThrow("identity_review_receipt_invalid");
  });
  it("accepts submit and revoke only with matching readback fields", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(receipt()).mockResolvedValueOnce({ ...receipt(), kind: "revoke", decision_id: "revoke", revokes_decision_id: "decision" });
    expect(await submitIdentityReview(submission)).toEqual(receipt());
    const result = await revokeIdentityReview({ campaign_id: "campaign", expected_revision: 1, decision_id: "revoke", revokes_decision_id: "decision", reviewer: submission.reviewer, rationale: "Correction" });
    expect(result.revokes_decision_id).toBe("decision");
  });
  it("offers only author-owned saved quote types, never external pages or search captions", () => {
    expect(identityQuoteOptions(reviewFixture()).map(option => option.key)).toEqual(["description", "transcript:0"]);
    expect(identityQuoteOptions(xReviewFixture()).map(option => option.key)).toEqual(["bio", "native"]);
    const failed = xReviewFixture(); failed.native_content!.status = "failed";
    expect(identityQuoteOptions(failed).map(option => option.key)).toEqual(["bio"]);
  });
  it.each([{ campaign_id: "other" }, { criteria_revision: 2 }, { decision_id: "other" }, { kind: "same_author" }, { revokes_decision_id: "other" }])("rejects a mismatched revocation receipt: %j", async patch => {
    vi.mocked(invoke).mockResolvedValue({ ...receipt(), kind: "revoke", decision_id: "revoke", revokes_decision_id: "decision", ...patch });
    await expect(revokeIdentityReview({ campaign_id: "campaign", expected_revision: 1, decision_id: "revoke", revokes_decision_id: "decision", reviewer: submission.reviewer, rationale: "Correction" })).rejects.toThrow("identity_review_receipt_invalid");
  });
  it("preserves uncertain revocation outcomes without automatic retry or readback", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("response lost"));
    await expect(revokeIdentityReview({ campaign_id: "campaign", expected_revision: 1, decision_id: "revoke", revokes_decision_id: "decision", reviewer: submission.reviewer, rationale: "Correction" })).rejects.toThrow("response lost");
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
