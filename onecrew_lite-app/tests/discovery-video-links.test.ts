import { test } from "node:test";
import assert from "node:assert/strict";
import { discoveryDescriptionLinks } from "../src/discovery/video-execution.ts";

test("描述线索沿原HTTPS、完整链接、去重与十条上限规则", () => {
  assert.deepEqual(discoveryDescriptionLinks("Try (https://example.com/tool). Also http://unsafe.test https://example.com/tool https://example.com/tool... https://example.com/tool… https://user:secret@example.com/private"), ["https://example.com/tool"]);
  assert.equal(discoveryDescriptionLinks(Array.from({ length: 12 }, (_, index) => `https://example.com/${index}`).join(" ")).length, 10);
  assert.deepEqual(discoveryDescriptionLinks(`https://example.com/${"x".repeat(2048)}`), []);
});
