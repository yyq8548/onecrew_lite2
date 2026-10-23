import { execFile } from "node:child_process";
import { lstat, realpath } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { promisify } from "node:util";
import { verifyPrivateFile, verifyPrivateDirectory } from "./private-backup.ts";

const execute = promisify(execFile);

// 只读源允许原 Rust OW/SYSTEM/Administrators 精确 DACL；新副本继续使用 private-backup。
export async function verifyLegacyPrivateFile(path: string): Promise<void> {
  await verifyLegacySource(path, "File");
}
export async function verifyLegacyPrivateDirectory(path: string): Promise<string> {
  await verifyLegacySource(path, "Directory");
  return resolve(path);
}
async function verifyLegacySource(path: string, kind: "File" | "Directory"): Promise<void> {
  const absolute = resolve(path);
  if (process.platform !== "win32") { if (kind === "File") await verifyPrivateFile(absolute); else await verifyPrivateDirectory(absolute); return; }
  const before = await lstat(absolute, { bigint: true });
  if ((kind === "File" ? !before.isFile() || before.nlink !== 1n : !before.isDirectory()) || before.isSymbolicLink() || relative(absolute, await realpath(absolute)) !== "") throw new Error("migration.legacy_source_file_invalid");
  const system = process.env.SystemRoot;
  if (!system) throw new Error("Windows system directory is unavailable");
  const result = await execute(join(system, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"),
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", runtimeAssetPath("storage/legacy-private-source.ps1", new URL("./legacy-private-source.ps1", import.meta.url)), "-Path", absolute, "-Kind", kind],
    { windowsHide: true, timeout: 30_000, maxBuffer: 8192 });
  if (result.stdout.trim() !== "legacy-source-acl-verified") throw new Error("migration.legacy_source_acl_invalid");
  const after = await lstat(absolute, { bigint: true });
  if (after.isSymbolicLink() || after.ino !== before.ino || after.dev !== before.dev || (kind === "File" && after.nlink !== 1n) || after.ctimeNs !== before.ctimeNs) throw new Error("migration.legacy_source_file_changed");
}
