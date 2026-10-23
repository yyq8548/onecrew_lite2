import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { assessDiscoveryProfile, assessDiscoveryYoutube, assessedDiscoveryEvidence, mergePendingDiscoveryEvidence, parseDiscoveryDouyinFollowers, parseDiscoveryFacebookFollowers, assessDiscoveryFacebook } from "../src/discovery/qualification.ts";

test("Facebook实际289K粉丝显示保留估计区间，门槛附近保持未知", () => {
  const raw = "289K followers";
  assert.deepEqual(parseDiscoveryFacebookFollowers(raw), { display: raw, nominal: 289000n, lower: 288000n, upper: 290000n });
  const criteria = { requires_ai_tool_demo: false, requires_public_tool_url: false };
  assert.equal(assessDiscoveryFacebook({ ...criteria, min_subscribers: 2000 }, {}, "61566847065209", raw).verdict, "qualified");
  assert.deepEqual(assessDiscoveryFacebook({ ...criteria, min_subscribers: 289000 }, {}, "61566847065209", raw).reasons, ["subscriber_count_near_threshold"]);
  assert.equal(assessDiscoveryFacebook({ ...criteria, min_subscribers: 300000 }, {}, "61566847065209", raw).verdict, "rejected");
  for (const invalid of ["289K likes", "289K", "about 289K followers", "2,000 followers", "0.2K followers", "289K followers 10 following"]) assert.equal(parseDiscoveryFacebookFollowers(invalid), null);
});

test("35个原Rust资格运算case完整匹配，包括未知/阈值/语言/舍入与u64最大值", () => {
  const cases = z.array(z.object({ mode: z.enum(["profile", "youtube"]), criteria: z.unknown(), evidence: z.unknown(), author: z.string(), count: z.union([z.number(), z.bigint()]).nullable(), expected: z.unknown(), assessed_evidence: z.unknown() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-qualification-rust.json", import.meta.url))));
  assert.equal(cases.length, 35);
  for (const value of cases) {
    const assessment = value.mode === "profile" ? assessDiscoveryProfile(value.criteria, value.evidence, value.author, value.count) : assessDiscoveryYoutube(value.criteria, value.evidence, value.author);
    assert.deepEqual(parseApplicationJson(Buffer.from(stringifyApplicationJson(assessment))), value.expected);
    assert.deepEqual(parseApplicationJson(Buffer.from(stringifyApplicationJson(assessedDiscoveryEvidence(value.evidence, assessment)))), value.assessed_evidence);
  }
  assert.deepEqual(parseDiscoveryDouyinFollowers("1.2万"), { display: "1.2万", nominal: 12000n, lower: 11000n, upper: 13000n });
  assert.equal(parseDiscoveryDouyinFollowers("01.2万"), null); assert.equal(parseDiscoveryDouyinFollowers("1.000001千"), null);
});

test("待审核证据合并保留已观察来源，不能继承审核证明或编造来源", () => {
  const current = { source_urls: Array.from({ length: 20 }, (_, index) => `https://current.example/${index}`), public_links: ["https://current.example/0"] };
  const prior = { source_urls: ["https://prior.example/count", "https://prior.example/demo"], public_links: ["https://prior.example/count"], subscribers_raw: "1万", subscribers_source_url: "https://prior.example/count", captured_at: 18446744073709551615n, own_ai_tool_demo: { source_url: "https://prior.example/demo", verified: true }, public_tool_url: { source_url: "https://invented.example/", verified: true }, reviewed_match: { verified: true }, reviewed_residence: { verified: true } };
  const merged = mergePendingDiscoveryEvidence(current, prior) as Record<string, unknown>;
  assert.deepEqual((merged.source_urls as string[]).slice(0, 3), [current.source_urls[0], prior.subscribers_source_url, prior.own_ai_tool_demo.source_url]);
  assert.equal((merged.source_urls as string[]).length, 20);
  assert.equal((merged.source_urls as string[]).includes(prior.public_tool_url.source_url), false);
  assert.equal(merged.captured_at, prior.captured_at);
  assert.equal(Object.hasOwn(merged, "reviewed_match"), false);
  assert.equal(Object.hasOwn(merged, "reviewed_residence"), false);
  assert.deepEqual(merged.public_links, [current.public_links[0], prior.public_links[0]]);
  assert.equal(current.source_urls.length, 20);
  assert.equal(Object.hasOwn(current, "subscribers_raw"), false);
  assert.deepEqual(mergePendingDiscoveryEvidence({ own_ai_tool_demo: null, subscribers_raw: "2万" }, prior), { own_ai_tool_demo: null, subscribers_raw: "2万", public_tool_url: prior.public_tool_url });
  assert.equal(mergePendingDiscoveryEvidence(null, prior), null);
});
