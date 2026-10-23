import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { readFileSync, lstatSync } from "node:fs";
import { mkdir, mkdtemp, open, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { loadOrCreateServiceProfile } from "../src/service/profile.ts";
import { startCandidateRuntime } from "../src/service/candidate-runtime.ts";

test("candidate serves actual staged PGlite and assets at the profile origin without activating or writing discovery", { timeout: 120_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/candidate-runtime-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "candidate-")), directory = await createPrivateDirectory(join(root, "owner")), lease = await acquireInstanceLock(directory);
  let candidate: Awaited<ReturnType<typeof startCandidateRuntime>> | undefined;
  try {
    const profile = await loadOrCreateServiceProfile(lease), originalProfile = await readFile(join(directory, "profile.json"));
    const generation = await createPrivateDirectory(join(directory, "generation-candidate")), vaultDirectory = await createPrivateDirectory(join(generation, "vault"));
    const vaultPath = join(vaultDirectory, "vault.sqlite");
    { await using file = await open(vaultPath, "wx", 0o600); }
    { using db = new DatabaseSync(vaultPath); db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8")); db.exec("PRAGMA user_version=4"); }
    await writeFile(join(generation, "masterkey"), randomBytes(32), { flag: "wx", mode: 0o600 });
    const tableDirectory = await createPrivateDirectory(join(generation, "tables"));
    const tables = await createEmbeddedTables(tableDirectory, `http://127.0.0.1:${profile.port}`);
    await tables.close();
    candidate = await startCandidateRuntime(lease, { generationRoot: generation, webAssetsDirectory: fileURLToPath(new URL("../../onecrew_lite-web/dist", import.meta.url)) });
    assert.equal(candidate.origin, `http://127.0.0.1:${profile.port}`);
    const health = await candidate.health();
    assert.equal(health.resource_identity.candidate, true);
    assert.equal(health.resource_identity.kind, "source_development");
    assert.equal(health.resource_identity.profile_port, profile.port);
    assert.match(health.resource_identity.source_tree_sha256, /^[a-f0-9]{64}$/u);
    assert.match(health.resource_identity.executable_sha256, /^[a-f0-9]{64}$/u);
    assert.equal((await fetch(candidate.origin)).status, 200);
    assert.equal((await fetch(`${candidate.origin}/_local/status`)).status, 401);
    assert.equal((await fetch(`${candidate.origin}/api/v1/dynamic-tables`)).status, 401);
    assert.deepEqual(await readFile(join(directory, "profile.json")), originalProfile);
    assert.equal(lstatSync(join(directory, "current.json"), { throwIfNoEntry: false }), undefined);
    assert.equal(lstatSync(join(directory, "server.json"), { throwIfNoEntry: false }), undefined);
    await candidate.close();
    await assert.rejects(candidate.health(), /stopping/);
    candidate = undefined;
    lease.assertOwned();
  } finally { if (candidate) await candidate.close(); await lease.close(); await rm(root, { recursive: true }); }
});
