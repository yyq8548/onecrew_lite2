import assert from "node:assert/strict";
import { test } from "node:test";
import { creatorAuthProbe, decodeCreatorAuthStatus } from "../src/platforms/opencli/auth-status.ts";

test("登录观察使用原生whoami，cookie观察不提升为已验证登录", () => {
  assert.deepEqual(creatorAuthProbe("twitter"), { site: "onecrew-twitter-auth", full: true, addon: true });
  assert.deepEqual(creatorAuthProbe("pinterest"), { site: "onecrew-pinterest-auth", full: true, addon: true });
  assert.deepEqual(creatorAuthProbe("youtube"), { site: "onecrew-youtube-auth", full: true, addon: true });
  assert.deepEqual(creatorAuthProbe("reddit"), { site: "reddit", full: true, addon: false });
  assert.throws(() => creatorAuthProbe("zhihu"));
  const site = "onecrew-twitter-auth";
  assert.equal(decodeCreatorAuthStatus([{ site, status: "logged_in", checked: "quick", identity: "private" }], site).status, "session_detected");
  assert.deepEqual(decodeCreatorAuthStatus([{ site, status: "error", checked: "full", error: "COMMAND_EXEC: Current-account identity could not be verified" }], site), { status: "unknown", method: "full", issue: "auth_signal_unavailable" });
  const failure = decodeCreatorAuthStatus([{ site, status: "error", checked: "full", error: "private token HTTP 429" }], site);
  assert.equal(failure.issue, "auth_probe_rate_limited"); assert.equal(JSON.stringify(failure).includes("private"), false);
  assert.throws(() => decodeCreatorAuthStatus([{ site: "other", status: "logged_in", checked: "full" }], site));
});
