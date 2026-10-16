import assert from "node:assert/strict";
import { readFileSync, copyFileSync, mkdtempSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { backup, DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { CreatorRepository } from "../src/creators/repository.ts";
import { normalizeContact } from "../src/creators/normalization.ts";

function database() {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys=ON");
  for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8"));
  db.exec("PRAGMA user_version=90");
  return db;
}
const count = (db: DatabaseSync, table: string) => db.prepare(`SELECT COUNT(*) AS n FROM ${table}`).get()!.n;

test("真实schema90手动新增一次事务保存Creator、来源、联系路由和审计", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const created = repository.create({ display_name: "  中文 😀  ", tags: [" 界 ", "Rust", "rust"], notes: "  notes  ", contact_routes: [{ channel: "email", value: "  PERSON@Example.com  ", source_url: " https://example.com/about " }] });
    assert.equal(created.display_name, "中文 😀");
    assert.deepEqual(created.tags, ["界", "Rust", "rust"]);
    assert.equal(created.stage, "prospect");
    assert.equal(created.contact_routes[0]!.locator.normalized_value, "person@example.com");
    assert.deepEqual(created.contact_routes[0]!.allowed_actions, ["update", "delete"]);
    const detail = repository.get(created.creator_id);
    assert.equal(detail.creator.notes, "notes");
    assert.equal(detail.creator.score, null);
    assert.deepEqual(detail.creator.follower_counts, {});
    assert.equal(detail.observation_sources.length, 2);
    assert.equal(detail.origins.length, 1);
    assert.equal(detail.account_links.length, 0);
    assert.equal(count(db, "audit_events"), 1);
    const before = ["creators", "observation_sources", "creator_origins", "creator_contact_routes", "audit_events"].map(table => count(db, table));
    assert.throws(() => repository.create({ display_name: "Collision", contact_routes: [{ channel: "website", value: "https://independent.example/" }, { channel: "email", value: "person@example.com" }] }), /another Creator/u);
    assert.deepEqual(["creators", "observation_sources", "creator_origins", "creator_contact_routes", "audit_events"].map(table => count(db, table)), before);
    assert.equal(count(db, "contact_locators"), 1);
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});

test("Creator字段验证、空补丁和归档保护保持原有语义", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    for (const input of [{ display_name: " " }, { display_name: "Creator", notes: " " }, { display_name: "Creator", tags: ["rust", " rust "] }, { display_name: "Creator", tags: ["界".repeat(13)] }, { display_name: "Creator", tags: ["x\n"] }, { display_name: "Creator", contact_routes: Array.from({ length: 17 }, () => ({ channel: "email", value: "a@example.com" })) }]) assert.throws(() => repository.create(input));
    assert.equal(count(db, "creators"), 0);
    const created = repository.create({ display_name: "Initial", tags: ["😀".repeat(12)] });
    assert.throws(() => repository.update({ creator_id: created.creator_id }), /at least one field/u);
    assert.throws(() => repository.update({ creator_id: created.creator_id, score: 101 }));
    assert.throws(() => repository.update({ creator_id: created.creator_id, follower_counts: { x: 1.5 } }));
    repository.update({ creator_id: created.creator_id, score: 82, assessment: "  strong  ", follower_counts: { x: 12000 } });
    const updated = repository.update({ creator_id: created.creator_id, display_name: "  Renamed  ", tags: [], follower_counts: { youtube: 500 } });
    assert.equal(updated.creator.display_name, "Renamed");
    assert.equal(updated.creator.score, 82);
    assert.equal(updated.creator.assessment, "strong");
    assert.deepEqual(updated.creator.tags, []);
    assert.deepEqual(updated.creator.follower_counts, { x: 12000, youtube: 500 });
    db.prepare("UPDATE creators SET archived_at=1 WHERE id=?").run(created.creator_id);
    assert.throws(() => repository.update({ creator_id: created.creator_id, score: 10 }), /archived Creator/u);
    assert.equal(repository.list({}).items.length, 0);
    assert.equal(repository.list({ include_archived: true }).items.length, 1);
  } finally { db.close(); }
});

test("原合同所有排序、全查询过滤、facets和keyset分页在超过100行时保持完整", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    for (let index = 0; index < 125; index++) {
      const created = repository.create({ display_name: `Creator ${String(index).padStart(3, "0")}`, contact_routes: index === 124 ? [{ channel: "email", value: "last@example.com" }] : [] });
      db.prepare("UPDATE creators SET created_at=?,updated_at=?,score=?,follower_counts_json=?,pinned_at=? WHERE id=?").run(index, 200 + index, index % 3 === 0 ? null : index % 101, JSON.stringify(index % 2 ? { x: index } : {}), index === 0 ? 100 : null, created.creator_id);
      db.prepare("UPDATE creator_relationships SET stage=? WHERE creator_id=?").run(index === 124 ? "engaged" : "prospect", created.creator_id);
    }
    const tail = repository.list({ stages: ["engaged"], updated_range: { from: 320, to_exclusive: 325 }, include_facets: true });
    assert.equal(tail.items.length, 1);
    assert.equal(tail.items[0]!.display_name, "Creator 124");
    assert.equal(tail.facets!.total, 1);
    assert.equal(tail.facets!.stages.reduce((sum, entry) => sum + entry.count, 0), 5);
    assert.equal(repository.list({ query: "@LAST@EXAMPLE.COM" }).items[0]!.display_name, "Creator 124");
    for (const field of ["display_name", "followers", "score", "last_touch_at", "updated_at", "created_at"]) for (const direction of ["asc", "desc"]) {
      const sorting = [{ field, direction }];
      let cursor: string | null = null;
      const ids: string[] = [];
      do {
        const page = repository.list({ limit: 17, sorting, cursor });
        ids.push(...page.items.map(item => item.id));
        cursor = page.next_cursor;
      } while (cursor !== null);
      assert.equal(ids.length, 125, `${field} ${direction}`);
      assert.equal(new Set(ids).size, 125);
      assert.equal(repository.get(ids[0]!).creator.display_name, "Creator 000");
      const expression = { display_name: "lower(display_name)", followers: "COALESCE(json_extract(follower_counts_json,'$.x'),-1)", score: "COALESCE(score,-1)", last_touch_at: "CAST(-1 AS INTEGER)", updated_at: "updated_at", created_at: "created_at" }[field]!;
      const expected = db.prepare(`SELECT id FROM creators ORDER BY COALESCE(pinned_at,0) DESC,(${expression}) ${direction === "asc" ? "ASC" : "DESC"},id DESC`).all().map(row => String(row.id));
      assert.deepEqual(ids, expected, `原SQL排序 ${field} ${direction}`);
    }
    const first = repository.list({ limit: 1 });
    assert.ok(first.next_cursor);
    assert.throws(() => repository.list({ cursor: first.next_cursor, stages: ["engaged"] }));
    assert.throws(() => repository.list({ cursor: first.next_cursor, sorting: [{ field: "display_name", direction: "asc" }] }));
    assert.throws(() => repository.list({ updated_range: { from: 2, to_exclusive: 1 } }));
    assert.throws(() => repository.list({ sorting: [{ field: "score", direction: "asc" }, { field: "score", direction: "desc" }] }));
    assert.equal(repository.list({ limit: 0 }).items.length, 1);
    assert.equal(repository.list({ limit: 5000 }).items.length, 100);
  } finally { db.close(); }
});

test("Creator详情完整读取账号观察、来源、路由动作和首个账号身份，并保留分组与平台facets", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const created = repository.create({ display_name: "Identity", contact_routes: [{ channel: "email", value: "identity@example.com" }] });
    const manual = repository.create({ display_name: "Manual" });
    db.exec("INSERT INTO creator_groups VALUES ('group','Group',1,0,1,1)");
    db.prepare("UPDATE creators SET group_id=? WHERE id=?").run("group", created.creator_id);
    db.exec("INSERT INTO platform_accounts VALUES ('telegram','telegram','user','tg-native',1,1),('x','x','user','x-native',2,2),('rejected','instagram','user','ig-native',0,0)");
    for (const [id, state] of [["telegram", "confirmed"], ["x", "claimed"], ["rejected", "rejected"]]) db.prepare("INSERT INTO creator_account_links VALUES (?,?,?,?,'exact',1,1)").run(`link-${id}`, created.creator_id, id!, state!);
    db.exec("INSERT INTO observation_sources VALUES ('source','profile_bio','x','https://x.com/User',NULL,NULL,'{\"counter\":2}',10)");
    db.exec("INSERT INTO platform_account_observations VALUES ('obs-tg','telegram','source','tguser','Telegram',NULL,'{}',5),('obs-old','x','source','old','Old',NULL,'{\"avatar_url\":\"https://old.example/avatar\"}',8),('obs-new','x','source','new','New',NULL,'{\"avatar_url\":\"https://new.example/avatar\",\"followers\":10}',10),('obs-rejected','rejected','source','ignored','Ignored',NULL,'{}',1)");
    const detail = repository.get(created.creator_id);
    assert.equal(detail.platform_accounts.length, 2);
    assert.equal(detail.account_links.length, 3);
    assert.equal(detail.account_observations.length, 4);
    assert.equal(detail.account_observations[0]!.public_data && typeof detail.account_observations[0]!.public_data, "object");
    assert.ok(detail.observation_sources.some(source => source.kind === "profile_bio"));
    assert.deepEqual(detail.group, { id: "group", name: "Group" });
    assert.deepEqual(detail.contact_routes[0]!.allowed_actions, ["update", "delete"]);
    const page = repository.list({ group: { kind: "group", group_id: "group" }, primary_platforms: ["telegram"], include_facets: true });
    assert.equal(page.items.length, 1);
    assert.equal(page.items[0]!.primary_platform, "telegram");
    assert.equal(page.items[0]!.primary_handle, "tguser");
    assert.deepEqual(page.items[0]!.platforms, ["telegram", "x"]);
    assert.deepEqual(page.items[0]!.channels, ["email"]);
    assert.equal(page.items[0]!.avatar_url, "https://new.example/avatar");
    assert.equal(page.facets!.primary_platforms[0]!.count, 1);
    assert.deepEqual(repository.list({ group: { kind: "uncategorized" } }).items.map(row => row.id), [manual.creator_id]);
    assert.equal(repository.list({ query: "new" }).items.length, 1);
    assert.equal(repository.list({ query: "old" }).items.length, 0);
    assert.equal(repository.list({ query: "ignored" }).items.length, 0);
    assert.throws(() => repository.list({ group: { kind: "group", group_id: "missing" } }), /group not found/u);
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});

test("历史未完成发送与损坏的归属信封阻止删除，失败不改动Creator", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const created = repository.create({ display_name: "Pending" });
    db.prepare("INSERT INTO idempotency_records (key,action,state,request_fingerprint,result_json,created_at,updated_at) VALUES ('pending','direct_message_send_v1','remote_started','sha256:v1:sample',?,1,1)").run(JSON.stringify({ version: 1, state: { identity: { creator_id: created.creator_id, operator_account_ref_id: "operator" }, fingerprint: "sha256:v1:sample" } }));
    assert.throws(() => repository.delete(created.creator_id), /unfinished outreach action/u);
    assert.equal(repository.get(created.creator_id).creator.display_name, "Pending");
    db.exec("UPDATE idempotency_records SET result_json='{}'");
    assert.throws(() => repository.delete(created.creator_id), /unfinished outreach action/u);
    db.exec("UPDATE idempotency_records SET state='completed'");
    repository.delete(created.creator_id);
    assert.equal(count(db, "idempotency_records"), 1);
  } finally { db.close(); }
});

test("原联系地址规范保留大小写规则、Telegram邀请与URL跟踪参数清理", () => {
  assert.equal(normalizeContact("email", "mailto:PERSON@Example.com?subject=x").normalized_value, "person@example.com");
  assert.equal(normalizeContact("x", "https://x.com/Some_User/posts").display_value, "Some_User");
  assert.equal(normalizeContact("telegram", "https://t.me/+Invite?utm_source=x").kind, "invite_link");
  assert.equal(normalizeContact("telegram", "https://t.me/Some_User").normalized_value, "some_user");
  assert.equal(normalizeContact("website", "https://example.com/?utm_source=x&keep=y#part").normalized_value, "https://example.com/?keep=y");
  assert.equal(normalizeContact("wechat", " MyWeChat ").display_value, "MyWeChat");
  assert.throws(() => normalizeContact("wechat", "含 空格"));
});

test("删除保留历史邮件与审计，并原子清除Creator归属及派生记录", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const created = repository.create({ display_name: "Delete", contact_routes: [{ channel: "email", value: "delete@example.com" }] });
    const route = created.contact_routes[0]!.route.id;
    db.exec("INSERT INTO mailbox_refs VALUES ('mail','gmail','mail','sender@example.com',NULL,'[]',1,NULL,1,1)");
    db.prepare("INSERT INTO email_threads (id,mailbox_ref_id,creator_id,contact_route_id,provider_thread_key,subject,unread_count,active_folder,created_at,updated_at) VALUES ('thread','mail',?,?,'provider','history',0,'sent',1,1)").run(created.creator_id, route);
    db.exec("INSERT INTO emails (id,thread_id,provider_message_id,direction,subject,delivery_status,occurred_at,origin) VALUES ('email','thread','remote','outbound','history','sent',3,'provider_import')");
    const removed = repository.delete(created.creator_id);
    assert.equal(removed.removed_contact_routes, 1);
    assert.equal(removed.removed_origins, 1);
    assert.equal(removed.cleared_references, 2);
    assert.equal(count(db, "emails"), 1);
    assert.equal(count(db, "audit_events"), 1);
    assert.equal(db.prepare("SELECT creator_id FROM email_threads").get()!.creator_id, null);
    assert.equal(db.prepare("SELECT creator_id FROM audit_events").get()!.creator_id, null);
    assert.throws(() => repository.get(created.creator_id), /not found/u);
    assert.throws(() => repository.delete(created.creator_id), /not found/u);
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});

test("直接打开Rust current构造器产生的真实数据库执行CRUD", { skip: !process.env.ONECREW_CREATOR_ORACLE }, async () => {
  const directory = mkdtempSync(fileURLToPath(new URL("../../.scratch/creator-crud-oracle-", import.meta.url)));
  const path = join(directory, "creator.sqlite");
  { using source = new DatabaseSync(process.env.ONECREW_CREATOR_ORACLE!, { readOnly: true }); await backup(source, path); }
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA foreign_keys=ON");
    const repository = new CreatorRepository(db);
    const created = repository.create({ display_name: "Rust schema readback", contact_routes: [{ channel: "email", value: "oracle@example.com" }] });
    assert.equal(repository.get(created.creator_id).contact_routes.length, 1);
    assert.equal(repository.update({ creator_id: created.creator_id, notes: "updated" }).creator.notes, "updated");
    assert.ok(repository.list({ query: "oracle@example.com" }).items.some(row => row.id === created.creator_id));
    assert.equal(repository.delete(created.creator_id).removed_contact_routes, 1);
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});
