import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { createPrivateDirectory, verifyPrivateDirectory } from "../src/storage/private-backup.ts";
import { verifyLegacyPrivateDirectory, verifyLegacyPrivateFile } from "../src/storage/legacy-private-source.ts";

test("legacy Windows inherited owner/SYSTEM/Administrators ACL is accepted while another identity is refused", { skip: process.platform !== "win32", timeout: 60_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/legacy-acl-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "acl-"));
  const directory = await createPrivateDirectory(join(root, "legacy-profile"));
  async function grant(identity: "Administrators" | "Everyone") {
    await promisify(execFile)(join(process.env.SystemRoot!, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"),
      ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("./legacy-acl-fixture.ps1", import.meta.url)), "-Directory", directory, "-Grant", identity],
      { windowsHide: true, timeout: 30_000, maxBuffer: 8192 });
  }
  try {
    await grant("Administrators");
    const inherited = join(directory, "inherited"), file = join(inherited, "non-secret-fixture");
    await mkdir(inherited); await writeFile(file, "non-secret", { flag: "wx" });
    await verifyLegacyPrivateDirectory(directory); await verifyLegacyPrivateDirectory(inherited); await verifyLegacyPrivateFile(file);
    await assert.rejects(verifyPrivateDirectory(inherited));
    await grant("Everyone");
    await assert.rejects(verifyLegacyPrivateDirectory(directory));
    await assert.rejects(verifyLegacyPrivateDirectory(inherited));
    await assert.rejects(verifyLegacyPrivateFile(file));
  } finally { await rm(root, { recursive: true }); }
});
