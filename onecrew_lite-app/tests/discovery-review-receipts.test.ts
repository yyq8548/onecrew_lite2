import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeDiscoveryLanguageSample } from "../src/discovery/review-receipts.ts";

test("原语言样本v1只在内存丢弃评论身份，v2保持当前合同", () => {
  const legacy = { author_id: "creator", source_url: "https://example.com/post", text: "原始内容", comments: [{ author_id: "commenter", source_url: "https://example.com/comment", text: "评论内容" }] };
  const stored = JSON.stringify(legacy);
  const decoded = decodeDiscoveryLanguageSample(stored, 1);
  assert.deepEqual(decoded, { author_id: legacy.author_id, source_url: legacy.source_url, text: legacy.text, comments: [{ source_url: legacy.comments[0]!.source_url, text: legacy.comments[0]!.text }] });
  assert.equal(JSON.stringify(legacy), stored);
  assert.deepEqual(decodeDiscoveryLanguageSample(JSON.stringify(decoded), 2), decoded);
  assert.throws(() => decodeDiscoveryLanguageSample(JSON.stringify(decoded), 1), /language_sample_invalid/);
  assert.throws(() => decodeDiscoveryLanguageSample(stored, 3), /language_sample_format_unsupported/);
  assert.throws(() => decodeDiscoveryLanguageSample(stored, 2));
});
