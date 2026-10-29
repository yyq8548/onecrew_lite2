import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { z } from "zod";
import { stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { OperationRetiredError, retiredOperations } from "../src/contracts/retired-operations.ts";
import { getPackagedRuntime } from "../src/packaging/runtime-context.ts";
import { buildCurrentMcpRegistry, loadCurrentMcpRegistry, McpCatalogue, parseCurrentMcpRegistry } from "../src/service/mcp-catalogue.ts";

test("构建目录与修改加载方式前的真实转换输出逐字节一致，开发加载保持原逻辑", async () => {
  assert.equal(getPackagedRuntime(), undefined);
  const built = await buildCurrentMcpRegistry(), bytes = stringifyApplicationJson(built);
  // 2026-10-28 修改加载方式前，由原 loadCurrentMcpRegistry 实际生成的输出摘要。
  assert.equal(createHash("sha256").update(bytes).digest("hex"), "e14eea011cacb575166b46a23cc17674af55981c6427676211fb1faaeae70bb6");
  assert.deepEqual(parseCurrentMcpRegistry(Buffer.from(bytes)), built);
  assert.deepEqual(await loadCurrentMcpRegistry(), built);
});

test("最终目录资源保留schema校验及重复ID拒绝", async () => {
  const built = await buildCurrentMcpRegistry();
  assert.throws(() => parseCurrentMcpRegistry(stringifyApplicationJson({ ...built, entries: [...built.entries, built.entries[0]!] })), /mcp_registry_duplicate_entry/);
  assert.throws(() => parseCurrentMcpRegistry(stringifyApplicationJson({ ...built, entries: built.entries.map(entry => entry.kind === "operation" ? { ...entry, input_schema: null } : entry) })), z.ZodError);
  const { surfaces, ...missingSurfaceContracts } = built;
  assert.ok(surfaces.length > 0);
  assert.throws(() => parseCurrentMcpRegistry(stringifyApplicationJson(missingSurfaceContracts)), z.ZodError);
});

test("构建目录保留关键操作、合同revision、handler完整性及退役410", async () => {
  const built = await buildCurrentMcpRegistry(), retired = new Set<string>(retiredOperations);
  const catalogue = new McpCatalogue(built, built.entries.filter(entry => entry.kind === "operation" && !retired.has(entry.metadata.id)).map(entry => entry.metadata.id));
  for (const id of ["onecrew.discovery.host_work.claim", "onecrew.discovery.host_work.get", "onecrew.discovery.host_work.submit", "onecrew.discovery.host_operation.get", "onecrew.discovery.execution.start", "onecrew.discovery.execution.get", "onecrew.discovery.queries.expand"]) {
    const entry = catalogue.operation(id, "full");
    assert.ok(entry, id);
    for (const direction of ["input", "output"] as const) assert.equal(entry[`${direction}_revision`], `sha256:${createHash("sha256").update(JSON.stringify(entry[`${direction}_schema`])).digest("hex")}`, `${id}:${direction}`);
    const contract = catalogue.get("full", { operation_ids: [id], include_output_schema: true }).operations[0]!;
    assert.equal(contract.schema_revision, entry.input_revision);
    assert.deepEqual(contract.input_schema, entry.input_schema);
    assert.deepEqual(contract.output_schema, entry.output_schema);
  }
  for (const id of ["onecrew.repair.get", "onecrew.repair.apply", "onecrew.repair.finish"]) assert.ok(catalogue.operation(id, "full"), id);
  assert.throws(() => new McpCatalogue(built, ["onecrew.contract.missing"]), /mcp_handler_contract_missing/);
  assert.throws(() => new McpCatalogue(built, ["x.message.send"]), (error: unknown) => error instanceof OperationRetiredError && error.status === 410 && error.code === "operation.retired");
});
