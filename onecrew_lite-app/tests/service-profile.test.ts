import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { createServer } from "node:net";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { loadOrCreateServiceProfile } from "../src/service/profile.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("new installation persists an operating-system allocated loopback port and never changes it when occupied", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/service-profile-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "allocated-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory);
  const server = createServer();
  try {
    const profile = await loadOrCreateServiceProfile(lease);
    assert.equal(profile.originSource, "new-install");
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(profile.port, "127.0.0.1", () => { server.off("error", reject); resolve(); });
    });
    assert.deepEqual(await loadOrCreateServiceProfile(lease), profile);
  } finally {
    if (server.listening) await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    await lease.close();
    await rm(root, { recursive: true });
  }
});

test("profile persists a verified legacy port and rejects conflicting upgrade identity", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/service-profile-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "profile-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory);
  try {
    const first = await loadOrCreateServiceProfile(lease, 34567);
    assert.equal(first.port, 34567);
    assert.equal(first.originSource, "legacy-upgrade");
    assert.deepEqual(await loadOrCreateServiceProfile(lease), first);
    await assert.rejects(loadOrCreateServiceProfile(lease, 34568), /conflicts/);
    await assert.rejects(loadOrCreateServiceProfile(lease, 0));
  } finally {
    await lease.close();
    await rm(root, { recursive: true });
  }
});
