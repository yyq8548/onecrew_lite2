import { z } from "zod";
import { trim } from "../creators/contracts.ts";

const text = z.string().refine(value => value.isWellFormed());
export const identityU32 = z.number().int().min(0).max(4_294_967_295);
const timestamp = z.union([z.number().int().safe(), z.bigint().min(-9_223_372_036_854_775_808n).max(9_223_372_036_854_775_807n)]);
export const identityTextValid = (value: string, maximum: number): boolean => trim(value).length > 0 && Buffer.byteLength(value) <= maximum && !/[\p{Cc}]/u.test(value.replace(/[\n\t]/g, ""));
const id = text.refine(value => identityTextValid(value, 128) && !/\p{Cc}/u.test(value));
const reviewer = z.object({ kind: z.enum(["human", "agent"]), label: text }).strict();
export const identityPairInput = z.object({ campaign_id: id, expected_revision: identityU32.min(1), left_search_hit_id: id, right_search_hit_id: id }).strict().refine(value => value.left_search_hit_id !== value.right_search_hit_id, "identity_merge_input_invalid");
export const identityQuote = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("description"), quote: text }).strict(),
  z.object({ kind: z.literal("transcript"), segment_index: identityU32, quote: text }).strict(),
  z.object({ kind: z.literal("native_content"), quote: text }).strict(),
  z.object({ kind: z.literal("native_profile_bio"), quote: text }).strict(),
]);
export const identityFinding = z.object({ source_search_hit_id: text, relationship: z.enum(["self_owned_account", "other_person", "unclear"]), citation: identityQuote }).strict();
export const identitySubmitInput = z.object({ pair: identityPairInput, material_hash: text.regex(/^[A-Fa-f\d]{64}$/u), decision_id: id, reviewer, decision: z.enum(["same_author", "different_authors", "insufficient"]), rationale: text, findings: z.array(identityFinding).max(8) }).strict();
export const identityRevokeInput = z.object({ campaign_id: id, expected_revision: identityU32.min(1), decision_id: id, revokes_decision_id: id, reviewer, rationale: text }).strict().refine(value => value.decision_id !== value.revokes_decision_id, "identity_review_input_invalid");
export const identityGetInput = z.object({ campaign_id: id, decision_id: id }).strict();
export const identityGroupsInput = z.object({ campaign_id: id, expected_revision: identityU32.min(1) }).strict();
export const identityReviewReceipt = z.object({ campaign_id: text, criteria_revision: identityU32, decision_id: text, operator_account_id: text, policy_version: text, kind: z.enum(["same_author", "different_authors", "insufficient", "revoke"]), revokes_decision_id: text.nullable(), candidate_ids: z.tuple([text, text]), native_author_ids: z.tuple([text, text]), platforms: z.tuple([text, text]), material_hash: text, created_at: timestamp });
export type IdentityPairInput = z.infer<typeof identityPairInput>;
export type IdentitySubmitInput = z.infer<typeof identitySubmitInput>;
export type IdentityRevokeInput = z.infer<typeof identityRevokeInput>;
export type IdentityQuote = z.infer<typeof identityQuote>;
export type IdentityReviewReceipt = z.infer<typeof identityReviewReceipt>;
export type IdentityRelationship = readonly [left: string, right: string, kind: string];
export const IDENTITY_REVIEW_POLICY = "reciprocal-native-account-review-v1";
export const IDENTITY_REVOCATION_POLICY = "audited-native-identity-revocation-v1";
