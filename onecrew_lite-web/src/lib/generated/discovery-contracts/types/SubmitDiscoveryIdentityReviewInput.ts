/* Generated. Do not edit. */

/**
 * Same-author needs reciprocal, author-owned account references and explicit reviewer
 * ownership judgments. A matching URL alone is not an ownership determination.
 */
export interface SubmitDiscoveryIdentityReviewInput {
  decision: "same_author" | "different_authors" | "insufficient";
  decision_id: string;
  findings: {
    /**
     * Only saved text owned by this exact native author can support account ownership.
     * External target pages, search names, product URLs and sponsor URLs are excluded.
     */
    citation:
      | {
          kind: "description";
          quote: string;
        }
      | {
          kind: "transcript";
          quote: string;
          segment_index: number;
        }
      | {
          kind: "native_content";
          quote: string;
        }
      | {
          kind: "native_profile_bio";
          quote: string;
        };
    /**
     * Self-reported reviewer interpretation of a quoted relationship, not a native fact.
     */
    relationship: "self_owned_account" | "other_person" | "unclear";
    /**
     * One of the pair's saved hit IDs; the target is always the other bound account.
     */
    source_search_hit_id: string;
  }[];
  material_hash: string;
  pair: {
    campaign_id: string;
    expected_revision: number;
    left_search_hit_id: string;
    right_search_hit_id: string;
  };
  rationale: string;
  /**
   * The label/model is self-reported; authenticated account identity is injected separately.
   */
  reviewer: {
    kind: "agent" | "human";
    label: string;
  };
}
