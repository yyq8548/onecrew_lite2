import { execFile } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, readFile, realpath, rename, unlink, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, toNamespacedPath } from "node:path";
import { promisify } from "node:util";
import { setTimeout as delay } from "node:timers/promises";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";

const execute = promisify(execFile);
const nativeLock = createRequire(import.meta.url)("fs-native-extensions") as { tryLock(fd: number): boolean };
const digest = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const script = () => runtimeAssetPath("service/windows-host.ps1", new URL("./windows-host.ps1", import.meta.url));
const windowsArgument = (value: string) => {
  if (!value.isWellFormed() || /["\p{Cc}]/u.test(value)) throw new Error("service.host_argument_invalid");
  return `"${value.replace(/\\+$/u, suffix => suffix + suffix)}"`;
};
export async function retainWindowsServiceBinary(executable: string, directory: string): Promise<string> {
  if (process.platform !== "win32") throw new Error("service.windows_host_platform_required");
  await verifyPrivateDirectory(directory);
  const source = toNamespacedPath(executable), before = await lstat(source, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n) throw new Error("service.host_binary_invalid");
  const bytes = await readFile(source), after = await lstat(source, { bigint: true });
  if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new Error("service.host_binary_changed");
  const hash = digest(bytes);
  const lockPath = join(directory, "service-install.lock");
  await using lease = await open(lockPath, constants.O_CREAT | constants.O_RDWR | (constants.O_NOFOLLOW ?? 0), 0o600);
  await verifyPrivateFile(lockPath);
  const deadline = performance.now() + 30_000;
  while (!nativeLock.tryLock(lease.fd)) {
    if (performance.now() >= deadline) throw new Error("service.host_install_busy");
    await delay(100);
  }
  const opened = await lease.stat(), current = await lstat(lockPath);
  if (current.isSymbolicLink() || current.dev !== opened.dev || current.ino !== opened.ino) throw new Error("service.host_install_lock_changed");
  let bin = directory;
  for (const name of [".windows-host", "bin"]) {
    bin = join(bin, name);
    if (await lstat(bin).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; })) await verifyPrivateDirectory(bin);
    else await createPrivateDirectory(bin);
  }
  const target = join(bin, hash + ".exe");
  async function verify(): Promise<string> {
    await verifyPrivateFile(target);
    if (digest(await readFile(target)) !== hash) throw new Error("service.host_binary_changed");
    return realpath(toNamespacedPath(target));
  }
  if (await lstat(target).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; })) return await verify();
  const stage = join(bin, `.candidate-${randomUUID()}.exe`);
  await writeFile(stage, bytes, { flag: "wx", mode: 0o600 });
  await verifyPrivateFile(stage);
  await rename(stage, target).then(undefined, async error => {
    if (error.code !== "EEXIST" && error.code !== "EPERM") throw error;
    await verify(); await unlink(stage);
  });
  return await verify();
}
async function invoke(directory: string, operation: "Start" | "Remove" | "Abort", configuration?: string): Promise<void> {
  if (process.platform !== "win32" || !process.env.SystemRoot) throw new Error("service.windows_host_platform_required");
  await execute(join(process.env.SystemRoot, "System32/WindowsPowerShell/v1.0/powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", await realpath(script()), "-Operation", operation, "-Directory", directory, ...(configuration ? ["-Configuration", configuration] : [])], { windowsHide: true, timeout: 30_000, maxBuffer: 16 * 1024 });
}
export async function startWindowsServiceHost(executable: string, args: readonly string[], directory: string): Promise<void> {
  await verifyPrivateDirectory(directory);
  const hostDirectory = join(directory, ".windows-host");
  const exists = await lstat(hostDirectory).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
  if (!exists) await createPrivateDirectory(hostDirectory); else await verifyPrivateDirectory(hostDirectory);
  const program = await realpath(executable), host = await realpath(script());
  const environment: Record<string, string | null> = {};
  for (const name of ["LOCALAPPDATA", "APPDATA", "USERPROFILE", "HOME"]) environment[name] = process.env[name] === undefined ? null : await realpath(process.env[name]!);
  const configuration = JSON.stringify({ format: 2, directory, executable: program, executable_sha256: digest(await readFile(program)), arguments: args.map(windowsArgument).join(" "), working_directory: dirname(program), environment, service_log: join(directory, "service.log"), host_script: host, host_script_sha256: digest(await readFile(host)) });
  const path = join(hostDirectory, `${digest(configuration)}.json`);
  await writeFile(path, configuration, { flag: "wx", mode: 0o600 }).then(undefined, async error => {
    if (error.code !== "EEXIST") throw error;
    await verifyPrivateFile(path);
    if (await readFile(path, "utf8") !== configuration) throw new Error("service.host_configuration_changed");
  });
  await verifyPrivateFile(path);
  await invoke(directory, "Start", path);
}
export async function removeWindowsServiceHost(directory: string): Promise<void> { await invoke(directory, "Remove"); }
export async function abortWindowsServiceHost(directory: string): Promise<void> { await invoke(directory, "Abort"); }
