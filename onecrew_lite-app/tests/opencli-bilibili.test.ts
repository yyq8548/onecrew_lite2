import assert from "node:assert/strict";
import { test } from "node:test";
import { BILIBILI_VIDEO_FIELDS, bilibiliVideoPlan } from "../src/platforms/opencli/bilibili-video.ts";

test("Bilibili原生视频合同验证完整字段、MID与付费状态，描述按字节保留", () => {
  const fields: Record<string, string> = { bvid: "BV1xx411c7mD", author: "Example creator (mid: 12345)", title: "公开教程", description: "中".repeat(3000), requires_payment: "false", pay_preview: "false" };
  const rows = BILIBILI_VIDEO_FIELDS.map(field => ({ field, value: fields[field] ?? "" }));
  const source = "https://www.bilibili.com/video/BV1xx411c7mD";
  const result = bilibiliVideoPlan(source, "12345").decode(rows);
  assert.equal(result.description, "中".repeat(2730)); assert.equal(result.description_truncated, true);
  assert.equal(result.payment_status_known, true); assert.equal(result.author_id, "12345");
  assert.equal(bilibiliVideoPlan(source).decode(rows).author_id, "12345");
  assert.throws(() => bilibiliVideoPlan(source, "12346").decode(rows));
  assert.throws(() => bilibiliVideoPlan(source + "/"));
  assert.throws(() => bilibiliVideoPlan(source).decode(rows.slice(1)));
  assert.throws(() => bilibiliVideoPlan(source).decode(rows.map(row => row.field === "payment_type" ? { ...row, value: "vip" } : row)));
});
