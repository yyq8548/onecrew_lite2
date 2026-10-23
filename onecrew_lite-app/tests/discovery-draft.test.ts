import assert from "node:assert/strict";
import { test } from "node:test";
import { buildDiscoveryDraft } from "../src/discovery/draft.ts";

test("草稿保留引用约束、目标人数和完整来源覆盖，始终允许自动补词", () => {
  const brief = "找鞋子创作者，至少1万粉丝，语言不限，所在地不限";
  const input = { id: "draft", brief, profile_ref: "work", opencli_version: "1.8.8" };
  const proposal = { table_subject: "鞋子", target_count: { state: "unspecified" }, intent: { original_request: brief, goal: "寻找鞋类创作者", category: { state: "specified", value: "鞋子", excerpt: "鞋子" }, region: { state: "unrestricted", excerpt: "所在地不限" }, language: { state: "unrestricted", excerpt: "语言不限" }, min_subscribers: { state: "specified", value: 10000, excerpt: "至少1万粉丝" }, excluded_platforms: [], other_hard_conditions: [], soft_preferences: [], suggested_keywords: ["鞋子测评", "sneaker review"] }, scopes: [{ platform: "youtube", keywords: ["鞋子测评"] }] };
  const sources = [{ platform: "youtube", registered: true, allowed_values: [] }];
  const draft = buildDiscoveryDraft(input, proposal, sources);
  assert.equal(draft.criteria.min_subscribers, 10000); assert.equal(draft.criteria.region, ""); assert.equal(draft.criteria.automatic_query_expansion, true); assert.equal(draft.criteria.table_subject, "鞋子"); assert.equal(draft.target_count, 30); assert.equal("max_calls" in draft, false);
  assert.throws(() => buildDiscoveryDraft(input, proposal, [...sources, { platform: "tiktok", registered: true, allowed_values: [] }]), /coverage_incomplete/);
  const conditioned = buildDiscoveryDraft(input, { ...proposal, intent: { ...proposal.intent, other_hard_conditions: [{ description: "仅鞋子", excerpt: "鞋子" }] } }, sources);
  assert.deepEqual(conditioned.criteria.requirements, [{ description: "仅鞋子", excerpt: "鞋子" }]);
  assert.equal(conditioned.criteria.contact_preference, "public_email_first");
  assert.throws(() => buildDiscoveryDraft(input, { ...proposal, intent: { ...proposal.intent, other_hard_conditions: [{ description: "未引用条件", excerpt: "不同的要求" }] } }, sources), /intent_invalid/);
  const explicit = buildDiscoveryDraft({ ...input, target_count: 5, scope_overrides: [{ platform: "youtube", keywords: ["鞋子实测"] }] }, proposal, sources);
  assert.equal(explicit.target_count, 5); assert.equal(explicit.criteria.automatic_query_expansion, true); assert.deepEqual(explicit.scopes[0]?.keywords, ["鞋子实测"]);
  assert.throws(() => buildDiscoveryDraft({ ...input, max_calls: 6 }, proposal, sources));
  assert.throws(() => buildDiscoveryDraft({ ...input, read_budget: { mode: "explicit", limit: 6 } }, proposal, sources));
  const inheritedProposal = { ...proposal, target_count: { state: "unspecified" }, intent: { ...proposal.intent, min_subscribers: { state: "unspecified" }, language: { state: "unspecified" }, category: { state: "unspecified" } }, scopes: [{ platform: "youtube", keywords: ["shoe review"] }] };
  const previous = { criteria: { language: "English", category: "shoes" }, target_count: 1 };
  const inherited = buildDiscoveryDraft(input, inheritedProposal, sources, previous);
  assert.equal(inherited.criteria.min_subscribers, 0); assert.equal(inherited.target_count, 1);
  assert.equal(inherited.criteria.language, "English"); assert.equal(inherited.criteria.category, "shoes");
  assert.equal(buildDiscoveryDraft(input, proposal, sources, previous).criteria.min_subscribers, 10000);
});

test("明确中文查询不能用仅英文词或未引用条件替代", () => {
  const brief = "找键盘作者\nContent language: 中文";
  const input = { id: "keyboard", brief, profile_ref: "work", opencli_version: "1.8.8", criteria_overrides: { language: "中文", requires_ai_tool_demo: false, requires_public_tool_url: false } };
  const plan = { target_count: { state: "unspecified" }, intent: { original_request: brief, goal: "找键盘作者", category: { state: "unspecified" }, region: { state: "unspecified" }, language: { state: "unspecified" }, min_subscribers: { state: "unspecified" }, excluded_platforms: [], other_hard_conditions: [{ description: "Content language", excerpt: "Content language: 中文" }], soft_preferences: [], suggested_keywords: ["键盘测评"] }, scopes: [{ platform: "youtube", keywords: ["keyboard review"] }] };
  const sources = [{ platform: "youtube", registered: true, allowed_values: [] }];
  assert.throws(() => buildDiscoveryDraft(input, plan, sources), /scope_invalid/);
  const result = buildDiscoveryDraft(input, { ...plan, scopes: [{ platform: "youtube", keywords: ["keyboard review", "键盘测评"] }] }, sources);
  assert.equal(result.criteria.language, "中文"); assert.deepEqual(result.scopes[0]?.keywords, ["键盘测评", "keyboard review"]);
});
