import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import test from "node:test";
import { loadCurrentMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { repairApplyInput, repairBeginInput, repairFinishInput } from "@onecrew/runtime-contracts/repair";

test("模型只能处理用户已创建的修复，修复操作不提供搜索审批或任意命令入口", async () => {
  const registry = await loadCurrentMcpRegistry();
  const operations = registry.entries.filter(entry => entry.kind === "operation" && entry.metadata.id.startsWith("onecrew.repair."));
  assert.deepEqual(operations.map(entry => entry.metadata.id).sort(), ["onecrew.repair.apply", "onecrew.repair.finish", "onecrew.repair.get"]);
  for (const entry of operations) if (entry.kind === "operation") {
    assert.equal(entry.effect, "write_local");
    assert.equal(entry.approval, "owner_guarded");
    assert.equal(entry.supports_search_plan, false);
    assert.deepEqual(entry.connection, { kind: "none" });
    assert.ok((entry.input_schema.required as string[]).includes("repair_id"));
  }
  const repair_id = randomUUID();
  for (const action of ["search_start", "execution_resume", "approve", "delete_data", "shell"]) {
    assert.equal(repairApplyInput.safeParse({ repair_id, action }).success, false);
  }
  assert.equal(repairApplyInput.safeParse({ repair_id, action: "sync_results", command: "anything" }).success, false);
  assert.equal(repairFinishInput.parse({ repair_id, summary: "当前检查通过" }).unresolved, false);
  assert.equal(repairFinishInput.parse({ repair_id, summary: "需要重新登录", unresolved: true }).unresolved, true);
});

test("修复请求只接受限定的页面和错误字段，拒绝凭据和任意错误正文", () => {
  const request_id = randomUUID();
  assert.equal(repairBeginInput.safeParse({ request_id, page: "search", client_errors: [{ command: "service_status", code: "web_session_required", at: Date.now() }] }).success, true);
  assert.equal(repairBeginInput.safeParse({ request_id, page: "mail" }).success, false);
  assert.equal(repairBeginInput.safeParse({ request_id, page: "connections", cookie: "present" }).success, false);
  assert.equal(repairBeginInput.safeParse({ request_id, page: "search", client_errors: [{ command: "service_status", code: "https://example.invalid/?token=present", at: Date.now() }] }).success, false);
});
