/* Generated. Do not edit. */

export interface DiscoveryIdentityReviewReceipt {
  campaign_id: string;
  /**
   * @minItems 2
   * @maxItems 2
   */
  candidate_ids: string[];
  created_at: number;
  criteria_revision: number;
  decision_id: string;
  kind: "same_author" | "different_authors" | "insufficient" | "revoke";
  material_hash: string;
  /**
   * @minItems 2
   * @maxItems 2
   */
  native_author_ids: string[];
  operator_account_id: string;
  /**
   * @minItems 2
   * @maxItems 2
   */
  platforms: string[];
  policy_version: string;
  revokes_decision_id: string | null;
  [k: string]: unknown;
}
