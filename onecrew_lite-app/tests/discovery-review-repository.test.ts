import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { loadDiscoveryReviewBasis } from "../src/discovery/review-basis.ts";
import { buildDiscoveryReviewMaterial } from "../src/discovery/review-material.ts";
import { DiscoveryReviewRepository } from "../src/discovery/review-repository.ts";

function input(database: DatabaseSync, request: string) {
  const hit = z.string().parse(database.prepare("SELECT id FROM discovery_search_hits ORDER BY id LIMIT 1").get()?.id);
  database.exec("BEGIN DEFERRED"); const material = buildDiscoveryReviewMaterial(loadDiscoveryReviewBasis(database, "merge-review", 1, hit)); database.exec("COMMIT");
  const quote = z.string().min(1).parse(material.video_description);
  return { campaign_id: "merge-review", expected_revision: 1, search_hit_id: hit, material_hash: material.material_hash, request_id: request, reviewer: { kind: "human", label: "Local saved-material transaction acceptance" }, findings: [{ criterion: "category_content", outcome: "supported", rationale: "The saved original description establishes product relevance.", citations: [{ kind: "description", quote }] }, { criterion: "match_fit", outcome: "supported", match_level: "good", rationale: "The saved description provides first-hand product context.", citations: [{ kind: "description", quote }] }] };
}

test("真实原SQLite审核双连接提交幂等、陈旧材料拒绝及写失败事务回滚", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-review-commit-", import.meta.url))), path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), path);
  const database = new DatabaseSync(path), other = new DatabaseSync(path), first = new DiscoveryReviewRepository(database), second = new DiscoveryReviewRepository(other);
  try {
    database.exec("PRAGMA foreign_keys=ON"); other.exec("PRAGMA foreign_keys=ON");
    const original = input(database, "node-concurrent"), preparedA = first.prepareSubmission("operator", original), preparedB = second.prepareSubmission("operator", original);
    assert.equal(preparedA.state, "prepared"); assert.equal(preparedB.state, "prepared");
    if (preparedA.state !== "prepared" || preparedB.state !== "prepared") throw new Error("Unexpected prior fixture receipt");
    const receipt = first.commitPrepared(preparedA.prepared); assert.equal(receipt.verdict, "qualified");
    assert.deepEqual(second.commitPrepared(preparedB.prepared), receipt); assert.deepEqual(second.submit("operator", original), receipt);
    assert.equal(database.prepare("SELECT COUNT(*) AS n FROM discovery_evidence_reviews WHERE request_id=?").get(original.request_id)?.n, 1);
    assert.deepEqual(first.get("operator", original.campaign_id, original.request_id), receipt); assert.equal(first.get("another-owner", original.campaign_id, original.request_id), null);
    assert.throws(() => second.submit("operator", { ...original, reviewer: { ...original.reviewer, label: "Changed reviewer" } }), /idempotency_conflict/);
    const stale = first.prepareSubmission("operator", input(database, "node-stale")); assert.equal(stale.state, "prepared");
    if (stale.state !== "prepared") throw new Error("Unexpected prior fixture receipt");
    other.prepare("UPDATE discovery_search_hits SET title=title||? WHERE id=?").run(" changed", original.search_hit_id);
    assert.throws(() => first.commitPrepared(stale.prepared), /review_material_changed/);
    assert.equal(first.get("operator", original.campaign_id, "node-stale"), null);
    assert.deepEqual(first.submit("operator", original), receipt);
    const denied = first.prepareSubmission("operator", input(database, "node-denied")); if (denied.state !== "prepared") throw new Error("Unexpected prior fixture receipt");
    database.exec("CREATE TRIGGER deny_candidate_review BEFORE UPDATE ON discovery_candidates BEGIN SELECT RAISE(ABORT,'candidate_review_denied'); END");
    assert.throws(() => first.commitPrepared(denied.prepared), /candidate_review_denied/);
    assert.equal(first.get("operator", original.campaign_id, "node-denied"), null);
    database.exec("DROP TRIGGER deny_candidate_review");
    denied.prepared.evidence_json = "{}";
    assert.throws(() => first.commitPrepared(denied.prepared), /preparation_not_owned_or_changed/);
    assert.throws(() => second.commitPrepared(preparedA.prepared), /preparation_not_owned_or_changed/);
  } finally { other.close(); database.close(); }
});
