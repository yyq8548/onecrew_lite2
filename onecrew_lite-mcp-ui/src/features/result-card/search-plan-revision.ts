import type { App } from "@modelcontextprotocol/ext-apps/react";
import type { ResultBinding, ResultSnapshot, SearchPlanPatch } from "@mcp-ui/lib/result-contract";
import { readCurrentResult, reviseSearchPlan, ResultClientError, type ResultClientFailure } from "@mcp-ui/lib/result-client";

export type SearchPlanRevisionOutcome =
  | { kind: "updated"; snapshot: ResultSnapshot }
  | { kind: "rejected"; failure: ResultClientFailure; current: ResultSnapshot | null };

function revisionApplied(current: ResultSnapshot, binding: ResultBinding, patch: SearchPlanPatch): boolean {
  const pending = current.pending_action;
  const hasDescription = typeof patch.description === "string";
  const hasKeywords = Array.isArray(patch.keywords);
  return (hasDescription || hasKeywords) && current.state.kind === "available" && pending?.kind === "search_plan" &&
    current.binding.result_ref === binding.result_ref && current.binding.origin === binding.origin &&
    current.binding.fingerprint === binding.fingerprint && current.binding.view.identity === binding.view.identity &&
    current.binding.revision > binding.revision &&
    (!hasDescription || pending.draft.description === patch.description) &&
    (!hasKeywords || JSON.stringify(pending.draft.keywords) === JSON.stringify(patch.keywords));
}

// Read authority after any rejected/ambiguous revision, but never retry the
// mutation. A newer canonical preview matching every submitted field proves
// the update applied even if its response was lost. Otherwise preserve edits.
export async function reviseSearchPlanWithReadback(app: App, binding: ResultBinding, patch: SearchPlanPatch): Promise<SearchPlanRevisionOutcome> {
  try { return { kind: "updated", snapshot: await reviseSearchPlan(app, binding, patch) }; }
  catch (error) {
    const failure: ResultClientFailure = error instanceof ResultClientError ? error.failure : { kind: "transport" };
    let current: ResultSnapshot | null = null;
    try { current = await readCurrentResult(app, binding); } catch { /* Unverified authority stays blocked. */ }
    if (current && revisionApplied(current, binding, patch)) return { kind: "updated", snapshot: current };
    return { kind: "rejected", failure, current };
  }
}
