import assert from "node:assert/strict";
import { test } from "node:test";
import { gmailReplyMessageIds, gmailReplyThreadId } from "../src/mail/gmail-transport.ts";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { GmailReplyRepository } from "../src/mail/gmail-reply-repository.ts";
import { decodeGmailReplyMessage } from "../src/mail/gmail-reply-message.ts";

test("Gmail 回复读取严格限定本地确认发送的会话", () => {
  assert.equal(gmailReplyThreadId({ id: "aa", threadId: "bb", labelIds: ["SENT"] }, "aa"), "bb");
  for (const seed of [{ id: "ab", threadId: "bb", labelIds: ["SENT"] }, { id: "aa", threadId: "bb", labelIds: ["INBOX"] }, { id: "aa", threadId: "../other", labelIds: ["SENT"] }]) assert.throws(() => gmailReplyThreadId(seed, "aa"));
  const thread = { id: "bb", messages: [
    { id: "aa", labelIds: ["SENT"] }, { id: "c1", labelIds: ["INBOX"] },
    { id: "c2" }, { id: "c1", labelIds: [] }, { id: "c3", labelIds: ["DRAFT"] },
    { id: "c4", labelIds: ["TRASH"] }, { id: "c5", labelIds: ["SPAM"] },
  ] };
  assert.deepEqual(gmailReplyMessageIds(thread, "bb", "aa"), ["c1", "c2"]);
  assert.throws(() => gmailReplyMessageIds({ ...thread, id: "bc" }, "bb", "aa"));
  assert.throws(() => gmailReplyMessageIds({ ...thread, messages: [] }, "bb", "aa"));
  assert.throws(() => gmailReplyMessageIds({ ...thread, messages: [{ id: "aa", labelIds: null }] }, "bb", "aa"));
});

test("Gmail结构化正文使用成熟MIME解码，选首个正文并排除附件", async () => {
  const value = { id: "c1", labelIds: ["INBOX"], internalDate: "1791786972000", payload: {
    mimeType: "multipart/mixed", body: {}, headers: [{ name: "From", value: "Sender <sender@example.com>" }, { name: "To", value: "list@example.com" }, { name: "Subject", value: "=?UTF-8?B?5rWL6K+V?=" }, { name: "Message-ID", value: "<reply@example.com>" }, { name: "References", value: "<seed@example.com>" }],
    parts: [{ mimeType: "text/plain", filename: "private.txt", body: { data: Buffer.from("附件").toString("base64url") } }, { mimeType: "text/plain", headers: [{ name: "Content-Type", value: "text/plain; name=private.txt" }], body: { data: Buffer.from("隐藏附件").toString("base64url") } }, { mimeType: "text/plain", body: { data: Buffer.from("你好正文").toString("base64url") } }, { mimeType: "text/plain", body: { data: Buffer.from("第二正文").toString("base64url") } }],
  } };
  const decoded = await decodeGmailReplyMessage(value, "c1", "owner@example.com");
  assert.equal(decoded.subject, "测试");
  assert.equal(decoded.bodyText?.trimEnd(), "你好正文");
  assert.deepEqual(decoded.receivedFor, ["owner@example.com"]);
  assert.deepEqual(decoded.references, ["<seed@example.com>"]);
  assert.equal(decoded.occurredAt, 1791786972);
  await assert.rejects(decodeGmailReplyMessage({ ...value, labelIds: ["DRAFT"] }, "c1", "owner@example.com"));
  await assert.rejects(decodeGmailReplyMessage(value, "c2", "owner@example.com"));
  let nested: unknown = { mimeType: "text/plain", body: { data: "" } };
  for (let depth = 0; depth < 18; depth++) nested = { mimeType: "multipart/mixed", body: {}, parts: [nested] };
  await assert.rejects(decodeGmailReplyMessage({ ...value, payload: nested }, "c1", "owner@example.com"), /gmail_mime_depth_exceeded/);
});

test("真实schema90回复页原子写入，保留已读/归档/删除，完整身份去重及作用域检查点", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/gmail-replies-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON");
    db.prepare("INSERT INTO mailbox_refs VALUES ('box','gmail','Gmail','owner@example.com',NULL,'[\"send\",\"inbox\",\"sent_readback\"]',1,NULL,1,1)").run();
    db.prepare("INSERT INTO email_threads VALUES ('thread','box',NULL,NULL,'local-seed','原主题',0,'sent',5,6,1,2)").run();
    db.prepare("INSERT INTO emails VALUES ('seed','thread','aa','<seed@example.com>','outbound','原主题','正文',NULL,NULL,'[]','sent','{}',1,'confirmed_send')").run();
    db.prepare("INSERT INTO emails VALUES ('old-reply','thread','ab','<old@example.com>','inbound','Re: 原主题','旧回复',NULL,NULL,'[]','read','{}',2,'provider_import')").run();
    const repository = new GmailReplyRepository(db, { accountId: "user", tenantId: "tenant", baseId: "base" });
    assert.deepEqual(repository.seeds("box"), [{ threadId: "thread", providerMessageId: "aa" }]);
    const received = { providerMessageId: "bb", internetMessageId: "<reply@example.com>", from: "sender@example.com", to: ["list@example.com"], cc: [], bcc: [], replyTo: [], receivedFor: ["owner@example.com"], subject: "Re: 原主题", bodyText: "你好😀", bodyHtml: null, inReplyTo: "<seed@example.com>", references: ["<seed@example.com>"], headers: {}, occurredAt: 100 };
    assert.deepEqual(repository.commitReplies("box", "thread", [received]), ["thread"]);
    assert.deepEqual([...repository.complete("box", ["bb", "cc"])], ["bb"]);
    assert.deepEqual({ ...db.prepare("SELECT active_folder,archived_at,deleted_at,unread_count FROM email_threads WHERE id='thread'").get() }, { active_folder: "inbox", archived_at: 5, deleted_at: 6, unread_count: 1 });
    db.prepare("UPDATE emails SET delivery_status='read' WHERE provider_message_id='bb'").run();
    repository.commitReplies("box", "thread", [received]);
    assert.equal(db.prepare("SELECT unread_count FROM email_threads WHERE id='thread'").get()!.unread_count, 0);
    assert.throws(() => repository.commitReplies("box", "thread", [{ ...received, providerMessageId: "cc" }, { ...received, providerMessageId: "dd", receivedFor: [] }]));
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM emails WHERE provider_message_id='cc'").get()!.n, 0);
    assert.throws(() => repository.commitReplies("box", "missing", [received]));
    db.prepare("INSERT INTO mailbox_sync_incremental_continuations VALUES ('box','inbound','old-inbox-token','old-anchor',50,1)").run();
    repository.completeCheckpoint("box");
    const state = db.prepare("SELECT * FROM mailbox_sync_state WHERE mailbox_ref_id='box' AND direction='inbound'").get()!;
    assert.equal(state.backfill_complete, 1);
    assert.equal(state.newest_provider_message_id, null);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM mailbox_sync_incremental_continuations").get()!.n, 0);
    repository.completeCheckpoint("box");
    assert.deepEqual(db.prepare("SELECT * FROM mailbox_sync_state WHERE mailbox_ref_id='box' AND direction='inbound'").get(), state);
  } finally { db.close(); }
});
