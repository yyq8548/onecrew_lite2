import assert from "node:assert/strict";
import { inspect } from "node:util";
import test from "node:test";
import { OwnerReviewContext, PeerResumePayload } from "../src/results/owner-context.ts";

test("owner context binds exact call identity and peer frame without accepting deserialized authority", () => {
  const context = new OwnerReviewContext("mcp:session", "save-1", "onecrew.creator.batch.create");
  const pending = { call_id: "save-1", operation_id: "onecrew.creator.batch.create", disposition: { kind: "not_run" } };
  context.validatePeer({ outcomes: [pending] });
  assert.throws(() => context.validateFrame({ outcomes: [] }), /call_missing/);
  assert.throws(() => context.validateFrame({ outcomes: [{ ...pending, disposition: { kind: "succeeded" } }] }), /call_mismatch/);
  assert.throws(() => context.validateFrame({ outcomes: [{ ...pending, operation_id: "onecrew.creator.delete" }] }), /call_mismatch/);
  assert.throws(() => context.validatePeer({ outcomes: [pending, pending] }), /peer_required/);
  assert.throws(() => OwnerReviewContext.assertIssued(JSON.parse(JSON.stringify(context))), /untrusted/);
  assert.throws(() => new OwnerReviewContext("bad scope", "save-1", "onecrew.creator.batch.create"), /origin_scope/);
  const input = Buffer.from("private peer continuation");
  const peer = new PeerResumePayload(input);
  const withPeer = new OwnerReviewContext("mcp:session", "save-1", "onecrew.creator.batch.create", Buffer.from("exact call"));
  withPeer.validatePeer({ outcomes: [pending, pending] }, peer);
  assert.throws(() => withPeer.validatePeer({ outcomes: [pending] }), /peer_context/);
  assert.throws(() => context.validatePeer({ outcomes: [pending] }, peer), /peer_context/);
  input.fill(0);
  assert.equal(peer.copyBytes().toString(), "private peer continuation");
  const copy = peer.copyBytes(); copy.fill(0);
  assert.equal(peer.copyBytes().toString(), "private peer continuation");
  assert.ok(!inspect(peer).includes("private peer continuation"));
  assert.throws(() => JSON.stringify(peer), /must not be serialized/);
  assert.throws(() => new PeerResumePayload(new Uint8Array()), /peer_resume_payload/);
});
