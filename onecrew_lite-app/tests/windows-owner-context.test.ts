import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import { verifyPrivateFile, verifyPrivateTree } from "../src/storage/private-backup.ts";
import { verifyLegacyPrivateFile } from "../src/storage/legacy-private-source.ts";

test("Windows native token ownership preserves private ACL checks", { skip: process.platform !== "win32", timeout: 60_000 }, async t => {
  const scratch = fileURLToPath(new URL("../../.scratch/windows-owner-context/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "owner-"));
  const powershell = join(process.env.SystemRoot!, "System32/WindowsPowerShell/v1.0/powershell.exe");
  const execute = promisify(execFile);
  const argumentsFor = (script: string, ...args: string[]) => ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL(script, import.meta.url)), ...args];
  try {
    const { stdout } = await execute(powershell, argumentsFor("./windows-owner-context.ps1", "-Directory", join(root, "private")), { windowsHide: true });
    const result = JSON.parse(stdout.trim().split(/\r?\n/).at(-1)!);
    await t.test("elevated administrator native file creation", { skip: !result.administrator }, () => {
      assert.ok([result.user, "S-1-5-32-544"].includes(result.owner));
    });
    await assert.rejects(verifyPrivateFile(result.file), /DACL|identity/);
    await assert.rejects(verifyPrivateTree(join(root, "private")), /DACL|identity/);
    await assert.rejects(verifyLegacyPrivateFile(result.file), /acl_invalid/);
    await assert.rejects(execute(powershell, argumentsFor("../src/platforms/browser/browser-profile-acl.ps1", "-Root", join(root, "private")), { windowsHide: true }), /access rules|identity/);
  } finally { await rm(root, { recursive: true }); }
});
