import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { readStoredAccessCredential } from "../src/identity/legacy-access-credential.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { initializeNewInstallVault } from "../src/storage/new-install-vault.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("fresh private Vault initializes once and never replaces a surviving key", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/new-vault-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "vault-"));
  const directory = await createPrivateDirectory(join(root, "owner"));
  const vault = join(directory, "vault.sqlite"), keyPath = join(directory, "master.key");
  const lease = await acquireInstanceLock(directory);
  try {
    await initializeNewInstallVault(lease, vault, keyPath);
    const key = await readFile(keyPath);
    assert.equal(key.length, 32);
    assert.deepEqual(await readStoredAccessCredential(vault, keyPath), { state: "missing" });
    await initializeNewInstallVault(lease, vault, keyPath);
    assert.deepEqual(await readFile(keyPath), key);
    await rm(vault);
    await assert.rejects(initializeNewInstallVault(lease, vault, keyPath), /both database and master key/);
    assert.deepEqual(await readFile(keyPath), key);
  } finally { await lease.close(); await rm(root, { recursive: true }); }
});

test("fresh Vault rejects paths outside its instance without creating files", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/new-vault-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "paths-"));
  const directory = await createPrivateDirectory(join(root, "owner"));
  const lease = await acquireInstanceLock(directory);
  try {
    await assert.rejects(initializeNewInstallVault(lease, join(root, "foreign.sqlite"), join(directory, "master.key")), /directly to the instance/);
    const keyPath = join(directory, "master.key");
    await writeFile(keyPath, Buffer.alloc(32, 7), { flag: "wx", mode: 0o600 });
    await assert.rejects(initializeNewInstallVault(lease, join(directory, "vault.sqlite"), keyPath), /both database and master key/);
  } finally { await lease.close(); await rm(root, { recursive: true }); }
});
