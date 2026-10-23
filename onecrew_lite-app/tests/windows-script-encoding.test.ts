import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import { PayloadPermissions } from "../src/packaging/payload-cache.ts";

test("PowerShell scripts with non-ASCII text declare UTF-8 for Windows PowerShell", async () => {
  const source = fileURLToPath(new URL("../src/", import.meta.url));
  for (const entry of await readdir(source, { recursive: true })) {
    if (!entry.endsWith(".ps1")) continue;
    const bytes = await readFile(join(source, entry));
    if (bytes.some(byte => byte > 127)) assert.deepEqual([...bytes.subarray(0, 3)], [239, 187, 191], entry);
  }
});

test("Chinese Windows decoding creates and verifies a real private plugin tree", { skip: process.platform !== "win32", timeout: 30_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/windows-script-encoding/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "native-"));
  const verifier = process.env.ONECREW_ENCODING_VERIFIER ?? fileURLToPath(new URL("../src/storage/private-backup.ps1", import.meta.url));
  try {
    const result = await promisify(execFile)(join(process.env.SystemRoot!, "System32/WindowsPowerShell/v1.0/powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("./windows-script-encoding.ps1", import.meta.url)), "-Verifier", verifier, "-Directory", join(directory, "private")], { windowsHide: true });
    assert.deepEqual(result.stdout.trim().split(/\r?\n/), ["private-acl-verified", "private-acl-verified"]);
    const permissions = new PayloadPermissions(await readFile(verifier));
    await permissions.create(join(directory, "embedded"), ["child"]);
    await permissions.verifyTree(join(directory, "embedded"));
  } finally { await rm(directory, { recursive: true }); }
});
