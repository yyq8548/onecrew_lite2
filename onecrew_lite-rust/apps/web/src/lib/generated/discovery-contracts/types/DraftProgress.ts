/* Generated. Do not edit. */

export interface DraftProgress {
  elapsed_ms: number;
  error_code: string | null;
  id: string;
  phase:
    "checking_connection" | "checking_codex" | "planning" | "validating_plan" | "saving_plan" | "finished" | "failed";
  phase_elapsed_ms: number;
  timings: {
    elapsed_ms: number;
    phase:
      "checking_connection" | "checking_codex" | "planning" | "validating_plan" | "saving_plan" | "finished" | "failed";
  }[];
  /**
   * Validated excerpts of the customer's request, never raw model output.
   */
  unsupported_conditions: string[];
}
