import { createHash, randomUUID } from "node:crypto";
import { lstat, mkdir, readFile, readdir, rename, writeFile, chmod, realpath } from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory } from "../storage/private-backup.ts";
import { runtimeCompatibility, type ClientPluginPackage } from "./contracts.ts";

export const codexPluginId = "onecrew_lite";
export const codexMarketplaceId = "onecrew_lite-cli";
export const codexPluginReference = `${codexPluginId}@${codexMarketplaceId}`;
const baseVersion = "0.7.1";
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const samePath = (a: string, b: string) => process.platform === "win32" ? a.toLowerCase() === b.toLowerCase() : a === b;
export interface CodexPackageOptions { executable: string; runtimeVersion: string; dataDirectory: string; pluginDirectory: string; codexHome: string }

export async function ensurePrivateDirectory(path: string): Promise<string> {
  const exists = await lstat(path).then(() => true, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return false; throw error; });
  return exists ? verifyPrivateDirectory(path) : createPrivateDirectory(path);
}
export async function readPluginTree(root: string): Promise<Map<string, Buffer>> {
  const canonical = await realpath(root);
  if (!samePath(canonical, resolve(root))) throw new Error("codex_plugin_tree_alias");
  const files = new Map<string, Buffer>();
  async function visit(directory: string): Promise<void> {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const path = join(directory, entry.name), before = await lstat(path, { bigint: true });
      if (before.isSymbolicLink()) throw new Error("codex_plugin_tree_symlink");
      if (before.isDirectory()) { await visit(path); continue; }
      if (!before.isFile() || before.nlink !== 1n || before.size > 2n * 1024n * 1024n * 1024n) throw new Error("codex_plugin_tree_file_invalid");
      const bytes = await readFile(path), after = await lstat(path, { bigint: true });
      if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new Error("codex_plugin_tree_changed");
      files.set(relative(canonical, path).replaceAll("\\", "/"), bytes);
    }
  }
  await visit(canonical); return files;
}
export function pluginDigestVersion(files: ReadonlyMap<string, Uint8Array>, base = baseVersion): string {
  const hash = createHash("sha256");
  for (const path of [...files.keys()].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))) {
    const name = Buffer.from(path), bytes = files.get(path)!, length = Buffer.alloc(8);
    length.writeBigUInt64LE(BigInt(name.length)); hash.update(length).update(name);
    length.writeBigUInt64LE(BigInt(bytes.length)); hash.update(length).update(bytes);
  }
  return `${base}+codex.${hash.digest("hex")}`;
}
function binaryTarget(bytes: Buffer): string {
  if (process.platform === "win32" && process.arch === "x64") {
    if (bytes.length < 64 || bytes.toString("ascii", 0, 2) !== "MZ") throw new Error("codex_plugin_executable_target_mismatch");
    const offset = bytes.readUInt32LE(60);
    if (offset + 26 > bytes.length || !bytes.subarray(offset, offset + 6).equals(Buffer.from([80, 69, 0, 0, 100, 134])) || bytes.readUInt16LE(offset + 24) !== 0x20b) throw new Error("codex_plugin_executable_target_mismatch");
    return "x86_64-pc-windows-msvc";
  }
  if (process.platform === "darwin" && ["arm64", "x64"].includes(process.arch)) {
    if (bytes.length < 32 || bytes.readUInt32LE(0) !== 0xfeedfacf || bytes.readUInt32LE(4) !== (process.arch === "arm64" ? 0x100000c : 0x1000007) || bytes.readUInt32LE(12) !== 2) throw new Error("codex_plugin_executable_target_mismatch");
    return process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  }
  throw new Error("codex_plugin_unsupported_target");
}
export async function codexMarketplaceRoot(options: Pick<CodexPackageOptions, "pluginDirectory" | "codexHome">): Promise<string> {
  if (!isAbsolute(options.pluginDirectory) || !isAbsolute(options.codexHome)) throw new Error("codex_plugin_absolute_directory_required");
  await ensurePrivateDirectory(options.pluginDirectory);
  return ensurePrivateDirectory(await codexMarketplacePath(options));
}
export async function codexMarketplacePath(options: Pick<CodexPackageOptions, "pluginDirectory" | "codexHome">): Promise<string> {
  if (!isAbsolute(options.pluginDirectory) || !isAbsolute(options.codexHome)) throw new Error("codex_plugin_absolute_directory_required");
  const home = await realpath(options.codexHome).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return resolve(options.codexHome); throw error; });
  return join(options.pluginDirectory, sha(home));
}
export async function atomicPluginJson(path: string, value: unknown): Promise<void> {
  const temporary = `${path}.${randomUUID()}.pending`;
  const current = await lstat(path).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
  if (current && (!current.isFile() || current.isSymbolicLink() || current.nlink !== 1)) throw new Error("codex_plugin_destination_invalid");
  await writeFile(temporary, JSON.stringify(value, null, 2), { flag: "wx", mode: 0o600 }); await rename(temporary, path);
}
export async function prepareCodexPlugin(options: CodexPackageOptions): Promise<ClientPluginPackage> {
  for (const path of [options.executable, options.dataDirectory]) if (!isAbsolute(path)) throw new Error("codex_plugin_absolute_path_required");
  if (options.runtimeVersion !== runtimeCompatibility.minimum || options.runtimeVersion !== runtimeCompatibility.maximum) throw new Error("codex_plugin_runtime_update_required");
  await verifyPrivateDirectory(options.dataDirectory);
  const sourceInfo = await lstat(options.executable, { bigint: true });
  if (!sourceInfo.isFile() || sourceInfo.isSymbolicLink() || sourceInfo.nlink !== 1n) throw new Error("codex_plugin_executable_invalid");
  const binary = await readFile(options.executable), target = binaryTarget(binary), binaryHash = sha(binary);
  const after = await lstat(options.executable, { bigint: true });
  if (sourceInfo.dev !== after.dev || sourceInfo.ino !== after.ino || sourceInfo.size !== after.size || sourceInfo.mtimeNs !== after.mtimeNs || sourceInfo.ctimeNs !== after.ctimeNs) throw new Error("codex_plugin_executable_changed");
  const files = await readPluginTree(runtimeAssetPath("client-plugin", new URL("./plugin-template/", import.meta.url)));
  const manifestPath = ".codex-plugin/plugin.json", manifestBytes = files.get(manifestPath);
  if (!manifestBytes) throw new Error("codex_plugin_manifest_missing");
  const manifest = parseApplicationJson(manifestBytes);
  if (!manifest || typeof manifest !== "object" || !("name" in manifest) || manifest.name !== codexPluginId || !("version" in manifest)) throw new Error("codex_plugin_manifest_invalid");
  manifest.version = baseVersion; files.set(manifestPath, Buffer.from(JSON.stringify(manifest, null, 2)));
  const filename = process.platform === "win32" ? "onecrew_lite.exe" : "onecrew_lite";
  let connection: Record<string, unknown> = { command: `./bin/${filename}`, cwd: ".", args: ["mcp", "--stdio", "--data-dir", options.dataDirectory] };
  if (process.platform === "win32") {
    if (/["\r\n]/u.test(options.dataDirectory)) throw new Error("codex_plugin_windows_directory_invalid");
    connection = { command: "cmd.exe", cwd: ".", args: ["/d", "/v:off", "/c", ".\\bin\\onecrew_lite-mcp.cmd"], env: { ONECREW_LITE_MCP_DATA_DIR: options.dataDirectory } };
    files.set("bin/onecrew_lite-mcp.cmd", Buffer.from('@echo off\r\nsetlocal DisableDelayedExpansion\r\n"%~dp0onecrew_lite.exe" mcp --stdio --data-dir "%ONECREW_LITE_MCP_DATA_DIR%"\r\nexit /b %errorlevel%\r\n'));
  }
  files.set(".mcp.json", Buffer.from(JSON.stringify({ mcpServers: { onecrew_lite: connection } }, null, 2)));
  files.set(".onecrew-runtime.json", Buffer.from(JSON.stringify({ format: 1, runtime_version: options.runtimeVersion, compatibility: runtimeCompatibility, target, binary_sha256: binaryHash, data_directory: options.dataDirectory }, null, 2)));
  files.set(`bin/${filename}`, binary);
  const version = pluginDigestVersion(files); manifest.version = version; files.set(manifestPath, Buffer.from(JSON.stringify(manifest, null, 2)));
  const root = await codexMarketplaceRoot(options), generations = await ensurePrivateDirectory(join(root, "generations")), generation = join(generations, version), pluginRoot = join(generation, "plugins", codexPluginId);
  const existing = await lstat(generation).then(() => true, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return false; throw error; });
  if (!existing) {
    const pending = await ensurePrivateDirectory(join(generations, `pending-${randomUUID()}`));
    for (const [path, bytes] of files) { const output = join(pending, "plugins", codexPluginId, ...path.split("/")); await mkdir(join(output, ".."), { recursive: true, mode: 0o700 }); await writeFile(output, bytes, { flag: "wx", mode: path === `bin/${filename}` ? 0o700 : 0o600 }); }
    if (process.platform !== "win32") await chmod(join(pending, "plugins", codexPluginId, "bin", filename), 0o700);
    await rename(pending, generation);
  }
  await verifyCodexPlugin(pluginRoot, version, binaryHash);
  await mkdir(join(root, ".agents", "plugins"), { recursive: true, mode: 0o700 });
  await atomicPluginJson(join(root, ".agents", "plugins", "marketplace.json"), { name: codexMarketplaceId, interface: { displayName: "OneCrew CLI" }, plugins: [{ name: codexPluginId, source: { source: "local", path: `./generations/${version}/plugins/${codexPluginId}` }, policy: { installation: "AVAILABLE", authentication: "ON_INSTALL" }, category: "Productivity" }] });
  return { marketplace_root: root, plugin_root: pluginRoot, version, target, binary_sha256: binaryHash, runtime_version: options.runtimeVersion };
}
export async function verifyCodexPlugin(root: string, version: string, expectedBinary?: string): Promise<void> {
  if (!/^0\.7\.1\+codex\.[a-f0-9]{64}$/u.test(version)) throw new Error("codex_plugin_version_invalid");
  const files = await readPluginTree(root), manifestBytes = files.get(".codex-plugin/plugin.json");
  if (!manifestBytes) throw new Error("codex_plugin_manifest_missing");
  const manifest = parseApplicationJson(manifestBytes);
  if (!manifest || typeof manifest !== "object" || !("name" in manifest) || manifest.name !== codexPluginId || !("version" in manifest) || manifest.version !== version) throw new Error("codex_plugin_manifest_mismatch");
  manifest.version = baseVersion; files.set(".codex-plugin/plugin.json", Buffer.from(JSON.stringify(manifest, null, 2)));
  if (pluginDigestVersion(files) !== version) throw new Error("codex_plugin_content_digest_mismatch");
  const binary = files.get(`bin/${process.platform === "win32" ? "onecrew_lite.exe" : "onecrew_lite"}`);
  if (!binary) throw new Error("codex_plugin_binary_missing");
  binaryTarget(binary);
  if (expectedBinary !== undefined && sha(binary) !== expectedBinary) throw new Error("codex_plugin_runtime_generation_mismatch");
}
