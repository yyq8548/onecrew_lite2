import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { CodexClientAdapter, readCodexConfiguration } from "../src/client-adapters/codex.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { parsePayloadManifest } from "../src/packaging/payload-manifest.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { verifyCodexPlugin } from "../src/client-adapters/codex-package.ts";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { resolveCodexExecutable } from "../src/client-adapters/codex-process.ts";
import { parse as parseCookies } from "set-cookie-parser";

const execute = promisify(execFile);

test("actual Codex CLI installs, verifies and removes only OneCrew in an isolated CODEX_HOME", { timeout: 240_000 }, async () => {
  const candidate = process.env.ONECREW_CODEX_TEST_EXECUTABLE;
  if (!candidate) throw new Error("Build the actual SEA candidate and set ONECREW_CODEX_TEST_EXECUTABLE before native ClientAdapter acceptance");
  const executable = resolve(candidate), manifest = parsePayloadManifest(await readFile(join(dirname(executable), "runtime-manifest.json")));
  const scratch = fileURLToPath(new URL("../../.scratch/client-adapter-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const root = await mkdtemp(join(scratch, "native-"));
  const privateRoot = await createPrivateDirectory(join(root, "private")), dataDirectory = await createPrivateDirectory(join(privateRoot, "data")), codexHome = await createPrivateDirectory(join(privateRoot, "codex")), pluginDirectory = await createPrivateDirectory(join(privateRoot, "plugins"));
  const adapter = new CodexClientAdapter({ executable, runtimeVersion: manifest.productVersion, dataDirectory, codexHome, pluginDirectory });
  const original = 'model_reasoning_effort = "low"\n# preserved unrelated settings\n[features]\nshell_tool = true\n';
  await writeFile(join(codexHome, "config.toml"), original, { mode: 0o600 });
  try {
    const detected = await adapter.detect();
    assert.equal(detected.client, "codex"); assert.equal(detected.semanticAuthority, false);
    const prepared = await adapter.prepare();
    const connection = parseApplicationJson(await readFile(join(prepared.plugin_root, ".mcp.json"))) as { mcpServers: { onecrew_lite: { command: string; args: string[]; env?: Record<string, string> } } };
    if (process.platform === "win32") assert.equal(connection.mcpServers.onecrew_lite.env?.ONECREW_LITE_MCP_DATA_DIR, dataDirectory);
    else assert.deepEqual(connection.mcpServers.onecrew_lite.args, ["mcp", "--stdio", "--data-dir", dataDirectory]);
    const installed = await adapter.install();
    assert.equal(installed.state, "installed"); assert.equal(installed.enabled, true); assert.equal(installed.installed_version, prepared.version);
    const cached = await adapter.status();
    assert.equal(cached.verification, "cached", JSON.stringify({ cached, config: await readCodexConfiguration(codexHome), root: prepared.marketplace_root }));
    assert.equal((await adapter.status(true)).state, "installed");
    assert.equal((await readCodexConfiguration(codexHome)).model_reasoning_effort, "low");
    const removed = await adapter.uninstall();
    assert.equal(removed.state, "not_installed");
    const remaining = await readCodexConfiguration(codexHome);
    assert.equal(remaining.plugins?.["onecrew_lite@onecrew_lite-cli"], undefined);
    assert.equal(remaining.model_reasoning_effort, "low");
    assert.match(await readFile(join(codexHome, "config.toml"), "utf8"), /preserved unrelated settings/u);
  } finally { await rm(root, { recursive: true }); }
});

test("packaged ClientAdapter and its installed MCP preserve the service data directory across a real SEA upgrade", { timeout: 240_000 }, async () => {
  const current = process.env.ONECREW_CODEX_TEST_EXECUTABLE, previous = process.env.ONECREW_CODEX_TEST_PREVIOUS_EXECUTABLE;
  if (!current || !previous) throw new Error("Set both actual SEA candidate paths before packaged ClientAdapter upgrade acceptance");
  const executable = resolve(current), oldExecutable = resolve(previous), codexExecutable = await resolveCodexExecutable();
  const scratch = fileURLToPath(new URL("../../.scratch/client-adapter-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const root = await mkdtemp(join(scratch, "packaged-"));
  const privateRoot = await createPrivateDirectory(join(root, "private"));
  const dataDirectory = join(privateRoot, "data"), codexHome = await createPrivateDirectory(join(privateRoot, "codex"));
  await mkdir(join(root, "Library", "Caches"), { recursive: true, mode: 0o700 });
  const env = Object.fromEntries(Object.entries({ ...process.env, HOME: root, LOCALAPPDATA: root }).filter((entry): entry is [string, string] => entry[1] !== undefined));
  const clientOptions = ["--codex-home", codexHome, "--codex-executable", codexExecutable];
  async function invoke(binary: string, command: string) {
    const args = [command, "--data-dir", dataDirectory];
    if (command === "start") args.push("--vault", join(dataDirectory, "vault.sqlite"), "--master-key", join(dataDirectory, "master.key"));
    if (["connect-codex", "disconnect-codex", "codex-package"].includes(command)) args.push(...clientOptions);
    const result = await execute(binary, args, { env, timeout: 120_000, maxBuffer: 1024 * 1024 });
    return parseApplicationJson(result.stdout) as Record<string, unknown>;
  }
  async function browserSession(binary: string) {
    const workspace = await invoke(binary, "workspace"), url = new URL(String(workspace.url));
    const ticket = new URLSearchParams(url.hash.slice(1)).get("onecrew_ticket");
    assert.ok(ticket);
    const exchange = await fetch(`${url.origin}/api/local/session`, { method: "POST", headers: { origin: url.origin, "content-type": "application/json" }, body: JSON.stringify({ ticket }) });
    assert.equal(exchange.status, 200);
    const cookies = parseCookies(exchange.headers.getSetCookie());
    assert.equal(cookies.length, 1);
    const cookie = cookies.map(value => `${value.name}=${value.value}`).join("; ");
    const handshake = await fetch(`${url.origin}/api/session`, { headers: { cookie } });
    assert.equal(handshake.status, 200);
    assert.equal((await handshake.json()).result.transport, "local_http");
    return { origin: url.origin, cookie };
  }
  let running: string | null = null;
  try {
    const first = await invoke(oldExecutable, "start"); running = oldExecutable;
    const oldPackage = await invoke(oldExecutable, "codex-package");
    assert.equal((await invoke(oldExecutable, "connect-codex")).installed_version, oldPackage.version);
    const oldBrowser = await browserSession(oldExecutable);
    assert.equal((await invoke(oldExecutable, "stop")).state, "stopped"); running = null;
    const next = await invoke(executable, "start"); running = executable;
    assert.equal(next.port, first.port);
    assert.notEqual(next.instance_id, first.instance_id);
    assert.equal((await fetch(`${oldBrowser.origin}/api/session`, { headers: { cookie: oldBrowser.cookie } })).status, 401);
    const newBrowser = await browserSession(executable);
    assert.equal(newBrowser.origin, oldBrowser.origin);
    assert.notEqual(newBrowser.cookie, oldBrowser.cookie);
    const prepared = await invoke(executable, "codex-package");
    assert.notEqual(prepared.binary_sha256, oldPackage.binary_sha256);
    assert.equal((await invoke(executable, "connect-codex")).installed_version, prepared.version);
    const cachedRoot = join(codexHome, "plugins", "cache", "onecrew_lite-cli", "onecrew_lite", String(prepared.version));
    await verifyCodexPlugin(cachedRoot, String(prepared.version), String(prepared.binary_sha256));
    const connection = parseApplicationJson(await readFile(join(cachedRoot, ".mcp.json"))) as { mcpServers: { onecrew_lite: { command: string; args: string[]; env?: Record<string, string> } } };
    const settings = connection.mcpServers.onecrew_lite;
    const client = new Client({ name: "native-installed-plugin-acceptance", version: "1" });
    const transport = new StdioClientTransport({ command: process.platform === "win32" ? settings.command : resolve(cachedRoot, settings.command), args: settings.args, cwd: cachedRoot, env: { ...env, ...settings.env }, stderr: "pipe" });
    try {
      await client.connect(transport);
      const result = await client.callTool({ name: "onecrew_status", arguments: {} });
      assert.notEqual(result.isError, true);
      const content = result.content as { type: string; text: string }[];
      const status = parseApplicationJson(content.find(item => item.type === "text")!.text) as { running: boolean; instance_id: string; binary_sha256: string };
      assert.equal(status.running, true);
      assert.equal(status.instance_id, next.instance_id);
      assert.equal(status.binary_sha256, prepared.binary_sha256);
      const resource = (await client.listResources()).resources[0]; assert.ok(resource);
      const card = (await client.readResource({ uri: resource.uri })).contents[0];
      assert.ok(card && "text" in card); assert.match(card.text, /<!doctype html>/iu);
    } finally { await client.close(); await transport.close(); }
    assert.equal((await invoke(executable, "disconnect-codex")).state, "not_installed");
  } finally {
    if (running) await invoke(running, "stop");
    await rm(root, { recursive: true });
  }
});

test("actual Codex CLI upgrades between real SEA generations and restores the previous plugin", { timeout: 240_000 }, async () => {
  const current = process.env.ONECREW_CODEX_TEST_EXECUTABLE, previous = process.env.ONECREW_CODEX_TEST_PREVIOUS_EXECUTABLE;
  if (!current || !previous) throw new Error("Set both actual SEA candidate paths before native ClientAdapter upgrade acceptance");
  const executable = resolve(current), oldExecutable = resolve(previous);
  const manifest = parsePayloadManifest(await readFile(join(dirname(executable), "runtime-manifest.json")));
  const oldManifest = parsePayloadManifest(await readFile(join(dirname(oldExecutable), "runtime-manifest.json")));
  const scratch = fileURLToPath(new URL("../../.scratch/client-adapter-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const root = await mkdtemp(join(scratch, "upgrade-"));
  const privateRoot = await createPrivateDirectory(join(root, "private"));
  const dataDirectory = await createPrivateDirectory(join(privateRoot, "data"));
  const codexHome = await createPrivateDirectory(join(privateRoot, "codex"));
  const pluginDirectory = await createPrivateDirectory(join(privateRoot, "plugins"));
  const options = { dataDirectory, codexHome, pluginDirectory };
  const oldAdapter = new CodexClientAdapter({ ...options, executable: oldExecutable, runtimeVersion: oldManifest.productVersion });
  const adapter = new CodexClientAdapter({ ...options, executable, runtimeVersion: manifest.productVersion });
  const configPath = join(codexHome, "config.toml");
  await writeFile(configPath, 'model_reasoning_effort = "low"\n# preserve settings through upgrade\n', { mode: 0o600 });
  try {
    const oldPackage = await oldAdapter.prepare();
    assert.equal((await oldAdapter.install()).installed_version, oldPackage.version);
    const outdated = await adapter.status();
    assert.equal(outdated.state, "unavailable");
    assert.equal(outdated.diagnostic_code, "codex_plugin_update_required");
    const prepared = await adapter.prepare();
    assert.notEqual(prepared.binary_sha256, oldPackage.binary_sha256);
    assert.notEqual(prepared.version, oldPackage.version);
    const installed = await adapter.install();
    assert.equal(installed.installed_version, prepared.version);
    assert.equal(installed.enabled, true);
    assert.equal((await adapter.status(true)).state, "installed");
    const cachedRoot = join(codexHome, "plugins", "cache", "onecrew_lite-cli", "onecrew_lite", prepared.version);
    await verifyCodexPlugin(cachedRoot, prepared.version, prepared.binary_sha256);
    const connection = parseApplicationJson(await readFile(join(cachedRoot, ".mcp.json"))) as { mcpServers: { onecrew_lite: { args: string[]; env?: Record<string, string> } } };
    if (process.platform === "win32") assert.equal(connection.mcpServers.onecrew_lite.env?.ONECREW_LITE_MCP_DATA_DIR, dataDirectory);
    else assert.deepEqual(connection.mcpServers.onecrew_lite.args, ["mcp", "--stdio", "--data-dir", dataDirectory]);
    assert.equal((await readCodexConfiguration(codexHome)).model_reasoning_effort, "low");
    assert.match(await readFile(configPath, "utf8"), /preserve settings through upgrade/u);
    assert.equal((await oldAdapter.install()).installed_version, oldPackage.version);
    assert.equal((await oldAdapter.status(true)).state, "installed");
    assert.equal((await oldAdapter.uninstall()).state, "not_installed");
    assert.equal((await readCodexConfiguration(codexHome)).model_reasoning_effort, "low");
  } finally { await rm(root, { recursive: true }); }
});
