import assert from "node:assert/strict";
import { copyFileSync, mkdtempSync, readFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import ExcelJS from "exceljs";
import JSZip from "jszip";
import { CreatorRepository } from "../src/creators/repository.ts";
import { buildBundle, readStagedBundle, type TransferCreator } from "../src/creators/transfer-contracts.ts";
import { canonicalWorkbookZip, decodeWorkbook, encodeWorkbook } from "../src/creators/workbook.ts";
import { creatorBatchPreview } from "../src/creators/batch.ts";

function database() { const db = new DatabaseSync(":memory:"); db.exec("PRAGMA foreign_keys=ON"); for (const name of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${name}`, import.meta.url), "utf8")); db.exec("PRAGMA user_version=90"); return db; }
function person(): TransferCreator { return { semantic_sha256: "", record_key: "", display_name: "公开作者", tags: ["研究"], notes: "Public source evidence", score: 80, follower_counts: { x: 1234, tiktok: 50 }, pinned: true, relationship: { stage: "engaged" }, accounts: [{ capture_order: 0, platform: "x", target_kind: "user", external_id: "1234", link_state: "confirmed", confidence: "exact", source_kind: "profile_bio", latest_profile: { handle: "writer", display_name: "公开作者", profile_url: "https://x.com/writer", avatar_url: "https://example.com/avatar.png" } }], contact_routes: [{ save_order: 0, channel: "email", locator_kind: "email_address", normalized_value: "writer@example.com", display_value: "writer@example.com", confidence: "asserted", source_kinds: ["profile_bio", "creator_import"] }, { save_order: 1, channel: "wechat", locator_kind: "handle", normalized_value: "Writer_WX", display_value: "Writer_WX", confidence: "asserted", source_kinds: ["creator_import"] }] }; }
test("canonical XLSX实际读写及schema90完整导入回读，prepare/confirm幂等", async () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), bundle = buildBundle([person()]), bytes = await encodeWorkbook(bundle);
    assert.deepEqual(await decodeWorkbook(bytes), bundle);
    const first = repository.importPrepare(bundle); assert.equal(first.status, "confirmation_required"); if (first.status !== "confirmation_required") throw new Error("missing preview");
    assert.equal(repository.list({}).items.length, 0); assert.deepEqual(repository.importPrepare(bundle), first);
    const imported = repository.importConfirm({ operation_id: first.operation_id }); assert.equal(imported.result.status, "imported"); assert.equal(imported.changed, true);
    assert.equal(repository.importConfirm({ operation_id: first.operation_id }).changed, false);
    assert.equal(repository.importPrepare(bundle).status, "already_imported");
    assert.deepEqual(repository.exportSnapshot().bundle, bundle);
    const listed = repository.list({}).items[0]!; assert.equal(listed.display_name, "公开作者"); assert.equal(repository.get(listed.id).creator.notes, "Public source evidence");
    assert.equal(db.prepare("PRAGMA foreign_key_check").all().length, 0);
  } finally { db.close(); }
});
test("导入只按公开稳定身份合并，填补空字段并保留私有备注/阶段/历史", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), bundle = buildBundle([person()]), first = repository.importPrepare(bundle); if (first.status !== "confirmation_required") throw new Error("missing preview"); repository.importConfirm({ operation_id: first.operation_id });
    const id = repository.list({}).items[0]!.id; repository.update({ creator_id: id, notes: "private user note", score: 90 }); repository.updateStage({ creator_id: id, stage: "collaborating" });
    const changed = person(); changed.display_name = "new public name"; changed.score = 99; changed.follower_counts.youtube = 700;
    const prepared = repository.importPrepare(buildBundle([changed])); assert.equal(prepared.status, "confirmation_required"); if (prepared.status !== "confirmation_required") throw new Error("missing preview");
    assert.equal(prepared.impact.follower_counts_filled, 1); repository.importConfirm({ operation_id: prepared.operation_id });
    const detail = repository.get(id); assert.equal(detail.creator.notes, "private user note"); assert.equal(detail.creator.score, 90); assert.equal(detail.creator.display_name, "公开作者"); assert.equal(detail.relationship.stage, "collaborating"); assert.equal(detail.creator.follower_counts.youtube, 700);
    assert.ok(!JSON.stringify(repository.exportSnapshot().bundle).includes("private user note"));
  } finally { db.close(); }
});
test("导入取消/过期/相关事实变化/冲突阻断均无图写入", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), bundle = buildBundle([person()]);
    const first = repository.importPrepare(bundle); if (first.status !== "confirmation_required") throw new Error("missing preview");
    assert.equal(repository.importCancel({ operation_id: first.operation_id }).status, "cancelled"); assert.equal(repository.importConfirm({ operation_id: first.operation_id }).result.status, "cancelled");
    const second = repository.importPrepare(bundle); if (second.status !== "confirmation_required") throw new Error("missing preview"); db.prepare("UPDATE creator_import_operations SET expires_at=0 WHERE id=?").run(second.operation_id);
    assert.equal(repository.importConfirm({ operation_id: second.operation_id }).result.status, "expired");
    const third = repository.importPrepare(bundle); if (third.status !== "confirmation_required") throw new Error("missing preview"); repository.create({ display_name: "other", contact_routes: [{ channel: "email", value: "writer@example.com" }] });
    assert.equal(repository.importConfirm({ operation_id: third.operation_id }).result.status, "stale"); const blocked = repository.importPrepare(bundle); assert.equal(blocked.status, "blocked"); if (blocked.status === "blocked") assert.equal(blocked.conflicts[0]!.kind, "possible_contact_match");
    assert.equal(repository.list({}).items.length, 1);
  } finally { db.close(); }
});
test("实际Library推荐样例包含主页hyperlink/Identity与评分说明，合并同主页保留微信邮件", async () => {
  const book = new ExcelJS.Workbook(), sheet = book.addWorksheet("推荐"); sheet.addRow(["名字", "类别", "平台", "评分", "联系方式"]);
  for (let i = 0; i < 2; i++) { const row = sheet.addRow([{ text: "Example", hyperlink: "https://space.bilibili.com/12345" }, "知识", "bilibili", "良好", "微信：Example_WX（备注合作）\nMAIL@example.com"]); row.getCell(1).note = 'Identity: {"platform":"bilibili","platform_id":"12345"}'; row.getCell(4).note = "Original score reason & evidence"; }
  const bundle = await decodeWorkbook(await canonicalWorkbookZip(Buffer.from(await book.xlsx.writeBuffer()))); assert.equal(bundle.creator_count, 1); assert.equal(bundle.creators[0]!.contact_routes.length, 3); assert.equal(bundle.creators[0]!.accounts.length, 0); assert.ok(bundle.creators[0]!.notes!.includes("Original score reason & evidence")); assert.deepEqual(await decodeWorkbook(await encodeWorkbook(bundle)), bundle);
});
test("成熟解析器拒绝公式、错误、额外列、无身份的推荐表和过大文件", async () => {
  const bytes = await encodeWorkbook(buildBundle([person()])), book = new ExcelJS.Workbook(); await book.xlsx.load(bytes as unknown as ExcelJS.Buffer); book.worksheets[0]!.getCell("B2").value = { formula: '"unsafe"', result: "unsafe" }; await assert.rejects(decodeWorkbook(await canonicalWorkbookZip(Buffer.from(await book.xlsx.writeBuffer()))), /formula/u);
  const invalid = new ExcelJS.Workbook(), sheet = invalid.addWorksheet("推荐"); sheet.addRow(["名字", "类别", "平台", "评分", "联系方式"]); sheet.addRow(["No identity"]); await assert.rejects(decodeWorkbook(await canonicalWorkbookZip(Buffer.from(await invalid.xlsx.writeBuffer()))), /主页链接/u);
  await assert.rejects(decodeWorkbook(Buffer.alloc(32 * 1048576 + 1)), /bounded/u);
});
test("原OOXML安全边界拒绝DTD/重复属性/宏/外链/超范围单元格和ZIP目录", async () => {
  const canonical = await encodeWorkbook(buildBundle([person()]));
  const mutate = async (part: string, change: (value: string) => string) => { const zip = await JSZip.loadAsync(canonical); const original = zip.file(part); if (!original) throw new Error("part missing"); zip.file(part, change(await original.async("string")), { createFolders: false }); return zip.generateAsync({ type: "nodebuffer", compression: "DEFLATE" }); };
  await assert.rejects(decodeWorkbook(await mutate("xl/worksheets/sheet1.xml", value => value.replace("<worksheet ", '<worksheet duplicate="1" duplicate="2" '))), /duplicate XML attribute/u);
  await assert.rejects(decodeWorkbook(await mutate("xl/worksheets/sheet1.xml", value => value.replace("<worksheet ", '<!DOCTYPE worksheet><worksheet '))), /DTD/u);
  await assert.rejects(decodeWorkbook(await mutate("xl/worksheets/sheet1.xml", value => value.replace('r="A2"', 'r="XFD1048576"'))), /dimensions/u);
  await assert.rejects(decodeWorkbook(await mutate("xl/_rels/workbook.xml.rels", value => value.replace('Target="worksheets/sheet1.xml"', 'Target="file:///C:/private.xml" TargetMode="External"'))), /external/u);
  const active = await JSZip.loadAsync(canonical); active.file("xl/vbaProject.bin", Buffer.from("actual rejected binary part"), { createFolders: false }); await assert.rejects(decodeWorkbook(await active.generateAsync({ type: "nodebuffer", compression: "DEFLATE" })), /active/u);
  const raw = new ExcelJS.Workbook(); raw.addWorksheet("Creators"); await assert.rejects(decodeWorkbook(Buffer.from(await raw.xlsx.writeBuffer())), /unsafe/u);
});
test("原批次50项上限、strict全拒绝、permissive逐项拒绝、真实幂等和原子回滚", () => {
  const db = database();
  try {
    const repository = new CreatorRepository(db), items = Array.from({ length: 50 }, (_, index) => ({ display_name: `Creator ${index}`, contact_routes: [{ channel: "x", value: `unique_${index}` }] }));
    const preview = creatorBatchPreview({ idempotency_key: "limit", validation: "strict", items }); assert.equal(preview.kind, "creator_batch_confirmation"); assert.equal(preview.sections[1]!.items!.length, 50); assert.equal(repository.list({}).items.length, 0);
    const full = repository.batchCreate({ idempotency_key: "limit", validation: "strict", items }); assert.equal(full.outcomes.length, 50); assert.equal(repository.list({ limit: 100 }).items.length, 50);
    assert.throws(() => repository.batchCreate({ idempotency_key: "over", validation: "strict", items: [...items, { display_name: "51" }] }));
    const strict = { idempotency_key: "strict", validation: "strict", items: [{ display_name: "valid" }, { display_name: "invalid", contact_routes: [{ channel: "email", value: "bad" }] }] };
    assert.throws(() => repository.batchCreate(strict), /item 1 rejected/u); assert.equal(repository.list({ limit: 100 }).items.length, 50); assert.equal(db.prepare("SELECT 1 FROM idempotency_records WHERE key='strict'").get(), undefined);
    const permissive = { ...strict, idempotency_key: "permissive", validation: "permissive" }, partial = repository.batchCreate(permissive); assert.deepEqual(partial.outcomes.map(o => (o as { kind: string }).kind), ["accepted", "rejected"]); assert.equal(repository.batchCreate(permissive).replayed, true);
    assert.throws(() => repository.batchCreate({ ...permissive, items: [{ display_name: "changed" }] }), /different request/u);
    db.exec("CREATE TRIGGER reject_specific_creator BEFORE INSERT ON creators WHEN NEW.display_name='reject write' BEGIN SELECT RAISE(ABORT,'reject real write'); END");
    const before = repository.list({ limit: 100 }).items.length;
    assert.throws(() => repository.batchCreate({ idempotency_key: "rollback", validation: "strict", items: [{ display_name: "first write" }, { display_name: "reject write" }] }), /reject real write/u);
    assert.equal(repository.list({ limit: 100 }).items.length, before); assert.equal(db.prepare("SELECT state FROM idempotency_records WHERE key='rollback'").get()!.state, "failed"); assert.equal(db.prepare("PRAGMA foreign_key_check").all().length, 0);
    db.exec("CREATE TRIGGER reject_failed_receipt BEFORE UPDATE ON idempotency_records WHEN NEW.state='failed' BEGIN SELECT RAISE(ABORT,'reject failed receipt'); END");
    assert.throws(() => repository.batchCreate({ idempotency_key: "two-errors", validation: "strict", items: [{ display_name: "reject write" }] }), (error: unknown) => error instanceof Error && error.name === "SuppressedError" && "error" in error && error.error instanceof Error && /reject failed receipt/u.test(error.error.message) && "suppressed" in error && error.suppressed instanceof Error && /reject real write/u.test(error.suppressed.message));
    assert.throws(() => creatorBatchPreview({ idempotency_key: "x", validation: "strict", items: [{ display_name: "x" }], confirmed: true }));
  } finally { db.close(); }
});
const oracleDirectory = process.env.ONECREW_CREATOR_IMPORT_ORACLE_DIRECTORY ?? fileURLToPath(new URL("./fixtures/creator-import/", import.meta.url));
test("原Rust真实v2/v3/v4 staged SQLite、canonical bytes、XLSX跨语言确认和receipt兼容", async () => {
  if (!oracleDirectory) throw new Error("Rust oracle directory required");
  const scratch = new URL("../../.scratch/", import.meta.url); mkdirSync(scratch, { recursive: true }); const directory = mkdtempSync(fileURLToPath(new URL("creator-import-oracle-", scratch)));
  for (const version of [2, 3, 4]) {
    const file = join(directory, `v${version}.sqlite`); copyFileSync(join(oracleDirectory, `v${version}.sqlite`), file);
    const db = new DatabaseSync(file);
    try {
      db.exec("PRAGMA foreign_keys=ON");
      const repository = new CreatorRepository(db), raw = readFileSync(join(oracleDirectory, `v${version}.bundle.json`), "utf8"), bundle = readStagedBundle(raw), xlsx = await decodeWorkbook(readFileSync(join(oracleDirectory, `v${version}.xlsx`)));
      if (version !== 2) assert.deepEqual(xlsx, bundle);
      const operation = db.prepare("SELECT id,plan_fingerprint,expires_at FROM creator_import_operations WHERE status='prepared'").get()!;
      let operationId = operation.id;
      if (Number(operation.expires_at) <= Math.floor(Date.now() / 1000)) {
        const expired = repository.importConfirm({ operation_id: operationId });
        assert.equal(expired.result.status, "expired"); assert.equal(expired.changed, false);
        const fresh = repository.importPrepare(bundle); assert.equal(fresh.status, "confirmation_required");
        if (fresh.status !== "confirmation_required") throw new Error("Fresh original bundle requires confirmation");
        operationId = fresh.operation_id;
        assert.equal(db.prepare("SELECT plan_fingerprint FROM creator_import_operations WHERE id=?").get(operationId)!.plan_fingerprint, operation.plan_fingerprint);
      }
      const confirmed = repository.importConfirm({ operation_id: operationId }); assert.equal(confirmed.result.status, "imported", `v${version} original Rust fingerprint must match`); assert.equal(confirmed.changed, true);
      assert.equal(repository.importConfirm({ operation_id: operationId }).changed, false); assert.equal(repository.importPrepare(xlsx).status, "already_imported");
      assert.deepEqual(repository.exportSnapshot().bundle, xlsx); assert.equal(db.prepare("PRAGMA foreign_key_check").all().length, 0);
    } finally { db.close(); }
  }
});
