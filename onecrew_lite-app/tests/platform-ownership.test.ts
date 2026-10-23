import { test } from "node:test";
import assert from "node:assert/strict";
import { createCipheriv, createHash, randomBytes, randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdir, open, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { prepareLegacyPlatformOwnership, PreparedPlatformOwnership } from "../src/platforms/browser/ownership.ts";
import type { VerifiedMigrationCredential } from "../src/identity/access-verification.ts";

test("real encrypted legacy platform review requires explicit matching refs and genuine migration authority", { timeout: 180_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await createPrivateDirectory(join(scratch, `po-${randomUUID().slice(0, 12)}`)), lease = await acquireInstanceLock(root);
  const generation = await createPrivateDirectory(join(root, `generation-${randomUUID()}`));
  const owner = { accountId: "selected-owner", tenantId: "tenant", baseId: "base" }, ownerKey = createHash("sha256").update(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId])).digest("hex");
  const directory = await createPrivateDirectory(join(generation, `account-${ownerKey}`)), creatorPath = join(directory, "creator.sqlite");
  const vaultDirectory = await createPrivateDirectory(join(generation, "vault")), vaultPath = join(vaultDirectory, "vault.sqlite"), masterKeyPath = join(generation, "masterkey");
  const key = randomBytes(32), selected = randomUUID(), unselected = randomUUID();
  try {
    await initializeNewCreatorDatabase(creatorPath);
    { await using file = await open(vaultPath, "wx", 0o600); }
    await writeFile(masterKeyPath, key, { flag: "wx", mode: 0o600 });
    {
      using vault = new DatabaseSync(vaultPath), creator = new DatabaseSync(creatorPath);
      vault.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8")); vault.exec("PRAGMA user_version=4");
      for (const id of [selected, unselected]) {
        const secret = { provider_account_id: id, cookies: [], x_chat_passcode: null, instagram_session_settings: null, telegram_api_id: null, telegram_api_hash: null, telegram_session: null };
        const nonce = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, nonce), envelope = Buffer.concat([nonce, cipher.update(JSON.stringify(secret)), cipher.final(), cipher.getAuthTag()]);
        vault.prepare("INSERT INTO profiles VALUES(?,'x',?,'local',?,'1','2')").run(id, id, envelope);
        creator.prepare("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES(?,'x','profile','local',1,2)").run(id);
      }
    }
    const input = { creatorPath, vaultPath, masterKeyPath, owner, deviceId: "dev_local", selectedAccountRefIds: [selected] };
    const prepared = await prepareLegacyPlatformOwnership(lease, input);
    assert.deepEqual(prepared.selection.profiles.map(value => value.accountRefId), [selected]);
    assert.equal(prepared.selection.profiles.some(value => value.accountRefId === unselected), false);
    assert.equal(JSON.stringify(prepared.selection).includes("cookies"), false);
    assert.throws(() => JSON.stringify(prepared), /not_serializable/u);
    assert.throws(() => Reflect.construct(PreparedPlatformOwnership, [Symbol(), lease, "", {}, new Map()]), /private_preparation_required/u);
    await assert.rejects(prepared.issue(lease, {} as VerifiedMigrationCredential), /verified_credential_required/u);
    await assert.rejects(prepareLegacyPlatformOwnership(lease, { ...input, selectedAccountRefIds: [selected, selected] }), /duplicate_selection/u);
    await assert.rejects(prepareLegacyPlatformOwnership(lease, { ...input, selectedAccountRefIds: [randomUUID()] }));
    { using creator = new DatabaseSync(creatorPath); creator.prepare("UPDATE account_refs SET active=0 WHERE id=?").run(selected); }
    assert.throws(() => prepared.assertPrepared(lease), /source_changed/u);
    await assert.rejects(prepareLegacyPlatformOwnership(lease, input));
  } finally { key.fill(0); await lease.close(); await rm(root, { recursive: true }); }
});
