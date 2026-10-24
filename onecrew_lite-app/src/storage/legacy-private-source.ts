import { execFile } from "node:child_process";
import { lstat, realpath } from "node:fs/promises";
import { dirname, extname, join, relative, resolve } from "node:path";
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
  // Mac 旧版在私有目录内创建的 SQLite 为 0644；只读源允许此权限，密钥和新副本仍须私有。
  if (process.platform === "darwin" && kind === "File" && [".db", ".sqlite"].includes(extname(absolute))) {
    await verifyPrivateDirectory(dirname(absolute));
    const file = await lstat(absolute, { bigint: true });
    if (!file.isFile() || file.isSymbolicLink() || file.nlink !== 1n || file.uid !== BigInt(process.getuid!()) || (file.mode & 0o022n) !== 0n || relative(absolute, await realpath(absolute)) !== "") throw new Error("migration.legacy_source_file_invalid");
    return;
  }
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
