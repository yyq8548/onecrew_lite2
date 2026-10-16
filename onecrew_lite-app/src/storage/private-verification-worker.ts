import { spawn } from "node:child_process";
import type { Socket } from "node:net";
import { createInterface } from "node:readline";
import { join } from "node:path";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";

const script = runtimeAssetPath("storage/private-verification-worker.ps1", new URL("./private-verification-worker.ps1", import.meta.url));
interface Request { operation: "Verify" | "VerifyTree"; directory: string; file?: string }
let queue: Promise<void> = Promise.resolve();
let worker: VerificationWorker | undefined;

// 只复用解释器；每个请求仍执行完整的现有权限与路径检查。
class VerificationWorker {
  readonly child;
  #pending: { resolve(): void; reject(error: Error): void; timer: NodeJS.Timeout } | undefined;
  #idle: NodeJS.Timeout | undefined;
  #closed = false;
  #stderr = "";
  constructor(systemRoot: string, verifier: string) {
    this.child = spawn(join(systemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script, "-Verifier", verifier], { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
    this.child.unref();
    for (const stream of [this.child.stdin, this.child.stdout, this.child.stderr]) (stream as Socket).unref();
    const lines = createInterface({ input: this.child.stdout });
    lines.on("line", line => {
      const pending = this.#pending;
      if (!pending || line !== "private-acl-verified") { this.#fail(new Error("Windows private verification returned an invalid response")); return; }
      clearTimeout(pending.timer); this.#pending = undefined; pending.resolve();
      this.#idle = setTimeout(() => this.#stop(), 5_000); this.#idle.unref();
    });
    this.child.stderr.on("data", (bytes: Buffer) => { this.#stderr = (this.#stderr + bytes.toString("utf8")).slice(-4096); });
    this.child.on("error", error => this.#fail(error));
    this.child.stdin.on("error", error => this.#fail(error));
    this.child.once("exit", code => { lines.close(); this.#fail(new Error(this.#stderr.trim() || `Windows private verification exited (${String(code)})`)); });
  }
  #stop() {
    if (this.#closed) return;
    this.#closed = true; clearTimeout(this.#idle);
    if (worker === this) worker = undefined;
    this.child.stdin.end();
    this.child.kill();
  }
  #fail(error: Error) {
    const pending = this.#pending; this.#pending = undefined;
    this.#stop();
    if (pending) { clearTimeout(pending.timer); pending.reject(error); }
  }
  request(value: Request): Promise<void> {
    if (this.#closed || this.#pending) return Promise.reject(new Error("Windows private verifier is unavailable"));
    clearTimeout(this.#idle);
    return new Promise((resolve, reject) => {
      this.#pending = { resolve, reject, timer: setTimeout(() => this.#fail(new Error("Windows private verification timed out")), 30_000) };
      this.child.stdin.write(`${JSON.stringify({ ...value, file: value.file ?? null })}\n`, error => { if (error) this.#fail(error); });
    });
  }
}

export function verifyWindowsPrivatePath(systemRoot: string, verifier: string, request: Request): Promise<void> {
  const next = queue.then(() => {
    worker ??= new VerificationWorker(systemRoot, verifier);
    return worker.request(request);
  });
  queue = next.then(() => undefined, () => undefined);
  return next;
}
