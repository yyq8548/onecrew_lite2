import { createHash } from "node:crypto";
import { copyFile, lstat, readdir, readFile } from "node:fs/promises";
import { constants } from "node:fs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { join } from "node:path";
import { createPrivateDirectory, verifyPrivateTree } from "../../storage/private-backup.ts";
import { runtimeAssetPath } from "../../packaging/runtime-context.ts";

async function verifyBrowserSource(source: string): Promise<void> {
  if (process.platform !== "win32") { await verifyPrivateTree(source); return; }
  if (!process.env.SystemRoot) throw new Error("browser.windows_system_root_missing");
  const result = await promisify(execFile)(join(process.env.SystemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", runtimeAssetPath("browser/browser-profile-acl.ps1", new URL("./browser-profile-acl.ps1", import.meta.url)), "-Root", source], { windowsHide: true, timeout: 120000, maxBuffer: 64 * 1024 });
  if (result.stdout.trim() !== "browser-private-tree-verified") throw new Error("browser.profile_acl_invalid");
}

// 重连始终操作独立副本；旧浏览器目录和凭据在发布前保持原样。
export async function copyOwnedBrowserProfile(source: string, destination: string, signal: AbortSignal): Promise<void> {
  await verifyBrowserSource(source); const observed: { path: string; hash: string; dev: bigint; ino: bigint }[] = [];
  async function copy(from: string, to: string): Promise<void> {
    signal.throwIfAborted(); await createPrivateDirectory(to);
    const names = (await readdir(from)).sort();
    for (const name of names) {
      const path = join(from, name), target = join(to, name), before = await lstat(path, { bigint: true });
      if (before.isSymbolicLink()) throw new Error("browser.profile_symlink_rejected");
      if (before.isDirectory()) await copy(path, target);
      else {
        if (!before.isFile() || before.nlink !== 1n) throw new Error("browser.profile_special_file_rejected");
        const hash = createHash("sha256").update(await readFile(path)).digest("hex");
        await copyFile(path, target, constants.COPYFILE_EXCL);
        const after = await lstat(path, { bigint: true });
        if (after.dev !== before.dev || after.ino !== before.ino || after.mtimeNs !== before.mtimeNs || after.ctimeNs !== before.ctimeNs || createHash("sha256").update(await readFile(target)).digest("hex") !== hash) throw new Error("browser.profile_changed_during_copy");
        observed.push({ path, hash, dev: before.dev, ino: before.ino });
      }
    }
    if (JSON.stringify((await readdir(from)).sort()) !== JSON.stringify(names)) throw new Error("browser.profile_changed_during_copy");
  }
  await copy(source, destination); await verifyPrivateTree(destination); await verifyBrowserSource(source);
  for (const item of observed) { signal.throwIfAborted(); const current = await lstat(item.path, { bigint: true }); if (!current.isFile() || current.isSymbolicLink() || current.nlink !== 1n || current.dev !== item.dev || current.ino !== item.ino || createHash("sha256").update(await readFile(item.path)).digest("hex") !== item.hash) throw new Error("browser.profile_changed_during_copy"); }
}
