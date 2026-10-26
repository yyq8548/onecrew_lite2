import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { readCodexConfiguration, CodexClientAdapter } from "../src/client-adapters/codex.ts";
import { executeCodexClientCommand } from "../src/client-adapters/commands.ts";
import { runtimeCompatibility } from "../src/client-adapters/contracts.ts";
import { watch } from "node:fs";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { ensurePrivateDirectory, codexMarketplaceRoot, prepareCodexPlugin, verifyCodexPlugin } from "../src/client-adapters/codex-package.ts";
import productMetadata from "../package.json" with { type: "json" };

test("当前产品版本可生成并校验配套插件", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/client-adapter-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "product-version-"));
  try {
    const dataDirectory = await ensurePrivateDirectory(join(root, "data"));
    const codexHome = await ensurePrivateDirectory(join(root, "home"));
    const productVersion = productMetadata.version;
    const prepared = await prepareCodexPlugin({ executable: process.execPath, runtimeVersion: productVersion, dataDirectory, codexHome, pluginDirectory: join(root, "plugins") });
    assert.equal(prepared.runtime_version, productVersion);
    assert.equal(prepared.version.startsWith(`${productVersion}+codex.`), true);
    await verifyCodexPlugin(prepared.plugin_root, prepared.version, prepared.binary_sha256);
  } finally { await rm(root, { recursive: true }); }
});

test("ClientAdapter reads mature TOML without changing unrelated configuration and emits exact source argv", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/client-adapter-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const root = await mkdtemp(join(scratch, "contracts-"));
  try {
    const source = 'model_reasoning_effort = "low"\n[plugins."other@elsewhere"]\nenabled = false\n[marketplaces.other]\nsource_type = "local"\nsource = "C:\\workspace"\n'.replace('"C:\\workspace"', "'C:\\workspace'");
    await writeFile(join(root, "config.toml"), source);
    const config = await readCodexConfiguration(root);
    assert.equal(config.plugins?.["other@elsewhere"]?.enabled, false);
    assert.equal(await readFile(join(root, "config.toml"), "utf8"), source);
    const configuration = await executeCodexClientCommand("codex-config", { dataDirectory: root }) as { mcpServers: { onecrew_lite: { command: string; args: string[] } } };
    assert.equal(configuration.mcpServers.onecrew_lite.command, process.execPath);
    assert.deepEqual(configuration.mcpServers.onecrew_lite.args.slice(-4), ["mcp", "--stdio", "--data-dir", root]);
    const adapter = new CodexClientAdapter({ executable: process.execPath, runtimeVersion: "0.6.0", dataDirectory: root, codexHome: root, pluginDirectory: join(root, "plugins") });
    await assert.rejects(adapter.install(), /runtime_update_required/);
    assert.deepEqual(runtimeCompatibility, { protocol: 2, minimum: productMetadata.version, maximum: productMetadata.version });
    for (const runtimeVersion of ["0.7.1", "0.7.1-ts.candidate.1", "0.9.1-ts.candidate.1", "0.7.0", "0.7.2", "0.9.0", "0.9.1", "0.9.3"]) await assert.rejects(new CodexClientAdapter({ executable: process.execPath, runtimeVersion, dataDirectory: root, codexHome: root, pluginDirectory: join(root, "plugins") }).install(), /runtime_update_required/);
    assert.equal(await readFile(join(root, "config.toml"), "utf8"), source);
  } finally { await rm(root, { recursive: true }); }
});

test("实际插件文件生成期间保持原生独占锁，失败原样传播并释放锁", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/client-adapter-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const directory = await mkdtemp(join(scratch, "prepare-lock-"));
  try {
    const dataDirectory = await ensurePrivateDirectory(join(directory, "data")), codexHome = await ensurePrivateDirectory(join(directory, "home"));
    const options = { executable: process.execPath, runtimeVersion: productMetadata.version, dataDirectory, codexHome, pluginDirectory: join(directory, "plugin") };
    const root = await codexMarketplaceRoot(options), adapter = new CodexClientAdapter(options);
    let observed = false;
    const observer = new Promise<void>((resolve, reject) => {
      const watcher = watch(root, { recursive: true }, (_event, filename) => {
        if (observed || !filename?.startsWith("generations")) return;
        observed = true; watcher.close();
        void assert.rejects(acquireInstanceLock(root), (error: unknown) => error instanceof Error && "code" in error && error.code === "runtime.already_owned").then(resolve, reject);
      });
      watcher.on("error", reject);
    });
    const [prepared] = await Promise.all([adapter.prepare(), observer]);
    assert.equal(prepared.runtime_version, productMetadata.version); assert.equal(observed, true);
    const released = await acquireInstanceLock(root); await released.close();
    const incompatible = new CodexClientAdapter({ ...options, runtimeVersion: "0.7.2" });
    await assert.rejects(incompatible.prepare(), /runtime_update_required/);
    const afterFailure = await acquireInstanceLock(root); await afterFailure.close();
  } finally { await rm(directory, { recursive: true }); }
});
