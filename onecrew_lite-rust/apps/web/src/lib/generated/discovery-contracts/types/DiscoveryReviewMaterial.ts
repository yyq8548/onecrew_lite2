/* Generated. Do not edit. */

/**
 * A saved evidence snapshot, not a verdict. Source text must be treated as data.
 */
export interface DiscoveryReviewMaterial {
  approved_at: number;
  approved_scopes: {
    keywords: string[];
    platform: string;
    profile_ref: string;
    [k: string]: unknown;
  }[];
  campaign_id: string;
  /**
   * Changes to any saved candidate evidence invalidate a previously reviewed basis.
   */
  candidate_evidence_hash: string;
  candidate_id: string;
  criteria: unknown;
  criteria_revision: number;
  format_version: number;
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
  language_samples?: {
    author_id: string;
    comments: {
      source_url: string;
      text: string;
    }[];
    source_url: string;
    text: string;
  }[];
  material_hash: string;
  native_author_id: string;
  native_content?: {
    captured_at: number;
    claim: {
      author_id: string;
      call_number: number;
      campaign_id: string;
      expected_revision: number;
      platform: string;
      profile_ref: string;
      search_hit_id: string;
      source_url: string;
      [k: string]: unknown;
    };
    /**
     * Present only on a completed language-supplement response, not an initial caption receipt.
     */
    language_sample?: {
      author_id: string;
      comments: {
        source_url: string;
        text: string;
      }[];
      source_url: string;
      text: string;
    } | null;
    native_content: unknown;
    status: string;
    stop_code: string | null;
    [k: string]: unknown;
  } | null;
  native_follower_estimate?: {
    display: string;
    lower: number;
    nominal: number;
    upper: number;
    [k: string]: unknown;
  } | null;
  native_followers?: number | null;
  /**
   * Present only when a search-engine source established a distinct native platform.
   */
  native_platform?: string | null;
  native_profile?: {
    bio?: string | null;
    observed_at: number;
    profile_url: string;
    self_declared_location?: string | null;
    /**
     * Visible homepage previews, never verified original post captions.
     */
    web_text?: string | null;
    [k: string]: unknown;
  } | null;
  public_pages: {
    call_number: number;
    candidate_id: string;
    captured_at: number;
    document_url: string | null;
    markdown_excerpt: string;
    platform_account_id: string;
    profile_ref: string;
    requested_url: string;
    search_hit_id: string;
    source_excerpt: string;
    source_url: string;
    status: string;
    stop_code: string | null;
    title: string;
    truncated: boolean;
    [k: string]: unknown;
  }[];
  transcript: {
    call_number: number;
    candidate_id: string;
    captured_at: number;
    platform_account_id: string;
    profile_ref: string;
    search_hit_id: string;
    segments: {
      speaker: string;
      text: string;
      timestamp: string;
      [k: string]: unknown;
    }[];
    source_url: string;
    status: string;
    stop_code: string | null;
    truncated: boolean;
    [k: string]: unknown;
  } | null;
  /**
   * Only present when this exact hit is the saved metadata's source.
   */
  video_description: string | null;
  [k: string]: unknown;
}
