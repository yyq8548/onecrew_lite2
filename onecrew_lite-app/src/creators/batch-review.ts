import { inspect } from "node:util";
import type { AccountContext } from "../identity/account-context.ts";
import { OwnerReviewContext } from "../results/owner-context.ts";
import { executedCallOutcome, presentationProjection, terminalResult, type PresentationProjection } from "../results/contracts.ts";
import { SubmittedOwnerGrant, type OwnerCallCompletion, type OwnerReviewAuthority, type PreparedOwnerReview } from "../results/grants.ts";
import { batchInput, batchResult, creatorBatchPreview, creatorBatchWireInput, type CreatorBatchInput, type CreatorBatchResult } from "./batch.ts";
import { trim } from "./contracts.ts";
import type { CreatorRepository } from "./repository.ts";

export class PreparedCreatorBatchAction implements OwnerReviewAuthority {
  readonly kind = "creator_batch" as const;
  readonly deadline: number;
  readonly #intent: CreatorBatchInput;
  private constructor(input: CreatorBatchInput) {
    this.#intent = structuredClone(input);
    this.deadline = performance.now() + 15 * 60 * 1000;
    Object.freeze(this);
  }
  static prepare(input: unknown, context: OwnerReviewContext): PreparedOwnerReview {
    OwnerReviewContext.assertIssued(context);
    const parsed = batchInput.parse(input);
    if (!trim(parsed.idempotency_key) || Buffer.byteLength(parsed.idempotency_key) > 128) throw new Error("idempotency_key must contain 1-128 characters");
    return { context, action: new PreparedCreatorBatchAction(parsed), presentation: presentationProjection.parse(creatorBatchPreview(parsed)), prompt: "Create these Creator records from the submitted batch?", label: "Create Creators" };
  }
  encodedPayload(): Buffer { return Buffer.from(JSON.stringify(["creator_batch_authority_v1", creatorBatchWireInput(this.#intent)])); }
  toJSON(): never { throw new Error("Creator batch authority must not be serialized"); }
  [inspect.custom](): string { return `PreparedCreatorBatchAction { items: ${this.#intent.items.length}, deadline: ${this.deadline} }`; }
  static executeSubmitted(grant: SubmittedOwnerGrant, account: AccountContext, repository: CreatorRepository): CreatorBatchResult {
    const action = grant.consume("creator_batch", account);
    if (!(action instanceof PreparedCreatorBatchAction)) throw new Error("Creator batch grant has an invalid authority");
    return batchResult.parse(repository.batchCreate(action.#intent));
  }
}
export const prepareCreatorBatch = (input: unknown, context: OwnerReviewContext): PreparedOwnerReview => PreparedCreatorBatchAction.prepare(input, context);
export const executeSubmittedCreatorBatch = (grant: SubmittedOwnerGrant, account: AccountContext, repository: CreatorRepository): CreatorBatchResult => PreparedCreatorBatchAction.executeSubmitted(grant, account, repository);
function boundedLabel(value: string): string | null { let result = ""; for (const character of trim(value)) { if (Buffer.byteLength(result + character) > 8192) break; result += character; } return result || null; }
export function intoCreatorBatchOwnerCompletion(input: CreatorBatchResult, context: OwnerReviewContext): OwnerCallCompletion {
  OwnerReviewContext.assertIssued(context);
  const result = batchResult.parse(input), accepted = result.outcomes.filter(item => item.kind === "accepted"), rejected = result.outcomes.length - accepted.length, code = "creator_batch_completed";
  let summary = `The Creator batch completed: ${accepted.length} created, ${rejected} rejected.`;
  if (result.replayed) summary += " This result replayed the existing record; no new Creator was created.";
  const resource_refs = accepted.map(item => ({ kind: "creator" as const, id: item.creator.creator_id, label: boundedLabel(item.creator.display_name) }));
  const outcome = executedCallOutcome.parse({ call_id: context.call_id, operation_id: context.operation_id, disposition: { kind: "succeeded" }, summary, resource_refs });
  const presentation: PresentationProjection = presentationProjection.parse({ kind: "creator_batch_outcome", title: "Creator batch result", summary, sections: [{ kind: "fields", heading: null, fields: [{ label: "Status", value: code }, { label: "Created", value: String(accepted.length) }, { label: "Rejected", value: String(rejected) }, { label: "Replayed", value: String(result.replayed) }] }] });
  return { outcome, presentation, terminal: terminalResult.parse({ kind: "resolved", code, summary }), continuation: { message: summary, requires_model_context: true, context: { kind: "execution", outcomes: [{ ...outcome }] } }, output: result, continue_batch: true };
}
