import assert from "node:assert/strict";
import { mkdtemp, readFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { MailHistoryRepository, historyDraftListInput, historyThreadListInput } from "../src/mail/mail-history.ts";
import { MailThreadMaintenanceRepository, mailThreadDispositionInput, mailThreadMarkReadInput } from "../src/mail/mail-thread-maintenance.ts";

const owner = { accountId: "history-user", tenantId: "history-tenant", baseId: "history-base" };

async function fixture() {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/gmail-history-", import.meta.url)));
  const privateDirectory = await createPrivateDirectory(join(directory, "private"));
  const path = join(privateDirectory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const writer = new DatabaseSync(path);
  try {
    writer.exec("PRAGMA foreign_keys=ON");
    for (const [id, provider] of [["gmail-box", "gmail"], ["old-resend", "resend"]]) writer.prepare("INSERT INTO mailbox_refs VALUES (?,?,?,?,NULL,'[\"send\",\"sent_readback\"]',?,NULL,1,2)").run(id!, provider!, `邮箱 ${id}`, `${id}@example.com`, Number(provider === "gmail"));
    const threadInsert = writer.prepare("INSERT INTO email_threads(id,mailbox_ref_id,creator_id,contact_route_id,provider_thread_key,subject,unread_count,active_folder,archived_at,deleted_at,created_at,updated_at) VALUES (?,?,NULL,NULL,?,?,0,?,NULL,NULL,10,20)");
    threadInsert.run("thread-😀", "gmail-box", "remote-a", "你好主题", "inbox");
    threadInsert.run("thread-z", "old-resend", "remote-z", "旧邮件", "sent");
    const emailInsert = writer.prepare("INSERT INTO emails(id,thread_id,provider_message_id,internet_message_id,direction,subject,body_text,body_html,in_reply_to,references_json,delivery_status,headers_json,occurred_at,origin) VALUES (?,?,?,NULL,?,'你好主题',?,NULL,NULL,'[]',?,'{\"private-provider-header\":\"private\"}',?,'provider_import')");
    emailInsert.run("email-a", "thread-😀", "provider-a", "inbound", "你好 😀 正文", "received", 100);
    emailInsert.run("email-b", "thread-😀", "provider-b", "outbound", "😀".repeat(205), "sent", 200);
    emailInsert.run("email-z", "thread-z", "provider-z", "outbound", "旧Resend历史", "sent", 200);
    for (const kind of ["from", "to", "bcc", "received_for"]) writer.prepare("INSERT INTO email_recipients(email_id,recipient_kind,address,locator_id) VALUES ('email-b',?,?,NULL)").run(kind, `${kind}@example.com`);
    for (const id of ["template-a", "template-b"]) writer.prepare("INSERT INTO email_templates VALUES (?,'中文模板','你好 {{name}}','正文',NULL,'[\"name\"]',1,1,2)").run(id);
    writer.prepare("INSERT INTO email_drafts(id,mailbox_ref_id,to_address,creator_id,contact_route_id,reply_to_email_id,subject,body_text,body_html,template_id,template_variables_json,state,sending_idempotency_key,send_reserved_at,revision,created_at,updated_at) VALUES ('draft-😀','old-resend','recipient@example.com',NULL,NULL,NULL,'旧草稿','你好草稿',NULL,'template-a','{\"name\":\"创作者\"}','draft','historic-key',1,1,1,2)").run();
    writer.prepare("INSERT INTO idempotency_records VALUES ('historic-key','manual_email_send','old-resend',NULL,'fingerprint','remote_started',NULL,1,2)").run();
  } finally { writer.close(); }
  const db = new DatabaseSync(path, { readOnly: true });
  return { path, db, repository: new MailHistoryRepository(db, owner) };
}

test("完整schema90本地读取保留旧provider、Unicode历史且不修改数据库", async () => {
  const { path, db, repository } = await fixture();
  try {
    const before = await readFile(path);
    const mailboxes = repository.mailboxes();
    assert.equal(mailboxes.length, 2);
    assert.equal(mailboxes[1]?.provider, "resend");
    assert.deepEqual(mailboxes[1]?.issues, ["unavailable"]);
    assert.equal(repository.mailboxGet("missing").status, "not_found");
    const folders = repository.folders();
    assert.equal(folders.find(folder => folder.folder === "sent")?.count, 2);
    assert.equal(folders.find(folder => folder.folder === "inbox")?.count, 1);
    const message = repository.message("email-b");
    assert.equal(message.text, "😀".repeat(205));
    assert.equal("bcc" in message, false);
    assert.equal("received_for" in message, false);
    assert.equal("headers" in message, false);
    assert.equal(repository.draft("draft-😀").template_variables.name, "创作者");
    assert.equal(repository.draft("draft-😀").pending_send_idempotency_key, "historic-key");
    const summary = repository.drafts({}).items[0]!;
    assert.equal(summary.sending, true);
    assert.equal("pending_send_idempotency_key" in summary, false);
    assert.equal("text" in summary, false);
    assert.equal(repository.outboundHistory(null).items[0]?.state, "remote_started");
    assert.throws(() => repository.message("missing"), /not found/);
    assert.throws(() => repository.draft("missing"), /not found/);
    assert.deepEqual(await readFile(path), before);
    const corruption = new DatabaseSync(path);
    try { corruption.prepare("UPDATE email_drafts SET template_variables_json='[]' WHERE id='draft-😀'").run(); }
    finally { corruption.close(); }
    assert.throws(() => repository.draft("draft-😀"));
  } finally { db.close(); }
});

test("线程活动、消息详情与模板使用有界稳定分页，过滤和账号不能复用cursor", async () => {
  const { db, repository } = await fixture();
  try {
    const first = repository.threads({ sort: "newest", folder: "all", limit: 1 });
    assert.equal(first.items[0]?.thread.id, "thread-😀");
    assert.equal([...first.items[0]!.latest_message!.preview_text!].length, 200);
    const second = repository.threads({ sort: "newest", folder: "all", limit: 1, cursor: first.next_cursor });
    assert.equal(second.items[0]?.thread.id, "thread-z");
    assert.equal(second.next_cursor, undefined);
    assert.throws(() => repository.threads({ sort: "oldest", folder: "all", cursor: first.next_cursor }));
    assert.throws(() => repository.threads({ sort: "newest", folder: "inbox", cursor: first.next_cursor }));
    const differentOwner = new MailHistoryRepository(db, { ...owner, tenantId: "other-tenant" });
    assert.throws(() => differentOwner.threads({ sort: "newest", folder: "all", cursor: first.next_cursor }));
    const messages = repository.thread({ thread_id: "thread-😀", limit: 1 });
    assert.equal(messages.messages[0]?.id, "email-b");
    assert.equal(repository.thread({ thread_id: "thread-😀", limit: 1, cursor: messages.next_message_cursor }).messages[0]?.id, "email-a");
    assert.throws(() => repository.thread({ thread_id: "thread-z", cursor: messages.next_message_cursor }));
    const templates = repository.templates({ limit: 1 });
    assert.equal(templates.items[0]?.id, "template-b");
    assert.equal(repository.templates({ cursor: templates.next_cursor }).items[0]?.id, "template-a");
    assert.equal("text" in templates.items[0]!, false);
    assert.deepEqual(repository.template("template-a").variables, ["name"]);
    assert.equal(historyThreadListInput.safeParse({ sort: "newest", accountId: "other" }).success, false);
    assert.equal(historyDraftListInput.safeParse({ tenantId: "other" }).success, false);
  } finally { db.close(); }
});

test("真实schema90收件维护仅改变本地未读与文件夹状态，保持线程时间及归档条件", async () => {
  const { path, db, repository } = await fixture();
  const writer = new DatabaseSync(path);
  try {
    writer.exec("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL");
    writer.prepare("UPDATE email_threads SET unread_count=1 WHERE id='thread-😀'").run();
    writer.prepare("UPDATE emails SET delivery_status='unread' WHERE id='email-a'").run();
    const maintenance = new MailThreadMaintenanceRepository(writer, owner);
    const before = repository.thread({ thread_id: "thread-😀" }).thread.updated_at;
    const marked = maintenance.markRead("thread-😀");
    assert.equal(marked.thread.unread_count, 0);
    assert.equal(marked.thread.updated_at, before);
    assert.equal(marked.messages.find(message => message.id === "email-a")?.delivery_status, "read");
    assert.equal(marked.messages.find(message => message.id === "email-b")?.delivery_status, "sent");
    assert.equal(maintenance.markRead("thread-😀").thread.updated_at, before);
    assert.throws(() => maintenance.markRead("missing"), /not found/);
    assert.throws(() => maintenance.updateDisposition("thread-z", "archive"), /outbound-only/);
    assert.throws(() => maintenance.updateDisposition("thread-z", "unarchive"), /outbound-only/);
    assert.equal(repository.thread({ thread_id: "thread-z" }).thread.updated_at, 20);
    assert.equal(maintenance.updateDisposition("thread-😀", "archive").thread.folder, "archive");
    assert.equal(maintenance.updateDisposition("thread-😀", "delete").thread.folder, "deleted");
    assert.equal(maintenance.updateDisposition("thread-😀", "restore").thread.folder, "archive");
    const restored = maintenance.updateDisposition("thread-😀", "unarchive");
    assert.equal(restored.thread.folder, "inbox");
    assert.ok(restored.thread.updated_at >= before);
    assert.equal(mailThreadMarkReadInput.safeParse({ input: { thread_id: "thread-😀", accountId: "other" } }).success, false);
    assert.equal(mailThreadDispositionInput.safeParse({ input: { thread_id: "thread-😀", action: "send" } }).success, false);
    assert.equal(mailThreadDispositionInput.safeParse({ input: { thread_id: "thread-😀", action: "delete", tenantId: "other" } }).success, false);
  } finally { writer.close(); db.close(); }
});
