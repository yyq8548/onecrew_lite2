import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { CreatorRepository } from "../src/creators/repository.ts";
import { writeDesktopExport } from "../src/creators/transfer-export.ts";
import { decodeWorkbook, encodeWorkbook } from "../src/creators/workbook.ts";
import { buildBundle, historicalDigests, validateBundle, type TransferCreator } from "../src/creators/transfer-contracts.ts";

function database() { const db = new DatabaseSync(":memory:"); db.exec("PRAGMA foreign_keys=ON"); for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8")); db.exec("PRAGMA user_version=90"); return db; }
test("原Rust portable_public安全整数边界允许MAX_SAFE且拒绝u64MAX，不改变Creator原资料", async () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), created = repository.create({ display_name: "Original integer boundary" });
    repository.update({ creator_id: created.creator_id, follower_counts: { x: Number.MAX_SAFE_INTEGER } });
    const snapshot = repository.exportSnapshot();
    assert.equal(snapshot.bundle.creators[0]!.follower_counts.x, Number.MAX_SAFE_INTEGER);
    assert.deepEqual(await decodeWorkbook(await encodeWorkbook(snapshot.bundle)), snapshot.bundle);
    for (const value of [9007199254740992, 18446744073709551615n]) {
      assert.throws(() => validateBundle({ ...snapshot.bundle, creators: snapshot.bundle.creators.map(creator => ({ ...creator, follower_counts: { x: value } })) }));
    }
    repository.update({ creator_id: created.creator_id, follower_counts: { x: 18446744073709551615n } });
    assert.throws(() => repository.exportSnapshot());
    assert.equal(repository.get(created.creator_id).creator.follower_counts.x, 18446744073709551615n);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM creator_import_operations").get()!.n, 0);
  } finally { db.close(); }
});
test("导出只携带原portable_public字段，私有备注/assessment/分组历史不进入bundle", async () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), created = repository.create({ display_name: "Private record", notes: "private note", contact_routes: [{ channel: "email", value: "public@example.com" }] }), group = repository.groupCreate({ name: "private group" });
    repository.update({ creator_id: created.creator_id, assessment: "private assessment", score: 22, follower_counts: { x: 45 } }); repository.groupSet({ creator_id: created.creator_id, destination: { kind: "group", group_id: group.id } });
    const snapshot = repository.exportSnapshot(), serialized = JSON.stringify(snapshot.bundle); assert.ok(!serialized.includes("private note")); assert.ok(!serialized.includes("private assessment")); assert.ok(!serialized.includes("private group")); assert.ok(!serialized.includes(created.creator_id)); assert.equal(snapshot.display.values().next().value!.group_name, "private group");
    const first = await encodeWorkbook(snapshot.bundle, snapshot.display), second = await encodeWorkbook(snapshot.bundle, snapshot.display); assert.deepEqual(first, second); assert.deepEqual(await decodeWorkbook(first), snapshot.bundle);
    const withoutDisplay = await encodeWorkbook(snapshot.bundle); assert.deepEqual(await decodeWorkbook(withoutDisplay), snapshot.bundle);
  } finally { db.close(); }
});
test("desktop导出真实文件原子无覆盖/重复使用/明确不可用，返回无绝对路径", async () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db); repository.create({ display_name: "Desktop sample" }); const snapshot = repository.exportSnapshot();
    const base = dirname(fileURLToPath(import.meta.url));
    const scratch = join(base, "..", "..", ".scratch"); await mkdir(scratch, { recursive: true }); const directory = mkdtempSync(join(scratch, "creator-export-"));
    const first = await writeDesktopExport(snapshot, async () => directory), second = await writeDesktopExport(snapshot, async () => directory); assert.equal(first.reused_existing, false); assert.equal(second.reused_existing, true); assert.equal(first.file_name, second.file_name); assert.ok(!JSON.stringify(first).includes(directory));
    assert.deepEqual(await decodeWorkbook(readFileSync(join(directory, first.file_name))), snapshot.bundle);
    const racedDirectory = mkdtempSync(join(scratch, "creator-export-race-")), raced = await Promise.all(Array.from({ length: 3 }, () => writeDesktopExport(snapshot, async () => racedDirectory)));
    assert.equal(raced.filter(result => !result.reused_existing).length, 1); assert.equal(new Set(raced.map(result => result.file_name)).size, 1);
    writeFileSync(join(directory, first.file_name), "existing user data"); await assert.rejects(writeDesktopExport(snapshot, async () => directory), /different content/u); assert.equal(readFileSync(join(directory, first.file_name), "utf8"), "existing user data");
    await assert.rejects(writeDesktopExport(snapshot, undefined), /unavailable/u);
  } finally { db.close(); }
});
test("原Rust v2 frozen Beta语义digest与record_key及历史根digest保持兼容", () => {
  const beta: TransferCreator = { semantic_sha256: "", record_key: "", display_name: "Beta", tags: [], score: null, follower_counts: {}, pinned: false, relationship: { stage: "prospect" }, accounts: [], contact_routes: [{ save_order: 0, channel: "email", locator_kind: "email_address", normalized_value: "beta@example.com", display_value: "beta@example.com", confidence: "exact", source_kinds: [] }] };
  const legacy = buildBundle([beta], 2).creators[0]!;
  assert.equal(legacy.semantic_sha256, "sha256:ce437e46267f6b66cea16c8c7afabaf9c62d51f1f2b060592f27272d65b909a1"); assert.equal(legacy.record_key, "sha256:66dfcc3a74153f78c75f5cc754e70b51a6729c0a3b166a16807e5fd367bb0529");
  const current = buildBundle([beta]); assert.ok(historicalDigests(current).includes(buildBundle([beta], 2).content_sha256)); assert.ok(historicalDigests(current).includes(buildBundle([beta], 1).content_sha256));
});
