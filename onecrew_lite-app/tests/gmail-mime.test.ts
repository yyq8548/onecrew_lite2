import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";
import { simpleParser } from "mailparser";
import { compileGmailBulkMessage, gmailMessageId, messageForGmailBulkClaim, verifyGmailBulkReadback } from "../src/mail/gmail-mime.ts";
import { mailFingerprint } from "../src/mail/gmail-bulk.ts";
import type { BulkBatch, BulkItem } from "../src/mail/gmail-bulk.ts";
import { decodeGmailResponse, gmailReadRetryPolicy, sendGmailBulkMessage } from "../src/mail/gmail-transport.ts";
import type { GmailBulkMessage } from "../src/mail/gmail-mime.ts";

const original: GmailBulkMessage = { from: "sender@example.com", to: "recipient@example.com", subject: "你好，批量邮件", body: "第一行\n第二行\nhttps://example.com/正文链接", idempotencyKey: "gmail-bulk-7cb6083c-7451-4635-a517-e2d635bc0d7f" };

test("真实 MailComposer 编译及 MailParser 回读，单收件人、Unicode与冻结内容完整", async () => {
  const compiled = await compileGmailBulkMessage(original);
  const parsed = await simpleParser(compiled.bytes);
  assert.equal(parsed.subject, original.subject);
  assert.equal(parsed.from?.value[0]?.address, original.from);
  assert.equal(Array.isArray(parsed.to) ? parsed.to[0]?.value.length : parsed.to?.value.length, 1);
  assert.equal(parsed.cc, undefined);
  assert.equal(parsed.bcc, undefined);
  assert.equal(parsed.attachments.length, 0);
  assert.equal(parsed.text?.trimEnd(), original.body);
  const digest = createHash("sha256").update(`${original.from}\0${original.idempotencyKey}`).digest("hex");
  assert.equal(parsed.messageId, `<${digest}@onecrew.local>`);
  assert.equal(gmailMessageId(original.from, original.idempotencyKey), compiled.messageId);
  assert.equal((await compileGmailBulkMessage(original)).messageId, compiled.messageId);
  assert.equal(compiled.bytes.toString("base64url"), compiled.raw);
  await verifyGmailBulkReadback({ id: "abc123", raw: compiled.raw, labelIds: ["SENT"], internalDate: "1791786972000" }, "abc123", original);
});

test("禁止附件、文件URL输入、额外头、多个收件人及头注入", async () => {
  for (const changed of [
    { attachments: [{ path: "file:///private" }] }, { body: { path: "https://example.com/private" } },
    { headers: { Bcc: "hidden@example.com" } }, { to: "a@example.com,b@example.com" },
    { from: "sender@example.com\r\nBcc: hidden@example.com" }, { subject: "subject\r\nBcc: hidden@example.com" },
    { cc: ["hidden@example.com"] }, { body: "\ud800" },
  ]) await assert.rejects(compileGmailBulkMessage({ ...original, ...changed } as GmailBulkMessage));
});

test("实际编译回读核对消息id、SENT标签、发件人收件人主题正文", async () => {
  const compiled = await compileGmailBulkMessage(original);
  for (const metadata of [{ id: "def456", raw: compiled.raw, labelIds: ["SENT"] }, { id: "abc123", raw: compiled.raw, labelIds: ["INBOX"] }, { id: "abc123", raw: compiled.raw, labelIds: ["SENT", "DRAFT"] }]) await assert.rejects(verifyGmailBulkReadback({ ...metadata, internalDate: "1791786972000" }, "abc123", original));
  for (const changed of [{ from: "other@example.com" }, { to: "other@example.com" }, { subject: "改动主题" }, { body: "改动正文" }, { idempotencyKey: "other-key" }]) {
    const different = await compileGmailBulkMessage({ ...original, ...changed });
    await assert.rejects(verifyGmailBulkReadback({ id: "abc123", raw: different.raw, labelIds: ["SENT"], internalDate: "1791786972000" }, "abc123", original));
  }
});

test("原重试上限及 Retry-After 规则使用真实运算核对，无网络", () => {
  assert.equal(gmailReadRetryPolicy(429, null, 2).retry, true);
  assert.equal(gmailReadRetryPolicy(429, null, 3).retry, false);
  assert.equal(gmailReadRetryPolicy(503, null, 1).retry, true);
  assert.equal(gmailReadRetryPolicy(503, null, 2).retry, false);
  assert.equal(gmailReadRetryPolicy(403, "rateLimitExceeded", 2).retry, true);
  for (const [status, reason] of [[403, "domainPolicy"], [429, "retry_after"], [500, "retry_after"], [401, null], [null, null]] as const) assert.equal(gmailReadRetryPolicy(status, reason, 0).retry, false);
});

test("已取消请求在任何网络操作前明确拒绝", async () => {
  const signal = AbortSignal.abort();
  const result = await sendGmailBulkMessage({ owner: { accountId: "unused", tenantId: "unused", baseId: "unused" }, mailboxRefId: "unused", emailAddress: original.from, accessToken: "not-a-real-token", expiresAt: 0 }, original, signal);
  assert.deepEqual(result, { state: "not_sent", code: "gmail_send_cancelled_before_attempt" });
});

test("内部邮件只能取冻结批次领取，幂等键来自原条目，其他邮箱或改动正文拒绝", () => {
  const item: BulkItem = { id: "item", recipient: original.to, source: "Creator", fingerprint: mailFingerprint(original.from, original.to, original.subject, original.body), state: "attempting", provider_message_id: null, detail: null };
  const batch: BulkBatch = { id: "batch", mailbox_ref_id: "mailbox", subject: original.subject, body: original.body, state: "running", created_at: 0, items: [{ ...item, state: "pending" }] };
  const message = messageForGmailBulkClaim(batch, item, original.from);
  assert.equal(message.idempotencyKey, "gmail-bulk-item");
  assert.equal(message.body, original.body);
  assert.throws(() => messageForGmailBulkClaim(batch, item, "other@example.com"), /mismatch/);
  assert.throws(() => messageForGmailBulkClaim({ ...batch, body: "changed" }, item, original.from), /mismatch/);
  assert.throws(() => messageForGmailBulkClaim(batch, { ...item, state: "unknown" }, original.from), /mismatch/);
});

test("响应严格拒绝损坏UTF-8，不用替换字符修复JSON", async () => {
  const corrupted = Buffer.concat([Buffer.from('{"id":"'), Buffer.from([0xff]), Buffer.from('"}')]);
  const result = await decodeGmailResponse(corrupted, 200, false);
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.code, "gmail_response_invalid_utf8");
  const malformed = await decodeGmailResponse(Buffer.from("{"), 200, false);
  assert.equal(malformed.ok, false);
  if (!malformed.ok) assert.equal(malformed.code, "gmail_response_invalid_json");
  const valid = await decodeGmailResponse(Buffer.from('{"id":"abc123"}'), 200, false);
  assert.deepEqual(valid, { ok: true, value: { id: "abc123" } });
});
