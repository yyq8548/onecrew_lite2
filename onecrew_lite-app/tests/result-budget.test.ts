import assert from "node:assert/strict";
import test from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { prepareCreatorBatch } from "../src/creators/batch-review.ts";
import { OwnerReviewContext } from "../src/results/owner-context.ts";
import { OwnerConfirmationStore } from "../src/results/grants.ts";
import { ResultChoiceStore } from "../src/results/choice-store.ts";
import { prepareChoice } from "../src/results/choice-flow.ts";
import { ResultStoreBudget } from "../src/results/budget.ts";

test("Choice and sealed Creator review share one process row limit and release ownership exactly once", () => {
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const budget = new ResultStoreBudget(1), owner = new OwnerConfirmationStore(accounts, budget), choice = new ResultChoiceStore(accounts, {}, budget);
  const context = new OwnerReviewContext("mcp:full", "batch", "onecrew.creator.batch.create"), frame = { outcomes: [{ call_id: context.call_id, operation_id: context.operation_id, disposition: { kind: "not_run" as const, reason: "confirmation_required" }, summary: "Review batch", resource_refs: [] }] };
  const question = prepareChoice({ interaction_id: "choice", title: "Choose", questions: [{ id: "q", header: "Choice", prompt: "Choose an option", options: [{ id: "a", label: "First" }, { id: "b", label: "Second" }] }] }, new OwnerReviewContext("mcp:full", "choice", "onecrew.ask_user_question"));
  try {
    const ref = owner.mint(prepareCreatorBatch({ idempotency_key: "batch", validation: "strict", items: [{ display_name: "Creator" }] }, context), frame, account);
    const shown = owner.present(ref, context.origin, account);
    assert.throws(() => choice.mint(question, account), /result_row_limit/);
    assert.equal(owner.read(shown.binding, account).state.kind, "available");
    owner.close(); owner.close();
    const choiceRef = choice.mint(question, account);
    assert.equal(choice.present(choiceRef, question.context.origin, account).state.kind, "available");
    choice.close(); choice.close();
  } finally { owner.close(); choice.close(); }
});

test("combined private payload bytes cannot exceed the shared limit or evict another pending review", () => {
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const budget = new ResultStoreBudget(100, 128 * 1024), owner = new OwnerConfirmationStore(accounts, budget), choice = new ResultChoiceStore(accounts, {}, budget);
  const context = new OwnerReviewContext("mcp:full", "batch", "onecrew.creator.batch.create"), frame = { outcomes: [{ call_id: context.call_id, operation_id: context.operation_id, disposition: { kind: "not_run" as const, reason: "confirmation_required" }, summary: "Review batch", resource_refs: [] }] };
  const question = prepareChoice({ interaction_id: "choice", title: "Choose", questions: [{ id: "q", header: "Choice", prompt: "A".repeat(6000), options: [{ id: "a", label: "First" }, { id: "b", label: "Second" }] }] }, new OwnerReviewContext("mcp:full", "choice", "onecrew.ask_user_question"));
  try {
    const ref = owner.mint(prepareCreatorBatch({ idempotency_key: "large-payload", validation: "strict", items: [{ display_name: "Creator", notes: "B".repeat(120000) }] }, context), frame, account), shown = owner.present(ref, context.origin, account);
    assert.throws(() => choice.mint(question, account), /result_payload_limit/);
    assert.equal(owner.read(shown.binding, account).state.kind, "available");
    owner.close();
    assert.ok(choice.mint(question, account));
  } finally { owner.close(); choice.close(); }
});
