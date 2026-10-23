import assert from "node:assert/strict";
import { test } from "node:test";
import { creatorAuthArguments, creatorAuthProbe, decodeCreatorAuthStatus } from "../src/platforms/opencli/auth-status.ts";

test("原生身份检测使用完整登录检查", () => {
  assert.deepEqual(creatorAuthArguments("twitter", "d5pskwub"), ["--profile", "d5pskwub", "auth", "status", "--site", "onecrew-twitter-auth", "--concurrency", "1", "--timeout", "20", "-f", "json", "--full"]);
});

test("已移除平台不能探测登录或构造登录检查命令", () => {
  for (const platform of ["reddit", "pinterest"]) {
    assert.throws(() => creatorAuthProbe(platform), { message: "opencli_read_command_unavailable" });
    assert.throws(() => creatorAuthArguments(platform, "d5pskwub"), { message: "opencli_read_command_unavailable" });
  }
});

test("登录观察使用原生whoami，cookie观察不提升为已验证登录", () => {
  assert.deepEqual(creatorAuthProbe("twitter"), { site: "onecrew-twitter-auth", full: true, addon: true });
  assert.deepEqual(creatorAuthProbe("youtube"), { site: "onecrew-youtube-auth", full: true, addon: true });
  assert.throws(() => creatorAuthProbe("zhihu"));
  const site = "onecrew-twitter-auth";
  assert.equal(decodeCreatorAuthStatus([{ site, status: "logged_in", checked: "quick", identity: "private" }], site).status, "session_detected");
  assert.deepEqual(decodeCreatorAuthStatus([{ site, status: "error", checked: "full", error: "COMMAND_EXEC: Current-account identity could not be verified" }], site), { status: "unknown", method: "full", issue: "auth_signal_unavailable" });
  const failure = decodeCreatorAuthStatus([{ site, status: "error", checked: "full", error: "private token HTTP 429" }], site);
  assert.equal(failure.issue, "auth_probe_rate_limited"); assert.equal(JSON.stringify(failure).includes("private"), false);
  assert.throws(() => decodeCreatorAuthStatus([{ site: "other", status: "logged_in", checked: "full" }], site));
});
