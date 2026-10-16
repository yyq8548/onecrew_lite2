import assert from "node:assert/strict";
import { test } from "node:test";
import { discoveryOperationStopped, loadDiscoveryReadStops } from "../src/discovery/read-health.ts";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

test("原读取健康只累计连续同类解析错误的三个不同作者，不把超时或单个作者当来源故障", () => {
  const invalid = (author: string) => ({ author, code: "opencli_invalid_output" });
  assert.equal(discoveryOperationStopped([invalid("a"), invalid("b"), invalid("c")]), true);
  assert.equal(discoveryOperationStopped([invalid("a"), invalid("a"), invalid("b")]), false);
  assert.equal(discoveryOperationStopped([invalid("a"), { author: "b", code: null }, invalid("c")]), false);
  assert.equal(discoveryOperationStopped([invalid("a"), { author: "b", code: "opencli_output_too_large" }, invalid("c")]), false);
  assert.equal(discoveryOperationStopped([{ author: "a", code: "opencli_timeout" }, invalid("b"), invalid("c")]), false);
  assert.equal(discoveryOperationStopped([{ author: "a", code: "opencli_auth_required" }]), true);
  assert.equal(discoveryOperationStopped([{ author: "a", code: "opencli_risk_control" }]), true);
  const db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), { readOnly: true });
  try { assert.deepEqual(loadDiscoveryReadStops(db, "merge-review", "profile"), []); assert.deepEqual(loadDiscoveryReadStops(db, "merge-review", "content"), []); }
  finally { db.close(); }
});
