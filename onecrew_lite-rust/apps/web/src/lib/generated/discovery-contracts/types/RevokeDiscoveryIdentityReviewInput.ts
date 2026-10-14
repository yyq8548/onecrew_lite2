/* Generated. Do not edit. */

export interface RevokeDiscoveryIdentityReviewInput {
  campaign_id: string;
  decision_id: string;
  expected_revision: number;
  rationale: string;
  /**
   * The label/model is self-reported; authenticated account identity is injected separately.
   */
  reviewer: {
    kind: "agent" | "human";
    label: string;
  };
  revokes_decision_id: string;
}
