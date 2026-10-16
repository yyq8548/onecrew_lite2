import assert from "node:assert/strict";
import { mkdtemp, copyFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";

test("真实历史计划修改复用原始搜索资料，重新评估资格并拒绝失效源版本", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-refinement-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await copyFile(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), path);
  const db = new DatabaseSync(path), repository = new DiscoveryRepository(db);
  try {
    db.exec("PRAGMA foreign_keys=ON");
    const source = repository.getPlan("merge-review");
    const scopes = source.scopes;
    const original = db.prepare("SELECT source_url,title,author_label,captured_at FROM discovery_search_hits WHERE discovery_campaign_id=? ORDER BY source_url").all(source.id);
    assert.ok(original.length > 0);
    const input = { id: "refined", brief: source.brief, opencli_version: source.opencli_version, criteria: { ...source.criteria, refinement: { campaign_id: source.id, expected_revision: source.revision } }, scopes, target_count: source.target_count, max_calls: source.max_calls };
    repository.create(input);
    assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_search_hits WHERE discovery_campaign_id=?").get(input.id)?.n, 0);
    repository.approve(input.id, 1);
    assert.deepEqual(db.prepare("SELECT source_url,title,author_label,captured_at FROM discovery_search_hits WHERE discovery_campaign_id=? ORDER BY source_url").all(input.id), original);
    assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_search_positions p JOIN discovery_search_hits h ON h.id=p.search_hit_id WHERE h.discovery_campaign_id=?").get(input.id)?.n, original.length);
    assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_candidates WHERE discovery_campaign_id=?").get(input.id)?.n, 0);
    assert.equal(repository.getPlan(input.id).used_calls, 0);
    assert.throws(() => repository.approve(input.id, 1), /revision_changed/);
    assert.throws(() => repository.create({ ...input, id: "stale", criteria: { ...input.criteria, refinement: { campaign_id: source.id, expected_revision: source.revision + 1 } } }), /source_changed/);
    assert.equal(repository.findPlan("stale"), null);
    repository.create({ ...input, id: "changed-before-approval" });
    db.prepare("UPDATE discovery_campaign_revisions SET approved_at=NULL WHERE discovery_campaign_id=? AND revision=?").run(source.id, source.revision);
    assert.throws(() => repository.approve("changed-before-approval", 1), /source_changed/);
    assert.equal(repository.getPlan("changed-before-approval").phase, "draft");
    assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_search_hits WHERE discovery_campaign_id=?").get("changed-before-approval")?.n, 0);
  } finally { db.close(); }
});
