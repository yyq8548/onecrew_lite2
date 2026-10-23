import type { z } from "zod";

export class DiscoverySemanticError extends Error {
  readonly code: "semantic_unavailable" | "semantic_login_required" | "semantic_failed" | "semantic_timeout" | "semantic_invalid_output";
  constructor(code: DiscoverySemanticError["code"], options?: ErrorOptions) { super(code, options); this.code = code; }
}
export type SemanticTimingPhase = "login_status" | "spawn" | "until_thread_started" | "until_turn_started" | "turn" | "exec" | "output_validation" | "host_queue" | "host_evaluation";
export interface SemanticTokenUsage { readonly input: number; readonly cached_input: number; readonly output: number; readonly reasoning_output: number }
export interface SemanticScope { readonly kind: "plan" | "queries" | "query_audit" | "score" | "professional_review"; readonly campaign_id: string; readonly revision: number; readonly search_hit_id?: string }
export interface SemanticStructuredRequest<T> {
  readonly prompt: string; readonly scoringContext?: string; readonly scope?: SemanticScope; readonly purpose?: SemanticScope["kind"];
  readonly savedEvidence?: readonly { readonly id: string; readonly citation: unknown }[];
  readonly schema: Readonly<Record<string, unknown>>; readonly decode: z.ZodType<T>; readonly planning: boolean;
  readonly signal: AbortSignal; onStarted?(): void; onTiming?(phase: SemanticTimingPhase, elapsedMs: number): void;
  onUsage?(usage: SemanticTokenUsage): void; assertCurrent(): Promise<void>;
}
export interface SemanticStructuredClient {
  structured<T>(input: SemanticStructuredRequest<T>): Promise<T>;
  scoringIdentity?(): Promise<string>;
}
export function scopedSemanticClient(client: SemanticStructuredClient, scope: SemanticScope): SemanticStructuredClient {
  return { structured: input => client.structured({ ...input, scope: { ...scope, ...(input.scope ?? {}), kind: input.purpose ?? input.scope?.kind ?? scope.kind } }) };
}
