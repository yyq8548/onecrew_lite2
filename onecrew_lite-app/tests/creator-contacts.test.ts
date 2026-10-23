import assert from "node:assert/strict";
import { copyFileSync, mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { backup, DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { CreatorRepository } from "../src/creators/repository.ts";
import { rm } from "node:fs/promises";
import { extractCreatorHistory } from "./creator-history.ts";

function database() { const db = new DatabaseSync(":memory:"); db.exec("PRAGMA foreign_keys=ON"); for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8")); db.exec("PRAGMA user_version=90"); return db; }
function prepared(repository: CreatorRepository, creator: string, route: string) { const result = repository.contactDeletePrepare({ creator_id: creator, contact_route_id: route }); assert.equal(result.status, "confirmation_required"); if (result.status !== "confirmation_required") throw new Error("deletion was blocked"); return result; }
const count = (db: DatabaseSync, table: string) => Number(db.prepare(`SELECT COUNT(*) AS n FROM ${table}`).get()!.n);

test("独立联系路由add/update保留规范碰撞、来源和replacement_required边界", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), creator = repository.create({ display_name: "Contact" }), other = repository.create({ display_name: "Other" });
    const added = repository.contactAdd({ creator_id: creator.creator_id, channel: "telegram", value: "old_handle" });
    assert.equal(added.status, "added");
    assert.equal(repository.contactAdd({ creator_id: creator.creator_id, channel: "telegram", value: "@old_handle" }).status, "already_exists");
    assert.equal(count(db, "creator_contact_routes"), 1);
    assert.throws(() => repository.contactAdd({ creator_id: other.creator_id, channel: "telegram", value: "@old_handle" }), /another Creator/u);
    const input = { creator_id: creator.creator_id, contact_route_id: added.route_id, channel: "telegram", value: "old_handle" };
    const before = count(db, "audit_events");
    assert.equal(repository.contactUpdate(input).status, "unchanged");
    assert.equal(count(db, "audit_events"), before);
    const updated = repository.contactUpdate({ ...input, value: "@old_handle", source_url: " https://t.me/old_handle " });
    assert.equal(updated.status, "updated");
    assert.equal(updated.route.observations[0]!.source.source_url, "https://t.me/old_handle");
    assert.equal(repository.contactUpdate({ ...input, value: "new_handle" }).status, "replacement_required");
    assert.equal(repository.get(creator.creator_id).contact_routes[0]!.locator.normalized_value, "old_handle");
    repository.archive(creator.creator_id);
    assert.throws(() => repository.contactAdd({ creator_id: creator.creator_id, channel: "email", value: "new@example.com" }), /archived Creator/u);
    assert.throws(() => repository.contactUpdate(input), /archived Creator/u);
  } finally { db.close(); }
});

test("删除预览保存完整原指纹，确认原子清理路由且终态回执可重放", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), creator = repository.create({ display_name: "Delete" });
    const route = repository.contactAdd({ creator_id: creator.creator_id, channel: "email", value: "delete@example.com" });
    const before = prepared(repository, creator.creator_id, route.route_id);
    assert.equal(before.remote_not_deleted, true);
    assert.equal(before.next_tool, "onecrew.creator.contact_route.delete.confirm");
    assert.deepEqual([before.impact.contact_routes, before.impact.observations, before.impact.contact_locators, before.impact.observation_sources, before.impact.audit_events], [1, 1, 1, 1, 1]);
    assert.equal(before.impact.local_records_total, 5);
    const fingerprint = JSON.parse(String(db.prepare("SELECT plan_fingerprint FROM creator_contact_route_delete_operations WHERE id=?").get(before.operation_id)!.plan_fingerprint));
    assert.deepEqual(Object.keys(fingerprint), ["creator_id", "contact_route_id", "locator_id", "route_updated_at", "locator_updated_at", "impact", "observations", "dependencies", "email_threads", "emails", "email_recipients", "email_drafts", "conversations", "messages", "touchpoints", "audit_events", "idempotency_records", "resolutions", "identity_conflicts", "observation_sources", "resolved_targets", "creator_origins"]);
    const first = repository.contactDeleteConfirm({ operation_id: before.operation_id });
    assert.equal(first.result.status, "deleted");
    assert.equal(first.changed, true);
    assert.deepEqual(repository.contactDeleteConfirm({ operation_id: before.operation_id }), { result: first.result, changed: false });
    assert.equal(repository.get(creator.creator_id).contact_routes.length, 0);
    assert.equal(count(db, "contact_locators"), 0);
    assert.equal(count(db, "audit_events"), 0);
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});

test("十分钟删除计划过期与资料变化终结为expired/stale，不能重放继续删除", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), creator = repository.create({ display_name: "TTL" });
    const route = repository.contactAdd({ creator_id: creator.creator_id, channel: "x", value: "original" });
    const first = prepared(repository, creator.creator_id, route.route_id);
    db.prepare("UPDATE creator_contact_route_delete_operations SET expires_at=1 WHERE id=?").run(first.operation_id);
    const expired = repository.contactDeleteConfirm({ operation_id: first.operation_id });
    assert.equal(expired.result.status, "expired");
    assert.equal(expired.changed, false);
    const second = prepared(repository, creator.creator_id, route.route_id);
    db.prepare("UPDATE creator_contact_routes SET updated_at=updated_at+1 WHERE id=?").run(route.route_id);
    const stale = repository.contactDeleteConfirm({ operation_id: second.operation_id });
    assert.equal(stale.result.status, "stale");
    assert.deepEqual(repository.contactDeleteConfirm({ operation_id: second.operation_id }), stale);
    assert.equal(repository.get(creator.creator_id).contact_routes.length, 1);
  } finally { db.close(); }
});

function thread(db: DatabaseSync, creator: string | null, route: string | null, third = false) {
  db.exec("INSERT INTO mailbox_refs VALUES ('mail','gmail','mail','Sender <sender@example.com>',NULL,'[]',1,NULL,1,1)");
  db.prepare("INSERT INTO email_threads (id,mailbox_ref_id,creator_id,contact_route_id,provider_thread_key,subject,unread_count,active_folder,created_at,updated_at) VALUES ('thread','mail',?,?,'provider','history',0,'sent',1,1)").run(creator, route);
  db.exec("INSERT INTO emails (id,thread_id,provider_message_id,direction,subject,delivery_status,occurred_at,origin) VALUES ('email','thread','remote','outbound','history','sent',3,'provider_import'); INSERT INTO email_recipients VALUES ('email','from','Sender <sender@example.com>',NULL),('email','to','Target <target@example.com>',NULL)");
  if (third) db.exec("INSERT INTO email_recipients VALUES ('email','cc','third@example.com',NULL)");
}
test("删除路由只清理归属明确且参与者专属的邮件历史，其他参与者和无归属线程阻止删除", () => {
  for (const mode of ["owned", "same_creator_unowned_route", "third_party", "unowned"] as const) {
    const db = database();
    try {
      const repository = new CreatorRepository(db), creator = repository.create({ display_name: "Email" });
      const route = repository.contactAdd({ creator_id: creator.creator_id, channel: "email", value: "target@example.com" });
      thread(db, mode === "unowned" ? null : creator.creator_id, mode === "same_creator_unowned_route" || mode === "unowned" ? null : route.route_id, mode === "third_party");
      const plan = repository.contactDeletePrepare({ creator_id: creator.creator_id, contact_route_id: route.route_id });
      if (mode === "owned" || mode === "same_creator_unowned_route") {
        assert.equal(plan.status, "confirmation_required");
        if (plan.status !== "confirmation_required") throw new Error("expected owned email deletion");
        assert.deepEqual([plan.impact.email_threads, plan.impact.email_messages, plan.impact.email_recipients], [1, 1, 2]);
        repository.contactDeleteConfirm({ operation_id: plan.operation_id });
        assert.equal(count(db, "emails"), 0);
      } else {
        assert.equal(plan.status, "blocked");
        if (plan.status !== "blocked") throw new Error("expected shared email block");
        assert.equal(plan.reason, "shared_local_history");
        assert.ok(plan.blocking_artifacts.some(item => item.kind === "shared_email_history"));
        assert.equal(count(db, "emails"), 1);
      }
      assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
    } finally { db.close(); }
  }
});

function socialHistory(db: DatabaseSync, locator: string, route: string | null) {
  db.exec("INSERT INTO account_refs (id,platform,kind,label,created_at,updated_at) VALUES ('operator','x','profile','operator',1,1); INSERT INTO resolved_targets VALUES ('target','operator','x','user','native',NULL,'{}',1,1)");
  db.prepare("INSERT INTO locator_resolutions VALUES ('resolution',?,'operator','target','active',1,NULL)").run(locator);
  db.exec("INSERT INTO conversations (id,account_ref_id,resolved_target_id,channel,provider_thread_id,created_at,updated_at) VALUES ('conversation','operator','target','x','thread',1,1); INSERT INTO messages VALUES ('message','conversation','remote',NULL,'outbound','history','sent','{}',5)");
  if (route !== null) db.prepare("INSERT INTO conversation_contact_routes VALUES ('conversation',?,1)").run(route);
}

test("DM历史专属归属才允许清理，共享会话、缺失归属和跨Creator审计均阻止删除", () => {
  for (const mode of ["owned", "shared", "ambiguous", "audit"] as const) {
    const db = database();
    try {
      const repository = new CreatorRepository(db), creator = repository.create({ display_name: "DM" }), other = repository.create({ display_name: "Other" });
      const route = repository.contactAdd({ creator_id: creator.creator_id, channel: "x", value: "target" });
      const record = repository.get(creator.creator_id).contact_routes[0]!;
      socialHistory(db, record.locator.id, mode === "ambiguous" ? null : route.route_id);
      if (mode === "shared") { const otherRoute = repository.contactAdd({ creator_id: other.creator_id, channel: "x", value: "other" }); db.prepare("INSERT INTO conversation_contact_routes VALUES ('conversation',?,1)").run(otherRoute.route_id); }
      if (mode === "audit") db.prepare("INSERT INTO audit_events (id,kind,creator_id,contact_locator_id,contact_route_id,created_at) VALUES ('foreign','historical',?,?,?,1)").run(other.creator_id, record.locator.id, route.route_id);
      const plan = repository.contactDeletePrepare({ creator_id: creator.creator_id, contact_route_id: route.route_id });
      if (mode === "owned") {
        assert.equal(plan.status, "confirmation_required");
        if (plan.status !== "confirmation_required") throw new Error("expected owned history");
        assert.deepEqual([plan.impact.direct_message_conversations, plan.impact.direct_messages, plan.impact.target_resolutions, plan.impact.resolved_targets], [1, 1, 1, 1]);
        repository.contactDeleteConfirm({ operation_id: plan.operation_id });
        assert.equal(count(db, "messages"), 0);
        assert.equal(count(db, "resolved_targets"), 0);
      } else {
        assert.equal(plan.status, "blocked");
        if (plan.status !== "blocked") throw new Error("expected shared history block");
        assert.equal(plan.reason, mode === "shared" ? "shared_local_history" : "ambiguous_legacy_history");
        assert.equal(count(db, "messages"), 1);
      }
      assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
    } finally { db.close(); }
  }
});

test("初始根账号保护和派生路由顺序保持，删除父路由只解除历史来源引用", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), creator = repository.create({ display_name: "Parent" }), childCreator = repository.create({ display_name: "Child" });
    const parent = repository.contactAdd({ creator_id: creator.creator_id, channel: "x", value: "parent" });
    let record = repository.get(creator.creator_id).contact_routes[0]!;
    db.prepare("UPDATE observation_sources SET kind='initial_platform_account' WHERE id=?").run(record.observations[0]!.source.id);
    const root = repository.contactDeletePrepare({ creator_id: creator.creator_id, contact_route_id: parent.route_id });
    assert.equal(root.status, "blocked");
    if (root.status !== "blocked") throw new Error("expected root protection");
    assert.equal(root.reason, "root_initial_platform_account");
    assert.throws(() => repository.contactUpdate({ creator_id: creator.creator_id, contact_route_id: parent.route_id, channel: "x", value: "@parent" }), /manually added/u);
    db.prepare("UPDATE observation_sources SET metadata_json=? WHERE id=?").run(JSON.stringify({ source: { kind: "creator_contact_route" } }), record.observations[0]!.source.id);
    const child = repository.contactAdd({ creator_id: childCreator.creator_id, channel: "email", value: "child@example.com" });
    db.prepare("INSERT INTO creator_contact_route_dependencies VALUES (?,?,'link_hub',1)").run(parent.route_id, child.route_id);
    record = repository.get(creator.creator_id).contact_routes[0]!;
    socialHistory(db, record.locator.id, parent.route_id);
    const childSource = db.prepare("SELECT source_id FROM creator_origins WHERE creator_id=? LIMIT 1").get(childCreator.creator_id)!.source_id;
    db.prepare("INSERT INTO creator_origins (id,kind,creator_id,source_id,discovery_id,source_creator_id,source_contact_route_id,operator_account_ref_id,source_resolved_target_id,administrator_role,community_external_id,community_title,community_target_kind,discovered_at) VALUES ('derived','community_administrator',?,?,'discovery',?,?,'operator','target','administrator','community','Community','supergroup',1)").run(childCreator.creator_id, String(childSource), creator.creator_id, parent.route_id);
    const blocked = repository.contactDeletePrepare({ creator_id: creator.creator_id, contact_route_id: parent.route_id });
    assert.equal(blocked.status, "blocked");
    if (blocked.status !== "blocked") throw new Error("expected children block");
    assert.equal(blocked.reason, "has_derived_contact_routes");
    assert.equal(blocked.children[0]!.contact_route_id, child.route_id);
    repository.contactDeleteConfirm({ operation_id: prepared(repository, childCreator.creator_id, child.route_id).operation_id });
    const plan = prepared(repository, creator.creator_id, parent.route_id);
    assert.equal(plan.impact.creator_origins_detached, 1);
    assert.equal(plan.impact.resolved_targets, 0);
    repository.contactDeleteConfirm({ operation_id: plan.operation_id });
    assert.equal(db.prepare("SELECT source_contact_route_id FROM creator_origins WHERE id='derived'").get()!.source_contact_route_id, null);
    assert.equal(count(db, "resolved_targets"), 1);
    assert.ok(repository.get(childCreator.creator_id).origins.some(origin => origin.kind === "community_administrator"));
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});

test("identity_conflicts仅清理当前Creator专属冲突，跨归属冲突和发送中信封阻止删除", () => {
  for (const mode of ["owned", "shared", "inflight"] as const) {
    const db = database();
    try {
      const repository = new CreatorRepository(db), creator = repository.create({ display_name: "Conflict" }), other = repository.create({ display_name: "Other" });
      const route = repository.contactAdd({ creator_id: creator.creator_id, channel: "email", value: "conflict@example.com" });
      const locator = repository.get(creator.creator_id).contact_routes[0]!.locator.id;
      if (mode !== "inflight") db.prepare("INSERT INTO identity_conflicts (id,conflict_key,kind,creator_id,locator_id,state,created_at) VALUES ('conflict','key','locator',?,?,'open',1)").run(mode === "owned" ? creator.creator_id : other.creator_id, locator);
      else db.exec("INSERT INTO idempotency_records (key,action,state,result_json,created_at,updated_at) VALUES ('pending','direct_message_send_v1','remote_started','{}',1,1)");
      const plan = repository.contactDeletePrepare({ creator_id: creator.creator_id, contact_route_id: route.route_id });
      if (mode === "owned") {
        assert.equal(plan.status, "confirmation_required");
        if (plan.status !== "confirmation_required") throw new Error("expected owned conflict removal");
        assert.equal(plan.impact.identity_conflicts, 1);
        repository.contactDeleteConfirm({ operation_id: plan.operation_id });
        assert.equal(count(db, "identity_conflicts"), 0);
      } else {
        assert.equal(plan.status, "blocked");
        if (plan.status !== "blocked") throw new Error("expected attribution block");
        assert.equal(plan.reason, mode === "inflight" ? "in_flight_external_action" : "shared_local_history");
        assert.equal(repository.get(creator.creator_id).contact_routes.length, 1);
      }
    } finally { db.close(); }
  }
});

test("只读identity.resolve只使用原平台原生ID或handle证据，不按名称合并", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), first = repository.create({ display_name: "Same" }), second = repository.create({ display_name: "Same" });
    const candidate = { platform: "x", external_id: "native", target_kind: "user", handle: "@handle", display_name: "Same", profile_url: null };
    assert.deepEqual(repository.identityResolve({ candidate }), { status: "new" });
    db.exec("INSERT INTO platform_accounts VALUES ('account','x','user','native',1,1); INSERT INTO observation_sources VALUES ('source','profile_bio','x',NULL,NULL,NULL,'{}',1); INSERT INTO platform_account_observations VALUES ('observation','account','source','handle',NULL,NULL,'{}',1)");
    db.prepare("INSERT INTO creator_account_links VALUES ('first',?,'account','confirmed','exact',1,1)").run(first.creator_id);
    assert.deepEqual(repository.identityResolve({ candidate }), { status: "exact_existing_creator", creator_id: first.creator_id });
    assert.deepEqual(repository.identityResolve({ candidate: { ...candidate, external_id: null } }), { status: "possible_match", creator_ids: [first.creator_id] });
    db.prepare("INSERT INTO creator_account_links VALUES ('second',?,'account','claimed','asserted',1,1)").run(second.creator_id);
    assert.deepEqual(repository.identityResolve({ candidate: { ...candidate, external_id: null } }), { status: "conflicting_matches", creator_ids: [first.creator_id, second.creator_id].sort() });
    db.exec("UPDATE creator_account_links SET state='rejected' WHERE id='second'");
    assert.deepEqual(repository.identityResolve({ candidate: { ...candidate, handle: null, external_id: null } }), { status: "new" });
    assert.equal(count(db, "identity_conflicts"), 0);
  } finally { db.close(); }
});

test("跨Creator观察与跨路由草稿保留共享历史，发送中草稿和损坏信封关闭删除入口", () => {
  for (const mode of ["observation", "reply_draft", "sending_draft", "malformed_pending"] as const) {
    const db = database();
    try {
      const repository = new CreatorRepository(db), creator = repository.create({ display_name: "Guard" }), other = repository.create({ display_name: "Other" });
      const route = repository.contactAdd({ creator_id: creator.creator_id, channel: "email", value: "target@example.com" });
      const detail = repository.get(creator.creator_id).contact_routes[0]!;
      if (mode === "observation") db.prepare("INSERT INTO locator_observations VALUES ('foreign',?,?,?,NULL,'target@example.com',2)").run(detail.locator.id, detail.observations[0]!.source.id, other.creator_id);
      else if (mode === "malformed_pending") db.exec("INSERT INTO idempotency_records (key,action,state,result_json,created_at,updated_at) VALUES ('corrupt','direct_message_send_v1','remote_started','{',1,1)");
      else {
        thread(db, creator.creator_id, route.route_id);
        if (mode === "reply_draft") db.exec("INSERT INTO email_drafts (id,mailbox_ref_id,to_address,reply_to_email_id,subject,body_text,state,revision,created_at,updated_at) VALUES ('draft','mail','different@example.com','email','reply','reply','draft',1,1,1)");
        else db.prepare("INSERT INTO email_drafts (id,mailbox_ref_id,to_address,creator_id,contact_route_id,subject,body_text,state,sending_idempotency_key,revision,created_at,updated_at) VALUES ('draft','mail','target@example.com',?,?,'pending','pending','draft','pending',1,1,1)").run(creator.creator_id, route.route_id);
      }
      const plan = repository.contactDeletePrepare({ creator_id: creator.creator_id, contact_route_id: route.route_id });
      assert.equal(plan.status, "blocked");
      if (plan.status !== "blocked") throw new Error("expected shared/inflight history guard");
      assert.equal(plan.reason, mode === "observation" || mode === "reply_draft" ? "shared_local_history" : "in_flight_external_action");
      assert.equal(repository.get(creator.creator_id).contact_routes.length, 1);
      assert.equal(count(db, "creator_contact_route_delete_operations"), 0);
    } finally { db.close(); }
  }
});

test("独立历史数据库副本支持联系路由预览确认与SQL影响统计", async () => {
  const directory = await extractCreatorHistory([90]), path = join(directory, "creator.sqlite");
  { using source = new DatabaseSync(join(directory, "creator-schema90-oracle.sqlite"), { readOnly: true }); await backup(source, path); }
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA foreign_keys=ON");
    const repository = new CreatorRepository(db), creator = repository.create({ display_name: "Contacts oracle" });
    const route = repository.contactAdd({ creator_id: creator.creator_id, channel: "x", value: "oracle" });
    const detail = repository.get(creator.creator_id).contact_routes[0]!;
    socialHistory(db, detail.locator.id, route.route_id);
    const plan = prepared(repository, creator.creator_id, route.route_id);
    assert.equal(plan.impact.direct_messages, Number(db.prepare("SELECT COUNT(*) AS n FROM messages WHERE conversation_id IN(SELECT conversation_id FROM conversation_contact_routes WHERE contact_route_id=?)").get(route.route_id)!.n));
    assert.equal(plan.impact.target_resolutions, Number(db.prepare("SELECT COUNT(*) AS n FROM locator_resolutions WHERE locator_id=?").get(detail.locator.id)!.n));
    assert.equal(repository.contactDeleteConfirm({ operation_id: plan.operation_id }).result.status, "deleted");
    assert.equal(repository.get(creator.creator_id).contact_routes.length, 0);
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); await rm(directory, { recursive: true }); }
});
