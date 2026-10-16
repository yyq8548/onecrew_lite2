import assert from "node:assert/strict";
import { execFile, spawn, type ChildProcess } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync } from "node:fs";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { setTimeout } from "node:timers/promises";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory, verifyPrivateFile, verifyPrivateDirectory } from "../src/storage/private-backup.ts";
import { disableLegacyWindowsRegistration, inspectLegacyWindowsRegistration, DisabledWindowsRegistration } from "../src/migration/windows-registration.ts";
import { stopLegacyWindowsService } from "../src/migration/legacy-control.ts";
import { readVaultMasterKey } from "../src/storage/vault-reader.ts";
import { backupLegacyDatabase, inspectLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { verifyLegacyPrivateDirectory, verifyLegacyPrivateFile } from "../src/storage/legacy-private-source.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { LegacyQuiescenceLease } from "../src/migration/legacy-quiescence.ts";
import { prepareSqliteRecovery } from "../src/migration/sqlite-recovery.ts";
import { prepareSqliteStaging } from "../src/migration/sqlite-staging.ts";

const execute = promisify(execFile);
test("real original Windows TaskScheduler registration is read, privately backed up, disabled, and verified", {
  skip: process.platform !== "win32" || !process.env.ONECREW_LEGACY_CLI_TEST_EXECUTABLE,
  timeout: 120_000,
}, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/windows-registration-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "registration-"));
  const owner = await createPrivateDirectory(join(root, "owner")), sourceParent = await createPrivateDirectory(join(root, "legacy-parent 中文 空格")), old = join(sourceParent, "旧数据 space");
  const lease = await acquireInstanceLock(owner);
  const service = await createPrivateDirectory(join(sourceParent, "service-image")), installed = await createPrivateDirectory(join(sourceParent, "installed"));
  const serviceExecutable = join(service, "onecrew_lite.exe"), packageExecutable = join(installed, "onecrew_lite.exe");
  await copyFile(process.env.ONECREW_LEGACY_CLI_TEST_EXECUTABLE!, serviceExecutable);
  await copyFile(process.env.ONECREW_LEGACY_CLI_TEST_EXECUTABLE!, packageExecutable);
  const executableSha256 = createHash("sha256").update(await readFile(serviceExecutable)).digest("hex");
  const hostScript = fileURLToPath(new URL("../../onecrew_lite-rust/crates/local-service/src/windows_service_host.ps1", import.meta.url));
  const powershell = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
  async function host(action: "install" | "remove") {
    await execute(powershell, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", hostScript], { windowsHide: true, timeout: 30_000,
      env: { ...process.env, ONECREW_HOST_BINARY: serviceExecutable, ONECREW_HOST_PACKAGE: packageExecutable, ONECREW_HOST_DATA: old, ONECREW_HOST_ACTION: action } });
  }
  let registered = false;
  let child: ChildProcess | undefined;
  try {
    const local = await createPrivateDirectory(join(sourceParent, "local")), roaming = await createPrivateDirectory(join(sourceParent, "roaming")), temporary = await createPrivateDirectory(join(sourceParent, "temporary"));
    child = spawn(serviceExecutable, ["__service", "--data-dir", old], { windowsHide: true, stdio: "ignore", cwd: installed,
      env: { SystemRoot: process.env.SystemRoot, WINDIR: process.env.WINDIR, PATH: join(process.env.SystemRoot ?? "C:\\Windows", "System32"),
        USERPROFILE: sourceParent, HOME: sourceParent, LOCALAPPDATA: local, APPDATA: roaming, TEMP: temporary, TMP: temporary, ONECREW_LITE_HOST_INSTALLED_EXECUTABLE: packageExecutable } });
    const exited = new Promise<number | null>(resolve => child!.once("exit", resolve));
    const deadline = Date.now() + 60_000;
    while (!lstatSync(join(old, ".onecrew_lite-service.json"), { throwIfNoEntry: false })) {
      if (child.exitCode !== null || Date.now() >= deadline) throw new Error("Original isolated service did not publish its descriptor");
      await setTimeout(50);
    }
    await assert.rejects(verifyPrivateDirectory(old));
    assert.equal(await verifyLegacyPrivateDirectory(old), old);
    await verifyLegacyPrivateFile(join(old, ".onecrew_lite-service-owner.lock"));
    await host("install"); registered = true;
    const input = { dataDirectory: old, serviceExecutable, packageExecutable, executableSha256 };
    const before = await inspectLegacyWindowsRegistration(input);
    assert.equal(before.enabled, true); assert.equal(before.running, false);
    const backup = join(owner, "old-task.xml");
    const receipt = await disableLegacyWindowsRegistration(lease, input, backup);
    await verifyPrivateFile(backup);
    assert.match((await readFile(backup)).toString("utf8"), /__service-host/);
    await receipt.verify(lease);
    assert.equal((await inspectLegacyWindowsRegistration(input)).enabled, false);
    await writeFile(join(old, "keep.txt"), "保留原数据", { flag: "wx", mode: 0o600 });
    await execute(packageExecutable, ["status", "--data-dir", old, "--json"], { windowsHide: true, timeout: 30_000, cwd: installed,
      env: { SystemRoot: process.env.SystemRoot, WINDIR: process.env.WINDIR, PATH: join(process.env.SystemRoot ?? "C:\\Windows", "System32"),
        USERPROFILE: old, HOME: old, LOCALAPPDATA: local, APPDATA: roaming, TEMP: temporary, TMP: temporary } });
    const stopped = await stopLegacyWindowsService(input, AbortSignal.timeout(60_000));
    assert.equal(stopped.state, "stop_requested");
    assert.equal(await exited, 0); child = undefined;
    assert.equal(await readFile(join(old, "keep.txt"), "utf8"), "保留原数据");
    assert.equal(lstatSync(join(old, ".onecrew_lite-service.json"), { throwIfNoEntry: false }), undefined);
    const keyPath = join(old, "vault.key"), originalKey = await readFile(keyPath);
    await assert.rejects(readVaultMasterKey(keyPath));
    const key = await readVaultMasterKey(keyPath, "legacy-source");
    try {
      assert.deepEqual(key, originalKey);
      assert.equal(inspectLegacyDatabase(join(old, "agent-platform.db"), "vault").version, 4);
      const backup = await backupLegacyDatabase(join(old, "agent-platform.db"), join(owner, "vault-copy", "vault.sqlite"), "vault");
      await verifyPrivateFile(backup.path);
      const privateKey = join(owner, "copied-masterkey");
      await writeFile(privateKey, key, { flag: "wx", mode: 0o600 });
      const copied = await readVaultMasterKey(privateKey);
      try { assert.deepEqual(copied, originalKey); } finally { copied.fill(0); }
      assert.deepEqual(await readFile(keyPath), originalKey);
    } finally { key.fill(0); originalKey.fill(0); }
    const creatorDirectory = await createPrivateDirectory(join(old, "creator-fixture")), creator = join(creatorDirectory, "creator.sqlite");
    await initializeNewCreatorDatabase(creator);
    await using quiescence = await LegacyQuiescenceLease.acquire(lease, { serviceDirectory: old, sources: { creator, vault: join(old, "agent-platform.db"), masterKey: keyPath, libraryDirectory: null } });
    quiescence.assertQuiescent({ creator, vault: join(old, "agent-platform.db"), masterKey: keyPath, libraryDirectory: null });
    const recovery = await prepareSqliteRecovery(lease, { creator, vault: join(old, "agent-platform.db") });
    const staging = await prepareSqliteStaging(lease, { recovery, sources: { creator, vault: join(old, "agent-platform.db"), masterKey: keyPath }, signal: AbortSignal.timeout(60_000) });
    assert.equal(staging.activated, false); assert.equal(staging.creator.inspection.version, 90); assert.equal(staging.vault.inspection.version, 4);
    await verifyPrivateFile(staging.masterKey);
    await verifyLegacyPrivateFile(keyPath);
    await receipt.verify(lease);
    assert.throws(() => JSON.stringify(receipt), /cannot be serialized/);
    assert.throws(() => Reflect.construct(DisabledWindowsRegistration, [Symbol(), lease, input, backup, "0".repeat(64)]), /unissued/);
    await assert.rejects(disableLegacyWindowsRegistration(lease, input, backup));
    await writeFile(backup, "changed");
    await assert.rejects(receipt.verify(lease), /backup_changed/);
    await assert.rejects(inspectLegacyWindowsRegistration({ ...input, executableSha256: "0".repeat(64) }), /executable_changed/);
  } finally { if (child && child.exitCode === null) { const exit = new Promise(resolve => child!.once("exit", resolve)); child.kill(); await exit; } if (registered) await host("remove"); await lease.close(); await rm(root, { recursive: true }); }
});
