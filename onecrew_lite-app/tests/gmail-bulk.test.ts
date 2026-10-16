import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";
import { bulkIdempotencyKey, claimBulkItem, completeIfSettled, controlBulk, duplicateAttempt, interruptBulk, mailFingerprint, normalizeEmailAddress, prepareBulk, recordBulkResult, requireFreshGmailAttempt } from "../src/mail/gmail-bulk.ts";
import type { BulkBatch, BulkItem, BulkItemState, BulkMailbox, PrepareBulkInput } from "../src/mail/gmail-bulk.ts";

const mailbox: BulkMailbox = { id: "mailbox", provider: "gmail", email_address: "Sender@Example.com", available: true, capabilities: ["send", "sent_readback"] };
function input(): PrepareBulkInput { return { mailbox_ref_id: "mailbox", subject: " 你好 ", body: " 正文\n ", recipients: [{ email: "Person <USER@Example.com>", source: "来源" }] }; }
function item(state: BulkItemState = "pending", id = "item"): BulkItem { return { id, recipient: "user@example.com", source: "source", fingerprint: "fingerprint", state, provider_message_id: null, detail: null }; }
function batch(items: readonly BulkItem[] = [item()]): BulkBatch { return { id: "batch", mailbox_ref_id: "mailbox", subject: "subject", body: "body", state: "draft", created_at: 1, items }; }

test("冻结正文，按 Rust 地址规则规范、去重并保留首个来源", () => {
  const request = input();
  request.recipients = [...request.recipients, { email: "user@example.com", source: "x".repeat(2049) }];
  const prepared = prepareBulk(request, mailbox);
  assert.equal(prepared.subject, "你好");
  assert.equal(prepared.body, "正文");
  assert.equal(prepared.items.length, 1);
  assert.equal(prepared.items[0]?.recipient, "user@example.com");
  assert.equal(prepared.items[0]?.source, "来源");
  const expected = createHash("sha256").update('["sender@example.com","user@example.com","你好","正文"]').digest("hex");
  assert.equal(prepared.items[0]?.fingerprint, expected);
  assert.equal(request.subject, " 你好 ");
  assert.equal(normalizeEmailAddress(" A B@EXAMPLE.COM "), "a b@example.com");
  assert.equal(normalizeEmailAddress("名 <Ä@EXAMPLE.COM>"), "Ä@example.com");
  assert.throws(() => normalizeEmailAddress("x@y@z"), /invalid/);
  assert.throws(() => normalizeEmailAddress("@domain"), /invalid/);
});

test("指纹包含发件人收件人主题正文，来源不影响指纹", () => {
  const original = mailFingerprint("A@X", "B@Y", "主题", "正文");
  assert.equal(original, mailFingerprint("a@x", "b@y", "主题", "正文"));
  for (const values of [["c@x", "b@y", "主题", "正文"], ["a@x", "c@y", "主题", "正文"], ["a@x", "b@y", "新主题", "正文"], ["a@x", "b@y", "主题", "新正文"]] as const) assert.notEqual(original, mailFingerprint(values[0], values[1], values[2], values[3]));
});

test("人数按去重前验证，主题正文和来源限制计算 UTF-8 字节", () => {
  assert.throws(() => prepareBulk({ ...input(), recipients: [] }, mailbox), /1–100/);
  assert.throws(() => prepareBulk({ ...input(), recipients: Array.from({ length: 101 }, () => input().recipients[0]!) }, mailbox), /1–100/);
  assert.equal(prepareBulk({ ...input(), recipients: Array.from({ length: 100 }, () => input().recipients[0]!) }, mailbox).items.length, 1);
  for (const subject of [" ", "a\nb", "a\rb", "中".repeat(333)]) assert.throws(() => prepareBulk({ ...input(), subject }, mailbox), /subject or body/);
  assert.doesNotThrow(() => prepareBulk({ ...input(), subject: "a".repeat(998), body: "a".repeat(100000) }, mailbox));
  for (const body of [" ", "a".repeat(100001)]) assert.throws(() => prepareBulk({ ...input(), body }, mailbox), /subject or body/);
  assert.throws(() => prepareBulk({ ...input(), recipients: [{ email: "a@b", source: "中".repeat(683) }] }, mailbox), /source/);
  assert.equal(prepareBulk({ ...input(), subject: "\ufeff" }, mailbox).subject, "\ufeff");
  assert.throws(() => prepareBulk({ ...input(), body: "\ud800" }, mailbox), /Unicode/);
});

test("发送邮箱必须匹配并具备 Gmail 可用性及发送回读能力", () => {
  for (const changed of [{ id: "other" }, { provider: "resend" }, { available: false }, { capabilities: ["send"] }, { capabilities: ["sent_readback"] }]) assert.throws(() => prepareBulk(input(), { ...mailbox, ...changed }));
});

test("草稿不领取，确认启动，暂停恢复，取消只影响待发送项", () => {
  assert.throws(() => claimBulkItem(batch(), "item", []), /not claimable/);
  const running = controlBulk(batch([item(), item("attempting", "inflight")]), "confirm");
  const paused = controlBulk(running, "pause");
  assert.throws(() => claimBulkItem(paused, "item", []), /not claimable/);
  assert.equal(controlBulk(paused, "resume").state, "running");
  const cancelled = controlBulk(running, "cancel");
  assert.deepEqual(cancelled.items.map(entry => entry.state), ["cancelled", "attempting"]);
  assert.equal(controlBulk(cancelled, "cancel"), cancelled);
  assert.equal(controlBulk(running, "confirm"), running);
  assert.equal(controlBulk(running, "resume"), running);
  assert.equal(controlBulk(paused, "pause"), paused);
  assert.throws(() => controlBulk(batch(), "resume"), /invalid/);
  assert.throws(() => controlBulk({ ...batch(), state: "completed" }, "confirm"), /invalid/);
});

test("中断保留未知项，恢复仅允许 pending，尝试标记阻止重新发送", () => {
  const interrupted = interruptBulk({ ...batch([item("attempting"), item("pending", "remaining")]), state: "running" });
  assert.equal(interrupted.state, "paused");
  assert.equal(interrupted.items[0]?.state, "unknown");
  const resumed = controlBulk(interrupted, "resume");
  assert.throws(() => claimBulkItem(resumed, "item", []), /not claimable/);
  assert.equal(claimBulkItem(resumed, "remaining", []).state, "attempting");
  const key = bulkIdempotencyKey(item());
  assert.equal(key, "gmail-bulk-item");
  assert.doesNotThrow(() => requireFreshGmailAttempt(key, new Set()));
  assert.throws(() => requireFreshGmailAttempt(key, new Set([key])), /automatic resend is disabled/);
});

test("跨批次查重只阻止已经尝试的效果状态", () => {
  const running = controlBulk(batch(), "confirm");
  for (const state of ["attempting", "accepted", "sent", "unknown"] as const) {
    assert.equal(duplicateAttempt(item(), [item(state, "prior")]), true);
    assert.equal(claimBulkItem(running, "item", [item(state, "prior")]).state, "failed");
  }
  for (const state of ["pending", "failed", "cancelled"] as const) assert.equal(claimBulkItem(running, "item", [item(state, "prior")]).state, "attempting");
  assert.equal(duplicateAttempt(item(), [item("sent")]), false);
});

test("成功、接受、失败和未知结果分别保存，completed 不代表全成功", () => {
  const attempting = item("attempting");
  const sent = recordBulkResult(attempting, { kind: "success", provider_message_id: "provider-id" });
  assert.equal(sent.provider_message_id, "provider-id");
  assert.equal(sent.state, "sent");
  for (const [effect, state] of [["not_sent", "failed"], ["sent", "accepted"], ["unknown", "unknown"]] as const) {
    const result = recordBulkResult(attempting, { kind: "failure", effect, message: "detail" });
    assert.equal(result.state, state);
    assert.equal(result.detail, "detail");
    assert.equal(completeIfSettled({ ...batch([result]), state: "running" }).state, "completed");
  }
  assert.equal(completeIfSettled({ ...batch(), state: "running" }).state, "running");
  assert.throws(() => recordBulkResult(item("unknown"), { kind: "success", provider_message_id: "id" }), /active send/);
});
