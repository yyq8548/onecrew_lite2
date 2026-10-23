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
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { SELECTION_QUALITY_REVIEWER } from "../src/discovery/quality-policy.ts";

function input(database: DatabaseSync, request: string) {
  const hit = z.string().parse(database.prepare("SELECT id FROM discovery_search_hits ORDER BY id LIMIT 1").get()?.id);
  database.exec("BEGIN DEFERRED"); const material = buildDiscoveryReviewMaterial(loadDiscoveryReviewBasis(database, "merge-review", 1, hit)); database.exec("COMMIT");
  const quote = z.string().min(1).parse(material.video_description);
  return { campaign_id: "merge-review", expected_revision: 1, search_hit_id: hit, material_hash: material.material_hash, request_id: request, reviewer: { kind: "agent", label: SELECTION_QUALITY_REVIEWER }, findings: [{ criterion: "selection_fit", outcome: "supported", rationale: "The saved author-bound shampoo review establishes the requested topic; its saved 20K subscriber observation exceeds the required 10K.", citations: [{ kind: "description", quote }] }, { criterion: "quality_rating", outcome: "supported", match_level: "low", rationale: "The short saved description establishes topic relevance but provides little evidence of depth or instructional quality, so its quality tier is low while the creator remains matched.", citations: [{ kind: "description", quote }] }] };
}

test("真实原SQLite审核双连接提交幂等、陈旧材料拒绝及写失败事务回滚", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-review-commit-", import.meta.url))), path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), path);
  const database = new DatabaseSync(path), other = new DatabaseSync(path), first = new DiscoveryReviewRepository(database), second = new DiscoveryReviewRepository(other);
  try {
    database.exec("PRAGMA foreign_keys=ON"); other.exec("PRAGMA foreign_keys=ON");
    const historical = z.object({ operator_account_id: z.string(), discovery_campaign_id: z.string(), criteria_revision: z.number(), search_hit_id: z.string(), material_hash: z.string(), request_id: z.string(), reviewer_json: z.string(), findings_json: z.string(), assessment_json: z.string() }).parse(database.prepare("SELECT operator_account_id,discovery_campaign_id,criteria_revision,search_hit_id,material_hash,request_id,reviewer_json,findings_json,assessment_json FROM discovery_evidence_reviews ORDER BY request_id LIMIT 1").get());
    const historicalInput = { campaign_id: historical.discovery_campaign_id, expected_revision: historical.criteria_revision, search_hit_id: historical.search_hit_id, material_hash: historical.material_hash, request_id: historical.request_id, reviewer: parseApplicationJson(Buffer.from(historical.reviewer_json)), findings: parseApplicationJson(Buffer.from(historical.findings_json)) }, historicalReceipt = parseApplicationJson(Buffer.from(historical.assessment_json));
    assert.deepEqual(first.get(historical.operator_account_id, historical.discovery_campaign_id, historical.request_id), historicalReceipt);
    assert.deepEqual(second.submit(historical.operator_account_id, historicalInput), historicalReceipt);
    const original = input(database, "node-concurrent"), preparedA = first.prepareSubmission("operator", original), preparedB = second.prepareSubmission("operator", original);
    assert.equal(preparedA.state, "prepared"); assert.equal(preparedB.state, "prepared");
    if (preparedA.state !== "prepared" || preparedB.state !== "prepared") throw new Error("Unexpected prior fixture receipt");
    const receipt = first.commitPrepared(preparedA.prepared); assert.equal(receipt.verdict, "qualified");
    assert.deepEqual(second.commitPrepared(preparedB.prepared), receipt); assert.deepEqual(second.submit("operator", original), receipt);
    assert.equal(database.prepare("SELECT COUNT(*) AS n FROM discovery_evidence_reviews WHERE request_id=?").get(original.request_id)?.n, 1);
    assert.deepEqual(first.get("operator", original.campaign_id, original.request_id), receipt); assert.equal(first.get("another-owner", original.campaign_id, original.request_id), null);
    assert.throws(() => second.submit("operator", { ...original, reviewer: { ...original.reviewer, label: "Changed reviewer" } }), /idempotency_conflict/);
    const current = input(database, "node-retired-scoring"), retired = { ...current, findings: [current.findings[0], { ...current.findings[1], criterion: "match_fit", match_level: "good" }] };
    assert.throws(() => first.submit("operator", retired), /legacy_match_scoring_retired/);
    assert.equal(first.get("operator", original.campaign_id, retired.request_id), null);
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
