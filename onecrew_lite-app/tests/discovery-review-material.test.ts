import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { buildDiscoveryReviewMaterial, containsDiscoveryCitation } from "../src/discovery/review-material.ts";

test("原Rust真实review仓储材料与transcript/page收据的全部字段、hash和引用匹配", () => {
  const cases = z.array(z.object({ basis: z.unknown(), expected: z.unknown() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-review-rust.json", import.meta.url))));
  assert.equal(cases.length, 2);
  for (const value of cases) assert.deepEqual(parseApplicationJson(Buffer.from(stringifyApplicationJson(buildDiscoveryReviewMaterial(value.basis)))), value.expected);
  const material = buildDiscoveryReviewMaterial(cases[1]!.basis);
  assert.equal(containsDiscoveryCitation(material, { kind: "transcript", segment_index: 0, quote: "I type a prompt." }), true);
  assert.equal(containsDiscoveryCitation(material, { kind: "transcript", segment_index: 0, quote: "invented text" }), false);
  assert.equal(containsDiscoveryCitation(material, { kind: "public_page", requested_url: "https://example.com/tool", quote: "AI editing software." }), true);
  assert.equal(containsDiscoveryCitation(material, { kind: "public_page", requested_url: "https://example.com/other", quote: "AI editing software." }), false);
});
