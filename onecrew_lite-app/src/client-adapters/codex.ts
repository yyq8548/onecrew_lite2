import { createHash } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { homedir } from "node:os";
import { isAbsolute, join, resolve, toNamespacedPath } from "node:path";
import { parse } from "smol-toml";
import { z } from "zod";
import { localServiceStatus, type LocalServiceStatus } from "@onecrew/runtime-contracts/status";
import type { ClientAdapter, ClientConfiguration, ClientPluginPackage } from "./contracts.ts";
import { runtimeCompatibility } from "./contracts.ts";
import { codexMarketplaceId, codexMarketplacePath, codexMarketplaceRoot, codexPluginId, codexPluginReference, atomicPluginJson, ensurePrivateDirectory, prepareCodexPlugin, verifyCodexPlugin, type CodexPackageOptions } from "./codex-package.ts";
import { CodexCommandError, detectCodex, runCodex } from "./codex-process.ts";
import { acquireInstanceLock } from "../service/instance-lock.ts";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { verifyPrivateDirectory } from "../storage/private-backup.ts";
import { ensureClientHome } from "./client-home.ts";

const marketplaceSource = z.object({ source_type: z.string(), source: z.string() }).passthrough();
const configSchema = z.object({ plugins: z.record(z.string(), z.object({ enabled: z.boolean().optional() }).passthrough()).optional(), marketplaces: z.record(z.string(), marketplaceSource).optional() }).passthrough();
const pluginStatus = localServiceStatus.shape.plugin;
const receipt = z.strictObject({ format: z.literal(1), runtime_version: z.string(), binary_sha256: z.string().regex(/^[a-f0-9]{64}$/u), expected_version: z.string(), status: pluginStatus });
const listSchema = z.object({ installed: z.array(z.object({ name: z.string(), marketplaceName: z.string(), installed: z.boolean(), version: z.string().nullable().optional(), enabled: z.boolean().optional() }).passthrough()).max(10000) }).passthrough();
const ownRetired = ["onecrew_lite-local@onecrew_lite-cli", "onecrew_lite-local@onecrew_lite-local"];
type Status = LocalServiceStatus["plugin"];
export interface CodexAdapterOptions extends CodexPackageOptions { codexExecutable?: string; binarySha256?: string }
const samePath = (a: string, b: string) => process.platform === "win32" ? toNamespacedPath(resolve(a)).toLowerCase() === toNamespacedPath(resolve(b)).toLowerCase() : resolve(a) === resolve(b);
const missing = (error: unknown) => (error as NodeJS.ErrnoException).code === "ENOENT";
function status(state: Status["state"], verification: Status["verification"], diagnostic_code: string | null = null): Status { return { state, verification, installed_version: null, enabled: null, last_verified_at: null, diagnostic_code }; }
async function optionalFile(path: string): Promise<Buffer | null> {
  const before = await lstat(path, { bigint: true }).then(value => value, error => { if (missing(error)) return null; throw error; });
  if (before === null) return null;
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || before.size > 4n * 1024n * 1024n || !samePath(await realpath(path), path)) throw new Error("codex_configuration_file_invalid");
  const bytes = await readFile(path), after = await lstat(path, { bigint: true });
  if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new Error("codex_configuration_file_changed");
  return bytes;
}
export async function readCodexConfiguration(home: string) {
  if (!isAbsolute(home)) throw new Error("codex_home_must_be_absolute");
  const bytes = await optionalFile(join(home, "config.toml"));
  return configSchema.parse(bytes === null ? {} : parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes), { integersAsBigInt: "asNeeded", unsafeKeyBehaviour: "throw", maxDepth: 100 }));
}

export class CodexClientAdapter implements ClientAdapter {
  readonly id = "codex";
  readonly #options: CodexAdapterOptions;
  #binaryHash: string | undefined;
  constructor(options: CodexAdapterOptions) {
    for (const path of [options.executable, options.dataDirectory, options.pluginDirectory, options.codexHome]) if (!isAbsolute(path)) throw new Error("codex_adapter_absolute_paths_required");
    this.#options = Object.freeze({ ...options }); this.#binaryHash = options.binarySha256;
    if (this.#binaryHash !== undefined && !/^[a-f0-9]{64}$/u.test(this.#binaryHash)) throw new Error("codex_adapter_binary_identity_invalid");
  }
  detect() { return detectCodex(this.#options.codexHome, this.#options.codexExecutable); }
  configuration(): ClientConfiguration { return { mcpServers: { onecrew_lite: { command: this.#options.executable, args: ["mcp", "--stdio", "--data-dir", this.#options.dataDirectory] } } }; }
  async #initialize() {
    await verifyPrivateDirectory(this.#options.dataDirectory);
    await ensureClientHome(this.#options.codexHome);
    // 默认安装目录逐层创建；内部显式目录由调用方提供已存在的父目录。
    if (samePath(this.#options.pluginDirectory, join(this.#options.dataDirectory, "client-adapters", "codex"))) await ensurePrivateDirectory(join(this.#options.dataDirectory, "client-adapters"));
    return codexMarketplaceRoot(this.#options);
  }
  async #hash(): Promise<string> { this.#binaryHash ??= createHash("sha256").update(await readFile(this.#options.executable)).digest("hex"); return this.#binaryHash; }
  async prepare(): Promise<ClientPluginPackage> {
    const root = await this.#initialize(); await using lock = { lease: await acquireInstanceLock(root), async [Symbol.asyncDispose]() { await this.lease.close(); } };
    return await prepareCodexPlugin(this.#options);
  }
  async #readback(list: unknown, expected?: ClientPluginPackage): Promise<Status> {
    const installed = listSchema.parse(list).installed.filter(item => item.name === codexPluginId && item.marketplaceName === codexMarketplaceId && item.installed);
    if (installed.length > 1) throw new Error("codex_plugin_readback_ambiguous");
    const item = installed[0];
    if (!item) return { ...status("not_installed", "fresh"), last_verified_at: Math.floor(Date.now() / 1000) };
    if (!item.version || item.enabled === undefined) throw new Error("codex_plugin_schema_unsupported");
    await verifyCodexPlugin(join(this.#options.codexHome, "plugins", "cache", codexMarketplaceId, codexPluginId, item.version), item.version, await this.#hash());
    if (expected && item.version !== expected.version) throw new Error("codex_plugin_install_readback_mismatch");
    return { state: "installed", verification: "fresh", installed_version: item.version, enabled: item.enabled, last_verified_at: Math.floor(Date.now() / 1000), diagnostic_code: null };
  }
  async #registration(root: string): Promise<Status | null> {
    const config = await readCodexConfiguration(this.#options.codexHome), source = config.marketplaces?.[codexMarketplaceId];
    if (source && (source.source_type !== "local" || !samePath(source.source, root))) return status("unavailable", "not_checked", "codex_marketplace_source_relocated");
    if (source && await optionalFile(join(source.source, ".agents", "plugins", "marketplace.json")) === null) return status("unavailable", "not_checked", "codex_marketplace_source_missing");
    return null;
  }
  async status(refresh = false): Promise<Status> {
    const root = await codexMarketplacePath(this.#options), registration = await this.#registration(root);
    if (registration) return registration;
    if (!refresh) {
      const bytes = await optionalFile(join(root, "status.json"));
      if (bytes === null) return status("unknown", "not_checked");
      const stored = receipt.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes), true));
      if (stored.runtime_version !== this.#options.runtimeVersion || stored.binary_sha256 !== await this.#hash()) return { ...stored.status, state: "unavailable", verification: "cached", diagnostic_code: "codex_plugin_update_required" };
      return { ...stored.status, verification: "cached" };
    }
    const detected = await this.detect();
    let verified: Status;
    try { verified = await this.#readback(await runCodex(detected.executable, this.#options.codexHome, ["plugin", "list", "--marketplace", codexMarketplaceId, "--json"])); }
    catch (error) { if (!(error instanceof CodexCommandError)) throw error; return status("unavailable", "fresh", error.code); }
    await this.#initialize();
    await atomicPluginJson(join(root, "status.json"), { format: 1, runtime_version: this.#options.runtimeVersion, binary_sha256: await this.#hash(), expected_version: verified.installed_version ?? "", status: verified });
    return verified;
  }
  async install(): Promise<Status> {
    if (this.#options.runtimeVersion !== runtimeCompatibility.minimum || this.#options.runtimeVersion !== runtimeCompatibility.maximum) throw new Error("codex_plugin_runtime_update_required");
    const root = await this.#initialize(), detected = await this.detect();
    await using lock = { lease: await acquireInstanceLock(root), async [Symbol.asyncDispose]() { await this.lease.close(); } };
    let config = await readCodexConfiguration(this.#options.codexHome);
    const unknownOwn = Object.keys(config.plugins ?? {}).filter(key => key.endsWith(`@${codexMarketplaceId}`) && key !== codexPluginReference && !ownRetired.includes(key));
    if (unknownOwn.length) throw new Error("codex_marketplace_contains_unowned_plugin");
    for (const name of ownRetired) if (config.plugins?.[name] !== undefined) {
      await runCodex(detected.executable, this.#options.codexHome, ["plugin", "remove", name, "--json"]);
      config = await readCodexConfiguration(this.#options.codexHome);
      if (config.plugins?.[name] !== undefined) throw new Error("codex_plugin_identity_change_required");
    }
    const prepared = await prepareCodexPlugin(this.#options), previous = config.marketplaces?.[codexMarketplaceId];
    if (previous && (previous.source_type !== "local" || !samePath(previous.source, prepared.marketplace_root))) await runCodex(detected.executable, this.#options.codexHome, ["plugin", "marketplace", "remove", codexMarketplaceId, "--json"]);
    await runCodex(detected.executable, this.#options.codexHome, ["plugin", "marketplace", "add", prepared.marketplace_root, "--json"]);
    await runCodex(detected.executable, this.#options.codexHome, ["plugin", "add", codexPluginReference, "--json"]);
    const verified = await this.#readback(await runCodex(detected.executable, this.#options.codexHome, ["plugin", "list", "--marketplace", codexMarketplaceId, "--json"]), prepared);
    if (verified.state !== "installed" || verified.enabled !== true) throw new Error("codex_plugin_install_readback_mismatch");
    await atomicPluginJson(join(root, "status.json"), { format: 1, runtime_version: this.#options.runtimeVersion, binary_sha256: prepared.binary_sha256, expected_version: prepared.version, status: verified });
    return verified;
  }
  async uninstall(): Promise<Status> {
    const root = await this.#initialize(), detected = await this.detect();
    await using lock = { lease: await acquireInstanceLock(root), async [Symbol.asyncDispose]() { await this.lease.close(); } };
    const config = await readCodexConfiguration(this.#options.codexHome);
    for (const name of [codexPluginReference, ...ownRetired]) if (config.plugins?.[name] !== undefined) await runCodex(detected.executable, this.#options.codexHome, ["plugin", "remove", name, "--json"]);
    const remaining = await readCodexConfiguration(this.#options.codexHome);
    if ([codexPluginReference, ...ownRetired].some(name => remaining.plugins?.[name] !== undefined)) throw new Error("codex_plugin_uninstall_readback_mismatch");
    const source = remaining.marketplaces?.[codexMarketplaceId];
    if (source?.source_type === "local" && samePath(source.source, root) && !Object.keys(remaining.plugins ?? {}).some(name => name.endsWith(`@${codexMarketplaceId}`))) await runCodex(detected.executable, this.#options.codexHome, ["plugin", "marketplace", "remove", codexMarketplaceId, "--json"]);
    const verified = { ...status("not_installed", "fresh"), last_verified_at: Math.floor(Date.now() / 1000) };
    await atomicPluginJson(join(root, "status.json"), { format: 1, runtime_version: this.#options.runtimeVersion, binary_sha256: await this.#hash(), expected_version: "", status: verified });
    return verified;
  }
}
export function defaultCodexHome(): string { return resolve(process.env.CODEX_HOME ?? join(homedir(), ".codex")); }
