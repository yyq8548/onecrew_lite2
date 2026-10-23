import { execFile, spawn, type ChildProcess } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { homedir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";
import { promisify } from "node:util";
import { z } from "zod";
import { runtimeAssetPath } from "../../packaging/runtime-context.ts";
import { parseApplicationJson } from "../../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory } from "../../storage/private-backup.ts";

const execute = promisify(execFile);
const script = () => runtimeAssetPath("browser/browser-host.ps1", new URL("./browser-host.ps1", import.meta.url));
const detectedSchema = z.object({ path: z.string().min(1), version: z.string().nullable(), install_scope: z.enum(["custom", "user", "machine"]) }).strict().nullable();
async function windows(operation: "Detect" | "Inspect" | "Close", args: string[] = []) {
  if (!process.env.SystemRoot) throw new Error("browser.windows_system_root_missing");
  return execute(join(process.env.SystemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script(), "-Operation", operation, ...args], { windowsHide: true, timeout: 30_000, maxBuffer: 64 * 1024 });
}
export async function detectChrome() {
  if (process.platform === "win32") return detectedSchema.parse(parseApplicationJson((await windows("Detect")).stdout));
  if (process.platform !== "darwin") throw new Error("browser.unsupported_platform");
  const choices = [...(process.env.CHROME_BIN ? [{ path: process.env.CHROME_BIN, install_scope: "custom" as const }] : []),
    { path: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", install_scope: "machine" as const }, { path: join(homedir(), "Applications", "Google Chrome.app", "Contents", "MacOS", "Google Chrome"), install_scope: "user" as const }];
  for (const choice of choices) {
    const exists = await lstat(choice.path).then(value => value.isFile(), error => { if (error.code === "ENOENT") return false; throw error; });
    if (!exists) continue;
    const result = await execute(choice.path, ["--version"], { timeout: 15_000, maxBuffer: 16 * 1024 });
    const version = /(?:Chrome|Chromium)\s+(\d+)(?:\.\d+)+/u.exec(result.stdout + result.stderr)?.[1] ?? null;
    return { ...choice, version };
  }
  return null;
}

export async function chromeStatus() {
  const install = await detectChrome(), os = process.platform === "win32" ? "windows" : "macos", provider = process.platform === "win32" ? "windows_legacy_dpapi" : "macos_safe_storage";
  return { os, state: install ? "ready" : "browser_not_found", installed: install !== null, version: install?.version ?? null, path: install?.path ?? null, channel: install ? "stable" : null, install_scope: install?.install_scope ?? null,
    cookie_provider: provider, limitations: install ? [] : ["Google Chrome was not detected on this device."], ...(install ? { completion_notice: { provider, message: process.platform === "win32" ? "Cookie extraction uses the current Windows user's protected context. Never supply a Windows password in the application or chat." : "macOS may request Chrome Safe Storage access in its native Keychain dialog. Never supply a system password in the application or chat." } } : { error_code: "browser_not_installed" }) };
}

export async function openChromeSetupPage(page: "download" | "extension"): Promise<void> {
  const url = page === "download" ? "https://opencli.info/download/" : page === "extension" ? "https://chromewebstore.google.com/detail/opencli/ildkmabpimmkaediidaifkhjpohdnifk" : null;
  if (!url) throw new Error("browser.setup_page_invalid"); const installation = await detectChrome(); if (!installation) throw new Error("browser_not_installed");
  const file = await lstat(installation.path); if (!file.isFile() || file.isSymbolicLink()) throw new Error("browser.executable_invalid");
  const child = spawn(installation.path, [url], { shell: false, windowsHide: true, stdio: "ignore" });
  await new Promise<void>((resolve, reject) => { child.once("spawn", resolve); child.once("error", reject); });
  child.unref();
}

const live = Symbol("owned-chrome-process");
export async function createOwnedChromeProfile(profiles: string): Promise<string> {
  await verifyPrivateDirectory(profiles);
  const directory = join(await realpath(profiles), randomBytes(10).toString("base64url"));
  // 为商店下载文件、临时后缀和 Windows Zone.Identifier 保留路径空间。
  if (process.platform === "win32" && directory.length > 140) throw new Error("browser.profile_path_too_long");
  return createPrivateDirectory(directory);
}
export class OwnedChromeProcess {
  readonly #child: ChildProcess;
  readonly #executable: string;
  readonly #hash: string;
  readonly #startTimeTicks: string | undefined;
  readonly profileDirectory: string;
  #closed = false;
  #closing: Promise<void> | undefined;
  constructor(token: symbol, child: ChildProcess, executable: string, hash: string, profileDirectory: string, startTimeTicks?: string) {
    if (token !== live) throw new Error("browser.owned_launch_required");
    this.#child = child; this.#executable = executable; this.#hash = hash; this.profileDirectory = profileDirectory; this.#startTimeTicks = startTimeTicks; Object.freeze(this);
  }
  assertRunning(): void {
    if (this.#closed || !this.#child.pid || this.#child.exitCode !== null || this.#child.signalCode !== null) throw new Error("browser.owned_process_not_running");
  }
  async close(): Promise<void> {
    if (this.#closing) return this.#closing;
    const operation = this.#close(); this.#closing = operation; return operation;
  }
  async #close(): Promise<void> {
    this.#closed = true;
    if (this.#child.exitCode !== null || this.#child.signalCode !== null) return;
    const exited = new Promise<void>((resolve, reject) => { this.#child.once("exit", () => resolve()); this.#child.once("error", reject); });
    try {
      if (process.platform === "win32") {
        const result = await windows("Close", ["-ProcessId", String(this.#child.pid), "-Executable", this.#executable, "-ExecutableSha256", this.#hash, "-StartTimeTicks", this.#startTimeTicks!]);
        if (result.stdout.trim() !== "owned-browser-closed") throw new Error("browser.graceful_close_failed");
      } else this.#child.kill("SIGTERM");
      await exited;
    } catch (error) { this.#child.kill("SIGKILL"); await exited; throw error; }
  }
  toJSON(): never { throw new Error("browser.process_authority_not_serializable"); }
}

export async function launchOwnedChrome(input: { profileDirectory: string; url: URL; proxy?: { host: string; port: number }; timezone?: string; acceptLanguage?: string; signal: AbortSignal }): Promise<OwnedChromeProcess> {
  input.signal.throwIfAborted(); await verifyPrivateDirectory(input.profileDirectory);
  const install = await detectChrome(); if (!install) throw new Error("browser_not_installed");
  if (!isAbsolute(install.path) || await realpath(install.path) !== resolve(install.path)) throw new Error("browser.executable_alias_rejected");
  const info = await lstat(install.path); if (!info.isFile() || info.isSymbolicLink()) throw new Error("browser.executable_invalid");
  const hash = createHash("sha256").update(await readFile(install.path)).digest("hex");
  const args = [`--user-data-dir=${resolve(input.profileDirectory)}`, "--force-webrtc-ip-handling-policy=disable_non_proxied_udp", "--no-first-run", "--no-default-browser-check", "--no-service-autorun", "--disable-sync", "--disable-background-mode", "--metrics-recording-only"];
  if (process.env.ONECREW_BROWSER_DIAGNOSTICS === "1") args.push("--enable-logging", `--log-file=${join(input.profileDirectory, "chrome-diagnostics.log")}`);
  if (process.platform === "win32" && process.env.ONECREW_BROWSER_DIAGNOSTICS === "1") args.push("--force-renderer-accessibility=complete");
  if (input.proxy) { if (input.proxy.host !== "127.0.0.1" || !Number.isInteger(input.proxy.port) || input.proxy.port < 1 || input.proxy.port > 65535) throw new Error("browser.loopback_proxy_required"); args.push(`--proxy-server=http://${input.proxy.host}:${input.proxy.port}`, "--disable-quic"); }
  if (input.acceptLanguage) { z.string().regex(/^[A-Za-z0-9,;=. -]+$/u).parse(input.acceptLanguage); args.push(`--accept-lang=${input.acceptLanguage}`, `--lang=${input.acceptLanguage.split(",")[0]}`); }
  args.push(input.url.href);
  const child = spawn(install.path, args, { shell: false, windowsHide: true, stdio: "ignore", ...(input.timezone ? { env: { ...process.env, TZ: input.timezone } } : {}) });
  await new Promise<void>((resolve, reject) => { child.once("spawn", resolve); child.once("error", reject); });
  let startTimeTicks: string | undefined;
  if (process.platform === "win32") {
    try { startTimeTicks = z.string().regex(/^[0-9]+$/u).parse((await windows("Inspect", ["-ProcessId", String(child.pid), "-Executable", install.path, "-ExecutableSha256", hash])).stdout.trim()); }
    catch (error) { child.kill("SIGKILL"); throw error; }
  }
  const owned = new OwnedChromeProcess(live, child, install.path, hash, input.profileDirectory, startTimeTicks);
  if (input.signal.aborted) { await owned.close(); input.signal.throwIfAborted(); }
  return owned;
}
