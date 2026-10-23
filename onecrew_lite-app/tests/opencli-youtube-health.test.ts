import assert from "node:assert/strict";
import { test } from "node:test";
import { classifyYoutubeApiFailure, youtubeHealthOutcome, youtubeHttpFailure } from "../src/platforms/browser/youtube-health.ts";

test("YouTube原机器原因优先级及受限健康结果不持久化provider文本", async () => {
  const body = (reason: string) => ({ error: { errors: [{ reason }], message: "private provider text" } });
  assert.equal(classifyYoutubeApiFailure(429, body("keyInvalid")), "rate_limited");
  assert.equal(classifyYoutubeApiFailure(503, body("keyInvalid")), "transient");
  assert.equal(classifyYoutubeApiFailure(403, body("keyInvalid")), "api_key_invalid");
  assert.equal(classifyYoutubeApiFailure(403, body("accessNotConfigured")), "access_not_configured");
  assert.equal(classifyYoutubeApiFailure(403, body("quotaExceeded")), "quota_exceeded");
  assert.equal(classifyYoutubeApiFailure(403, body("userRateLimitExceeded")), "rate_limited");
  assert.equal(classifyYoutubeApiFailure(403, body("unrecognized")), "ambiguous_forbidden");
  assert.equal(classifyYoutubeApiFailure(401, null), "api_key_invalid");
  assert.equal(classifyYoutubeApiFailure(400, { error: { message: "API key not valid PRIVATE" } }), "api_key_invalid");
  assert.deepEqual(youtubeHealthOutcome("quota_exceeded", 100), { message: "YouTube API quota is exhausted", disabled: false, cooldownUntil: 1000 });
  assert.equal(youtubeHealthOutcome("api_key_invalid", 100).disabled, true);
  const error = await youtubeHttpFailure(403, JSON.stringify(body("unknown")));
  assert.equal(error.health, "ambiguous_forbidden"); assert.ok(!error.message.includes("private"));
  assert.equal((await youtubeHttpFailure(403, "not JSON")).health, "ambiguous_forbidden");
});
