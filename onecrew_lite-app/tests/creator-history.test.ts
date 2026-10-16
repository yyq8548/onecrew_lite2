import assert from "node:assert/strict";
import { readFile, copyFile, mkdir, mkdtemp } from "node:fs/promises";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { CreatorHistoryRepository } from "../src/creators/history-repository.ts";
import { parseApplicationJson, stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { historyExcerpt } from "../src/creators/history-contracts.ts";

async function database() {
  await mkdir(resolve(".scratch"), { recursive: true }); const directory = await mkdtemp(resolve(".scratch", "creator-history-")), path = join(directory, "creator.sqlite");
  await copyFile(new URL("./fixtures/creator-history/baseline.sqlite", import.meta.url), path);
  const db = new DatabaseSync(path); db.exec("PRAGMA foreign_keys=ON"); return db;
}
const canonical = (value: unknown): unknown => parseApplicationJson(stringifyApplicationJson(value));
test("原Rust真实schema90历史会话、未分类债务、最后联系账户与分类完整输出一致", async () => {
  const db = await database();
  try {
    const repository = new CreatorHistoryRepository(db), expected = JSON.parse(await readFile(new URL("./fixtures/creator-history/expected.json", import.meta.url), "utf8"));
    assert.deepEqual(canonical(repository.conversationList({})), canonical(expected.before));
    const view = repository.conversationGet("conv-1"); assert.ok(view);
    assert.deepEqual(canonical(view.pending_triage), canonical(expected.pending));
    assert.equal(view.messages.length, 2); assert.equal(view.messages[0]!.id, "msg-out");
    assert.deepEqual(canonical(repository.lastContact("creator-1", "route-1")), canonical(expected.last));
    const result = repository.triageRecord({ message_id: "msg-in", intent: "interested", summary: "  原始分类  ", suggested_reply: "  建议文本  " });
    db.prepare("UPDATE reply_triage SET id=?,created_at=123 WHERE id=?").run(expected.list.items[0].id, result.id);
    assert.deepEqual(canonical(repository.triageList({})), canonical(expected.list));
    assert.deepEqual(canonical(repository.conversationList({})), canonical(expected.conversations));
    assert.deepEqual(canonical(repository.conversationGet("conv-1")!.pending_triage), canonical(expected.debt));
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});

test("真实历史分类覆盖、确认幂等、过滤及查询绑定游标不修改消息或触发发送", async () => {
  const db = await database();
  try {
    const repository = new CreatorHistoryRepository(db), messages = db.prepare("SELECT * FROM messages ORDER BY id").all();
    assert.throws(() => repository.triageRecord({ intent: "other", summary: "x" }), /exactly one/);
    assert.throws(() => repository.triageRecord({ message_id: "msg-out", intent: "other", summary: "x" }), /inbound/);
    assert.throws(() => repository.triageRecord({ message_id: "missing", intent: "other", summary: "x" }), /not found/);
    assert.throws(() => repository.triageRecord({ message_id: "msg-in", intent: "other", summary: "  " }), /required/);
    const initial = repository.triageRecord({ message_id: "msg-in", intent: "question", summary: "档期" }), acknowledged = repository.triageAcknowledge(initial.id);
    assert.ok(acknowledged.acknowledged_at); assert.deepEqual(repository.triageAcknowledge(initial.id), acknowledged);
    assert.equal(repository.triageList({}).items.length, 0); assert.equal(repository.triageList({ include_acknowledged: true }).items.length, 1);
    const updated = repository.triageRecord({ message_id: "msg-in", intent: "interested", summary: " 新判断 ", suggested_reply: " " });
    assert.equal(updated.id, initial.id); assert.equal(updated.summary, "新判断"); assert.equal(updated.acknowledged_at, undefined); assert.equal(updated.suggested_reply, undefined);
    db.prepare("INSERT INTO messages(id,conversation_id,direction,body,status,sent_at) VALUES('second','conv-1','inbound','再问','received',11)").run();
    const second = repository.triageRecord({ message_id: "second", intent: "question", summary: "second" });
    db.prepare("UPDATE reply_triage SET created_at=123").run();
    const page = repository.triageList({ limit: 1 }); assert.equal(page.items.length, 1); assert.ok(page.next_cursor);
    const next = repository.triageList({ limit: 1, cursor: page.next_cursor }); assert.equal(next.items.length, 1); assert.notEqual(next.items[0]!.id, page.items[0]!.id); assert.equal(next.next_cursor, undefined);
    assert.throws(() => repository.triageList({ include_acknowledged: true, cursor: page.next_cursor }));
    repository.triageAcknowledge(second.id);
    assert.equal(repository.conversationList({ query: "@Operator", status: "waiting_reply" }).items.length, 1);
    assert.equal(repository.conversationList({ status: "replied" }).items.length, 0);
    assert.equal(repository.conversationGet("missing"), null); assert.throws(() => repository.lastContact("other", "route-1"), /does not belong/);
    assert.deepEqual(db.prepare("SELECT * FROM messages WHERE id!='second' ORDER BY id").all(), messages);
    assert.equal(historyExcerpt(`  x\u0085${"😀".repeat(161)} `), `x ${"😀".repeat(158)}…`);
  } finally { db.close(); }
});

test("真实idempotency历史回执保留退役状态和无损当前密封阶段，损坏拒绝且不恢复动作", async () => {
  const db = await database();
  try {
    const repository = new CreatorHistoryRepository(db), insert = db.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,result_json,contact_route_id,created_at,updated_at) VALUES(?,?,?,?,?,'route-1',123,123)");
    insert.run("legacy", "direct_message_send", null, "failed", '{"kind":"retired_action_unresolved"}');
    assert.deepEqual(repository.actionStatus("legacy"), { kind: "retired", operation: "direct_message_send", phase: "retired_action_unresolved", marked: true });
    assert.equal(repository.actionStatus("absent"), null);
    const identity = { operation: "direct_message_send", creator_id: "creator-1", operator_account_ref_id: "account-1", platform: "x", target_platform_account_id: "target-platform", target_kind: "user", external_target_id: "peer-1", contact_route_id: "route-1", resolved_target_id: "target-1", conversation: null, content_id: null, reply_to_id: null, relationship_action: null, allow_unfollowed_recipient: false };
    const state = { operation: "direct_message_send", identity, fingerprint: `sha256:v1:${"a".repeat(64)}`, claimed_at: 123, phase: "completed", receipt: { kind: "direct_message", external_message_id: "external", provider_thread_id: null }, verification: { kind: "direct_message", external_message_id: "external", provider_thread_id: "thread-1", sent_at: 18446744073709551615n }, record: { kind: "direct_message", conversation_id: "conv-1", message_id: "msg-out", touchpoint_id: "touch", audit_event_id: "audit" } };
    insert.run("current", "direct_message_send_v1", state.fingerprint, "completed", stringifyApplicationJson({ version: 1, state }));
    const current = repository.actionStatus("current"); assert.equal(current?.kind, "current"); if (current?.kind === "current") assert.equal(current.state.verification?.kind === "direct_message" && current.state.verification.sent_at, 18446744073709551615n);
    db.prepare("UPDATE idempotency_records SET state='remote_started' WHERE key='current'").run(); assert.throws(() => repository.actionStatus("current"), /storage is invalid/);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM messages").get()!.n, 2);
  } finally { db.close(); }
});

test("真实邮件历史分类包含线程与Creator，拒绝出站邮件并保留邮件及会话数据", async () => {
  const db = await database();
  try {
    db.exec(`INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES('mailbox','gmail','历史','history@example.test','[]',0,1,1);
      INSERT INTO email_threads(id,mailbox_ref_id,creator_id,provider_thread_key,subject,active_folder,created_at,updated_at) VALUES('email-thread','mailbox','creator-1','provider-thread','主题','inbox',1,1);
      INSERT INTO emails(id,thread_id,provider_message_id,direction,subject,body_text,delivery_status,occurred_at,origin) VALUES('email-in','email-thread','provider-in','inbound','主题','  已有\n 邮件  ','received',10,'provider_import'),('email-out','email-thread','provider-out','outbound','主题','出站历史','sent',5,'confirmed_send');`);
    const repository = new CreatorHistoryRepository(db), before = db.prepare("SELECT * FROM emails ORDER BY id").all();
    assert.throws(() => repository.triageRecord({ email_id: "email-out", intent: "other", summary: "x" }), /inbound email/);
    assert.throws(() => repository.triageRecord({ email_id: "email-in", message_id: "msg-in", intent: "other", summary: "x" }), /exactly one/);
    const result = repository.triageRecord({ email_id: "email-in", intent: "declined", summary: "已有邮件分类", suggested_reply: "本地建议" });
    const view = repository.triageList({}).items[0]!;
    assert.equal(view.id, result.id); assert.equal("email_thread_id" in view && view.email_thread_id, "email-thread"); assert.equal(view.creator_id, "creator-1"); assert.equal(view.creator_name, "受访者"); assert.equal(view.excerpt, "已有 邮件");
    repository.triageAcknowledge(result.id); assert.deepEqual(repository.triageList({}).items, []);
    assert.deepEqual(db.prepare("SELECT * FROM emails ORDER BY id").all(), before); assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});
