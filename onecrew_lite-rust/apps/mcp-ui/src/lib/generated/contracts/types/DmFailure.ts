/* Generated. Do not edit. */

/**
 * A transient diagnostic annotated by the runtime that actually ran the stage.
 * This is deliberately separate from durable receipts and effect state.
 */
export interface DmFailure {
  /**
   * Safe evidence from the actual protocol. Codes have meaning only within the
   * platform, protocol and origin that supplied them. Absence is not a diagnosis.
   * Human messages must be owner-authored constants, never raw provider Display.
   */
  diagnostic: {
    code: string | null;
    http_status: number | null;
    message: string;
    /**
     * Where a diagnostic was established, not an effect or recovery classification.
     */
    origin: "provider" | "local";
    platform: string;
    protocol: string;
    rpc_code: number | null;
  };
  stage: string;
}
