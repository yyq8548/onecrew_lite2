import { execFile } from "node:child_process";
import { lstat, mkdir, readdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { promisify } from "node:util";
import { verifyWindowsPrivatePath } from "./private-verification-worker.ts";

const execute = promisify(execFile);
const script = runtimeAssetPath("storage/private-backup.ps1", new URL("./private-backup.ps1", import.meta.url));

export async function createPrivateDirectory(path: string): Promise<string> {
  const absolute = resolve(path);
  if (process.platform === "win32") await privateWindowsBackupDirectory("Create", absolute);
  else {
    await verifyUnixAncestors(dirname(absolute));
    await mkdir(absolute, { mode: 0o700 });
  }
  return verifyPrivateDirectory(absolute);
}

export async function verifyPrivateDirectory(path: string): Promise<string> {
  const absolute = resolve(path);
  if (process.platform === "win32") await privateWindowsBackupDirectory("Verify", absolute);
  else {
    await verifyUnixAncestors(absolute);
    const info = await lstat(absolute);
    if (!process.getuid || info.uid !== process.getuid() || (info.mode & 0o077) !== 0) {
      throw new Error("Private directory must belong to the current user with no group or other permissions");
    }
  }
  return absolute;
}

async function verifyUnixAncestors(path: string): Promise<void> {
  let current = path;
  while (true) {
    const info = await lstat(current);
    if (info.isSymbolicLink() || !info.isDirectory()) throw new Error("Private directory path contains a symbolic link or non-directory");
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

export async function verifyPrivateFile(path: string): Promise<void> {
  const absolute = resolve(path);
  await verifyPrivateDirectory(dirname(absolute));
  if (process.platform === "win32") await privateWindowsBackupDirectory("Verify", dirname(absolute), absolute);
  else {
    const info = await lstat(absolute);
    if (!info.isFile() || info.isSymbolicLink() || !process.getuid || info.uid !== process.getuid() || (info.mode & 0o077) !== 0) {
      throw new Error("Private file must be regular, owned by the current user, and inaccessible to group or other users");
    }
  }
}

export async function verifyPrivateTree(path: string): Promise<void> {
  const absolute = resolve(path);
  if (process.platform === "win32") await privateWindowsBackupDirectory("VerifyTree", absolute);
  else {
    await verifyPrivateDirectory(absolute);
    for (const entry of await readdir(absolute, { withFileTypes: true })) {
      const child = join(absolute, entry.name);
      if (entry.isSymbolicLink()) throw new Error("Private tree contains a symbolic link");
      if (entry.isDirectory()) await verifyPrivateTree(child);
      else await verifyPrivateFile(child);
    }
  }
}

export async function privateWindowsBackupDirectory(operation: "Create" | "Verify" | "VerifyTree", directory: string, file?: string): Promise<void> {
  if (process.platform !== "win32") throw new Error("Windows private ACL adapter requires Windows");
  const systemRoot = process.env.SystemRoot;
  if (!systemRoot) throw new Error("Windows system directory is unavailable");
  if (operation === "Verify" || operation === "VerifyTree") {
    await verifyWindowsPrivatePath(systemRoot, script, { operation, directory, ...(file === undefined ? {} : { file }) });
    return;
  }
  const args = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script, "-Operation", operation, "-Directory", directory];
  if (file !== undefined) args.push("-File", file);
  const { stdout } = await execute(join(systemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), args, {
    windowsHide: true, timeout: 30_000, maxBuffer: 64 * 1024,
  });
  if (stdout.trim() !== "private-acl-verified") throw new Error("Windows private backup ACL verification did not complete");
}
