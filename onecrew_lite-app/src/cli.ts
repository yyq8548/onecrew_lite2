import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync } from "node:fs";
import { open, readFile } from "node:fs/promises";
import { join, resolve, toNamespacedPath } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { setTimeout as delay } from "node:timers/promises";
import { acquireInstanceLock } from "./service/instance-lock.ts";
import { readRuntimeDescriptor, readVerifiedRuntime, startLocalRuntime, type RuntimeDescriptor } from "./service/runtime.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "./storage/private-backup.ts";
import { getPackagedRuntime, runtimeAssetPath, runtimeExecutablePath } from "./packaging/runtime-context.ts";
import { codexClientCommands, executeCodexClientCommand, type CodexClientCommand } from "./client-adapters/commands.ts";
import { startWindowsServiceHost, removeWindowsServiceHost, abortWindowsServiceHost, retainWindowsServiceBinary } from "./service/windows-host.ts";
import { rememberServiceStart, repairInstallation } from "./service/installation-repair.ts";

function publicStatus(value: RuntimeDescriptor) {
  return { service: value.service, state: "running", instance_id: value.instance_id, pid: value.pid,
    port: value.port, url: value.url, migration_state: "development", production_ready: false };
}
function output(value: unknown): void { process.stdout.write(`${JSON.stringify(value)}\n`); }
class CliError extends Error {
  readonly code: string;
  constructor(message: string, code = "cli.failed") { super(message); this.code = code; }
}

async function runningOwner(directory: string): Promise<RuntimeDescriptor | undefined> {
  const result = await acquireInstanceLock(directory).then(lease => ({ lease }), error => {
    if (error && typeof error === "object" && "code" in error && error.code === "runtime.already_owned") return undefined;
    throw error;
  });
  if (!result) return readVerifiedRuntime(directory);
  await result.lease.close();
  return undefined;
}

export async function runCli(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({ args, allowPositionals: true, strict: true,
    options: { "data-dir": { type: "string" }, vault: { type: "string" }, "master-key": { type: "string" }, "legacy-port": { type: "string" }, "web-assets": { type: "string" }, "library-web-assets": { type: "string" }, stdio: { type: "boolean" }, "codex-home": { type: "string" }, "codex-executable": { type: "string" }, "plugin-directory": { type: "string" }, refresh: { type: "boolean" }, client: { type: "string" } } });
  if (positionals.length === 1 && positionals[0] === "installation-discover") {
    for (const name of Object.keys(values)) if (!["data-dir", "codex-home"].includes(name)) throw new CliError("Installation discovery accepts only data-dir and codex-home");
    const { discoverInstallation, InstallationDiscoveryError } = await import("./service/installation-discovery.ts");
    output(await discoverInstallation(values["data-dir"], values["codex-home"]).catch(error => {
      if (error instanceof InstallationDiscoveryError) throw new CliError(error.message, error.code);
      throw error;
    }));
    return;
  }
  if (positionals.length !== 1 || !["start", "serve", "status", "stop", "repair", "data-upgrade", "workspace", "mcp", ...codexClientCommands].includes(positionals[0]!)) throw new CliError("Usage: onecrew_lite <installation-discover|start|serve|status|stop|repair|data-upgrade|workspace|mcp|connect-codex|disconnect-codex|codex-status|codex-config|codex-package> --data-dir <private directory>");
  if (!values["data-dir"]) throw new CliError("An explicit data directory is required");
  if (values["web-assets"] !== undefined && !values["web-assets"].trim()) throw new CliError("Web assets directory must not be empty");
  if (values["library-web-assets"] !== undefined && !values["library-web-assets"].trim()) throw new CliError("Library web assets directory must not be empty");
  const packaged = getPackagedRuntime();
  const webAssetsDirectory = values["web-assets"] ? resolve(values["web-assets"]) : packaged ? runtimeAssetPath("web", new URL("../../onecrew_lite-web/dist/", import.meta.url)) : undefined;
  const libraryWebAssetsDirectory = values["library-web-assets"] ? resolve(values["library-web-assets"]) : packaged ? runtimeAssetPath("library-web", new URL("../../onecrew_lite-library/web/dist/", import.meta.url)) : undefined;
  const portText = values["legacy-port"];
  if (portText !== undefined && (!/^\d{1,5}$/u.test(portText) || Number(portText) < 1 || Number(portText) > 65535)) throw new CliError("Legacy port must be an integer from 1 through 65535");
  const legacyPort = portText === undefined ? undefined : Number(portText);
  const command = positionals[0]!;
  if (values.client !== undefined && (command !== "repair" || !["codex", "mcp"].includes(values.client))) throw new CliError("--client accepts codex or mcp with repair only");
  if (values.stdio && command !== "mcp") throw new CliError("--stdio requires the mcp command");
  const directory = resolve(values["data-dir"]);
  if ((codexClientCommands as readonly string[]).includes(command)) {
    if (values.vault || values["master-key"] || values["legacy-port"] || values["web-assets"] || values["library-web-assets"]) throw new CliError("Service options cannot be used with a Codex client command");
    for (const option of ["codex-home", "codex-executable", "plugin-directory"] as const) if (values[option] !== undefined && !values[option].trim()) throw new CliError("Codex paths must not be empty");
    output(await executeCodexClientCommand(command as CodexClientCommand, { dataDirectory: directory, ...(values["codex-home"] ? { codexHome: values["codex-home"] } : {}), ...(values["codex-executable"] ? { codexExecutable: values["codex-executable"] } : {}), ...(values["plugin-directory"] ? { pluginDirectory: values["plugin-directory"] } : {}), ...(values.refresh === undefined ? {} : { refresh: values.refresh }) }));
    return;
  }
  if (values["codex-home"] !== undefined || values["codex-executable"] !== undefined || values["plugin-directory"] !== undefined || values.refresh !== undefined) throw new CliError("Codex options require a Codex client command");
  if (command === "start" || command === "serve") {
    if (!values.vault || !values["master-key"]) throw new CliError("Vault and master key paths are required for unified identity");
    if (!lstatSync(directory, { throwIfNoEntry: false })) await createPrivateDirectory(directory);
  }
  await verifyPrivateDirectory(directory);
  if (command === "repair") {
    for (const name of Object.keys(values)) if (!["data-dir", "client"].includes(name)) throw new CliError("Repair uses the saved original service configuration; only data-dir and client are accepted");
    const result = await repairInstallation(directory, values.client === "codex" ? "codex" : "mcp");
    output(result); if (result.state === "needs_action") process.exitCode = 1;
    return;
  }
  if (command === "data-upgrade") {
    for (const name of Object.keys(values)) if (name !== "data-dir") throw new CliError("Data upgrade accepts only data-dir");
    const { upgradeLocalAccountDatabases, LocalAccountUpgradeError } = await import("./storage/local-account-upgrade.ts");
    const lease = await acquireInstanceLock(directory);
    try {
      const result = await upgradeLocalAccountDatabases(lease).catch(error => {
        if (error instanceof LocalAccountUpgradeError) throw new CliError(error.message, error.code);
        throw error;
      });
      output({ service: "onecrew_lite", ...result });
    }
    finally { await lease.close(); }
    return;
  }
  if (command === "mcp") {
    const { serveMcpStdio } = await import("./service/mcp-adapter.ts");
    await serveMcpStdio(directory);
    return;
  }
  if (command === "workspace") {
    const runtime = await readVerifiedRuntime(directory);
    const response = await fetch(`${runtime.url}/_local/workspace`, { method: "POST", headers: { authorization: `Bearer ${runtime.control_token}` }, redirect: "error", signal: AbortSignal.timeout(15_000) });
    if (!response.ok) throw new CliError(`Workspace access was rejected with HTTP ${response.status}`);
    const { z } = await import("zod");
    const result = z.object({ url: z.url(), instance_id: z.uuid() }).strict().parse(await response.json());
    const url = new URL(result.url);
    if (result.instance_id !== runtime.instance_id || url.origin !== runtime.url || url.pathname !== "/" || url.search || !/^#onecrew_ticket=[A-Za-z0-9_-]+$/u.test(url.hash)) throw new CliError("Workspace ticket does not belong to the running service");
    output({ service: "onecrew_lite", ...result });
    return;
  }
  if (command === "status") {
    const owner = await runningOwner(directory);
    if (!owner) {
      output({ service: "onecrew_lite", state: "stopped", production_ready: false });
    } else output(publicStatus(owner));
    return;
  }
  if (command === "stop") {
    const runtime = await readVerifiedRuntime(directory);
    const originalDescriptor = lstatSync(join(directory, "server.json"));
    const response = await fetch(`${runtime.url}/_local/shutdown`, { method: "POST", headers: { authorization: `Bearer ${runtime.control_token}` }, redirect: "error", signal: AbortSignal.timeout(5000) });
    if (response.status !== 202) throw new CliError(`Shutdown was rejected with HTTP ${response.status}`);
    const deadline = performance.now() + 120_000;
    for (;;) {
      const info = lstatSync(join(directory, "server.json"), { throwIfNoEntry: false });
      if (!info) {
        const released = await acquireInstanceLock(directory).then(async lease => { await lease.close(); return true; }, error => {
          if (error && typeof error === "object" && "code" in error && error.code === "runtime.already_owned") return false;
          throw error;
        });
        if (released) break;
      } else if (info.dev !== originalDescriptor.dev || info.ino !== originalDescriptor.ino) {
        if ((await readRuntimeDescriptor(directory)).instance_id !== runtime.instance_id) throw new CliError("Runtime owner changed while waiting for shutdown");
      }
      if (performance.now() >= deadline) throw new CliError("Service shutdown did not finish before its deadline");
      await delay(100);
    }
    if (process.platform === "win32") await removeWindowsServiceHost(directory);
    output({ service: "onecrew_lite", state: "stopped", instance_id: runtime.instance_id, production_ready: false });
    return;
  }
  if (command === "serve") {
    const { unifiedResources } = await import("./application/unified-resources.ts");
    const runtime = await startLocalRuntime(directory, unifiedResources({ vaultPath: resolve(values.vault!), masterKeyPath: resolve(values["master-key"]!), ...(webAssetsDirectory ? { webAssetsDirectory } : {}), ...(libraryWebAssetsDirectory ? { libraryWebAssetsDirectory } : {}) }), legacyPort);
    const stop = () => { void runtime.close(); };
    process.once("SIGINT", stop);
    process.once("SIGTERM", stop);
    try {
      await rememberServiceStart({ format: 1, directory: runtime.descriptor.data_dir, vaultPath: resolve(values.vault!), masterKeyPath: resolve(values["master-key"]!), port: runtime.descriptor.port,
        ...(values["web-assets"] && webAssetsDirectory ? { webAssetsDirectory } : {}), ...(values["library-web-assets"] && libraryWebAssetsDirectory ? { libraryWebAssetsDirectory } : {}) });
      output(publicStatus(runtime.descriptor));
      await runtime.stopped;
    } finally {
      process.removeListener("SIGINT", stop);
      process.removeListener("SIGTERM", stop);
      await runtime.close();
    }
    return;
  }
  const owner = await runningOwner(directory);
  if (owner) {
    if (packaged) {
      const response = await fetch(`${owner.url}/_local/status`, { headers: { authorization: `Bearer ${owner.control_token}` }, redirect: "error", signal: AbortSignal.timeout(5000) });
      if (!response.ok) throw new CliError("Running service build could not be verified", "service.build_verification_failed");
      const expected = createHash("sha256").update(await readFile(toNamespacedPath(runtimeExecutablePath()))).digest("hex");
      if ((await response.json()).resource_identity?.executable_sha256 !== expected) throw new CliError("Running service uses a different build; stop that service before starting this build", "service.build_update_required");
    }
    if (legacyPort !== undefined && owner.port !== legacyPort) throw new CliError("Running service port conflicts with the requested legacy port");
    if (webAssetsDirectory) {
      const { WebAssets } = await import("./service/web-assets.ts");
      const expected = await WebAssets.load(webAssetsDirectory);
      const response = await fetch(`${owner.url}/_local/status`, { headers: { authorization: `Bearer ${owner.control_token}` }, redirect: "error", signal: AbortSignal.timeout(5000) });
      if (!response.ok || (await response.json()).web_assets_sha256 !== expected.fingerprint()) throw new CliError("Running web assets differ from the requested build; restart the unified service");
    }
    if (libraryWebAssetsDirectory) {
      const { WebAssets } = await import("./service/web-assets.ts");
      const expected = await WebAssets.load(libraryWebAssetsDirectory);
      const response = await fetch(`${owner.url}/_local/library/status`, { headers: { authorization: `Bearer ${owner.control_token}` }, redirect: "error", signal: AbortSignal.timeout(5000) });
      if (!response.ok || (await response.json()).web_assets_sha256 !== expected.fingerprint()) throw new CliError("Running Library web assets differ from the requested build; restart the unified service");
    }
    output(publicStatus(owner));
    return;
  }
  const priorDescriptor = lstatSync(join(directory, "server.json"), { throwIfNoEntry: false });
  const logPath = join(directory, "service.log");
  if (lstatSync(logPath, { throwIfNoEntry: false })) await verifyPrivateFile(logPath);
  const log = await open(logPath, "a", 0o600);
  try {
    await verifyPrivateFile(logPath);
    const childArgs = [...(packaged ? [] : [fileURLToPath(import.meta.url)]), "serve", "--data-dir", directory, "--vault", resolve(values.vault!), "--master-key", resolve(values["master-key"]!)];
    if (legacyPort !== undefined) childArgs.push("--legacy-port", String(legacyPort));
    if (values["web-assets"] && webAssetsDirectory) childArgs.push("--web-assets", webAssetsDirectory);
    if (values["library-web-assets"] && libraryWebAssetsDirectory) childArgs.push("--library-web-assets", libraryWebAssetsDirectory);
    let failure: Error | undefined;
    const child = process.platform === "win32" ? undefined : spawn(process.execPath, childArgs, { detached: true, windowsHide: true, stdio: ["ignore", log.fd, log.fd] });
    if (child) {
      child.once("error", error => { failure = error; });
      child.once("exit", code => { failure = new CliError(`Unified service exited before readiness (${String(code)})`); });
      child.unref();
    } else await startWindowsServiceHost(packaged ? await retainWindowsServiceBinary(runtimeExecutablePath(), directory) : process.execPath, childArgs, directory);
    const deadline = performance.now() + 60_000;
    for (;;) {
      if (failure) throw failure;
      const currentDescriptor = lstatSync(join(directory, "server.json"), { throwIfNoEntry: false });
      if (currentDescriptor && (!priorDescriptor || currentDescriptor.dev !== priorDescriptor.dev || currentDescriptor.ino !== priorDescriptor.ino)) {
        output(publicStatus(await readVerifiedRuntime(directory)));
        return;
      }
      if (performance.now() >= deadline) {
        child?.kill();
        if (process.platform === "win32") await abortWindowsServiceHost(directory);
        throw new CliError("Unified service did not become ready before its deadline");
      }
      await delay(100);
    }
  } finally { await log.close(); }
}

export async function runCliMain(args: string[]): Promise<void> {
  if (process.platform !== "win32") process.umask(0o077);
  await runCli(args).then(undefined, error => {
    const failure = { service: "onecrew_lite", state: "failed", code: error && typeof error === "object" && "code" in error ? String(error.code) : "cli.failed", ...(error instanceof CliError ? { message: error.message } : {}), ...(process.env.ONECREW_LITE_DIAGNOSTICS === "1" && error && typeof error === "object" && "syscall" in error && "path" in error ? { diagnostic: { operation: String(error.syscall), path: String(error.path) } } : {}) };
    if (args[0] === "mcp") process.stderr.write(`${JSON.stringify(failure)}\n`);
    else output(failure);
    process.exitCode = 1;
  });
}
if (!getPackagedRuntime() && process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) await runCliMain(process.argv.slice(2));
