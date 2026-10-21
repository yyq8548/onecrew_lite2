import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { codexStructuredArguments, SemanticEventTimings, type SemanticTimingPhase } from "../src/discovery/semantic-client.ts";
import { decodeFlatSemanticPlan, flatSemanticPlan, structuredSemanticSchema, loadOriginalDiscoveryQueryGuidance, loadBundledDiscoveryQueryGuidance, discoveryPlanningPrompt } from "../src/discovery/semantic-planner.ts";
import { loadBundledDiscoveryMatchRubric } from "../src/discovery/semantic-match.ts";

test("原flat语义状态完整保留，矛盾null值明确失败，schema仅供受限结构输出", () => {
  const unspecified = { state: "unspecified", value: null, excerpt: null };
  const raw = { target_count: unspecified, max_calls: unspecified, intent: { original_request: "苹果手机", goal: "iPhone creators", category: { state: "specified", value: "iPhone", excerpt: "苹果手机" }, region: unspecified, language: unspecified, min_subscribers: unspecified, excluded_platforms: [], other_hard_conditions: [], soft_preferences: [], suggested_keywords: ["iPhone review"] }, scopes: [{ platform: "youtube", keywords: ["iPhone review"] }], contact_search_terms: ["email", "contact", "official website"] };
  const plan = decodeFlatSemanticPlan(raw, "iPhone"); assert.equal(plan.table_subject, "iPhone"); assert.deepEqual(plan.intent.region, { state: "unspecified" });
  assert.throws(() => decodeFlatSemanticPlan({ ...raw, target_count: { state: "unspecified", value: null, excerpt: "苹果手机" } }, "iPhone"));
  const schema = structuredSemanticSchema(flatSemanticPlan);
  assert.equal(schema.additionalProperties, false); assert.ok(Array.isArray(schema.required));
  assert.ok(JSON.stringify(schema).includes("uint64"));
});

test("原完整查询指导资源通过固定hash，保留复合产品/全部来源/语言与原生查询语义", async () => {
  const guidance = await loadOriginalDiscoveryQueryGuidance(fileURLToPath(new URL("../src/discovery/assets/search-query-guidance.txt", import.meta.url)));
  assert.equal(await loadBundledDiscoveryQueryGuidance(), guidance);
  assert.ok((await loadBundledDiscoveryMatchRubric()).includes("at least THREE distinct substantive original comments"));
  assert.ok(guidance.includes("苹果手机 means Apple iPhone")); assert.ok(guidance.includes("Query generation never authorizes reads"));
  const prompt = discoveryPlanningPrompt("苹果手机\nSearch only these entries: youtube", [{ platform: "youtube", registered: true, allowed_values: [] }, { platform: "tiktok", registered: true, allowed_values: [] }], guidance, [], { product_or_topic: "iPhone", purpose: "promotion", desired_creators: "reviewers", explicit_requirements_with_quotes: [], relevant_content_angles: ["iPhone review"], misleading_matches_to_avoid: ["fruit"] });
  assert.ok(prompt.includes('"explicitly_excluded_entries":["tiktok"]')); assert.ok(prompt.toLowerCase().includes("do not invent a model")); assert.ok(prompt.includes("Manual free-text scope terms"));
});

test("实际Codex传输参数仅允许工具关闭的原结构任务，提示通过stdin而非shell", () => {
  const directory = fileURLToPath(new URL("../../.scratch/semantic-arguments/", import.meta.url));
  const args = codexStructuredArguments(directory, `${directory}/schema.json`, `${directory}/result.json`, true);
  assert.equal(args[0], "exec"); assert.equal(args.at(-1), "-");
  for (const feature of ["shell_tool", "unified_exec", "apps", "plugins", "browser_use", "computer_use", "multi_agent", "memories"]) assert.ok(args.some((value, index) => value === "--disable" && args[index + 1] === feature));
  assert.ok(args.includes("--ignore-user-config")); assert.ok(args.includes("--ignore-rules")); assert.ok(args.includes("read-only"));
  assert.ok(args.includes("--json")); assert.ok(args.includes("--ephemeral"));
});

test("模型事件只投影阶段耗时，缺少、重复或乱序事件不能报告成功", () => {
  const timings: Array<{ phase: SemanticTimingPhase; elapsed: number }> = [];
  const events = new SemanticEventTimings(performance.now(), (phase, elapsed) => timings.push({ phase, elapsed }));
  events.accept({ type: "thread.started" });
  events.accept({ type: "turn.started" });
  events.accept({ type: "turn.completed" }); events.finish(0);
  assert.deepEqual(timings.map(item => item.phase), ["until_thread_started", "until_turn_started", "turn"]);
  assert.ok(timings.every(item => Number.isSafeInteger(item.elapsed) && item.elapsed >= 0));
  assert.throws(() => events.accept({ type: "turn.completed" }), /semantic_invalid_output/);
  assert.throws(() => new SemanticEventTimings(performance.now()).accept({ type: "turn.started" }), /semantic_invalid_output/);
  assert.throws(() => new SemanticEventTimings(performance.now()).finish(0), /semantic_invalid_output/);
  const unavailable = new SemanticEventTimings(performance.now());
  unavailable.accept({ type: "error", message: "401 unauthorized" });
  assert.throws(() => unavailable.finish(1), /semantic_login_required/);
});
