import type { AccountContext } from "../identity/account-context.ts";

export type DiscoveryDraftPhase = "checking_connection" | "checking_codex" | "planning" | "validating_plan" | "saving_plan" | "finished" | "failed";
export interface DiscoveryDraftProgress { id: string; phase: DiscoveryDraftPhase; elapsed_ms: number; phase_elapsed_ms: number; timings: { phase: DiscoveryDraftPhase; elapsed_ms: number }[]; error_code: string | null; unsupported_conditions: string[] }
export class DiscoveryDraftProgressState {
  readonly #owner: string; readonly #started = performance.now(); #entered = this.#started;
  readonly #value: DiscoveryDraftProgress;
  constructor(context: AccountContext, id: string) {
    this.#owner = JSON.stringify([context.principal.user.id, context.principal.tenantId, context.principal.baseId]);
    this.#value = { id, phase: "checking_connection", elapsed_ms: 0, phase_elapsed_ms: 0, timings: [], error_code: null, unsupported_conditions: [] };
  }
  phase(phase: DiscoveryDraftPhase): void {
    if (this.#value.phase === phase) return;
    const now = performance.now(); this.#value.timings.push({ phase: this.#value.phase, elapsed_ms: Math.floor(now - this.#entered) });
    this.#value.phase = phase; this.#value.elapsed_ms = Math.floor(now - this.#started); this.#value.phase_elapsed_ms = 0; this.#entered = now;
  }
  fail(code: string, validatedExcerpts: readonly string[] = []): void { this.phase("failed"); this.#value.error_code = code; this.#value.unsupported_conditions = [...validatedExcerpts]; }
  snapshot(context: AccountContext, id: string): DiscoveryDraftProgress | null {
    if (JSON.stringify([context.principal.user.id, context.principal.tenantId, context.principal.baseId]) !== this.#owner || id !== this.#value.id) return null;
    const value = structuredClone(this.#value);
    if (value.phase !== "finished" && value.phase !== "failed") { const now = performance.now(); value.elapsed_ms = Math.floor(now - this.#started); value.phase_elapsed_ms = Math.floor(now - this.#entered); }
    return value;
  }
}
