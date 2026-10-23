import test from "node:test";
import assert from "node:assert/strict";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { readStorageSql } from "../src/storage/sql-resource.ts";
import { decryptVaultEnvelope } from "../src/storage/vault-reader.ts";
import { encryptOwnerVaultSecret, OwnerPlatformVaultStore } from "../src/platforms/browser/owner-vault-store.ts";

test("private owner Vault generations preserve same labels independently and recover actual SQLite changesets after publication", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await createPrivateDirectory(join(scratch, `ov-${randomUUID().slice(0, 12)}`));
  const keys: Buffer[] = [], databases: DatabaseSync[] = [];
  try {
    for (const ownerId of ["owner-A", "owner-B"]) {
      const directory = await createPrivateDirectory(join(root, ownerId)), generation = `platform-vault-${randomUUID()}`, storage = await createPrivateDirectory(join(directory, generation));
      const key = randomBytes(32), plaintext = Buffer.from(JSON.stringify({ value: randomBytes(32).toString("hex") })), encrypted = encryptOwnerVaultSecret(key, plaintext); keys.push(key);
      await writeFile(join(storage, "masterkey"), key, { mode: 0o600, flag: "wx" }); await writeFile(join(storage, "vault.sqlite"), "", { mode: 0o600, flag: "wx" });
      const vault = new DatabaseSync(join(storage, "vault.sqlite")); vault.exec(readStorageSql("vault-schema.sql") + ";PRAGMA user_version=4");
      const id = randomUUID(); vault.prepare("INSERT INTO service_credentials(id,provider,label,encrypted_secret,created_at,updated_at) VALUES (?,'youtube','same-label',?,'now','now')").run(id, encrypted);
      const actual = vault.prepare("SELECT encrypted_secret FROM service_credentials WHERE id=?").get(id)!.encrypted_secret as Uint8Array;
      const clear = decryptVaultEnvelope(key, actual); assert.deepEqual(clear, plaintext); clear.fill(0); plaintext.fill(0); vault.close();
      const creatorPath = join(directory, "creator.sqlite"); await initializeNewCreatorDatabase(creatorPath); const creator = new DatabaseSync(creatorPath); databases.push(creator);
      const accountRefId = randomUUID(), manifest = Buffer.from(JSON.stringify({ format: 2, owner: { accountId: ownerId, tenantId: "tenant", baseId: "base" }, deviceId: "local-storage-fixture", profiles: [], services: [{ accountRefId, credentialId: id, provider: "youtube", vaultRevision: createHash("sha256").update(encrypted).digest("hex") }], storage: { kind: "account_vault", directory: generation } }));
      const receiptKey = `platform-vault-store:${randomUUID()}`, fingerprint = createHash("sha256").update(manifest).digest("hex"), session = creator.createSession();
      creator.exec("BEGIN IMMEDIATE"); creator.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,created_at,updated_at) VALUES (?,'platform_vault_commit',?,'completed',1,1)").run(receiptKey, fingerprint);
      creator.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES (?,'youtube','service_credential','same-label',1,'available',1,1)").run(accountRefId);
      const changeset = Buffer.from(session.changeset()); creator.exec("ROLLBACK"); session.close(); assert.equal(creator.prepare("SELECT count(*) AS n FROM idempotency_records").get()!.n, 0);
      const journals = await createPrivateDirectory(join(directory, "platform-vault-journals")), journalPath = join(journals, `${randomUUID()}.json`);
      await writeFile(journalPath, JSON.stringify({ format: 1, state: "prepared", oldManifest: null, newManifest: manifest.toString("base64"), receiptKey, changeset: changeset.toString("base64") }), { mode: 0o600, flag: "wx" });
      await writeFile(join(directory, "platform-accounts.json"), manifest, { mode: 0o600, flag: "wx" });
      await OwnerPlatformVaultStore.recoverStorage(directory, creator);
      assert.equal(creator.prepare("SELECT state FROM idempotency_records WHERE key=?").get(receiptKey)!.state, "completed");
      assert.equal(JSON.parse(await readFile(journalPath, "utf8")).state, "completed");
      await OwnerPlatformVaultStore.recoverStorage(directory, creator); assert.equal(creator.prepare("SELECT count(*) AS n FROM idempotency_records").get()!.n, 1);
      const abandonedPath = join(journals, `${randomUUID()}.json`);
      await writeFile(abandonedPath, JSON.stringify({ format: 1, state: "prepared", oldManifest: manifest.toString("base64"), newManifest: "unpublished", receiptKey: `platform-vault-store:${randomUUID()}`, changeset: "" }), { mode: 0o600, flag: "wx" });
      await OwnerPlatformVaultStore.recoverStorage(directory, creator);
      assert.equal(JSON.parse(await readFile(abandonedPath, "utf8")).state, "aborted"); assert.deepEqual(await readFile(join(directory, "platform-accounts.json")), manifest);
      const completed = JSON.parse(await readFile(journalPath, "utf8")); completed.state = "prepared"; await writeFile(journalPath, JSON.stringify(completed));
      const damaged = new DatabaseSync(join(storage, "vault.sqlite")), broken = Buffer.from(encrypted); broken[broken.length - 1] = broken.at(-1)! ^ 1;
      damaged.prepare("UPDATE service_credentials SET encrypted_secret=? WHERE id=?").run(broken, id); damaged.close();
      await assert.rejects(OwnerPlatformVaultStore.recoverStorage(directory, creator), /recovery_vault_changed/u);
      assert.equal(creator.prepare("SELECT count(*) AS n FROM idempotency_records").get()!.n, 1); assert.deepEqual(await readFile(join(directory, "platform-accounts.json")), manifest);
    }
    assert.throws(() => decryptVaultEnvelope(keys[1]!, encryptOwnerVaultSecret(keys[0]!, Buffer.from("private"))));
  } finally { for (const key of keys) key.fill(0); for (const database of databases) database.close(); await rm(root, { recursive: true }); }
});
