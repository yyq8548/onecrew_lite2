import { createHash } from "node:crypto";
import { lstat } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createPrivateDirectory, verifyPrivateDirectory } from "../../storage/private-backup.ts";

export function managedChromeProfilesDirectory(accountDirectory: string): string {
  if (process.platform !== "win32") return join(accountDirectory, "profiles");
  const scope = createHash("sha256").update(JSON.stringify(["onecrew-managed-chrome-v1", resolve(accountDirectory).toLowerCase()])).digest("hex");
  // Windows 商店宿主会重定向 LocalAppData，使用用户目录避免实际路径再次增长。
  return join(homedir(), ".onecrew-browsers", scope);
}

async function ensurePrivateDirectory(directory: string): Promise<void> {
  const exists = await lstat(directory).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
  if (exists) await verifyPrivateDirectory(directory);
  else await createPrivateDirectory(directory);
}

export async function prepareManagedChromeProfilesDirectory(accountDirectory: string): Promise<string> {
  const directory = managedChromeProfilesDirectory(accountDirectory);
  if (process.platform === "win32") await ensurePrivateDirectory(dirname(directory));
  await ensurePrivateDirectory(directory);
  return directory;
}
