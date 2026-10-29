import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { decodeFlatSemanticPlan, flatSemanticPlan, structuredSemanticSchema, loadOriginalDiscoveryQueryGuidance, loadBundledDiscoveryQueryGuidance, discoveryPlanningPrompt } from "../src/discovery/semantic-planner.ts";
import { loadBundledDiscoveryMatchRubric } from "../src/discovery/semantic-match.ts";
import { buildDiscoveryDraft } from "../src/discovery/draft.ts";
import { discoveryCriteria } from "../src/discovery/contracts.ts";

test("原flat语义状态完整保留，矛盾null值明确失败，schema仅供受限结构输出", () => {
  const unspecified = { state: "unspecified", value: null, excerpt: null };
  const raw = { target_count: unspecified, intent: { original_request: "苹果手机", goal: "iPhone creators", category: { state: "specified", value: "iPhone", excerpt: "苹果手机" }, region: unspecified, language: unspecified, min_subscribers: unspecified, excluded_platforms: [], other_hard_conditions: [], soft_preferences: [], suggested_keywords: ["iPhone review"] }, scopes: [{ platform: "youtube", keywords: ["iPhone review"] }], contact_search_terms: ["email", "contact", "official website"] };
  const plan = decodeFlatSemanticPlan(raw, "iPhone"); assert.equal(plan.table_subject, "iPhone"); assert.deepEqual(plan.intent.region, { state: "unspecified" });
  assert.throws(() => decodeFlatSemanticPlan({ ...raw, target_count: { state: "unspecified", value: null, excerpt: "苹果手机" } }, "iPhone"));
  const schema = structuredSemanticSchema(flatSemanticPlan);
  assert.equal(schema.additionalProperties, false); assert.ok(Array.isArray(schema.required));
  assert.ok(JSON.stringify(schema).includes("uint64"));
  assert.equal(plan.intent.occupational_review_applicability, "unknown");
  const sources = [{ platform: "youtube", registered: true, allowed_values: [] }];
  for (const applicability of ["required", "not_required", "unknown"]) {
    const current = decodeFlatSemanticPlan({ ...raw, intent: { ...raw.intent, occupational_review_applicability: applicability } }, "iPhone");
    const draft = buildDiscoveryDraft({ id: "occupational", brief: "苹果手机", profile_ref: "work", opencli_version: "1.8.8" }, current, sources);
    assert.equal(draft.criteria.occupational_review_applicability, applicability);
  }
  const refined = buildDiscoveryDraft({ id: "occupational-refined", brief: "苹果手机", profile_ref: "work", opencli_version: "1.8.8" }, plan, sources, { criteria: { occupational_review_applicability: "not_required" }, target_count: 30 });
  assert.equal(refined.criteria.occupational_review_applicability, "unknown");
  assert.equal(discoveryCriteria.safeParse({ occupational_review_applicability: "anything" }).success, false);
});

test("原完整查询指导资源通过固定hash，保留复合产品/全部来源/语言与原生查询语义", async () => {
  const guidance = await loadOriginalDiscoveryQueryGuidance(fileURLToPath(new URL("../src/discovery/assets/search-query-guidance.txt", import.meta.url)));
  assert.equal(await loadBundledDiscoveryQueryGuidance(), guidance);
  assert.ok((await loadBundledDiscoveryMatchRubric()).includes("at least THREE distinct substantive original comments"));
  assert.ok(guidance.includes("苹果手机 means Apple iPhone")); assert.ok(guidance.includes("Query generation never authorizes reads"));
  const prompt = discoveryPlanningPrompt("苹果手机\nSearch only these entries: youtube", [{ platform: "youtube", registered: true, allowed_values: [] }, { platform: "tiktok", registered: true, allowed_values: [] }], guidance, [], { product_or_topic: "iPhone", purpose: "promotion", desired_creators: "reviewers", explicit_requirements_with_quotes: [], relevant_content_angles: ["iPhone review"], misleading_matches_to_avoid: ["fruit"] });
  assert.ok(prompt.includes('"explicitly_excluded_entries":["tiktok"]')); assert.ok(prompt.toLowerCase().includes("do not invent a model")); assert.ok(prompt.includes("Manual free-text scope terms"));
});
