import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { getPackagedRuntime, runtimeExecutablePath } from "../packaging/runtime-context.ts";
import { CodexClientAdapter, defaultCodexHome } from "./codex.ts";

export const codexClientCommands = ["connect-codex", "disconnect-codex", "codex-status", "codex-config", "codex-package"] as const;
export type CodexClientCommand = typeof codexClientCommands[number];
export interface CodexClientCommandOptions { dataDirectory: string; codexHome?: string; codexExecutable?: string; pluginDirectory?: string; refresh?: boolean }
export async function executeCodexClientCommand(command: CodexClientCommand, options: CodexClientCommandOptions): Promise<unknown> {
  const packaged = getPackagedRuntime(), directory = resolve(options.dataDirectory);
  if (command === "codex-config" && !packaged) return { mcpServers: { onecrew_lite: { command: process.execPath, args: [fileURLToPath(new URL("../cli.ts", import.meta.url)), "mcp", "--stdio", "--data-dir", directory] } } };
  if ((command === "connect-codex" || command === "codex-package") && !packaged) throw new Error("codex_plugin_packaged_runtime_required");
  const adapter = new CodexClientAdapter({ executable: runtimeExecutablePath(), runtimeVersion: packaged?.manifest.productVersion ?? "0.7.1", dataDirectory: directory, codexHome: options.codexHome === undefined ? defaultCodexHome() : resolve(options.codexHome), pluginDirectory: options.pluginDirectory === undefined ? join(directory, "client-adapters", "codex") : resolve(options.pluginDirectory), ...(options.codexExecutable === undefined ? {} : { codexExecutable: resolve(options.codexExecutable) }) });
  if (command === "codex-config") return adapter.configuration();
  if (command === "connect-codex") return adapter.install();
  if (command === "disconnect-codex") return adapter.uninstall();
  if (command === "codex-package") return adapter.prepare();
  return adapter.status(options.refresh ?? false);
}
