import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { inspect } from "node:util";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { openDatabase } from "../../onecrew_lite-library/dist/database.js";
import { createCredentialStore } from "../../onecrew_lite-library/dist/modules/access/store.js";
import { readLibraryMigrationAccount } from "../src/identity/library-migration-account.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("isolated Library inspection selects only its active local row and never revives historical accounts", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/library-account-tests", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "account-"));
  const owner = await createPrivateDirectory(join(directory, "owner"));
  const copy = await createPrivateDirectory(join(owner, "copy"));
  await createPrivateDirectory(join(copy, "pglite"));
  const lease = await acquireInstanceLock(owner);
  const credential = (userId: string) => {
    const credentialId = randomUUID();
    return { bearer: `${credentialId}_${randomBytes(32).toString("base64url")}`, installation: {
      installation_id: randomUUID(), credential_id: credentialId, access_user_id: userId,
      access_device_id: `dev_${randomBytes(18).toString("base64url")}`, access_pairing_id: `pair_${randomBytes(18).toString("base64url")}`,
      source_instance_id: randomUUID(), client_platform: "windows" as const, app_version: "0.1.4", status: "active" as const,
      revision: 1, created_at: 1, updated_at: 1, last_seen_at: null, revoked_at: null,
    } };
  };
  try {
    const first = credential("historical-owner"), selected = credential("active-owner");
    const database = await openDatabase(copy);
    try { const store = createCredentialStore(database.db); await store.save(first); await store.save(selected); }
    finally { await database.close(); }
    const loaded = await readLibraryMigrationAccount(lease, copy);
    assert.equal(loaded.snapshot.state, "present");
    assert.equal(loaded.snapshot.userId, "active-owner");
    assert.deepEqual(loaded.knownAccountIds, ["active-owner", "historical-owner"]);
    assert.equal(loaded.credential!.exposeBearer(), selected.bearer);
    assert.ok(!inspect(loaded).includes(selected.bearer));
    assert.throws(() => JSON.stringify(loaded), /cannot be serialized/);
    loaded.credential!.dispose();
    const reopened = await openDatabase(copy);
    try { await createCredentialStore(reopened.db).clear(); }
    finally { await reopened.close(); }
    const signedOut = await readLibraryMigrationAccount(lease, copy);
    assert.equal(signedOut.snapshot.state, "missing");
    assert.equal(signedOut.credential, undefined);
    assert.notEqual(signedOut.sourceFingerprint, loaded.sourceFingerprint);
    assert.deepEqual(signedOut.knownAccountIds, loaded.knownAccountIds);
  } finally { await lease.close(); await rm(directory, { recursive: true }); }
});
