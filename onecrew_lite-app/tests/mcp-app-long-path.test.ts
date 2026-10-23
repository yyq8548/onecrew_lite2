import assert from "node:assert/strict";
import test from "node:test";
import { copyFile, mkdir, mkdtemp, rm } from "node:fs/promises";
import { join, toNamespacedPath } from "node:path";
import { fileURLToPath } from "node:url";
import { loadMcpAppResources } from "../src/service/mcp-app-resources.ts";

test("actual MCP card assets load from Windows long canonical cache paths", { skip: process.platform !== "win32" }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/mcp-path-", import.meta.url)));
  const directory = join(root, "a".repeat(70), "b".repeat(70), "c".repeat(70), "mcp");
  try {
    await mkdir(directory, { recursive: true });
    const source = fileURLToPath(new URL("../../onecrew_lite-mcp-ui/dist/", import.meta.url));
    for (const name of ["operation-inspector.html", "retired-operation-inspector.html"]) await copyFile(join(source, name), join(directory, name));
    const regular = await loadMcpAppResources(directory);
    const canonical = await loadMcpAppResources(toNamespacedPath(directory));
    assert.equal(regular.uri, canonical.uri);
    assert.match(regular.read(regular.uri).contents[0]!.text, /<!doctype html>/iu);
    assert.throws(() => regular.read("file:///private"), /resource_not_found/u);
  } finally { await rm(root, { recursive: true }); }
});
