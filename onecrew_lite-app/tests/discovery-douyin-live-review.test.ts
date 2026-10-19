import assert from "node:assert/strict";
import { test } from "node:test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { discoveryReviewBasis, buildDiscoveryReviewMaterial } from "../src/discovery/review-material.ts";
import { prepareDiscoveryReview } from "../src/discovery/review-submission.ts";

const evidence = process.env.ONECREW_DOUYIN_REVIEW_EVIDENCE;
test("真实抖音评分保留精确粉丝数并正确判断粉丝门槛", { skip: !evidence }, async context => {
  const captured = parseApplicationJson(await readFile(evidence!)) as Record<string, unknown>;
  const original = discoveryReviewBasis.parse(captured.basis);
  assert.equal(original.hit.platform, "douyin");
  assert.equal(BigInt(original.native_followers!), 4722n);
  for (const minimum of [null, 4000]) await context.test(minimum === null ? "保留已核实的精确数量" : "原生数量超过4000时通过门槛", () => {
    const criteria = parseApplicationJson(Buffer.from(original.criteria_json)) as Record<string, unknown>;
    if (minimum !== null) criteria.min_subscribers = minimum;
    const basis = { ...original, criteria_json: stringifyApplicationJson(criteria) };
    const material = buildDiscoveryReviewMaterial(basis);
    const prepared = prepareDiscoveryReview(String(captured.account), {
      campaign_id: basis.campaign_id, expected_revision: basis.revision, search_hit_id: basis.hit.id,
      material_hash: material.material_hash, request_id: randomUUID(), reviewer: captured.reviewer, findings: captured.findings,
    }, basis);
    assert.notEqual(prepared.receipt.subscribers, null);
    assert.equal(BigInt(prepared.receipt.subscribers!), 4722n);
    assert.equal(prepared.receipt.verdict, "qualified");
    const result = parseApplicationJson(Buffer.from(prepared.evidence_json)) as { assessment: { subscribers_normalized: number | bigint | null } };
    assert.notEqual(result.assessment.subscribers_normalized, null);
    assert.equal(BigInt(result.assessment.subscribers_normalized!), 4722n);
  });
});
