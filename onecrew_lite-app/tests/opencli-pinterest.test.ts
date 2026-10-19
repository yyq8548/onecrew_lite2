import assert from "node:assert/strict";
import { test } from "node:test";
import { pinterestProfilePlan, pinterestContentPlan } from "../src/platforms/opencli/pinterest.ts";

test("Pinterest原生账户与可见链接合同保留未知粉丝，拒绝换属", () => {
  const plan = pinterestProfilePlan("123", "native_author");
  const row = { author_id: "123", username: "native_author", author_label: "Display Name", profile_url: "https://www.pinterest.com/native_author/", follower_count: null, bio: "Business: studio@example.org", public_links: ["https://studio.example.org/contact"] };
  assert.equal(plan.decode([row]).follower_count, null);
  assert.throws(() => plan.decode([{ ...row, author_id: "999" }]));
  assert.throws(() => plan.decode([{ ...row, public_links: ["https://127.0.0.1/"] }]));
  assert.throws(() => plan.decode([{ ...row, follower_count: undefined }]));
});
test("Pinterest图钉原caption固定归属且明确保留部分文本标记", () => {
  const row = { pin_id: "456", author_id: "123", author_username: "native_author", profile_url: "https://www.pinterest.com/native_author/", source_url: "https://www.pinterest.com/pin/456/", text: "Keyboard review", text_truncated: true };
  assert.equal(pinterestContentPlan("123", "native_author").decode([row]).pin_id, "456");
  const exact = pinterestContentPlan("123", "native_author", "456");
  assert.equal(exact.name, "content-pin");
  assert.equal(exact.decode([row]).text_truncated, true);
  assert.equal(exact.decode([{ ...row, text_truncated: false }]).text_truncated, false);
  assert.throws(() => pinterestContentPlan("123", "native_author", "789").decode([row]));
  assert.throws(() => exact.decode([{ ...row, session: "private" }]));
});
