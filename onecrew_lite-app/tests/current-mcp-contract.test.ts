import assert from "node:assert/strict";
import test from "node:test";
import { loadCurrentMcpRegistry, loadOriginalMcpRegistry, McpCatalogue } from "../src/service/mcp-catalogue.ts";
import { FrozenMcpOutputValidator } from "../src/service/mcp-json-schema.ts";

test("当前MCP连接合同移除XChat字段并保留独立的原历史合同", async () => {
  const original = await loadOriginalMcpRegistry(), current = await loadCurrentMcpRegistry();
  const previous = original.entries.find(entry => entry.kind === "operation" && entry.metadata.id === "x.connection.get")!;
  const entry = current.entries.find(entry => entry.kind === "operation" && entry.metadata.id === "x.connection.get")!;
  if (previous.kind !== "operation" || entry.kind !== "operation") throw new Error("connection contract missing");
  assert.ok(JSON.stringify(previous.output_schema).includes("x_chat_passcode_status"));
  assert.ok(!JSON.stringify(entry.output_schema).includes("x_chat_passcode_status"));
  assert.notEqual(entry.output_revision, previous.output_revision);
  assert.equal(entry.input_revision, previous.input_revision);
  const catalogue = new McpCatalogue(current, ["x.connection.get"]);
  for (const surface of entry.metadata.surfaces) {
    const result = catalogue.get(surface, { operation_ids: ["x.connection.get"], include_output_schema: true });
    assert.deepEqual(result.operations[0]!.output_schema, entry.output_schema);
    new FrozenMcpOutputValidator(current, surface);
  }
  const guides = current.entries.filter(value => value.kind === "toolkit_guide" && ["instagram", "telegram"].includes(value.metadata.toolkit));
  for (const guide of guides) assert.doesNotMatch(guide.metadata.summary, /direct messages|private chats/iu);
});
