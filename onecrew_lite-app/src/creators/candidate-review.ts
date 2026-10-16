import { inspect } from "node:util";
import type { AccountContext } from "../identity/account-context.ts";
import { creatorSaveReview, type CreatorSaveReview, type CreatorSaveSelection } from "../results/creator-save-contracts.ts";
import { SubmittedOwnerGrant, type OwnerCallCompletion, type OwnerReviewAuthority, type PreparedOwnerReview } from "../results/grants.ts";
import { OwnerReviewContext } from "../results/owner-context.ts";
import { candidateJson, type CandidateAccount } from "./candidate-contracts.ts";
import { CandidateCommitUnknownError } from "./candidate-execute.ts";
import { boundedCandidateText, materializeCreatorCandidates } from "./candidate-materialize.ts";
import type { CreatorSaveProfileResolver } from "./candidate-input.ts";
import type { CandidatePreparedRow } from "./candidate-prepare.ts";
import { projectCreatorCandidateReview } from "./candidate-projection.ts";
import { candidateReviewSummary, completeCreatorCandidateSave } from "./candidate-outcome.ts";
import { candidatePreparedPayload, candidateSelectedPayload, selectCandidateRoutes, type SelectedCandidateBatch } from "./candidate-selection.ts";
import type { CreatorRepository } from "./repository.ts";

const selectedIssuer = Symbol("selected-creator-save");
export class PreparedCreatorSaveAction implements OwnerReviewAuthority {
  readonly kind = "creator_save" as const;
  readonly deadline: number;
  readonly #rows: CandidatePreparedRow[];
  readonly #review: CreatorSaveReview;
  private constructor(rows: readonly CandidatePreparedRow[], review: CreatorSaveReview, deadline: number) { this.#rows = structuredClone([...rows]); this.#review = structuredClone(review); this.deadline = deadline; Object.freeze(this); }
  static create(rows: readonly CandidatePreparedRow[], review: CreatorSaveReview, deadline: number): PreparedCreatorSaveAction { return new PreparedCreatorSaveAction(rows, creatorSaveReview.parse(review), deadline); }
  encodedPayload(): Buffer { return candidatePreparedPayload(this.#rows, this.#review.intent); }
  select(selections: readonly CreatorSaveSelection[]): OwnerReviewAuthority {
    if (performance.now() >= this.deadline) throw new Error("creator_save_review_expired");
    const visible = new Map(this.#review.rows.map(row => [row.row_id, new Set(row.preview.routes.map(route => route.route_id))]));
    if (selections.some(selection => selection.confirmed_route_ids.some(id => !visible.get(selection.row_id)?.has(id)))) throw new Error("creator_save_routes");
    const selected = selectCandidateRoutes(this.#rows, this.#review.intent, selections);
    return new SelectedCreatorSaveAction(selectedIssuer, selected, this.#review, selections, this.deadline, this.#rows);
  }
  toJSON(): never { throw new Error("Creator save authority must not be serialized"); }
  [inspect.custom](): string { return `PreparedCreatorSaveAction { rows: ${this.#rows.length}, deadline: ${this.deadline} }`; }
}
class SelectedCreatorSaveAction implements OwnerReviewAuthority {
  readonly kind = "creator_save" as const;
  readonly deadline: number;
  readonly #selected: SelectedCandidateBatch;
  readonly #review: CreatorSaveReview;
  readonly #selections: readonly CreatorSaveSelection[];
  readonly #accounts: ReadonlyMap<string, CandidateAccount>;
  constructor(issuer: symbol, selected: SelectedCandidateBatch, review: CreatorSaveReview, selections: readonly CreatorSaveSelection[], deadline: number, rows: readonly CandidatePreparedRow[]) { if (issuer !== selectedIssuer) throw new Error("creator_save_untrusted_selection"); this.#selected = structuredClone(selected); this.#review = structuredClone(review); this.#selections = structuredClone(selections); this.#accounts = new Map(rows.map(row => [row.input.row_id, structuredClone(row.input.account)])); this.deadline = deadline; Object.freeze(this); }
  encodedPayload(): Buffer { return Buffer.from(candidateJson(candidateSelectedPayload(this.#selected))); }
  toJSON(): never { throw new Error("Selected Creator save authority must not be serialized"); }
  [inspect.custom](): string { return `SelectedCreatorSaveAction { rows: ${this.#selected.rows.length} }`; }
  static execute(grant: SubmittedOwnerGrant, account: AccountContext, repository: CreatorRepository): { completion: OwnerCallCompletion; changed: boolean } {
    const authority = grant.consume("creator_save", account); if (!(authority instanceof SelectedCreatorSaveAction)) throw new Error("Creator save grant has an invalid selected authority");
    const context = grant.context(), executionKey = `creator-save:${grant.binding().result_ref}:${context.call_id}`;
    try { const result = repository.candidateExecute(authority.#selected, executionKey); return { completion: completeCreatorCandidateSave(authority.#review, authority.#selections, result.result, context, grant.locale(), false, authority.#accounts), changed: result.changed }; }
    catch (error) { if (!(error instanceof CandidateCommitUnknownError)) throw error; return { completion: completeCreatorCandidateSave(authority.#review, authority.#selections, null, context, grant.locale(), false, authority.#accounts), changed: false }; }
  }
}
export type CreatorCandidateSavePreparation = { kind: "review"; review: PreparedOwnerReview } | { kind: "noop"; completion: OwnerCallCompletion; review_summary: ReturnType<typeof candidateReviewSummary> };
export async function prepareCreatorCandidateSave(raw: unknown, context: OwnerReviewContext, repository: CreatorRepository, resolver: CreatorSaveProfileResolver): Promise<CreatorCandidateSavePreparation> {
  OwnerReviewContext.assertIssued(context);
  const materialized = await materializeCreatorCandidates(raw, resolver), prepared = materialized.canonical.length ? repository.candidatePrepare(materialized.canonical, materialized.intent) : { rows: [], failures: [], intent: materialized.intent };
  const failures = [...materialized.failures, ...prepared.failures.map(failure => ({ row_id: failure.row_id, display_name: boundedCandidateText(failure.display_name), platform: failure.platform, handle: failure.handle, profile_url: failure.profile_url, reason: "preparation_failed" as const }))];
  for (const failure of prepared.failures) materialized.warnings.add("These selections are not in this review because their details could not be prepared for saving.", materialized.labels.get(failure.row_id) ?? null);
  const deadline = performance.now() + 15 * 60 * 1000, expires = BigInt(Math.floor(Date.now() / 1000) + 15 * 60);
  const projected = projectCreatorCandidateReview(prepared.rows, failures, prepared.intent, expires, materialized.warnings);
  if (projected.review.summary.eligible_new + projected.review.summary.eligible_update === 0) return { kind: "noop", completion: completeCreatorCandidateSave(projected.review, [], null, context, "en-US", true), review_summary: candidateReviewSummary(projected.review) };
  return { kind: "review", review: { context, action: PreparedCreatorSaveAction.create(prepared.rows, projected.review, deadline), review: projected.review, presentation: projected.presentation } };
}
export const executeSubmittedCreatorSave = (grant: SubmittedOwnerGrant, account: AccountContext, repository: CreatorRepository): { completion: OwnerCallCompletion; changed: boolean } => SelectedCreatorSaveAction.execute(grant, account, repository);
