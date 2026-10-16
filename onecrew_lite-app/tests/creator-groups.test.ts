import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { CreatorRepository } from "../src/creators/repository.ts";

function database() {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys=ON");
  for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8"));
  db.exec("PRAGMA user_version=90");
  return db;
}

test("Creator分组名称、大小写唯一性与乐观revision完整保留", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const first = repository.groupCreate({ name: "  Launch  " });
    const second = repository.groupCreate({ name: "长期" });
    assert.equal(first.name, "Launch");
    assert.equal(first.position, 0);
    assert.equal(second.position, 1);
    assert.equal(repository.groupCreate({ name: "😀".repeat(64) }).revision, 1);
    for (const name of [" ", "界".repeat(65), "launch"]) assert.throws(() => repository.groupCreate({ name }));
    const noop = repository.groupUpdate({ group_id: first.id, expected_revision: 1, name: " Launch " });
    assert.deepEqual(noop, { group: first, changed: false });
    const renamed = repository.groupUpdate({ group_id: first.id, expected_revision: 1, name: "Launch partners" });
    assert.equal(renamed.changed, true);
    assert.equal(renamed.group.revision, 2);
    assert.throws(() => repository.groupUpdate({ group_id: first.id, expected_revision: 1, name: "Stale" }), /revision changed/u);
    assert.throws(() => repository.groupUpdate({ group_id: first.id, expected_revision: 2, name: "长期" }), /already exists/u);
    assert.throws(() => repository.groupDelete({ group_id: first.id, expected_revision: 1 }), /revision changed/u);
    assert.equal(repository.groupList({}).items[0]!.group.name, "Launch partners");
  } finally { db.close(); }
});

test("分组revision在safe integer之外保持精确CAS", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const group = repository.groupCreate({ name: "Large revision" });
    const revision = 9_007_199_254_740_993n;
    db.prepare("UPDATE creator_groups SET revision=? WHERE id=?").run(revision, group.id);
    assert.equal(repository.groupList({}).items[0]!.group.revision, revision);
    const changed = repository.groupUpdate({ group_id: group.id, expected_revision: revision, name: "Renamed" });
    assert.equal(changed.group.revision, revision + 1n);
    assert.throws(() => repository.groupDelete({ group_id: group.id, expected_revision: revision }), /revision changed/u);
    repository.groupDelete({ group_id: group.id, expected_revision: revision + 1n });
    assert.equal(repository.groupList({}).items.length, 0);
  } finally { db.close(); }
});

test("分组成员单移和100项批移原子提交，移出及删组保留Creator和updated_at", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const first = repository.groupCreate({ name: "First" }), second = repository.groupCreate({ name: "Second" });
    const ids = Array.from({ length: 100 }, (_, index) => repository.create({ display_name: `Creator ${index}` }).creator_id);
    db.exec("UPDATE creators SET updated_at=123");
    const moved = repository.groupMove({ creator_ids: ids, destination: { kind: "group", group_id: first.id } });
    assert.equal(moved.moved_count, 100);
    assert.deepEqual(moved.assignments.map(assignment => assignment.creator_id), ids);
    assert.ok(moved.assignments.every(assignment => assignment.group?.id === first.id));
    const replay = repository.groupMove({ creator_ids: ids, destination: { kind: "group", group_id: first.id } });
    assert.deepEqual([replay.moved_count, replay.unchanged_count], [0, 100]);
    assert.throws(() => repository.groupMove({ creator_ids: [ids[0], "missing"], destination: { kind: "group", group_id: second.id } }), /creator not found/u);
    assert.equal(repository.get(ids[0]!).group!.id, first.id);
    assert.throws(() => repository.groupMove({ creator_ids: [...ids, "over-limit"], destination: { kind: "uncategorized" } }));
    assert.throws(() => repository.groupMove({ creator_ids: [ids[0], ids[0]], destination: { kind: "uncategorized" } }));
    assert.throws(() => repository.groupMove({ creator_ids: [], destination: { kind: "uncategorized" } }));
    assert.throws(() => repository.groupSet({ creator_id: ids[0], destination: { kind: "group", group_id: "missing" } }), /group not found/u);
    const ungrouped = repository.groupSet({ creator_id: ids[0], destination: { kind: "uncategorized" } });
    assert.deepEqual(ungrouped.assignments, [{ creator_id: ids[0], group: null }]);
    const deleted = repository.groupDelete({ group_id: first.id, expected_revision: 1 });
    assert.equal(deleted.uncategorized_creator_count, 99);
    assert.equal(repository.groupList({}).uncategorized_count, 100);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM creators WHERE updated_at=123").get()!.n, 100);
    assert.deepEqual(db.prepare("PRAGMA foreign_key_check").all(), []);
  } finally { db.close(); }
});

test("分组keyset覆盖100项之后的记录，统计、查询与归档游标边界保持原合同", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db);
    const groups = Array.from({ length: 105 }, (_, index) => repository.groupCreate({ name: `Group ${String(index).padStart(3, "0")}` }));
    const active = repository.create({ display_name: "Active" }), archived = repository.create({ display_name: "Archived" });
    repository.groupMove({ creator_ids: [active.creator_id, archived.creator_id], destination: { kind: "group", group_id: groups[104]!.id } });
    repository.archive(archived.creator_id);
    repository.create({ display_name: "Uncategorized" });
    const ids: string[] = [];
    let cursor: string | null = null;
    do { const page = repository.groupList({ limit: 23, cursor }); ids.push(...page.items.map(item => item.group.id)); cursor = page.next_cursor; } while (cursor !== null);
    assert.deepEqual(ids, groups.map(group => group.id));
    assert.equal(repository.groupList({ limit: 0 }).items.length, 1);
    assert.equal(repository.groupList({ limit: 1000 }).items.length, 100);
    const last = repository.groupList({ query: "104" });
    assert.equal(last.items[0]!.creator_count, 1);
    assert.equal(last.uncategorized_count, 1);
    assert.equal(repository.groupList({ query: " 104 ", include_archived: true }).items[0]!.creator_count, 2);
    const first = repository.groupList({ limit: 1 });
    assert.throws(() => repository.groupList({ cursor: first.next_cursor, query: "different" }));
    assert.throws(() => repository.groupList({ cursor: first.next_cursor, include_archived: true }));
    const envelope = JSON.parse(Buffer.from(first.next_cursor!, "base64url").toString("utf8"));
    assert.equal(envelope.context, "creator_group_list|query=|archived=false");
    assert.equal(envelope.position, `0|${groups[0]!.id}`);
  } finally { db.close(); }
});

test("关系阶段、置顶和归档使用原事务与幂等语义，置顶不改资料时间", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), created = repository.create({ display_name: "Lifecycle" });
    db.prepare("UPDATE creators SET updated_at=123 WHERE id=?").run(created.creator_id);
    const stage = repository.updateStage({ creator_id: created.creator_id, stage: "engaged" });
    assert.equal(stage.relationship.stage, "engaged");
    assert.equal(stage.changed, true);
    assert.equal(repository.updateStage({ creator_id: created.creator_id, stage: "engaged" }).changed, false);
    assert.throws(() => repository.updateStage({ creator_id: created.creator_id, stage: "archived" }), /use onecrew_creator_archive/u);
    assert.equal(repository.pin({ creator_id: created.creator_id, pinned: true }), null);
    assert.equal(repository.get(created.creator_id).creator.updated_at, 123);
    assert.ok(repository.get(created.creator_id).creator.pinned_at);
    repository.pin({ creator_id: created.creator_id, pinned: false });
    assert.equal(repository.get(created.creator_id).creator.pinned_at, null);
    const archived = repository.archive(created.creator_id);
    assert.equal(archived.changed, true);
    assert.equal(repository.get(created.creator_id).relationship.stage, "archived");
    assert.equal(repository.archive(created.creator_id).changed, false);
    assert.throws(() => repository.pin({ creator_id: created.creator_id, pinned: true }), /archived Creator/u);
    assert.throws(() => repository.updateStage({ creator_id: created.creator_id, stage: "prospect" }), /archived Creator/u);
  } finally { db.close(); }
});
