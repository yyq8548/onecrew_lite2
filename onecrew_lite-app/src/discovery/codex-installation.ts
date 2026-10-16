import { lstat, readdir, realpath, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { delimiter, isAbsolute, join } from "node:path";
import { DiscoverySemanticError } from "./semantic-client.ts";

export function codexVersionDirectory(value: string): boolean { return /^[A-Fa-f0-9]{16,64}$/u.test(value); }
async function nativeFile(path: string): Promise<string | null> {
  const metadata = await stat(path).then(value => value, error => { if (["ENOENT", "ENOTDIR"].includes((error as NodeJS.ErrnoException).code ?? "")) return null; throw error; });
  if (!metadata?.isFile()) return null;
  const resolved = await realpath(path), actual = await lstat(resolved);
  return actual.isFile() && !actual.isSymbolicLink() ? resolved : null;
}
// 受信启动配置可指定原生路径；请求数据永远不能传入此函数。
export async function resolveCodexCli(explicit?: string): Promise<string> {
  if (explicit !== undefined) {
    if (!isAbsolute(explicit) || explicit.includes("\0") || /\.(?:cmd|bat|ps1)$/iu.test(explicit)) throw new DiscoverySemanticError("semantic_unavailable");
    const selected = await nativeFile(explicit); if (!selected) throw new DiscoverySemanticError("semantic_unavailable"); return selected;
  }
  const name = process.platform === "win32" ? "codex.exe" : "codex";
  const candidates = (process.env.PATH ?? "").split(delimiter).filter(isAbsolute).map(directory => join(directory, name));
  if (process.platform === "win32" && process.env.LOCALAPPDATA) {
    const root = join(process.env.LOCALAPPDATA, "OpenAI", "Codex", "bin"), entries = await readdir(root, { withFileTypes: true }).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return []; throw error; });
    const desktop: { path: string; modified: number }[] = [];
    for (const entry of entries) if (codexVersionDirectory(entry.name)) {
      const path = join(root, entry.name, name), metadata = await stat(path).then(value => value, error => { if (["ENOENT", "ENOTDIR"].includes((error as NodeJS.ErrnoException).code ?? "")) return null; throw error; });
      if (metadata?.isFile()) desktop.push({ path, modified: metadata.mtimeMs });
    }
    desktop.sort((left, right) => right.modified - left.modified || Buffer.compare(Buffer.from(right.path), Buffer.from(left.path)));
    candidates.push(...desktop.map(candidate => candidate.path));
  }
  if (process.platform === "darwin") for (const root of ["/Applications", join(homedir(), "Applications")]) for (const app of ["Codex.app", "ChatGPT.app"]) for (const entry of ["codex-cli/bin/codex", "codex"]) candidates.push(join(root, app, "Contents", "Resources", entry));
  for (const path of candidates) { const selected = await nativeFile(path); if (selected) return selected; }
  throw new DiscoverySemanticError("semantic_unavailable");
}
