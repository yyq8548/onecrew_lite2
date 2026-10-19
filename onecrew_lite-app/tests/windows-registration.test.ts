import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory, verifyPrivateFile } from "../src/storage/private-backup.ts";
import { disableLegacyWindowsRegistration, inspectLegacyWindowsRegistration, DisabledWindowsRegistration } from "../src/migration/windows-registration.ts";

const execute = promisify(execFile);
test("real Windows previous-registration metadata is privately backed up, disabled, and verified without an old executable", { skip: process.platform !== "win32", timeout: 120_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/windows-registration-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "registration-"));
  const owner = await createPrivateDirectory(join(root, "owner"));
  const old = await createPrivateDirectory(join(root, "旧数据 space"));
  const service = await createPrivateDirectory(join(root, "service-image"));
  const installed = await createPrivateDirectory(join(root, "installed"));
  const serviceExecutable = join(service, "onecrew_lite.exe"), packageExecutable = join(installed, "onecrew_lite.exe");
  await copyFile(process.execPath, serviceExecutable);
  await copyFile(process.execPath, packageExecutable);
  const executableSha256 = createHash("sha256").update(await readFile(serviceExecutable)).digest("hex");
  const lease = await acquireInstanceLock(owner);
  const hostScript = fileURLToPath(new URL("./native/previous-registration.ps1", import.meta.url));
  const powershell = join(process.env.SystemRoot!, "System32/WindowsPowerShell/v1.0/powershell.exe");
  const host = (operation: "Install" | "Remove") => execute(powershell, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", hostScript, "-Operation", operation, "-Directory", old, "-Executable", serviceExecutable, "-Package", packageExecutable], { windowsHide: true, timeout: 30_000 });
  let registered = false;
  try {
    await host("Install"); registered = true;
    const input = { dataDirectory: old, serviceExecutable, packageExecutable, executableSha256 };
    const before = await inspectLegacyWindowsRegistration(input);
    assert.equal(before.enabled, true); assert.equal(before.running, false);
    const backup = join(owner, "old-task.xml");
    const receipt = await disableLegacyWindowsRegistration(lease, input, backup);
    await verifyPrivateFile(backup);
    assert.match(await readFile(backup, "utf8"), /__service-host/);
    await receipt.verify(lease);
    assert.equal((await inspectLegacyWindowsRegistration(input)).enabled, false);
    assert.throws(() => JSON.stringify(receipt), /cannot be serialized/);
    assert.throws(() => Reflect.construct(DisabledWindowsRegistration, [Symbol(), lease, input, backup, "0".repeat(64)]), /unissued/);
    await assert.rejects(disableLegacyWindowsRegistration(lease, input, backup));
    await writeFile(backup, "changed");
    await assert.rejects(receipt.verify(lease), /backup_changed/);
    await assert.rejects(inspectLegacyWindowsRegistration({ ...input, executableSha256: "0".repeat(64) }), /executable_changed/);
  } finally {
    if (registered) await host("Remove");
    await lease.close(); await rm(root, { recursive: true });
  }
});
