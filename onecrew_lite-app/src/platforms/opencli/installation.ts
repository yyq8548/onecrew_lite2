import { lstat, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { delimiter, isAbsolute, join } from "node:path";
import type { TrustedOpenCliProcess } from "./process.ts";
import { OpenCliError } from "./errors.ts";

export const VERIFIED_OPENCLI_VERSION = "1.8.8";
const MANAGED_NODE_VERSION = "24.21.0";
async function file(path: string): Promise<boolean> {
  return stat(path).then(value => value.isFile(), error => {
    if ((error as NodeJS.ErrnoException).code === "ENOENT" || (error as NodeJS.ErrnoException).code === "ENOTDIR") return false;
    throw error;
  });
}
async function managedInstallation(): Promise<TrustedOpenCliProcess | null> {
  const platform = process.platform === "win32" && process.arch === "x64" ? "win-x64" : process.platform === "darwin" && ["x64", "arm64"].includes(process.arch) ? `darwin-${process.arch}` : null;
  if (!platform) return null;
  const root = process.platform === "win32" ? process.env.LOCALAPPDATA ? join(process.env.LOCALAPPDATA, "onecrew_lite", "opencli", "managed-v1") : null : join(homedir(), ".local", "share", "onecrew_lite", "opencli", "managed-v1");
  if (!root || !isAbsolute(root)) throw new OpenCliError("opencli_unavailable");
  const metadata = await lstat(root).then(value => ({ value }), error => ({ error: error as NodeJS.ErrnoException }));
  if ("error" in metadata) { if (metadata.error.code === "ENOENT") return null; throw new OpenCliError("opencli_unavailable"); }
  if (!metadata.value.isDirectory() || metadata.value.isSymbolicLink()) throw new OpenCliError("opencli_unavailable");
  const nodeRoot = join(root, `node-v${MANAGED_NODE_VERSION}-${platform}`);
  const executable = platform.startsWith("win-") ? join(nodeRoot, "node.exe") : join(nodeRoot, "bin", "node");
  const script = join(root, "cli", "node_modules", "@jackwener", "opencli", "dist", "src", "main.js");
  for (const path of [executable, script]) {
    const checked = await lstat(path).then(value => ({ value }), error => ({ error: error as NodeJS.ErrnoException }));
    if ("error" in checked || !checked.value.isFile() || checked.value.isSymbolicLink()) throw new OpenCliError("opencli_unavailable");
  }
  return Object.freeze({ executable, script });
}

// 原 managed 安装损坏时明确失败；只在其不存在时查找固定系统安装布局。
export async function resolveOpenCliInstallation(): Promise<TrustedOpenCliProcess> {
  const managed = await managedInstallation();
  if (managed) return managed;
  const directories = (process.env.PATH ?? "").split(delimiter).filter(isAbsolute);
  if (process.platform === "win32") {
    let pathNode: string | undefined;
    for (const directory of directories) if (await file(join(directory, "node.exe"))) { pathNode = join(directory, "node.exe"); break; }
    const roots = process.env.LOCALAPPDATA ? [join(process.env.LOCALAPPDATA, "OpenCLIApp"), join(process.env.LOCALAPPDATA, "Programs", "OpenCLIApp")] : [];
    for (const root of roots) for (const resources of [root, join(root, "resources")]) {
      const script = join(resources, "node_modules", "@jackwener", "opencli", "dist", "src", "main.js");
      if (!await file(script)) continue;
      for (const executable of [join(resources, "node.exe"), join(resources, "node_modules", "node", "bin", "node.exe"), join(root, "node.exe"), ...(pathNode ? [pathNode] : [])]) if (await file(executable)) return Object.freeze({ executable, script });
    }
    for (const directory of directories) {
      const script = join(directory, "node_modules", "@jackwener", "opencli", "dist", "src", "main.js");
      if (pathNode && await file(join(directory, "opencli.cmd")) && await file(script)) return Object.freeze({ executable: pathNode, script });
    }
  } else {
    for (const directory of [...directories, join(homedir(), ".local", "bin"), "/usr/local/bin", "/opt/homebrew/bin"]) {
      const executable = join(directory, "opencli");
      const metadata = await stat(executable).then(value => ({ value }), error => ({ error: error as NodeJS.ErrnoException }));
      if ("value" in metadata && metadata.value.isFile() && (metadata.value.mode & 0o111) !== 0) return Object.freeze({ executable });
      if ("error" in metadata && !["ENOENT", "ENOTDIR"].includes(metadata.error.code ?? "")) throw new OpenCliError("opencli_unavailable");
    }
  }
  throw new OpenCliError("opencli_unavailable");
}
