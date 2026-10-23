/* Generated. Do not edit. */

export interface ResultPresentResult {
  /**
   * A bounded, secret-free terminal cause for a failed presented result.
   * Successful and non-failed states carry `null` so the model cannot
   * mistake an absent cause for an unreported failure.
   */
  failure: {
    code: string;
    retryable: boolean;
    summary: string;
  } | null;
  result_ref: string;
  status: "available" | "submitted" | "resolved" | "failed" | "unknown";
}
