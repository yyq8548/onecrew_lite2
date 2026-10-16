import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { z } from "zod";
import { loadOriginalMcpRegistry, McpCatalogue } from "../src/service/mcp-catalogue.ts";
import { McpCatalogueSearch } from "../src/service/mcp-catalogue-search.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { retiredOperations } from "../src/contracts/retired-operations.ts";

test("MCP search retains original Rust evaluation recall across all retained operations", async () => {
  const data = await loadOriginalMcpRegistry();
  const retired = new Set<string>(retiredOperations);
  const catalogue = new McpCatalogue(data, data.entries.filter(entry => entry.kind === "operation" && !retired.has(entry.metadata.id)).map(entry => entry.metadata.id));
  const search = new McpCatalogueSearch(catalogue, data);
  const cases = z.array(z.object({ expected_id: z.string(), input: z.unknown(), result: z.object({ operations: z.array(z.object({ id: z.string() })) }) })).parse(parseApplicationJson(await readFile(new URL("./fixtures/mcp-search-rust.json", import.meta.url))));
  const missing: string[] = [];
  for (const value of cases) {
    if (retired.has(value.expected_id)) continue;
    const result = search.search("full", value.input);
    if (!result.operations.some(operation => operation.id === value.expected_id)) missing.push(value.expected_id);
    assert.equal(result.operations[0]?.id, value.result.operations.find(operation => !retired.has(operation.id))?.id, JSON.stringify(value.input));
  }
  assert.deepEqual(missing, []);
  assert.equal(cases.length, 129);
});

test("MCP catalogue only exposes registered handlers and complete playbooks", async () => {
  const data = await loadOriginalMcpRegistry();
  const catalogue = new McpCatalogue(data, ["onecrew.creator.list"]);
  assert.deepEqual(catalogue.operations("full").map(entry => entry.metadata.id), ["onecrew.creator.list"]);
  assert.deepEqual(catalogue.playbooks("full"), []);
  assert.equal(catalogue.directory("full").length, 1);
  assert.throws(() => catalogue.get("full", { operation_ids: ["onecrew.creator.list", "youtube.video.search"] }));
  assert.equal(catalogue.get("full", { operation_ids: ["onecrew.creator.list"] }).operations[0]?.output_schema, null);
  assert.ok(catalogue.get("full", { operation_ids: ["onecrew.creator.list"], include_output_schema: true }).operations[0]?.output_schema);
  assert.throws(() => new McpCatalogue(data, ["x.chat.send"]));
});
