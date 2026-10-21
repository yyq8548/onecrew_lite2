import assert from "node:assert/strict";
import test from "node:test";
import { loadCurrentMcpRegistry, loadOriginalMcpRegistry, McpCatalogue } from "../src/service/mcp-catalogue.ts";
import { FrozenMcpOutputValidator } from "../src/service/mcp-json-schema.ts";
import { McpJsonSchemaValidator } from "../src/service/mcp-json-schema.ts";
import { readFile } from "node:fs/promises";
import { z } from "zod";

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

test("真实Ivan网站联系方式通过当前MCP账号分组合同，历史合同仍保持原样", async () => {
  const actual = JSON.parse(await readFile(new URL("./fixtures/linkedin-contact-website-20261020.json", import.meta.url), "utf8"));
  const original = await loadOriginalMcpRegistry(), current = await loadCurrentMcpRegistry();
  for (const [data, valid] of [[original, false], [current, true]] as const) {
    const entry = data.entries.find(value => value.kind === "operation" && value.metadata.id === "onecrew.discovery.identity.groups.get");
    if (entry?.kind !== "operation") throw new Error("identity groups contract missing");
    let schema: unknown = entry.output_schema;
    for (const key of ["properties", "groups", "items", "properties", "members", "items", "properties", "sources", "items", "properties", "contacts", "items"]) schema = z.record(z.string(), z.unknown()).parse(schema)[key];
    assert.equal(new McpJsonSchemaValidator().getValidator(z.record(z.string(), z.unknown()).parse(schema))(actual).valid, valid);
  }
});
