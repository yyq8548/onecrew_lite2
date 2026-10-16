import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { createMcpAdapter } from "../src/service/mcp-adapter.ts";
import { unifiedResources } from "../src/application/unified-resources.ts";
import { startLocalRuntime } from "../src/service/runtime.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("SDK adapter uses the real unified service for logged-out status and local authorization cancellation", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/mcp-adapter-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "service-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const runtime = await startLocalRuntime(directory, unifiedResources({ vaultPath: join(directory, "vault.sqlite"), masterKeyPath: join(directory, "master.key"), webAssetsDirectory: fileURLToPath(new URL("../../onecrew_lite-rust/apps/web/dist", import.meta.url)), libraryWebAssetsDirectory: fileURLToPath(new URL("../../onecrew_lite-library/web/dist", import.meta.url)) }));
  const adapter = await createMcpAdapter(directory), client = new Client({ name: "native-acceptance", version: "1" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  try {
    await adapter.connect(serverTransport);
    await client.connect(clientTransport);
    const resources = await client.listResources();
    assert.equal(resources.resources.length, 1);
    const resource = resources.resources[0]!;
    assert.equal(resource.mimeType, "text/html;profile=mcp-app");
    const currentContent = (await client.readResource({ uri: resource.uri })).contents[0]!;
    if (!("text" in currentContent)) throw new Error("Expected actual HTML resource");
    assert.match(currentContent.text, /<!doctype html>/i);
    const retired = resource.uri.replace(/[0-9a-f]{16}\.html$/, "0000000000000000.html");
    const retiredContent = (await client.readResource({ uri: retired })).contents[0]!;
    if (!("text" in retiredContent)) throw new Error("Expected retired HTML resource");
    assert.notEqual(retiredContent.text, currentContent.text);
    await assert.rejects(client.readResource({ uri: "file:///private" }));
    const manifest = (await client.listTools()).tools;
    assert.deepEqual(manifest.map(tool => tool.name), ["onecrew_status", "onecrew_login", "onecrew_login_cancel", "onecrew_logout", "onecrew_web", "onecrew_workspace_card_request", "onecrew_connection_list", "onecrew_result_present", "onecrew_result_state", "onecrew_tool_execute", "onecrew_tool_get", "onecrew_tool_search"]);
    assert.deepEqual(manifest.find(tool => tool.name === "onecrew_result_state")?._meta?.ui, { visibility: ["app"] });
    assert.equal((manifest.find(tool => tool.name === "onecrew_result_present")?._meta?.ui as { resourceUri: string }).resourceUri, resource.uri);
    await assert.rejects(client.callTool({ name: "onecrew_tool_search", arguments: { query: "list creators" } }));
    const result = await client.callTool({ name: "onecrew_status", arguments: {} });
    const content = result.content as { type: string; text: string }[];
    assert.equal(JSON.parse(content[0]!.text).access.state, "signed_out");
    assert.equal(JSON.parse(content[0]!.text).instance_id, runtime.descriptor.instance_id);
    const pairing = await client.callTool({ name: "onecrew_login", arguments: {} });
    const authorize = new URL(JSON.parse((pairing.content as { text: string }[])[0]!.text).authorize_url);
    assert.equal(authorize.origin, "https://access.onecrewai.io");
    const cancelled = await client.callTool({ name: "onecrew_login_cancel", arguments: {} });
    assert.equal(JSON.parse((cancelled.content as { text: string }[])[0]!.text).state, "signed_out");
    const workspace = await client.callTool({ name: "onecrew_web", arguments: {} });
    const card = JSON.parse((workspace.content as { text: string }[])[0]!.text);
    assert.equal(card.kind, "workspace");
    assert.equal(new URL(card.url).origin, runtime.descriptor.url);
    assert.equal(new URL(card.library_url).pathname, "/library");
    assert.equal(card.library_error, null);
    assert.equal(card.workspace_activity, null);
    const viewId = workspace._meta?.viewUUID;
    assert.equal(typeof viewId, "string");
    const notification = await client.callTool({ name: "onecrew_workspace_card_request", arguments: { view_id: viewId } });
    assert.deepEqual(JSON.parse((notification.content as { text: string }[])[0]!.text), { kind: "none" });
    assert.equal((await fetch(`${runtime.descriptor.url}/_local/mcp/workspace/register`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ view_id: viewId }) })).status, 401);
    await assert.rejects(client.callTool({ name: "onecrew_status", arguments: { extra: true } }));
    await assert.rejects(client.callTool({ name: "onecrew_send_email", arguments: {} }));
    const processClient = new Client({ name: "native-stdio-acceptance", version: "1" });
    const processTransport = new StdioClientTransport({ command: process.execPath, args: [fileURLToPath(new URL("../src/cli.ts", import.meta.url)), "mcp", "--data-dir", directory], stderr: "pipe" });
    try {
      await processClient.connect(processTransport);
      const actual = await processClient.callTool({ name: "onecrew_status", arguments: {} });
      assert.equal(JSON.parse((actual.content as { text: string }[])[0]!.text).instance_id, runtime.descriptor.instance_id);
    } finally { await processClient.close(); await processTransport.close(); }
  } finally {
    await client.close();
    await adapter.close();
    await runtime.close();
    await rm(root, { recursive: true });
  }
});
