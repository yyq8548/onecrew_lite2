import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { z } from "zod";
import { advancedXSearchInput, compileAdvancedXQuery, youtubeSearchInput } from "../src/discovery/native-search-inputs.ts";
import { discoveryPlanInput, appendDiscoveryQueries, assertRegisteredDiscoveryScopes } from "../src/discovery/contracts.ts";
import { validateDiscoveryIntent, numbersInDiscoveryExcerpt } from "../src/discovery/intent.ts";
import { canAdvanceSearchAfterStop, discoveryReadPlatformStatus, nextDiscoverySearchKeyword } from "../src/discovery/stops.ts";
import { applySearchPlanPatch, searchPlanEditorDraft, searchScopeApprovalToken } from "../src/discovery/search-plan-contracts.ts";

test("发现需求保留未指定条件，拒绝改写粉丝，保留有原文的额外条件", () => {
  const request = "找鞋子创作者，至少1万粉丝，语言不限，所在地不限";
  const proposal = { original_request: request, goal: "寻找鞋类创作者", category: { state: "specified", value: "鞋子", excerpt: "鞋子" }, region: { state: "unspecified" }, language: { state: "unrestricted", excerpt: "语言不限" }, min_subscribers: { state: "specified", value: 10000, excerpt: "至少1万粉丝" }, excluded_platforms: [], other_hard_conditions: [], soft_preferences: [], suggested_keywords: ["鞋子测评", "sneaker review"] };
  assert.deepEqual(validateDiscoveryIntent(request, proposal, ["youtube"]), []);
  const changed = { ...proposal, min_subscribers: { state: "specified", value: 5000, excerpt: "至少1万粉丝" }, region: { state: "specified", value: "美国", excerpt: "美国" } };
  assert.deepEqual(validateDiscoveryIntent(request, changed, ["youtube"]), [{ field: "region", code: "excerpt_not_in_request" }, { field: "min_subscribers", code: "number_not_in_excerpt" }]);
  assert.deepEqual(numbersInDiscoveryExcerpt("1万 5千 2K 1,000 18446744073709551616"), [10000n, 5000n, 2000n, 1000n]);
  assert.deepEqual(validateDiscoveryIntent(request, { ...proposal, other_hard_conditions: [{ description: "仅鞋子", excerpt: "鞋子" }] }, ["youtube"]), []);
  assert.equal(validateDiscoveryIntent(request, { ...proposal, other_hard_conditions: [{ description: "仅美国", excerpt: "美国" }] }, ["youtube"])[0]?.code, "invalid_condition");
});

test("原计划约束保留Unicode字符与字节边界，查询扩充仅追加原范围且不改criteria", () => {
  const scope = { platform: "youtube", profile_ref: "work", keywords: ["鞋".repeat(160)] };
  const plan = discoveryPlanInput.parse({ id: "plan", brief: "鞋".repeat(4000), opencli_version: "1.8.8", criteria: { min_subscribers: 5000, language: "English" }, scopes: [scope], target_count: 2 });
  assert.equal(plan.brief.length, 4000); assertRegisteredDiscoveryScopes(plan.scopes);
  assert.throws(() => assertRegisteredDiscoveryScopes([{ ...scope, platform: "hackernews" }]));
  assert.throws(() => discoveryPlanInput.parse({ ...plan, criteria: { min_subscribers: 1, min_followers: 1 } }));
  const expected = [{ ...scope, keywords: ["headphone review"] }];
  assert.deepEqual(appendDiscoveryQueries(expected, [{ ...scope, keywords: ["headphones comfort test"] }])[0]?.keywords, ["headphone review", "headphones comfort test"]);
  assert.throws(() => appendDiscoveryQueries(expected, [{ ...scope, keywords: [" HEADPHONE REVIEW "] }]));
  assert.throws(() => appendDiscoveryQueries(expected, [{ ...scope, keywords: ["鞋".repeat(54)] }]));
  assert.deepEqual(expected[0]?.keywords, ["headphone review"]);
});

test("搜索超时换新关键词，三次失败停止平台，中断和风控保留停止边界", () => {
  const run = { status: "technical_stop", stop_code: "opencli_command_failed", active_query: "first", calls_used: 1 };
  assert.equal(nextDiscoverySearchKeyword(["first", "second"], run, new Set()), "second");
  assert.equal(canAdvanceSearchAfterStop("interrupted_read"), false);
  assert.equal(canAdvanceSearchAfterStop("opencli_timeout"), true);
  assert.equal(canAdvanceSearchAfterStop("opencli_risk_control"), false);
  assert.equal(canAdvanceSearchAfterStop("opencli_auth_required"), false);
  assert.equal(nextDiscoverySearchKeyword(["first", "second"], { ...run, cursor: JSON.stringify({ last_query: "first", consecutive_failures: 3 }) }, new Set()), null);
  assert.equal(discoveryReadPlatformStatus("opencli_timeout"), "completed");
  assert.equal(discoveryReadPlatformStatus("opencli_risk_control"), "risk_control_stop");
  assert.equal(discoveryReadPlatformStatus("opencli_auth_required"), "unauthorized");
  assert.equal(discoveryReadPlatformStatus("interrupted_read"), "technical_stop");
  assert.equal(nextDiscoverySearchKeyword(["first", "second"], { ...run, status: "completed", stop_code: null }, new Set(["first", "second"])), null);
});

test("原SearchPlan编辑仅改变验收描述与关键词，operation/revision/connection/arguments全部绑定确认", () => {
  const scope = { operation_id: "youtube.channels.search", schema_revision: "exact-revision", connection: { kind: "service_credential", toolkit: "youtube", credential_id: "selected" }, arguments: { query: "rust", count: 5 } };
  const draft = { description: "Find technical creators", keywords: ["rust"], scopes: [scope] };
  const patched = applySearchPlanPatch(draft, { description: "Review tutorials", keywords: ["tutorial"] });
  assert.deepEqual(patched.scopes, draft.scopes);
  assert.throws(() => applySearchPlanPatch(draft, { keywords: [" duplicate "] }));
  assert.throws(() => searchPlanEditorDraft.parse({ ...draft, scopes: [] }));
  assert.throws(() => searchPlanEditorDraft.parse({ ...draft, description: "鞋".repeat(2731) }));
  const token = searchScopeApprovalToken(scope);
  assert.equal(token, searchScopeApprovalToken({ ...scope, arguments: { count: 5, query: "rust" } }));
  for (const changed of [{ ...scope, operation_id: "other" }, { ...scope, schema_revision: "new" }, { ...scope, connection: { ...scope.connection, credential_id: "other" } }, { ...scope, arguments: { query: "new", count: 5 } }]) assert.notEqual(token, searchScopeApprovalToken(changed));
});

test("真实Rust oracle的数字键、非BMP键和u64 scope token逐项相同", () => {
  const cases = z.array(z.object({ scope: z.unknown(), token: z.string() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-search-plan-rust.json", import.meta.url))));
  assert.equal(cases.length, 2);
  for (const item of cases) assert.equal(searchScopeApprovalToken(item.scope), item.token);
});

test("原X高级查询严格编译selector，YouTube规范日期和计数在提供方前验证", () => {
  const input = advancedXSearchInput.parse({ all_words: ["headphone review"], any_words: ["Sony", "Bose"], from_accounts: ["@creator"], language: "en", min_likes: 5, since: "2026-01-01", result_mode: "latest" });
  assert.equal(compileAdvancedXQuery(input), '"headphone review" (Sony OR Bose) from:creator lang:en min_faves:5 since:2026-01-01');
  assert.throws(() => compileAdvancedXQuery(advancedXSearchInput.parse({ all_words: ["OR"] })));
  assert.throws(() => compileAdvancedXQuery(advancedXSearchInput.parse({ all_words: ["from:other"] })));
  assert.throws(() => compileAdvancedXQuery(advancedXSearchInput.parse({ all_words: ["word"], since: "2026-02-30" })));
  assert.throws(() => compileAdvancedXQuery(advancedXSearchInput.parse({ exclude_words: ["word"] })));
  assert.equal(youtubeSearchInput.parse({ query: "rust" }).count, 20);
  assert.equal(youtubeSearchInput.parse({ query: "rust", published_after: "2026-01-01T00:00:60Z" }).published_after, "2026-01-01T00:00:60Z");
  assert.throws(() => youtubeSearchInput.parse({ query: "rust", published_after: "2026-02-30T00:00:00Z" }));
  assert.throws(() => youtubeSearchInput.parse({ query: "rust", published_after: "2026-01-01T00:00:00+00:00" }));
});
