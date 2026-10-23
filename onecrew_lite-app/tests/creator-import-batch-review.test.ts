import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { inspect } from "node:util";
import { AccountContextService } from "../src/identity/account-context.ts";
import { OwnerReviewContext } from "../src/results/owner-context.ts";
import { OwnerConfirmationStore } from "../src/results/grants.ts";
import { CreatorRepository } from "../src/creators/repository.ts";
import { executeSubmittedCreatorBatch, intoCreatorBatchOwnerCompletion, prepareCreatorBatch } from "../src/creators/batch-review.ts";

function database() { const db = new DatabaseSync(":memory:"); db.exec("PRAGMA foreign_keys=ON"); for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8")); db.exec("PRAGMA user_version=90"); return db; }
const principal = { user: { id: "batch-review-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
const frame = (context: OwnerReviewContext) => ({ outcomes: [{ call_id: context.call_id, operation_id: context.operation_id, disposition: { kind: "not_run" as const, reason: "owner review required" }, summary: "Review the Creator batch", resource_refs: [] }] });
test("Creator批次审核冻结全部输入/key，15分钟期限与展示预算保持，序列化及日志不泄露输入", () => {
  const context = new OwnerReviewContext("mcp", "call-1", "onecrew.creator.batch.create"), input = { idempotency_key: "original-key", validation: "strict", items: [{ display_name: "Original Creator", notes: "Private note", contact_routes: [{ channel: "email", value: "original@example.com" }] }] };
  const before = performance.now(), review = prepareCreatorBatch(input, context), after = performance.now(), sealed = review.action.encodedPayload();
  assert.ok(review.action.deadline >= before + 900000 && review.action.deadline <= after + 900000); assert.equal(review.prompt, "Create these Creator records from the submitted batch?"); assert.equal(review.label, "Create Creators");
  input.idempotency_key = "changed"; input.items[0]!.display_name = "Changed Creator"; input.items[0]!.contact_routes[0]!.value = "changed@example.com";
  assert.deepEqual(review.action.encodedPayload(), sealed); assert.throws(() => JSON.stringify(review.action), /must not be serialized/u); assert.ok(!inspect(review.action).includes("Original Creator")); assert.ok(!inspect(review.action).includes("original@example.com"));
  assert.deepEqual(JSON.parse(sealed.toString()), ["creator_batch_authority_v1", { idempotency_key: "original-key", validation: "strict", items: [{ display_name: "Original Creator", notes: "Private note", contact_routes: [{ channel: "email", value: "original@example.com", source_url: null }] }] }]);
  const oversized = prepareCreatorBatch({ idempotency_key: "50", validation: "strict", items: Array.from({ length: 50 }, () => ({ display_name: "名".repeat(4096), contact_routes: [{ channel: "email", value: `${"x".repeat(4096)}@example.com` }] })) }, context);
  assert.ok(Buffer.byteLength(JSON.stringify(oversized.presentation)) < 65536);
  const rows = oversized.presentation.sections[1]!; assert.equal(rows.kind, "items"); if (rows.kind !== "items") throw new Error("missing rows"); assert.equal(rows.items.length, 50); assert.ok(rows.items.every(row => Buffer.byteLength(row.title) <= 256 && Buffer.byteLength(row.description!) <= 512));
  assert.throws(() => prepareCreatorBatch({ ...input, confirmed: true }, context));
});
test("真实共享store提交单次授权，只执行冻结批次，终态Succeeded含完整Creator引用和continuation", async () => {
  const db = database(), accounts = new AccountContextService(), account = accounts.activate(principal), store = new OwnerConfirmationStore(accounts);
  try {
    const repository = new CreatorRepository(db), context = new OwnerReviewContext("mcp", "batch-call", "onecrew.creator.batch.create"), input = { idempotency_key: "sealed", validation: "permissive", items: [{ display_name: "Sealed name" }, { display_name: " " }] }, review = prepareCreatorBatch(input, context);
    const ref = store.mint(review, frame(context), account); assert.equal(repository.list({}).items.length, 0);
    const snapshot = store.present(ref, "mcp", account, "zh-CN"); assert.equal(snapshot.binding.revision, 2n);
    input.items[0]!.display_name = "Substituted name"; input.idempotency_key = "substituted-key";
    const submits = await Promise.allSettled([Promise.resolve().then(() => store.submit(snapshot.binding, account)), Promise.resolve().then(() => store.submit(snapshot.binding, account))]);
    assert.equal(submits.filter(result => result.status === "fulfilled").length, 1);
    const issued = submits.find(result => result.status === "fulfilled"); if (!issued || issued.status !== "fulfilled") throw new Error("grant missing"); const grant = issued.value;
    assert.equal(grant.locale(), "zh-CN"); assert.throws(() => JSON.stringify(grant), /must not be serialized/u);
    const result = executeSubmittedCreatorBatch(grant, account, repository); assert.deepEqual(result.outcomes.map(item => item.kind), ["accepted", "rejected"]); assert.equal(repository.list({}).items[0]!.display_name, "Sealed name"); assert.equal(db.prepare("SELECT key FROM idempotency_records").get()!.key, "sealed");
    assert.throws(() => executeSubmittedCreatorBatch(grant, account, repository), /consumed/u);
    const completion = intoCreatorBatchOwnerCompletion(result, grant.context()); assert.equal(completion.continue_batch, true); assert.deepEqual(completion.outcome.disposition, { kind: "succeeded" }); assert.equal(completion.outcome.resource_refs.length, 1); assert.equal(completion.outcome.summary, "The Creator batch completed: 1 created, 1 rejected."); assert.deepEqual(completion.output, result);
    const completed = store.complete(grant.binding(), account, { batch: { outcomes: [completion.outcome] }, presentation: completion.presentation, terminal: completion.terminal }); assert.equal(completed.state.kind, "resolved"); assert.equal(completed.binding.revision, 4n);
    const replay = intoCreatorBatchOwnerCompletion({ ...result, replayed: true }, context); assert.ok(replay.outcome.summary.endsWith("This result replayed the existing record; no new Creator was created.")); assert.equal(replay.outcome.resource_refs.length, 1);
  } finally { store.close(); db.close(); }
});
test("真实store隔离账号/调用frame并拒绝重复密封，换账号后原grant不能写入", () => {
  const db = database(), accounts = new AccountContextService(), account = accounts.activate(principal), store = new OwnerConfirmationStore(accounts);
  try {
    const repository = new CreatorRepository(db), context = new OwnerReviewContext("mcp", "guard-call", "onecrew.creator.batch.create"), input = { idempotency_key: "guard", validation: "strict", items: [{ display_name: "Guarded" }] }, review = prepareCreatorBatch(input, context);
    assert.throws(() => store.mint(review, { outcomes: [{ ...frame(context).outcomes[0]!, operation_id: "onecrew.creator.get" }] }, account), /call_mismatch/u);
    const ref = store.mint(review, frame(context), account); assert.throws(() => store.mint(review, frame(context), account), /already_admitted/u);
    const presented = store.present(ref, "mcp", account), grant = store.submit(presented.binding, account);
    const switched = accounts.activate({ ...principal, user: { id: "other", email: "other@example.com" } });
    assert.throws(() => executeSubmittedCreatorBatch(grant, switched, repository), /account changed|account_changed/u); assert.equal(repository.list({}).items.length, 0);
  } finally { store.close(); db.close(); }
});
