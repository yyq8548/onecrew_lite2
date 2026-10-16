/* Generated. Do not edit. */

/**
 * Judgments reference the previously read owner material. No account, policy, proof or verdict input.
 */
export interface SubmitDiscoveryReviewInput {
  campaign_id: string;
  expected_revision: number;
  /**
   * @maxItems 40
   *
   * Items: Supported demo requires first-hand actions/results in saved transcript, not a title,
   * advertisement, list, third-party retelling or mere tool mention. Text is not visual proof.
   * Supported tool requires an author-observed link and a successful target-page quotation.
   * Language requires exact saved author text; spoken language requires transcript.
   * MatchFit is optional: excellent = strong relevant evidence and an advantage;
   * good = clear supported relevance; medium = partial fit with a limitation;
   * low = a cited significant weakness. Missing optional data never lowers fit.
   * Fit cannot override required eligibility. Unknown fit has insufficient outcome
   * and no match_level. Absence in one sample cannot reject the whole creator.
   * match_fit 的 supported 表示证据支持所选等级，包括 low；
   * 未知匹配使用 insufficient 且不填等级，match_fit 不接受 contradicted。
   */
  findings: {
    /**
     * @maxItems 8
     */
    citations: (
      | {
          kind: "description";
          quote: string;
        }
      | {
          kind: "search_caption";
          quote: string;
        }
      | {
          kind: "transcript";
          quote: string;
          segment_index: number;
        }
      | {
          kind: "native_profile";
          profile_url: string;
          quote: string;
        }
      | {
          kind: "native_content";
          quote: string;
        }
      | {
          kind: "public_page";
          quote: string;
          requested_url: string;
        }
    )[];
    /**
     * ISO 3166-1 alpha-2 location identified by the reviewer from cited evidence.
     */
    country_code?: string | null;
    criterion:
      | (
          | "own_ai_tool_demo"
          | "public_tool_url"
          | "apparel_business_content"
          | "category_content"
          | "creator_residence_outside_mainland"
          | "content_language"
          | "match_fit"
        )
      | {
          requirement: number;
        };
    match_level?: ("excellent" | "good" | "medium" | "low") | null;
    outcome: "supported" | "contradicted" | "insufficient";
    rationale: string;
    tool?: {
      name: string;
      requested_url: string;
    } | null;
  }[];
  material_hash: string;
  request_id: string;
  /**
   * The label/model is self-reported; authenticated account identity is injected separately.
   */
  reviewer: {
    kind: "agent" | "human";
    label: string;
  };
  search_hit_id: string;
}
