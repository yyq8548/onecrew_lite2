import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { loadDiscoveryReviewBasis } from "../src/discovery/review-basis.ts";
import { buildDiscoveryReviewMaterial } from "../src/discovery/review-material.ts";
import { discoveryReviewRequestHash, prepareDiscoveryReview } from "../src/discovery/review-submission.ts";
import { SELECTION_QUALITY_POLICY, SELECTION_QUALITY_REVIEWER } from "../src/discovery/quality-policy.ts";

test("原Rust实际审核ledger的typed请求hash保留Option省略和字段序列", () => {
  let checked = 0;
  for (const file of ["baseline.sqlite", "merged.sqlite", "stale.sqlite", "revoked.sqlite"]) {
    const db = new DatabaseSync(fileURLToPath(new URL(`./fixtures/discovery-identity/${file}`, import.meta.url)), { readOnly: true });
    try {
      for (const row of db.prepare("SELECT operator_account_id,discovery_campaign_id,criteria_revision,search_hit_id,material_hash,request_id,reviewer_json,findings_json,request_hash FROM discovery_evidence_reviews").all()) {
        const input = { campaign_id: row.discovery_campaign_id, expected_revision: row.criteria_revision, search_hit_id: row.search_hit_id, material_hash: row.material_hash, request_id: row.request_id, reviewer: parseApplicationJson(Buffer.from(z.string().parse(row.reviewer_json))), findings: parseApplicationJson(Buffer.from(z.string().parse(row.findings_json))) };
        assert.equal(discoveryReviewRequestHash(z.string().parse(row.operator_account_id), input), row.request_hash); checked++;
      }
    } finally { db.close(); }
  }
  assert.ok(checked >= 8);
});

test("真实原材料准备审核只接受其引用及material hash，编码保留审计归属且不改来源", () => {
  const db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), { readOnly: true });
  try {
    const hit = z.string().parse(db.prepare("SELECT id FROM discovery_search_hits ORDER BY id LIMIT 1").get()?.id);
    db.exec("BEGIN DEFERRED"); const basis = loadDiscoveryReviewBasis(db, "merge-review", 1, hit); db.exec("COMMIT");
    const material = buildDiscoveryReviewMaterial(basis), quote = z.string().min(1).parse(material.video_description);
    const input = { campaign_id: "merge-review", expected_revision: 1, search_hit_id: hit, material_hash: material.material_hash, request_id: "node-offline-prepare", reviewer: { kind: "agent", label: SELECTION_QUALITY_REVIEWER }, findings: [ { criterion: "selection_fit", outcome: "supported", rationale: "The saved author-bound shampoo review establishes the requested topic; its saved 20K subscriber observation exceeds the required 10K.", citations: [{ kind: "description", quote }] }, { criterion: "quality_rating", outcome: "supported", match_level: "low", rationale: "The short saved description establishes topic relevance but offers little evidence of depth or instructional quality, supporting the low quality tier without excluding the matched creator.", citations: [{ kind: "description", quote }] } ] };
    const prepared = prepareDiscoveryReview("operator", input, basis);
    assert.equal(prepared.receipt.operator_account_id, "operator"); assert.equal(prepared.receipt.candidate_id, basis.candidate_id); assert.equal(prepared.receipt.verdict, "qualified");
    assert.equal(prepared.receipt.policy_version, SELECTION_QUALITY_POLICY);
    const quality = z.object({ reviewed_quality: z.object({ level: z.string().nullable(), score: z.number().nullable() }) }).parse(parseApplicationJson(Buffer.from(prepared.evidence_json))).reviewed_quality;
    assert.deepEqual(quality, { level: "low", score: 40 });
    const unrated = prepareDiscoveryReview("operator", { ...input, findings: [input.findings[0], { ...input.findings[1], outcome: "insufficient", match_level: null, rationale: "The saved short description does not establish a quality tier." }] }, basis);
    assert.equal(unrated.receipt.verdict, "qualified");
    assert.deepEqual(z.object({ reviewed_quality: z.object({ level: z.string().nullable(), score: z.number().nullable() }) }).parse(parseApplicationJson(Buffer.from(unrated.evidence_json))).reviewed_quality, { level: null, score: null });
    assert.equal(prepared.request_hash, discoveryReviewRequestHash("operator", input));
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_evidence_reviews WHERE request_id=?").get(input.request_id)?.n, 0);
    assert.throws(() => prepareDiscoveryReview("operator", { ...input, material_hash: "0".repeat(64) }, basis), /review_material_changed/);
    assert.throws(() => prepareDiscoveryReview("operator", { ...input, findings: [{ ...input.findings[0], citations: [{ kind: "description", quote: "invented provider text" }] }, input.findings[1]] }, basis), /selection_quality_citation_invalid/);
    assert.throws(() => prepareDiscoveryReview("operator", { ...input, findings: [input.findings[0], { ...input.findings[1], criterion: "match_fit", match_level: "good" }] }, basis), /legacy_match_scoring_retired/);
  } finally { db.close(); }
});
