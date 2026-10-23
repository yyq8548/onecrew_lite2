import assert from "node:assert/strict";
import { test } from "node:test";
import { discoveryOperationStopped, loadDiscoveryReadStops } from "../src/discovery/read-health.ts";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

test("连续三个不同账号读取失败后跳过平台，成功清除连续失败，登录和风控立即停止", () => {
  const invalid = (author: string) => ({ author, code: "opencli_invalid_output" });
  assert.equal(discoveryOperationStopped([invalid("a"), invalid("b"), invalid("c")]), true);
  assert.equal(discoveryOperationStopped([invalid("a"), invalid("a"), invalid("b")]), false);
  assert.equal(discoveryOperationStopped([invalid("a"), { author: "b", code: null }, invalid("c")]), false);
  assert.equal(discoveryOperationStopped([invalid("a"), { author: "b", code: "opencli_output_too_large" }, invalid("c")]), true);
  assert.equal(discoveryOperationStopped([{ author: "a", code: "opencli_timeout" }, invalid("b"), invalid("c")]), true);
  assert.equal(discoveryOperationStopped([{ author: "a", code: "opencli_timeout" }, { author: "b", code: "opencli_timeout" }]), false);
  assert.equal(discoveryOperationStopped([{ author: "a", code: "opencli_auth_required" }]), true);
  assert.equal(discoveryOperationStopped([{ author: "a", code: "opencli_risk_control" }]), true);
  const db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), { readOnly: true });
  try { assert.deepEqual(loadDiscoveryReadStops(db, "merge-review", "profile"), []); assert.deepEqual(loadDiscoveryReadStops(db, "merge-review", "content"), []); }
  finally { db.close(); }
});
