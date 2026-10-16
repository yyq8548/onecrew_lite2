import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { lstat, open, readFile, rm } from "node:fs/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";

export class DiscoverySemanticError extends Error {
  readonly code: "semantic_unavailable" | "semantic_login_required" | "semantic_failed" | "semantic_timeout" | "semantic_invalid_output";
  constructor(code: DiscoverySemanticError["code"]) { super(code); this.code = code; }
}
const disabled = ["shell_tool", "unified_exec", "apps", "plugins", "hooks", "browser_use", "computer_use", "code_mode", "multi_agent", "multi_agent_v2", "image_generation", "memories"] as const;
export function codexStructuredArguments(directory: string, schemaPath: string, outputPath: string, planning: boolean): readonly string[] {
  if (![directory, schemaPath, outputPath].every(value => isAbsolute(value) && !value.includes("\0"))) throw new Error("Semantic working paths must be absolute");
  return ["exec", "--ephemeral", "--ignore-user-config", "--ignore-rules", "--sandbox", "read-only", "--skip-git-repo-check", "--color", "never", "-c", 'web_search="disabled"', ...(planning ? ["-c", 'model_reasoning_effort="medium"'] : []), ...disabled.flatMap(feature => ["--disable", feature]), "--cd", directory, "--output-schema", schemaPath, "--output-last-message", outputPath, "-"];
}
export interface SemanticStructuredRequest<T> { readonly prompt: string; readonly schema: Readonly<Record<string, unknown>>; readonly decode: z.ZodType<T>; readonly planning: boolean; readonly signal: AbortSignal; onStarted?(): void; assertCurrent(): Promise<void> }
export interface SemanticStructuredClient { structured<T>(input: SemanticStructuredRequest<T>): Promise<T> }

// 可执行文件和工作目录只能由受信启动配置提供，不读取Codex插件路径或请求中的命令。
export class CodexSemanticClient implements SemanticStructuredClient {
  readonly #executable: string;
  readonly #workingRoot: string;
  constructor(executable: string, workingRoot: string) {
    if (!isAbsolute(executable) || /\.(?:cmd|bat|ps1)$/iu.test(executable) || !isAbsolute(workingRoot) || executable.includes("\0") || workingRoot.includes("\0")) throw new Error("Semantic client requires trusted native executable and private working paths");
    this.#executable = executable; this.#workingRoot = workingRoot;
  }
  async #run(args: readonly string[], prompt: string | null, directory: string, timeout: number, signal: AbortSignal): Promise<number> {
    signal.throwIfAborted(); const metadata = await lstat(this.#executable);
    if (!metadata.isFile() || metadata.isSymbolicLink()) throw new DiscoverySemanticError("semantic_unavailable"); signal.throwIfAborted();
    return new Promise((resolve, reject) => {
      const child = spawn(this.#executable, [...args], { cwd: directory, shell: false, windowsHide: true, stdio: [prompt === null ? "ignore" : "pipe", "ignore", "ignore"] });
      let failure: unknown, timer: ReturnType<typeof setTimeout> | undefined;
      const stop = (error: unknown) => { if (failure !== undefined) return; failure = error; child.kill(); };
      const abort = () => stop(signal.reason);
      signal.addEventListener("abort", abort, { once: true });
      timer = setTimeout(() => stop(new DiscoverySemanticError("semantic_timeout")), timeout);
      child.once("error", () => { failure ??= new DiscoverySemanticError("semantic_unavailable"); });
      child.once("close", code => { clearTimeout(timer); signal.removeEventListener("abort", abort); if (failure !== undefined) reject(failure); else resolve(code ?? -1); });
      if (prompt !== null) {
        child.stdin!.once("error", () => stop(new DiscoverySemanticError("semantic_failed")));
        child.stdin!.end(prompt, "utf8");
      }
      if (signal.aborted) abort();
    });
  }
  async structured<T>(input: SemanticStructuredRequest<T>): Promise<T> {
    input.signal.throwIfAborted(); await input.assertCurrent(); await verifyPrivateDirectory(this.#workingRoot);
    if (!input.prompt.isWellFormed() || Buffer.byteLength(input.prompt) > 4 * 1024 * 1024) throw new DiscoverySemanticError("semantic_failed");
    const login = await this.#run(["login", "status"], null, this.#workingRoot, 10000, input.signal);
    await input.assertCurrent(); if (login !== 0) throw new DiscoverySemanticError("semantic_login_required");
    input.onStarted?.();
    const directory = await createPrivateDirectory(join(this.#workingRoot, `semantic-${randomUUID()}`));
    const directoryIdentity = await lstat(directory);
    const result = await this.#structuredInDirectory(input, directory).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    const cleanup = await (async () => {
      await verifyPrivateDirectory(this.#workingRoot); await verifyPrivateDirectory(directory);
      const current = await lstat(directory);
      if (dirname(resolve(directory)) !== resolve(this.#workingRoot) || current.ino !== directoryIdentity.ino || current.dev !== directoryIdentity.dev) throw new Error("semantic_cleanup_directory_changed");
      await rm(directory, { recursive: true });
    })().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!cleanup.ok) {
      if (!result.ok) throw new AggregateError([result.error, cleanup.error], "semantic_execution_and_cleanup_failed");
      throw cleanup.error;
    }
    if (!result.ok) throw result.error;
    return result.value;
  }
  async #structuredInDirectory<T>(input: SemanticStructuredRequest<T>, directory: string): Promise<T> {
    const schemaPath = join(directory, "schema.json"), outputPath = join(directory, "result.json");
    const schema = await open(schemaPath, "wx", 0o600);
    try { await schema.writeFile(stringifyApplicationJson(input.schema), "utf8"); await schema.sync(); } finally { await schema.close(); }
    await input.assertCurrent();
    const exit = await this.#run(codexStructuredArguments(directory, schemaPath, outputPath, input.planning), input.prompt, directory, 180000, input.signal);
    await input.assertCurrent(); if (exit !== 0) throw new DiscoverySemanticError("semantic_failed");
    await verifyPrivateFile(outputPath); const metadata = await lstat(outputPath);
    if (metadata.size > 65536 || metadata.nlink !== 1) throw new DiscoverySemanticError("semantic_invalid_output");
    const bytes = await readFile(outputPath); if (bytes.length > 65536) throw new DiscoverySemanticError("semantic_invalid_output");
    const output = await Promise.resolve().then(() => input.decode.parse(parseApplicationJson(bytes))).then(value => value, error => { if (error instanceof SyntaxError || error instanceof z.ZodError) throw new DiscoverySemanticError("semantic_invalid_output"); throw error; });
    await input.assertCurrent(); input.signal.throwIfAborted(); return output;
  }
}
