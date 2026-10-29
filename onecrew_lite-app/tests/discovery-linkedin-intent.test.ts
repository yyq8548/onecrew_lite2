import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { buildDiscoveryDraft, prepareDiscoveryDraftInput } from "../src/discovery/draft.ts";
import { decodeFlatSemanticPlan } from "../src/discovery/semantic-planner.ts";
import { discoveryCriteriaForPlatform } from "../src/discovery/qualification.ts";

const source = z.object({ platform: z.string(), registered: z.boolean(), allowed_values: z.array(z.string()) });
const compact = z.object({ understanding: z.object({ product_or_topic: z.string() }), plan: z.object({ intent: z.object({ exclusion_groups: z.array(z.object({ platforms: z.array(z.string()), excerpt: z.string() })), min_subscribers_exclusive: z.boolean(), min_subscribers_in_requirements: z.boolean().default(false) }).passthrough() }).passthrough() });
const captured = z.object({ input: prepareDiscoveryDraftInput.strip(), sources: z.array(source), trace: z.array(z.unknown()) }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-intent-baseline-20261020.json", import.meta.url))));
function proposal(raw: unknown) {
  const actual = compact.parse(raw);
  const { exclusion_groups, min_subscribers_exclusive, min_subscribers_in_requirements, ...intent } = actual.plan.intent;
  const { max_calls: retiredLimit, ...current } = actual.plan;
  assert.ok(retiredLimit === undefined || typeof retiredLimit === "object");
  return { ...decodeFlatSemanticPlan({ ...current, intent: { ...intent, excluded_platforms: exclusion_groups.flatMap(group => group.platforms.map(platform => ({ platform, excerpt: group.excerpt }))) } }, actual.understanding.product_or_topic), min_subscribers_exclusive, min_subscribers_in_requirements };
}

test("真实中文目标摘要按字符计长，保留公司岗位逻辑和明确不限设置", () => {
  const actual = compact.parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-natural-goal-20261020.json", import.meta.url))));
  const plan = proposal(actual);
  assert.ok(Buffer.byteLength(plan.intent.goal) > 400);
  assert.ok([...plan.intent.goal].length <= 400);
  const input = prepareDiscoveryDraftInput.parse({ id: "linkedin-natural-goal", brief: plan.intent.original_request, profile_ref: "work", opencli_version: "1.8.8" });
  const draft = buildDiscoveryDraft(input, plan, captured.sources);
  assert.equal(draft.target_count, 30);
  assert.equal(draft.criteria.min_subscribers, 0);
  assert.equal(draft.criteria.quality_filter, null);
  assert.deepEqual(draft.scopes.map(scope => scope.platform), ["linkedin"]);
  assert.deepEqual(draft.criteria.requirements, plan.intent.other_hard_conditions);
  assert.match(plan.intent.other_hard_conditions[0]!.description, /组内分别为 OR，组间为 AND/u);
  assert.throws(() => buildDiscoveryDraft(input, { ...plan, intent: { ...plan.intent, goal: "a".repeat(401) } }, captured.sources), /discovery_draft_intent_invalid/u);
});

test("真实自然语言规划未要求粉丝时，LinkedIn新草稿不附加2000门槛", () => {
  const draft = buildDiscoveryDraft(captured.input, proposal(captured.trace[0]), captured.sources);
  assert.equal(draft.criteria.min_subscribers, 0);
  assert.equal(draft.criteria.min_subscribers_source, "default");
});

const cases = z.array(z.object({ index: z.number(), input: prepareDiscoveryDraftInput.strip(), sources: z.array(source), trace: z.array(z.unknown()), draft: z.object({ criteria: z.record(z.string(), z.unknown()), target_count: z.number(), scopes: z.array(z.object({ platform: z.string() })) }) })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-intent-results-20261020.json", import.meta.url))));
for (const actual of cases) test(`真实自然语言规划与独立审查回归${actual.index}：${actual.input.brief}`, () => {
  const draft = buildDiscoveryDraft(actual.input, proposal(actual.trace[0]), actual.sources);
  const expectedCriteria: Record<string, unknown> = { ...actual.draft.criteria, quality_filter: null, occupational_review_applicability: "unknown" };
  assert.deepEqual(draft.criteria, expectedCriteria);
  assert.equal(draft.target_count, 10);
  assert.deepEqual(draft.scopes.map(scope => scope.platform), actual.draft.scopes.map(scope => scope.platform));
  if ([0, 2, 3, 6].includes(actual.index)) assert.equal(draft.criteria.min_subscribers, 0);
  if (actual.index === 1) assert.equal(draft.criteria.min_subscribers, 5000);
  if (actual.index === 3 || actual.index === 6) {
    assert.equal(draft.criteria.min_subscribers_source, "requirements");
    const groups = z.array(z.object({ description: z.string(), excerpt: z.string() })).parse(draft.criteria.requirements);
    assert.equal(groups.filter(group => group.excerpt.includes("followers") && group.excerpt.includes("connections")).length, 1);
  }
  if (actual.index === 4) assert.equal(draft.criteria.min_subscribers, 2000);
  if (actual.index === 5) {
    assert.equal(discoveryCriteriaForPlatform(draft.criteria, "linkedin").min_subscribers, 0);
    assert.equal(discoveryCriteriaForPlatform(draft.criteria, "youtube").min_subscribers, 2000);
    assert.equal(draft.criteria.min_subscribers, 2000);
  }
});

test("真实历史草稿缺少阈值来源时保留已冻结门槛，重新规划才使用LinkedIn默认值", () => {
  const historical = z.object({ draft: z.object({ criteria: z.record(z.string(), z.unknown()), target_count: z.number() }) }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-intent-baseline-20261020.json", import.meta.url)))).draft;
  assert.equal(discoveryCriteriaForPlatform(historical.criteria, "linkedin").min_subscribers, 2000);
  const inherited = buildDiscoveryDraft(captured.input, proposal(captured.trace[0]), captured.sources, historical);
  assert.equal(inherited.criteria.min_subscribers, 2000);
  assert.equal(inherited.criteria.min_subscribers_source, undefined);
});
