import assert from "node:assert/strict";
import { createCipheriv, randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { link, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { decryptVaultEnvelope, readVaultMasterKey, readVaultSecret } from "../src/storage/vault-reader.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("Rust AES-GCM envelope layout reads from a real v4 vault and rejects damaged authentication", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-vault-", import.meta.url)));
  const key = randomBytes(32);
  const plaintext = Buffer.from(JSON.stringify({ value: randomBytes(24).toString("hex") }));
  try {
    const nonce = randomBytes(12);
    const cipher = createCipheriv("aes-256-gcm", key, nonce);
    const envelope = Buffer.concat([nonce, cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]);
    const path = join(directory, "vault.sqlite");
    const db = new DatabaseSync(path);
    db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8"));
    db.exec("PRAGMA user_version = 4");
    db.prepare("INSERT INTO service_credentials (id,provider,label,encrypted_secret,created_at,updated_at) VALUES (?,?,?,?,?,?)").run("id", "gmail", "test", envelope, "now", "now");
    db.close();
    const restored = readVaultSecret(path, key, "service_credentials", "id");
    assert.deepEqual(restored, plaintext);
    restored.fill(0);
    const damaged = Buffer.from(envelope);
    damaged[damaged.length - 1] = damaged[damaged.length - 1]! ^ 1;
    assert.throws(() => decryptVaultEnvelope(key, damaged));
    assert.throws(() => decryptVaultEnvelope(randomBytes(32), envelope));
    assert.throws(() => decryptVaultEnvelope(randomBytes(31), envelope), /32 bytes/);
    assert.throws(() => decryptVaultEnvelope(key, envelope.subarray(0, 27)), /truncated/);
    assert.throws(() => readVaultSecret(path, key, "service_credentials", "missing"), /not found/);
    const newer = new DatabaseSync(path);
    newer.exec("PRAGMA user_version = 5");
    newer.close();
    assert.throws(() => readVaultSecret(path, key, "service_credentials", "id"), /version 4/);
  } finally {
    key.fill(0);
    plaintext.fill(0);
    await rm(directory, { recursive: true });
  }
});

test("master keys require private regular 32-byte files and never create replacement keys", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-key-", import.meta.url)));
  const key = randomBytes(32);
  try {
    const privatePath = await createPrivateDirectory(join(directory, "private"));
    const path = join(privatePath, "vault.key");
    await writeFile(path, key, { mode: 0o600, flag: "wx" });
    const restored = await readVaultMasterKey(path);
    assert.deepEqual(restored, key);
    restored.fill(0);
    for (const length of [0, 31, 33]) {
      const wrong = join(privatePath, `wrong-${length}.key`);
      await writeFile(wrong, randomBytes(length), { mode: 0o600 });
      await assert.rejects(readVaultMasterKey(wrong), /32/);
    }
    const junction = join(directory, "junction");
    await symlink(privatePath, junction, process.platform === "win32" ? "junction" : "dir");
    await assert.rejects(readVaultMasterKey(join(junction, "vault.key")));
    const directoryKey = join(privatePath, "directory.key");
    await createPrivateDirectory(directoryKey);
    await assert.rejects(readVaultMasterKey(directoryKey));
    const missing = join(privatePath, "missing.key");
    await assert.rejects(readVaultMasterKey(missing));
    await assert.rejects(readFile(missing), { code: "ENOENT" });
    assert.deepEqual(await readFile(path), key);
    const hardLink = join(privatePath, "hardlink.key");
    await link(path, hardLink);
    await assert.rejects(readVaultMasterKey(path), /hard link/);
    await rm(hardLink);
    if (process.platform !== "win32") {
      const symbolic = join(privatePath, "symbolic.key");
      await symlink(path, symbolic);
      await assert.rejects(readVaultMasterKey(symbolic));
    }
  } finally {
    key.fill(0);
    await rm(directory, { recursive: true });
  }
});
