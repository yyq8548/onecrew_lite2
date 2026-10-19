/* Generated. Do not edit. */

/**
 * Shared by peer adapters and Library. Identity-group counts do not certify that
 * all groups are distinct natural people; missing identity evidence stays visible.
 */
export interface DiscoveryIdentityGroupsSnapshot {
  campaign_id: string;
  counts: {
    identity_groups: number;
    native_accounts: number;
    qualified_identity_groups: number;
    qualified_native_accounts: number;
    qualified_native_only_groups: number;
    stale_identity_reviews: number;
    unbound_source_hits: number;
    [k: string]: unknown;
  };
  criteria_revision: number;
  groups: {
    decision_ids: string[];
    /**
     * Deterministic membership key, not a persisted person fact or an editable note key.
     */
    group_id: string;
    has_same_author_review: boolean;
    members: {
      candidate_id: string;
      display_name: string;
      native_author_id: string;
      platform: string;
      profile_url: string;
      reasons: string[];
      sources: {
        candidate_id: string | null;
        contacts?: {
          /**
           * A source observation does not establish the destination's owner or purpose.
           */
          attribution: "author_published" | "unconfirmed";
          captured_at: number;
          kind: "email" | "telegram" | "hidden_email" | "social_profile" | "whatsapp" | "wechat" | "wechat_official";
          native_author_id: string;
          normalized_value: string;
          original_value: string;
          source_excerpt: string;
          source_url: string;
        }[];
        evidence_version: string;
        /**
         * Owner-bound count; a leading ~ preserves a rounded public estimate.
         */
        followers?: string | null;
        hit: {
          author_id: string | null;
          author_label: string;
          author_observed_at: number | null;
          author_url: string | null;
          campaign_id: string;
          captured_at: number;
          id: string;
          platform: string;
          profile_ref: string;
          search_keyword: string;
          source_url: string;
          title: string;
          [k: string]: unknown;
        };
        match_rating?: {
          /**
           * Evidence-based fit, independent of eligibility. Unknown fit has no level.
           */
          level: "excellent" | "good" | "medium" | "low";
          reason: string;
          [k: string]: unknown;
        } | null;
        native_author_id: string | null;
        /**
         * Actual creator platform for an identity bound through a search-engine source.
         */
        native_platform?: string | null;
        reasons: string[];
        verdict: string;
        [k: string]: unknown;
      }[];
      verdict: string;
      [k: string]: unknown;
    }[];
    qualified_member_count: number;
    [k: string]: unknown;
  }[];
  reviews: {
    receipt: {
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
    };
    state: "current" | "evidence_changed" | "revoked" | "previous_criteria";
    [k: string]: unknown;
  }[];
  snapshot_hash: string;
  [k: string]: unknown;
}
