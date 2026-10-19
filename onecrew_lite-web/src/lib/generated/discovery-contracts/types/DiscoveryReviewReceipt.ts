/* Generated. Do not edit. */

/**
 * A committed historical receipt, not a promise that the candidate has not since changed.
 */
export interface DiscoveryReviewReceipt {
  campaign_id: string;
  candidate_id: string;
  created_at: number;
  criteria_revision: number;
  judgments: {
    criterion:
      | "own_ai_tool_demo"
      | "public_tool_url"
      | "apparel_business_content"
      | "category_content"
      | "creator_residence_outside_mainland"
      | "content_language"
      | "match_fit";
    outcome: "supported" | "contradicted" | "insufficient";
    [k: string]: unknown;
  }[];
  material_hash: string;
  native_author_id: string;
  operator_account_id: string;
  policy_version: string;
  profile_ref: string;
  reasons: string[];
  request_id: string;
  review_id: string;
  search_hit_id: string;
  subscribers: number | null;
  verdict: string;
  [k: string]: unknown;
}
