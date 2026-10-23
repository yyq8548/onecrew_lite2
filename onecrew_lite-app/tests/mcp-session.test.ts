import assert from "node:assert/strict";
import test from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { McpSessions } from "../src/service/mcp-sessions.ts";
import { encodeMcpResult } from "../src/service/mcp-result.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";

test("independent MCP adapter sessions own result origins and expire on account changes", () => {
  const accounts = new AccountContextService(), principal = { user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const account = accounts.activate(principal), sessions = new McpSessions(accounts);
  const a = sessions.create(account, "full"), b = sessions.create(account, "full");
  try {
    const first = sessions.authorize(a.session_id), second = sessions.authorize(b.session_id);
    assert.notEqual(first.origin, second.origin);
    assert.equal(sessions.assertResultOrigin(a.session_id, first.origin), account);
    assert.throws(() => sessions.assertResultOrigin(b.session_id, first.origin), /origin_mismatch/);
    const calls = [{ call_id: "first", operation_id: "onecrew.creator.batch.create", schema_revision: "sha256:actual-schema", connection: null, arguments: {} }, { call_id: "second", operation_id: "onecrew.search.plan.prepare", schema_revision: "sha256:actual-schema", connection: null, arguments: {} }];
    const review = sessions.reviewContexts(a.session_id, calls);
    const frame = { outcomes: calls.map(call => ({ ...call, disposition: { kind: "not_run" } })) };
    review.contexts[0]!.validatePeer(frame, review.peer);
    assert.equal(review.contexts[0]!.origin, first.origin);
    assert.notEqual(review.contexts[0]!.call_digest, review.contexts[1]!.call_digest);
    assert.throws(() => sessions.reviewContexts(a.session_id, [calls[0]!, calls[0]!]), /batch_invalid/);
    sessions.revoke(b.session_id);
    assert.throws(() => sessions.authorize(b.session_id), /session_invalid/);
    accounts.activate(principal);
    assert.throws(() => sessions.authorize(a.session_id), /session_invalid/);
    assert.throws(() => sessions.create(account, "full"), /account changed/);
  } finally { sessions.close(); }
});

test("MCP wire keeps u64 exact across ordinary Host JSON parsing", () => {
  const snapshot = { binding: { view: { identity: `rv_${"a".repeat(32)}` }, revision: 18446744073709551615n }, followers: 18446744073709551615n };
  const payload = encodeMcpResult({ kind: "snapshot", snapshot }, snapshot);
  const transported = JSON.parse(JSON.stringify(payload));
  assert.equal(transported.structuredContent.application_json, transported.content[0].text);
  assert.equal(transported._meta["onecrew/result"], undefined);
  assert.equal(transported._meta["onecrew/lossless-required"], true);
  assert.deepEqual(parseApplicationJson(transported.content[0].text), { kind: "snapshot", snapshot });
  assert.deepEqual(parseApplicationJson(transported._meta["onecrew/result-json"]), snapshot);
  assert.equal(transported._meta.viewUUID, snapshot.binding.view.identity);
  const safe = encodeMcpResult({ kind: "snapshot", snapshot: { binding: { view: snapshot.binding.view, revision: 1n } } });
  assert.equal(JSON.parse(JSON.stringify(safe)).structuredContent.snapshot.binding.revision, 1);
});
