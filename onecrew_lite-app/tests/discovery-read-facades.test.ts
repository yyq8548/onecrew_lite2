import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { readWorkflowGuide, workflowGuideInput, workflowTopics } from "../src/service/workflow-guides.ts";
import { retiredOperations } from "../src/contracts/retired-operations.ts";
import { parseHeldLinkHub, parseLinkHubExtraction, linkHubUrl, LinkHubReadError } from "../src/platforms/candidates/link-hub.ts";

test("原Rust真实project导出的八份完整指南及目录逐字段读取，当前指南不泄露已退役操作", async () => {
  const bytes = readFileSync(new URL("../src/service/workflow-guides.json", import.meta.url));
  assert.equal(createHash("sha256").update(bytes).digest("hex"), "0e09ec33a50ed0022279252c921514ed3f3097343b78b7db22f59aca783a6a1a");
  const original = JSON.parse(bytes.toString("utf8")); assert.deepEqual(await readWorkflowGuide(null), original.directory);
  for (const topic of workflowTopics) { const actual = await readWorkflowGuide(topic); assert.deepEqual(actual, original[topic]); const encoded = JSON.stringify(actual); for (const retired of retiredOperations) assert.equal(encoded.includes(retired), false, retired); }
  assert.equal(workflowGuideInput.safeParse({ topic: "outreach" }).success, false); assert.equal(workflowGuideInput.safeParse({ topic: "find-one", send: true }).success, false);
});
test("原真实Linktree HTML保留三层来源，旧候选held合同逐字段兼容且非法来源零网络拒绝", () => {
  const html = readFileSync(new URL("./fixtures/public-pages/linktree_bitcoinmx.html", import.meta.url), "utf8"), source = "https://linktr.ee/bitcoinmx", extraction = parseLinkHubExtraction(html, source, source);
  assert.deepEqual(parseHeldLinkHub(html, source, source), { source_url: extraction.source_url, links: extraction.links.map(({ url, text }) => ({ url, text })), emails: extraction.emails.map(({ value }) => ({ value })) });
  assert.ok(extraction.links.length > 0); assert.deepEqual(new Set([...extraction.links, ...extraction.emails].map(item => item.layer)), new Set(["anchor", "json_ld", "hydration"]));
  assert.throws(() => linkHubUrl("https://untrusted.example/owner?private=secret"), error => { assert.ok(error instanceof LinkHubReadError); assert.equal(error.code, "link_hub_host_unsupported"); assert.equal(error.message.includes("secret"), false); return true; });
  assert.throws(() => linkHubUrl("https://user:password@linktr.ee/owner"), { code: "link_hub_url_invalid" });
});
