import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { LegacyQuiescenceLease } from "../src/migration/legacy-quiescence.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("migration native lease conflicts with a real independent Node process and drains every Library lock", { timeout: 120_000 }, async () => {
  const rootPath = fileURLToPath(new URL("../../", import.meta.url)), scratch = join(rootPath, ".scratch", "legacy-quiescence-tests");
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "source-")), ownerDirectory = await createPrivateDirectory(join(root, "new-owner")), old = await createPrivateDirectory(join(root, "legacy")), library = await createPrivateDirectory(join(root, "library"));
  const owner = await acquireInstanceLock(ownerDirectory);
  const sources = { creator: join(old, "creator.sqlite"), vault: join(old, "vault.sqlite"), masterKey: join(old, "master.key"), libraryDirectory: library };
  for (const path of [sources.creator, sources.vault, sources.masterKey, join(old, ".onecrew_lite-service-owner.lock"), join(library, "server.lock"), join(library, "database.lock")]) await writeFile(path, "", { flag: "wx", mode: 0o600 });
  const childModule = fileURLToPath(new URL("./native/lock-owner.ts", import.meta.url));
  try {
    assert.throws(() => Reflect.construct(LegacyQuiescenceLease, [Symbol(), owner, sources, []]), /quiescence_untrusted/);
    for (const lockPath of [join(old, ".onecrew_lite-service-owner.lock"), join(library, "server.lock"), join(library, "database.lock")]) {
      const child = spawn(process.execPath, [childModule, lockPath], { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
      const exit = once(child, "exit");
      try {
        const [receipt] = await once(child.stdout, "data");
        assert.equal(String(receipt).trim(), "locked");
        await assert.rejects(LegacyQuiescenceLease.acquire(owner, { serviceDirectory: old, sources }), /legacy_writer_running/);
      } finally { child.stdin.end("release"); assert.equal((await exit)[0], 0); }
    }
    const lease = await LegacyQuiescenceLease.acquire(owner, { serviceDirectory: old, sources });
    lease.assertQuiescent(sources);
    assert.equal(lease.serviceDirectory, old);
    assert.throws(() => lease.assertQuiescent({ ...sources, creator: sources.vault }), /source_owner_mismatch/);
    assert.throws(() => JSON.stringify(lease), /must not be serialized/);
    await lease.close();
    assert.throws(() => lease.assertQuiescent(sources), /quiescence_unavailable/);
    assert.throws(() => lease.serviceDirectory, /quiescence_unavailable/);
    await writeFile(join(old, ".onecrew-service.json"), "{}", { flag: "wx", mode: 0o600 });
    await assert.rejects(LegacyQuiescenceLease.acquire(owner, { serviceDirectory: old, sources }), /advanced_edition_directory_rejected/);
  } finally { await owner.close(); await rm(root, { recursive: true }); }
});
