import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { loadDiscoveryReviewBasis } from "../src/discovery/review-basis.ts";
import { buildDiscoveryReviewMaterial } from "../src/discovery/review-material.ts";
import { applyDiscoveryRequirements } from "../src/discovery/review-requirements.ts";
import { assessDiscoveryProfile } from "../src/discovery/qualification.ts";
import { validateDiscoveryQueryAudit } from "../src/discovery/query-audit.ts";

test("真实已存作者材料的额外要求绑定索引、原生身份及引用，匹配分不能覆盖缺证据", () => {
  const db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), { readOnly: true });
  try {
    const hit = db.prepare("SELECT id FROM discovery_search_hits ORDER BY id LIMIT 1").get()!.id as string;
    db.exec("BEGIN DEFERRED"); const basis = loadDiscoveryReviewBasis(db, "merge-review", 1, hit); db.exec("COMMIT");
    const original = buildDiscoveryReviewMaterial(basis), quote = original.video_description!;
    const requirement = { description: "核实原任务中的内容条件", excerpt: original.video_description!.slice(0, 20) };
    const material = { ...original, criteria: { requires_ai_tool_demo: false, requires_public_tool_url: false, requirements: [requirement], contact_preference: "public_email_first" } };
    const evidence = parseApplicationJson(basis.evidence_json) as Record<string, unknown>;
    const author = material.native_author_id;
    evidence.source_urls = [material.hit.source_url];
    evidence.reviewed_match = { verified: true, policy_version: "initial-match-evaluation-v1", native_author_id: author, review_id: "contract-review", reason: quote, source_url: material.hit.source_url, outcome: "supported", level: "good" };
    assert.equal(assessDiscoveryProfile(material.criteria, evidence, author, 6000).verdict, "pending");
    const finding = { criterion: { requirement: 0 }, outcome: "supported" as const, rationale: quote, citations: [{ kind: "description" as const, quote }] };
    applyDiscoveryRequirements(material, [finding], evidence, "owner-requirement-review", 1);
    assert.equal(assessDiscoveryProfile(material.criteria, evidence, author, 6000).verdict, "qualified");
    applyDiscoveryRequirements(material, [{ ...finding, outcome: "contradicted" }], evidence, "owner-requirement-review", 1);
    assert.equal(assessDiscoveryProfile(material.criteria, evidence, author, 6000).verdict, "rejected");
    assert.throws(() => applyDiscoveryRequirements(material, [], evidence, "review", 1), /incomplete/);
    assert.throws(() => applyDiscoveryRequirements(material, [{ ...finding, criterion: { requirement: 1 } }], evidence, "review", 1), /mismatch/);
    assert.throws(() => applyDiscoveryRequirements(material, [{ ...finding, citations: [] }], evidence, "review", 1), /citation_invalid/);
    assert.equal(stringifyApplicationJson(original.criteria).includes("核实原任务"), false);
  } finally { db.close(); }
});

test("查询审查逐项对应当前平台、词序和全部条件", () => {
  const scopes = [{ platform: "pinterest", keywords: ["makeup artist"] }];
  const audit = { requirements_preserved: true, requirement_reason: "保留客户条件", queries: [{ platform: "pinterest", keyword: "makeup artist", suitable: true, reason: "账号搜索使用领域和角色词" }] };
  validateDiscoveryQueryAudit(audit, scopes);
  assert.throws(() => validateDiscoveryQueryAudit({ ...audit, requirements_preserved: false }, scopes), /intent_invalid/);
  assert.throws(() => validateDiscoveryQueryAudit({ ...audit, queries: [] }, scopes), /scope_invalid/);
  assert.throws(() => validateDiscoveryQueryAudit({ ...audit, queries: [{ ...audit.queries[0], keyword: "makeup tutorial" }] }, scopes), /scope_invalid/);
});
