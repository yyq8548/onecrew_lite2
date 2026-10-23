import assert from "node:assert/strict";
import { inspect } from "node:util";
import { test } from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { OwnerReviewContext } from "../src/results/owner-context.ts";
import { PeerResumePayload } from "../src/results/owner-context.ts";
import { ResultChoiceStore, SubmittedChoiceGrant } from "../src/results/choice-store.ts";
import { resultChoiceAnswer, resultChoiceQuestions, validateChoiceAnswers } from "../src/results/choice-contracts.ts";
import type { ChoiceInput, ChoiceTerminalUpdate } from "../src/results/choice-store.ts";
import type { ResultChoiceAnswer } from "../src/results/choice-contracts.ts";

const principal = { user: { id: "choice-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
function input(call = "choice-call"): ChoiceInput { const context = new OwnerReviewContext("mcp", call, "onecrew.choice"); return { context, payload: Buffer.from("private-choice-context"), questions: resultChoiceQuestions.parse([{ id: "route", header: "Sources", prompt: "Choose public sources", selection_mode: "multiple", options: [{ id: "x", label: "X" }, { id: "youtube", label: "YouTube" }] }, { id: "detail", header: "Depth", prompt: "Choose detail", options: [{ id: "full", label: "Full" }] }]), batch: { outcomes: [{ call_id: context.call_id, operation_id: context.operation_id, disposition: { kind: "not_run", reason: "choice_required" }, summary: "Choose public reading sources", resource_refs: [] }] }, presentation: { kind: "choice", title: "Public reading choice", summary: null, sections: [] } }; }
const answers: ResultChoiceAnswer[] = [{ question_id: "detail", text: "原公开资料\nwith detail\t" }, { question_id: "route", option_ids: ["youtube", "x"] }];
function update(value: ChoiceInput, ordered: ResultChoiceAnswer[]): ChoiceTerminalUpdate { return { batch: { outcomes: value.batch.outcomes.map(call => ({ ...call, disposition: { kind: "succeeded" }, summary: "User choices recorded" })) }, presentation: { kind: "choice_result", title: "Choice recorded", summary: null, sections: [] }, terminal: { kind: "resolved", code: "choice_recorded", summary: "User choices recorded" }, continuation: { message: "Continue with the user's choices.", requires_model_context: true, context: { kind: "choice", answers: ordered } } }; }

test("原Choice问题/答案封闭形状、Unicode字节、single/multiple/custom及原问题顺序保持", () => {
  const questions = input().questions;
  assert.deepEqual(validateChoiceAnswers(questions, answers), [{ question_id: "route", option_ids: ["x", "youtube"] }, answers[0]]);
  assert.equal(questions[1]!.selection_mode, "single"); assert.equal(questions[0]!.options[0]!.description, null);
  for (const invalid of [[], [{ question_id: "detail", option_ids: ["full"] }, answers[1]], [{ question_id: "detail", option_id: "full" }, { question_id: "route", option_ids: ["x", "x"] }], [{ question_id: "detail", option_id: "full" }, { question_id: "route", option_ids: ["other"] }]]) assert.throws(() => validateChoiceAnswers(questions, invalid));
  for (const invalid of [{ question_id: "q", option_id: "a", text: "ambiguous" }, { question_id: "q", text: "x\ry" }, { question_id: "q", text: "界".repeat(683) }, { question_id: "q", text: "   " }, { question_id: "q", option_ids: ["a"], extra: true }]) assert.equal(resultChoiceAnswer.safeParse(invalid).success, false);
  const many = resultChoiceQuestions.parse(Array.from({ length: 32 }, (_, i) => ({ ...questions[1], id: `q${i}` })));
  assert.throws(() => validateChoiceAnswers(many, many.map(question => ({ question_id: question.id, text: "x".repeat(1024) }))), /choice_answers_too_large/);
});

test("真实account context下Choice CAS/私有单次grant/精确continuation及失效重放", async () => {
  const accounts = new AccountContextService(), account = accounts.activate(principal), store = new ResultChoiceStore(accounts);
  try {
    const value = input(), ref = store.mint(value, account); value.payload[0] = 0; value.questions[0]!.options[0]!.label = "changed";
    assert.throws(() => store.present(ref, "foreign-origin", account), /origin_mismatch/);
    const shown = store.present(ref, "mcp", account, "zh-CN"); assert.equal(shown.binding.revision, 2n); assert.equal(shown.locale, "zh-CN"); assert.equal(shown.pending_action!.questions[0]!.options[0]!.label, "X"); assert.ok(!JSON.stringify(shown, (_key, v) => typeof v === "bigint" ? String(v) : v).includes("private-choice-context"));
    assert.throws(() => store.present(ref, "mcp", account), /already_presented/);
    const before = store.read(shown.binding, account); assert.throws(() => store.answer(shown.binding, account, [])); assert.deepEqual(store.read(shown.binding, account), before);
    const submissions = await Promise.allSettled([Promise.resolve().then(() => store.answer(shown.binding, account, answers)), Promise.resolve().then(() => store.answer(shown.binding, account, answers))]); assert.equal(submissions.filter(item => item.status === "fulfilled").length, 1);
    const result = submissions.find(item => item.status === "fulfilled"); if (!result || result.status !== "fulfilled") throw new Error("Choice grant missing"); const grant = result.value;
    assert.ok(!inspect(grant).includes("private-choice-context")); assert.throws(() => JSON.stringify(grant), /must not be serialized/);
    const held = grant.consume(account); assert.equal(held.payload.toString(), "private-choice-context"); assert.deepEqual(held.answers, validateChoiceAnswers(input().questions, answers)); assert.throws(() => grant.consume(account), /consumed/);
    assert.throws(() => store.read(shown.binding, account), /revision_conflict/); assert.equal(store.current(shown.binding, account).state.kind, "submitted");
    const terminal = update(input(), held.answers), wrong = structuredClone(terminal); if (wrong.continuation?.context.kind !== "choice") throw new Error("Choice continuation missing"); wrong.continuation.context.answers.reverse();
    assert.throws(() => store.complete(grant.binding(), account, wrong), /choice_continuation_answers/); assert.equal(store.read(grant.binding(), account).state.kind, "submitted");
    const done = store.complete(grant.binding(), account, terminal); assert.equal(done.binding.revision, 4n); assert.equal(done.state.kind, "resolved"); assert.equal(done.pending_action, null); assert.equal(done.continuation.state, "pending"); assert.throws(() => store.answer(done.binding, account, answers), /consumed/);
    const lease = store.claimContinuation(done.binding, account, done.continuation.continuation_revision, "pending"); assert.deepEqual(lease.request, terminal.continuation); assert.throws(() => store.claimContinuation(done.binding, account, done.continuation.continuation_revision, "pending"));
    const paused = store.completeContinuation(done.binding, account, lease.continuation_revision, lease.lease_id, "paused"); assert.throws(() => store.claimContinuation(done.binding, account, paused.continuation.continuation_revision, "pending"), /explicit_mode/);
    const retry = store.claimContinuation(done.binding, account, paused.continuation.continuation_revision, "retry_paused"), delivered = store.completeContinuation(done.binding, account, retry.continuation_revision, retry.lease_id, "delivered"); assert.equal(delivered.continuation.state, "delivered");
  } finally { store.close(); }
});

test("Choice阻止猜测binding、账号epoch置换、重启与伪造grant", () => {
  const accounts = new AccountContextService(), account = accounts.activate(principal), store = new ResultChoiceStore(accounts);
  try {
    const shown = store.present(store.mint(input(), account), "mcp", account);
    for (const binding of [{ ...shown.binding, origin: "foreign" }, { ...shown.binding, view: { ...shown.binding.view, kind: "confirmation" as const } }, { ...shown.binding, fingerprint: `sha256:${"0".repeat(64)}` }, { ...shown.binding, revision: 0n }, { ...shown.binding, revision: 100n }]) assert.throws(() => store.answer(binding, account, answers));
    const grant = store.answer(shown.binding, account, answers), next = accounts.activate(principal); assert.notEqual(next.epoch, account.epoch); assert.throws(() => grant.consume(account), /account changed/); assert.throws(() => store.read(grant.binding(), next), /account_changed/);
    const restarted = new ResultChoiceStore(accounts); try { assert.throws(() => restarted.read(grant.binding(), next), /absent/); } finally { restarted.close(); }
    assert.throws(() => new SubmittedChoiceGrant(Symbol("forged"), { owner: next, binding: grant.binding(), context: input().context, answers: [], payload: Buffer.from("forged"), peer: undefined, validate() {} }), /untrusted/);
  } finally { store.close(); }
});

test("Choice混合批次取消及终态保持已完成调用、详情前缀和原continuation，无发送能力", () => {
  const accounts = new AccountContextService(), account = accounts.activate(principal), store = new ResultChoiceStore(accounts);
  try {
    const value = input("mixed"), completed = { call_id: "earlier", operation_id: "onecrew.creator.get", disposition: { kind: "succeeded" as const }, summary: "Already read Creator", resource_refs: [{ kind: "creator" as const, id: "creator-one", label: "Saved Creator" }] }, section = { kind: "notice" as const, heading: null, body: "Earlier public result retained", tone: "success" as const };
    value.batch.outcomes.unshift(completed); value.presentation.sections.push(section); value.completed_sections_count = 1;
    const shown = store.present(store.mint(value, account), "mcp", account, "zh-CN"), cancelled = store.cancel(shown.binding, account, { message: "Cancel remaining choices", context: { kind: "cancelled" }, requires_model_context: true });
    assert.deepEqual(cancelled.batch.outcomes[0], completed); assert.deepEqual(cancelled.presentation.sections[0], section); assert.equal(cancelled.completed_sections_count, 1); assert.equal(cancelled.batch.outcomes[1]!.disposition.kind, "not_run"); assert.equal(cancelled.state.kind, "resolved");
    const lease = store.claimContinuation(cancelled.binding, account, cancelled.continuation.continuation_revision, "pending"); assert.equal(lease.request.context.kind, "execution"); assert.ok(lease.request.message.includes("先前已完成的调用不应重复"));
    assert.throws(() => store.cancel(cancelled.binding, account), /consumed/);
    const shown2 = store.present(store.mint({ ...value, context: new OwnerReviewContext("mcp", "mixed", "onecrew.choice") }, account), "mcp", account), grant = store.answer(shown2.binding, account, answers), held = grant.consume(account), terminal = update(value, held.answers); terminal.presentation.sections = [section];
    const bad = structuredClone(terminal); bad.batch.outcomes[0]!.summary = "rewritten earlier read"; assert.throws(() => store.complete(grant.binding(), account, bad), /completed_call_mismatch/);
    terminal.batch.outcomes[0] = completed; assert.equal(store.complete(grant.binding(), account, terminal).state.kind, "resolved");
  } finally { store.close(); }
});

test("Choice原32选项上限全部保留，peer单次私有交接不进入快照", () => {
  const accounts = new AccountContextService(), account = accounts.activate(principal), store = new ResultChoiceStore(accounts);
  try {
    const value = input("peer-choice"), bytes = Buffer.from(JSON.stringify({ calls: [{ id: "peer-choice", operation_id: "onecrew.choice", input: { questions: [] } }] }));
    value.context = new OwnerReviewContext("mcp", "peer-choice", "onecrew.choice", bytes); value.peer = new PeerResumePayload(bytes); value.payload = Buffer.alloc(0);
    value.questions = resultChoiceQuestions.parse([{ id: "all", header: "Select public evidence", prompt: "Select the retained sources", selection_mode: "multiple", options: Array.from({ length: 32 }, (_, index) => ({ id: `option-${index}`, label: `Source ${index}` })) }]);
    const shown = store.present(store.mint(value, account), "mcp", account), ids = value.questions[0]!.options.map(option => option.id);
    const grant = store.answer(shown.binding, account, [{ question_id: "all", option_ids: [...ids].reverse() }]), result = grant.consume(account); assert.deepEqual(result.answers, [{ question_id: "all", option_ids: ids }]); assert.equal(result.payload.length, 0); assert.deepEqual(result.peer?.copyBytes(), bytes);
    assert.equal(JSON.stringify(shown, (_key, value) => typeof value === "bigint" ? String(value) : value).includes("calls"), false); assert.throws(() => grant.consume(account), /consumed/);
  } finally { store.close(); }
});

test("Choice真实时间TTL释放available/resolved，submitted和unknown仍保留，容量失败原子且不驱逐", async () => {
  const accounts = new AccountContextService(), account = accounts.activate(principal), store = new ResultChoiceStore(accounts, { max_results: 1, available_ttl_ms: 100, terminal_ttl_ms: 0 });
  try {
    const value = input("retention"), shown = store.present(store.mint(value, account), "mcp", account), grant = store.answer(shown.binding, account, answers), held = grant.consume(account);
    await new Promise(resolve => setTimeout(resolve, 120)); assert.equal(store.cleanup().removed, 0); assert.equal(store.current(shown.binding, account).state.kind, "submitted");
    assert.throws(() => store.mint(input("second"), account), /row_limit/);
    const terminal = update(value, held.answers); terminal.terminal = { kind: "unknown", code: "unverified", summary: "Readback outcome is unknown" };
    const done = store.complete(grant.binding(), account, terminal); assert.equal(store.cleanup().removed, 0); assert.equal(store.read(done.binding, account).state.kind, "unknown"); assert.throws(() => store.mint(input("third"), account), /row_limit/);
    const empty = new ResultChoiceStore(accounts, { max_total_payload_bytes: 1 }); try { assert.throws(() => empty.mint(input(), account), /payload_limit/); assert.deepEqual(empty.cleanup(), { removed: 0, active: 0, payload_bytes: 0 }); } finally { empty.close(); }
    const expires = new ResultChoiceStore(accounts, { available_ttl_ms: 0 }); try { const ref = expires.mint(input(), account); assert.equal(expires.cleanup().removed, 1); assert.throws(() => expires.present(ref, "mcp", account), /absent/); } finally { expires.close(); }
    const resolved = new ResultChoiceStore(accounts, { terminal_ttl_ms: 0 }); try { const ref = resolved.mint(input(), account), shown = resolved.present(ref, "mcp", account), cancelled = resolved.cancel(shown.binding, account); assert.equal(cancelled.state.kind, "resolved"); assert.equal(resolved.cleanup().removed, 1); } finally { resolved.close(); }
  } finally { store.close(); }
});

test("Choice终态容量拒绝不改变submitted快照、continuation或记录计数", () => {
  const accounts = new AccountContextService(), account = accounts.activate(principal), probe = new ResultChoiceStore(accounts); probe.mint(input(), account); const ceiling = probe.cleanup().payload_bytes + 64; probe.close();
  const store = new ResultChoiceStore(accounts, { max_total_payload_bytes: ceiling });
  try {
    const value = input(), shown = store.present(store.mint(value, account), "mcp", account), grant = store.answer(shown.binding, account, answers), held = grant.consume(account), before = store.read(grant.binding(), account), metrics = store.cleanup(), terminal = update(value, held.answers);
    terminal.presentation.title = "x".repeat(4000); assert.throws(() => store.complete(grant.binding(), account, terminal), /payload_limit/); assert.deepEqual(store.read(grant.binding(), account), before); assert.deepEqual(store.cleanup(), metrics);
    assert.throws(() => store.claimContinuation(grant.binding(), account, before.continuation.continuation_revision, "pending"), /requires_terminal/);
    store.complete(grant.binding(), account, { ...update(value, held.answers), continuation: null });
  } finally { store.close(); }
});
