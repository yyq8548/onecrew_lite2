import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { CodexClientAdapter, readCodexConfiguration } from "../src/client-adapters/codex.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { parsePayloadManifest } from "../src/packaging/payload-manifest.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";

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
