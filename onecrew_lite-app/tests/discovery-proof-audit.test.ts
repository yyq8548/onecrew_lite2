import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { stripUnauditedDiscoveryProofs } from "../src/discovery/review-proof-audit.ts";

test("真实schema90审核查询移除没有持久审计收据的五类verified标签，保留历史原始观察", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-audit-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite"); await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path);
  const source = "https://www.youtube.com/watch?v=LR04bU_yV5k", common = { verified: true, native_author_id: "author", source_url: source, review_id: "uncommitted", material_hash: "uncommitted" };
  const raw = { source_urls: [source], own_ai_tool_demo: { source_url: source }, reviewed_apparel_content: { ...common, policy_version: "apparel-residence-review-v1" }, reviewed_category_content: { ...common, category: "shoes", policy_version: "category-content-review-v1" }, reviewed_residence: { ...common, country_code: "US", policy_version: "apparel-native-review-v2" }, reviewed_language: { ...common, language: "English", outcome: "supported", reason: "stored text", policy_version: "language-match-review-v1" }, reviewed_match: { ...common, outcome: "supported", level: "good", reason: "stored text", policy_version: "initial-match-evaluation-v1" } };
  try {
    assert.throws(() => stripUnauditedDiscoveryProofs(db, raw, "candidate", "plan", 1, "author"), /requires_snapshot/);
    db.exec("BEGIN DEFERRED");
    const audited = stripUnauditedDiscoveryProofs(db, raw, "candidate", "plan", 1, "author");
    assert.deepEqual(audited, { source_urls: [source], own_ai_tool_demo: { source_url: source } });
    assert.equal(Object.hasOwn(raw, "reviewed_match"), true); db.exec("COMMIT");
  } finally { db.close(); }
});
