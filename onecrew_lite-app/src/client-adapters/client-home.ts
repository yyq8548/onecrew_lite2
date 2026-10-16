import { lstat, realpath } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { join, relative, resolve } from "node:path";
import { createPrivateDirectory, verifyPrivateDirectory } from "../storage/private-backup.ts";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";

export async function ensureClientHome(path: string): Promise<void> {
  const absolute = resolve(path);
  const info = await lstat(absolute).then(value => value, error => { if (error.code === "ENOENT") return null; throw error; });
  if (!info) { await createPrivateDirectory(absolute); return; }
  if (!info.isDirectory() || info.isSymbolicLink() || relative(absolute, await realpath(absolute)) !== "") throw new Error("client_home_path_invalid");
  if (process.platform !== "win32") { await verifyPrivateDirectory(absolute); return; }
  if (!process.env.SystemRoot) throw new Error("client_home_system_root_missing");
  // Codex 管理已有目录；允许继承和只读沙箱权限，拒绝其他身份写入，不修改其 DACL。
  const result = await promisify(execFile)(join(process.env.SystemRoot, "System32/WindowsPowerShell/v1.0/powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", runtimeAssetPath("storage/client-home.ps1", new URL("../storage/client-home.ps1", import.meta.url)), "-Directory", absolute], { windowsHide: true, timeout: 30000, maxBuffer: 8192 });
  if (result.stdout.trim() !== "client-home-verified") throw new Error("client_home_verification_failed");
}
