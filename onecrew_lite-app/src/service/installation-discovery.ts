import { execFile } from "node:child_process";
import { lstat, readFile, readdir, realpath } from "node:fs/promises";
import { homedir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";
import { promisify } from "node:util";
import { z } from "zod";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { verifyPrivateDirectory } from "../storage/private-backup.ts";

const exists = (path: string) => lstat(path).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
const key = (path: string) => resolve(path).toLowerCase();
export class InstallationDiscoveryError extends Error {
  readonly code = "installation.multiple_data_directories";
  constructor(directories: readonly string[]) { super(`Multiple OneCrew data directories: ${JSON.stringify(directories)}. Select --data-dir explicitly.`); }
}

export async function discoverInstallation(explicit?: string, codexHome = process.env.CODEX_HOME ?? join(homedir(), ".codex")) {
  if (process.platform !== "win32" || !process.env.LOCALAPPDATA || !process.env.SystemRoot) throw new Error("installation.windows_environment_required");
  const base = process.env.LOCALAPPDATA;
  const defaultDirectory = join(base, "OneCrewLite", "TypeScript");
  const candidates = new Map<string, string>();
  const staleRegistrations: string[] = [];
  const add = (path: string) => { if (!isAbsolute(path)) throw new Error("installation.absolute_path_required"); candidates.set(key(path), resolve(path)); };
  if (explicit) add(resolve(explicit));
  else {
    for (const path of [defaultDirectory, join(base, "OneCrewLite", "TypeScript-Test"), join(base, "onecrew_lite", "unified-data")]) if (await exists(path)) add(path);
    const script = runtimeAssetPath("storage/windows-installations.ps1", new URL("../storage/windows-installations.ps1", import.meta.url));
    const result = await promisify(execFile)(join(process.env.SystemRoot, "System32/WindowsPowerShell/v1.0/powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script], { windowsHide: true, timeout: 30_000, maxBuffer: 256 * 1024 });
    for (const path of z.array(z.string()).parse(JSON.parse(result.stdout))) {
      const host = join(path, ".windows-host");
      if (!await exists(path)) { staleRegistrations.push(path); continue; }
      if (!await exists(host)) throw new Error("installation.registered_directory_missing");
      for (const name of await readdir(host)) {
        if (!name.endsWith(".json")) continue;
        const config = JSON.parse(await readFile(join(host, name), "utf8"));
        if (config.format === 2 && typeof config.environment?.LOCALAPPDATA === "string" && key(config.environment.LOCALAPPDATA) === key(base)) add(path);
      }
    }
    const cache = join(codexHome, "plugins", "cache", "onecrew_lite-cli", "onecrew_lite");
    if (await exists(cache)) for (const version of await readdir(cache, { withFileTypes: true })) {
      if (!version.isDirectory() || version.isSymbolicLink()) continue;
      const manifest = join(cache, version.name, ".mcp.json");
      if (!await exists(manifest)) continue;
      const config = JSON.parse(await readFile(manifest, "utf8"));
      const directory = config.mcpServers?.onecrew_lite?.env?.ONECREW_LITE_MCP_DATA_DIR;
      if (typeof directory === "string" && await exists(directory)) add(directory);
    }
  }
  const found: string[] = [];
  for (const path of candidates.values()) {
    if (!await exists(path)) continue;
    const entries = await readdir(path);
    if (!entries.length) continue;
    await verifyPrivateDirectory(path);
    if (!await exists(join(path, "vault.sqlite"))) throw new Error(`installation.unrecognized_data_directory:${path}`);
    const physical = await realpath(path);
    if (!found.some(value => key(value) === key(physical))) found.push(physical);
  }
  if (found.length > 1) throw new InstallationDiscoveryError(found);
  const directory = found[0] ?? (explicit ? resolve(explicit) : defaultDirectory);
  const keys: string[] = [];
  for (const name of ["masterkey", "master.key"]) if (await exists(join(directory, name))) keys.push(join(directory, name));
  if (found.length && keys.length !== 1) throw new Error(`installation.master_key_ambiguous:${directory}`);
  return { state: found.length ? "existing" : "new", data_directory: directory, vault_path: join(directory, "vault.sqlite"), master_key_path: keys[0] ?? join(directory, "masterkey"), stale_registrations: staleRegistrations };
}
