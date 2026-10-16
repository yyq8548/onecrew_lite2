import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { readdir, realpath, stat } from "node:fs/promises";
import { delimiter, isAbsolute, join } from "node:path";
import { homedir } from "node:os";
import { coerce } from "semver";
import type { ClientDetection } from "./contracts.ts";
import { parseApplicationJson } from "../contracts/application-json.ts";

const execute = promisify(execFile);
export class CodexCommandError extends Error { readonly code = "codex_cli_command_failed"; constructor(cause: unknown) { super("Codex CLI command failed", { cause }); } }
async function exists(path: string): Promise<boolean> { return stat(path).then(value => value.isFile(), error => { if (["ENOENT", "ENOTDIR"].includes((error as NodeJS.ErrnoException).code ?? "")) return false; throw error; }); }
export async function resolveCodexExecutable(explicit?: string): Promise<string> {
  if (explicit !== undefined) { if (!isAbsolute(explicit) || !await exists(explicit)) throw new Error("codex_cli_unavailable"); return realpath(explicit); }
  const filename = process.platform === "win32" ? "codex.exe" : "codex";
  const candidates = (process.env.PATH ?? "").split(delimiter).filter(isAbsolute).map(directory => join(directory, filename));
  if (process.platform === "win32" && process.env.LOCALAPPDATA) {
    const directory = join(process.env.LOCALAPPDATA, "OpenAI", "Codex", "bin");
    const entries = await readdir(directory, { withFileTypes: true }).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return []; throw error; });
    const native = await Promise.all(entries.filter(entry => entry.isDirectory() && /^[a-f0-9]{16,64}$/iu.test(entry.name)).map(async entry => { const path = join(directory, entry.name, filename); return await exists(path) ? { path, modified: (await stat(path)).mtimeMs } : null; }));
    candidates.push(...native.filter((value): value is { path: string; modified: number } => value !== null).sort((a, b) => b.modified - a.modified).map(value => value.path));
  }
  if (process.platform === "darwin") for (const directory of ["/Applications", join(homedir(), "Applications")]) for (const app of ["Codex.app", "ChatGPT.app"]) for (const relative of ["codex-cli/bin/codex", "codex"]) candidates.push(join(directory, app, "Contents", "Resources", relative));
  for (const candidate of candidates) if (await exists(candidate)) return realpath(candidate);
  throw new Error("codex_cli_unavailable");
}
export async function runCodexText(executable: string, home: string, args: readonly string[]): Promise<string> {
  if (!isAbsolute(executable) || !isAbsolute(home) || args.some(value => value.includes("\0"))) throw new Error("codex_cli_invalid_input");
  const result = await execute(executable, [...args], { cwd: home, env: { ...process.env, CODEX_HOME: home }, windowsHide: true, timeout: 60_000, maxBuffer: 4 * 1024 * 1024, encoding: "utf8" }).then(value => value, error => { throw new CodexCommandError(error); });
  return result.stdout;
}
export async function runCodex(executable: string, home: string, args: readonly string[]): Promise<unknown> { return parseApplicationJson(await runCodexText(executable, home, args)); }
export async function detectCodex(home: string, explicit?: string): Promise<ClientDetection> {
  const executable = await resolveCodexExecutable(explicit), output = await runCodexText(executable, home, ["--version"]), version = coerce(output)?.version;
  if (!version) throw new Error("codex_cli_version_invalid");
  // 实际插件子命令存在才报告协议支持；读取 help 不修改宿主配置。
  const help = await runCodexText(executable, home, ["plugin", "--help"]);
  if (!["marketplace", "list", "add", "remove"].every(command => new RegExp(`\\b${command}\\b`, "u").test(help))) throw new Error("codex_cli_plugin_protocol_unavailable");
  return { client: "codex", executable, version, pluginProtocol: "local_marketplace_v1", semanticAuthority: false };
}
