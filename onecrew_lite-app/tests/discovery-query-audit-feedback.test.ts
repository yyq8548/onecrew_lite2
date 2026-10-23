import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { validateDiscoveryQueryAudit } from "../src/discovery/query-audit.ts";
import { discoveryReadFailures } from "@onecrew/runtime-contracts/status";

const recorded = JSON.parse(await readFile(new URL("./fixtures/query-audit-recorded-20261022.json", import.meta.url), "utf8"));

test("真实审核记录的关键词与原平台逐项绑定", () => {
  validateDiscoveryQueryAudit(recorded.audit, recorded.scopes);
});

test("真实审核条目重排后保存具体绑定错误，供原查询重新审核", () => {
  assert.throws(() => validateDiscoveryQueryAudit({ ...recorded.audit, queries: recorded.audit.queries.toReversed() }, recorded.scopes), (error: unknown) => {
    assert.ok(error instanceof Error && "diagnostics" in error);
    const diagnostic = error.diagnostics as { stage: string; kind: string; issues: string[] };
    assert.equal(diagnostic.stage, "query_audit");
    assert.equal(diagnostic.kind, "format");
    assert.ok(diagnostic.issues.some(issue => issue.includes("AI agent trainer") && issue.includes("CrewAI community")));
    const saved = discoveryReadFailures.parse([{ recorded_at: 1, step: "query_audit", platform: null, target_id: null, call_number: null, code: "discovery_draft_scope_invalid", diagnostic }]);
    assert.deepEqual(saved[0]!.diagnostic, diagnostic);
    return true;
  });
});

test("真实审核记录缺少条目时保留缺失项，不能用部分审核放行", () => {
  assert.throws(() => validateDiscoveryQueryAudit({ ...recorded.audit, queries: recorded.audit.queries.slice(0, 3) }, recorded.scopes), (error: unknown) => {
    assert.ok(error instanceof Error && "diagnostics" in error);
    const diagnostic = error.diagnostics as { kind: string; issues: string[] };
    assert.equal(diagnostic.kind, "format");
    assert.ok(diagnostic.issues.some(issue => issue.includes("CrewAI community")));
    return true;
  });
});
