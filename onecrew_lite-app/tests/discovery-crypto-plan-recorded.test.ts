import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { decodeFlatSemanticPlan } from "../src/discovery/semantic-planner.ts";
import { buildDiscoveryDraft, prepareDiscoveryDraftInput } from "../src/discovery/draft.ts";

const path = process.env.ONECREW_PLAN_RECORDING;
test("真实加密货币需求和模型返回生成四平台20人草稿，保持未批准和原粉丝门槛", { skip: !path }, () => {
  const saved = parseApplicationJson(readFileSync(path!)) as { tasks: { id: string; prompt: string; result: unknown }[] };
  const task = saved.tasks.find(task => task.id === "5f80663471e3caffaf37ed0c5ff7f2faca2d4b9a3c7336928d064459bbd6631f")!;
  assert.ok(task);
  const request = JSON.parse(task.prompt.split("\n").find(line => line.startsWith('{"customer_request":'))!) as { customer_request: string; registered_sources: { platform: string; registered: boolean; command: string; allowed_values: string[] }[] };
  const result = task.result as { understanding: { product_or_topic: string }; plan: { intent: { exclusion_groups: { platforms: string[]; excerpt: string }[]; min_subscribers_exclusive: boolean; min_subscribers_in_requirements: boolean }; [key: string]: unknown } };
  const { exclusion_groups, min_subscribers_exclusive, min_subscribers_in_requirements, ...intent } = result.plan.intent;
  const plan = { ...decodeFlatSemanticPlan({ ...result.plan, intent: { ...intent, excluded_platforms: exclusion_groups.flatMap(group => group.platforms.map(platform => ({ platform, excerpt: group.excerpt }))) } }, result.understanding.product_or_topic), min_subscribers_exclusive, min_subscribers_in_requirements };
  const input = prepareDiscoveryDraftInput.parse({ id: "b3d44eff-a06b-4aae-b75b-000d2584382d", brief: request.customer_request, profile_ref: "d5pskwub", opencli_version: "1.8.8" });
  const draft = buildDiscoveryDraft(input, plan, request.registered_sources);
  assert.equal(draft.target_count, 20);
  assert.equal(draft.criteria.min_subscribers, 2000);
  assert.equal(draft.criteria.quality_filter, null);
  assert.deepEqual(draft.scopes.map(scope => scope.platform).sort(), ["instagram", "tiktok", "twitter", "youtube"]);
  assert.throws(() => buildDiscoveryDraft(input, { ...plan, scopes: plan.scopes.map(scope => ({ ...scope, keywords: [] })) }, request.registered_sources), /scope_invalid/);
  assert.throws(() => buildDiscoveryDraft(input, { ...plan, intent: { ...plan.intent, min_subscribers: { state: "specified", value: 2001, excerpt: "粉丝不少于2000" } } }, request.registered_sources), /intent_invalid/);
  assert.throws(() => buildDiscoveryDraft(input, { ...plan, intent: { ...plan.intent, suggested_keywords: [" "] } }, request.registered_sources), /intent_invalid/);
});
