import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { promisify } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { verifyLegacyPrivateDirectory } from "../storage/legacy-private-source.ts";

const execute = promisify(execFile);
const digest = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
const statusSchema = z.object({ taskName: z.string().regex(/^OneCrew-Service-[a-f0-9]{64}$/u), enabled: z.boolean(), running: z.boolean(), executableSha256: z.string().regex(/^[a-f0-9]{64}$/u) }).strict();
export type LegacyWindowsRegistration = Readonly<{ dataDirectory: string; serviceExecutable: string; packageExecutable: string; executableSha256: string }>;
const issuer = Symbol("disabled-original-windows-registration");

async function validatePaths(input: LegacyWindowsRegistration): Promise<void> {
  if (process.platform !== "win32") throw new Error("migration.windows_registration_platform_unsupported");
  z.string().regex(/^[a-f0-9]{64}$/u).parse(input.executableSha256);
  for (const path of [input.dataDirectory, input.serviceExecutable, input.packageExecutable]) {
    if (path.includes('"') || relative(resolve(path), await realpath(path)) !== "") throw new Error("migration.registration_path_invalid");
    const stat = await lstat(path);
    if (stat.isSymbolicLink()) throw new Error("migration.registration_path_invalid");
  }
  await verifyLegacyPrivateDirectory(input.dataDirectory);
  for (const path of [input.serviceExecutable, input.packageExecutable]) {
    if (!(await lstat(path)).isFile() || digest(await readFile(path)) !== input.executableSha256) throw new Error("migration.registration_executable_changed");
  }
}
async function operation(input: LegacyWindowsRegistration, action: "Inspect" | "Disable" | "VerifyDisabled" | "VerifyProcess", backupFile?: string, processId?: number) {
  await validatePaths(input);
  const args = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", runtimeAssetPath("migration/windows-registration.ps1", new URL("./windows-registration.ps1", import.meta.url)), "-Operation", action,
    "-DataDirectory", input.dataDirectory, "-ServiceExecutable", input.serviceExecutable, "-PackageExecutable", input.packageExecutable, "-ExpectedSha256", input.executableSha256,
    ...(backupFile ? ["-BackupFile", backupFile] : []), ...(processId === undefined ? [] : ["-ProcessId", String(z.number().int().min(1).max(2147483647).parse(processId))])];
  const result = await execute(join(process.env.SystemRoot ?? "C:\\Windows", "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), args,
    { windowsHide: true, timeout: 30_000, maxBuffer: 64 * 1024, encoding: "buffer" });
  const status = statusSchema.parse(parseApplicationJson(result.stdout));
  if (status.executableSha256 !== input.executableSha256 || ((action === "Disable" || action === "VerifyDisabled") && status.enabled)) throw new Error("migration.registration_readback_mismatch");
  return status;
}

export async function inspectLegacyWindowsRegistration(input: LegacyWindowsRegistration) {
  return operation(input, "Inspect");
}
export async function verifyLegacyWindowsProcess(input: LegacyWindowsRegistration, processId: number): Promise<void> {
  await operation(input, "VerifyProcess", undefined, processId);
}

// 该凭据只证明精确旧计划任务已停用；不替代旧进程排空、数据锁和新服务健康验收。
export class DisabledWindowsRegistration {
  readonly #lease: InstanceLease;
  readonly #input: LegacyWindowsRegistration;
  readonly #backup: string;
  readonly #backupSha256: string;
  constructor(token: symbol, lease: InstanceLease, input: LegacyWindowsRegistration, backup: string, backupSha256: string) {
    if (token !== issuer) throw new Error("migration.registration_receipt_unissued");
    this.#lease = lease; this.#input = Object.freeze({ ...input }); this.#backup = backup; this.#backupSha256 = backupSha256;
  }
  async verify(lease: InstanceLease): Promise<void> {
    this.#lease.assertOwned();
    if (lease !== this.#lease) throw new Error("migration.registration_lease_changed");
    await verifyPrivateFile(this.#backup);
    if (digest(await readFile(this.#backup)) !== this.#backupSha256) throw new Error("migration.registration_backup_changed");
    const status = await operation(this.#input, "VerifyDisabled");
    if (status.running) throw new Error("migration.registration_old_host_running");
    lease.assertOwned();
  }
  toJSON(): never { throw new Error("Registration authority cannot be serialized"); }
}

export async function disableLegacyWindowsRegistration(lease: InstanceLease, input: LegacyWindowsRegistration, backupFile: string): Promise<DisabledWindowsRegistration> {
  lease.assertOwned();
  const backupParent = relative(resolve(lease.directory), resolve(dirname(backupFile)));
  if (backupParent === ".." || backupParent.startsWith(`..${sep}`) || isAbsolute(backupParent)) throw new Error("migration.registration_backup_outside_lease");
  await verifyPrivateDirectory(dirname(backupFile));
  await operation(input, "Disable", backupFile);
  await verifyPrivateFile(backupFile);
  const receipt = new DisabledWindowsRegistration(issuer, lease, input, backupFile, digest(await readFile(backupFile)));
  lease.assertOwned();
  return receipt;
}
