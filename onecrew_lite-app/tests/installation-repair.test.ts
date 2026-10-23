import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readdir, rm, stat } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { createMcpAdapter } from "../src/service/mcp-adapter.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

const execute = promisify(execFile);
const cli = fileURLToPath(new URL("../src/cli.ts", import.meta.url));
const scratch = fileURLToPath(new URL("../../.scratch/installation-repair-tests/", import.meta.url));

test("repair without original configuration reports the failure and preserves the directory", async () => {
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "missing-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const before = await readdir(directory);
  const observed = await execute(process.execPath, [cli, "repair", "--data-dir", directory], { timeout: 30_000 })
    .then(value => value, error => error as { stdout: string; stderr: string });
  const result = JSON.parse(observed.stdout);
  assert.equal(result.state, "needs_action");
  assert.equal(result.failure_code, "repair.original_configuration_missing");
  assert.deepEqual(await readdir(directory), before);
  await rm(root, { recursive: true });
});

test("offline MCP repair starts the original real service once without changing its key or port", { timeout: 180_000 }, async () => {
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "offline-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const key = join(directory, "original-key.data");
  const invoke = async (command: string) => JSON.parse((await execute(process.execPath,
    [cli, command, "--data-dir", directory, ...(command === "start" ? ["--vault", join(directory, "original-vault.sqlite"), "--master-key", key] : [])],
    { timeout: 90_000, maxBuffer: 128 * 1024, env: { ...process.env, ONECREW_LITE_DIAGNOSTICS: "1" } })).stdout);
  let running = false;
  let completed = false;
  const adapter = await createMcpAdapter(directory), client = new Client({ name: "real-offline-repair", version: "1" });
  const [left, right] = InMemoryTransport.createLinkedPair();
  try {
    await adapter.connect(right); await client.connect(left);
    const tool = (await client.listTools()).tools.find(item => item.name === "onecrew_repair");
    assert.ok(tool, "主 HTTP 服务停止时也应提供内置修复命令");
    const original = await invoke("start"); running = true;
    const keyBefore = await stat(key);
    await invoke("stop"); running = false;
    const input = { request_id: "92c0cdaf-dc90-4735-9801-9c0812534ebc", client: "mcp", wait_ms: 1000 };
    let reply: Record<string, any>;
    do {
      const value = await client.callTool({ name: "onecrew_repair", arguments: input });
      assert.notEqual(value.isError, true);
      reply = JSON.parse((value.content as { text: string }[])[0]!.text);
    } while (reply.state === "repairing");
    running = (await invoke("status")).state === "running";
    assert.equal(reply.state, "needs_action", "真实未登录服务应提示登录，不能假报全部完成");
    assert.equal(reply.service.running, true);
    assert.equal(reply.service.port, original.port);
    assert.notEqual(reply.service.instance_id, original.instance_id);
    assert.equal(reply.failure_code, "access.login_required");
    const repeated = await client.callTool({ name: "onecrew_repair", arguments: input });
    assert.equal(JSON.parse((repeated.content as { text: string }[])[0]!.text).service.pid, reply.service.pid);
    assert.equal((await stat(key)).ino, keyBefore.ino);
    assert.equal(reply.search_resumed, false);
    assert.equal(JSON.stringify(reply).includes("control_token"), false);
    assert.equal(JSON.stringify(reply).includes("original-key.data"), false);
    await assert.rejects(client.callTool({ name: "onecrew_repair", arguments: { ...input, command: "anything" } }));
    completed = true;
  } finally {
    if (running || (await invoke("status")).state === "running") await invoke("stop");
    await client.close(); await adapter.close();
    if (completed) await rm(root, { recursive: true });
    else console.error(`Installation repair diagnostics retained at ${root}`);
  }
});
