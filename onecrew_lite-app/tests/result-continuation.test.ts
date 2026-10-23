import assert from "node:assert/strict";
import test from "node:test";
import { ResultContinuation } from "../src/results/continuation.ts";

test("continuation leases preserve explicit pause retry and unknown-delivery boundaries", () => {
  const continuation = new ResultContinuation();
  const request = { message: "Creator records were created.", context: { kind: "execution" as const, outcomes: [{ call_id: "create-1", operation_id: "onecrew.creator.batch.create", disposition: { kind: "succeeded" as const }, summary: "One Creator created.", resource_refs: [] }] }, requires_model_context: true };
  continuation.apply(request);
  request.message = "changed after storing";
  const first = continuation.claim(2n, "pending");
  assert.equal(first.request.message, "Creator records were created.");
  assert.equal(first.dispatch_not_after_ms - first.dispatch_started_at_ms, 120000);
  assert.match(first.lease_id, /^rcl_[0-9a-f]{32}$/);
  assert.throws(() => continuation.claim(2n, "pending"), /revision_conflict/);
  assert.throws(() => continuation.complete(3n, "rcl_wrong", "delivered"), /lease_mismatch/);
  assert.equal(continuation.complete(3n, first.lease_id, "paused").state, "paused");
  assert.throws(() => continuation.claim(4n, "pending"), /explicit_mode/);
  const second = continuation.claim(4n, "retry_paused");
  assert.notEqual(second.lease_id, first.lease_id);
  assert.equal(continuation.complete(5n, second.lease_id, "delivery_unknown").state, "delivery_unknown");
  assert.throws(() => continuation.claim(6n, "retry_paused"), /not_claimable/);
  assert.notEqual(continuation.heldRequest(), null);
});

test("delivered continuation erases retained request and cannot dispatch twice", () => {
  const continuation = new ResultContinuation();
  continuation.apply({ message: "The user cancelled this action.", context: { kind: "cancelled" }, requires_model_context: false });
  const claimed = continuation.claim(2n, "pending");
  assert.equal(continuation.complete(3n, claimed.lease_id, "delivered").state, "delivered");
  assert.equal(continuation.heldRequest(), null);
  assert.throws(() => continuation.claim(4n, "pending"), /not_claimable/);
  assert.throws(() => continuation.complete(4n, claimed.lease_id, "delivered"), /not_dispatching/);
});
