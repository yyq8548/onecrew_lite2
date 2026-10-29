import assert from "node:assert/strict";
import { createCipheriv, randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdtemp, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test, { after, before } from "node:test";
import { fileURLToPath } from "node:url";
import { extractCreatorHistory } from "./creator-history.ts";
import { prepareSqliteRecovery } from "../src/migration/sqlite-recovery.ts";
import { prepareSqliteStaging } from "../src/migration/sqlite-staging.ts";
import { fingerprintSqlite } from "../src/migration/sqlite-fingerprint.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { backupLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { createPrivateDirectory, verifyPrivateFile } from "../src/storage/private-backup.ts";
import { readVaultMasterKey, readVaultSecret } from "../src/storage/vault-reader.ts";

let oracle: string;
before(async () => {
  oracle = await extractCreatorHistory();
});
after(async () => { if (oracle) await rm(oracle, { recursive: true }); });

function envelope(key: Buffer, plaintext: Buffer): Buffer {
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, nonce);
  return Buffer.concat([nonce, cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]);
}

async function fixture(options: { creatorVersion?: 27 | 90; vaultVersion?: 2 | 3 | 4; wal?: boolean; badTag?: boolean; keyLength?: number } = {}) {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/sqlite-staging-", import.meta.url)));
  const old = await createPrivateDirectory(join(directory, "旧数据"));
  const owner = await createPrivateDirectory(join(directory, "新实例"));
  const creator = join(old, "creator", "creator.sqlite");
  await backupLegacyDatabase(join(oracle, `creator-schema${options.creatorVersion ?? 90}-oracle.sqlite`), creator, "creator");
  const creatorDb = new DatabaseSync(creator);
  if (options.wal) creatorDb.exec("PRAGMA journal_mode=WAL");
  creatorDb.exec("INSERT INTO creators(id,display_name,notes,created_at,updated_at) VALUES('history','历史作者','保留作者数据',1,2)");
  if (!options.wal) creatorDb.close();
  const key = randomBytes(32);
  const plaintext = Buffer.from(JSON.stringify({ session: randomBytes(24).toString("hex") }));
  const vault = join(old, "vault.sqlite");
  const db = new DatabaseSync(vault);
  try {
    db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8"));
    db.exec(`PRAGMA user_version=${options.vaultVersion ?? 3}`);
    db.prepare("INSERT INTO profiles(id,platform,provider_account_id,username,encrypted_secret,created_at,updated_at) VALUES('profile','x','public-id','history',?,'created','updated')").run(envelope(key, plaintext));
    const service = envelope(key, plaintext);
    if (options.badTag) service[service.length - 1] = service[service.length - 1]! ^ 1;
    db.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,created_at,updated_at) VALUES('credential',?,'history','{}',?,'created','updated')")
      .run(options.vaultVersion === 4 ? "email_resend_api_key" : "resend", service);
  } finally { db.close(); }
  const masterKey = join(old, "masterkey");
  await writeFile(masterKey, key.subarray(0, options.keyLength ?? 32), { flag: "wx", mode: 0o600 });
  await verifyPrivateFile(masterKey);
  const lease = await acquireInstanceLock(owner);
  const sources = { creator, vault, masterKey };
  const recovery = await prepareSqliteRecovery(lease, sources);
  const fingerprints = { creator: fingerprintSqlite(creator), vault: fingerprintSqlite(vault) };
  const recoveryBytes = { creator: await readFile(recovery.creator.path), vault: await readFile(recovery.vault.path) };
  return { directory, old, owner, sources, recovery, lease, key, plaintext, fingerprints, recoveryBytes,
    async close() { await lease.close(); if (options.wal) creatorDb.close(); key.fill(0); plaintext.fill(0); await rm(directory, { recursive: true }); } };
}

async function assertUnchanged(value: Awaited<ReturnType<typeof fixture>>) {
  assert.deepEqual(fingerprintSqlite(value.sources.creator), value.fingerprints.creator);
  assert.deepEqual(fingerprintSqlite(value.sources.vault), value.fingerprints.vault);
  assert.deepEqual(await readFile(value.recovery.creator.path), value.recoveryBytes.creator);
  assert.deepEqual(await readFile(value.recovery.vault.path), value.recoveryBytes.vault);
}

async function failedJournal(value: Awaited<ReturnType<typeof fixture>>) {
  const directories = (await readdir(value.owner)).filter(name => name.startsWith("staging-"));
  assert.equal(directories.length, 1);
  const path = join(value.owner, directories[0]!, "journal.json");
  await verifyPrivateFile(path);
  const journal = JSON.parse(await readFile(path, "utf8"));
  assert.equal(journal.activated, false);
  assert.equal(journal.state, "failed");
  return { journal, directory: join(value.owner, directories[0]!) };
}

for (const vaultVersion of [2, 3, 4] as const) test(`SQLite staging upgrades real Vault${vaultVersion} and authenticates every envelope without activation`, async () => {
  const value = await fixture({ vaultVersion, creatorVersion: vaultVersion === 2 ? 27 : 90, wal: vaultVersion === 2 });
  try {
    const sourceCreatorBytes = await readFile(value.sources.creator);
    const sourceWalBytes = vaultVersion === 2 ? await readFile(`${value.sources.creator}-wal`) : undefined;
    const result = await prepareSqliteStaging(value.lease, value);
    assert.equal(result.activated, false);
    assert.equal(result.state, "sqlite_staging_ready");
    assert.equal(result.creator.inspection.version, 97);
    assert.equal(result.vault.inspection.version, 4);
    assert.deepEqual(result.envelopes, { profiles: 1, service_credentials: 1 });
    assert.deepEqual(await readFile(result.masterKey), value.key);
    const copiedKey = await readVaultMasterKey(result.masterKey);
    try {
      for (const [table, id] of [["profiles", "profile"], ["service_credentials", "credential"]] as const) {
        const bytes = readVaultSecret(result.vault.path, copiedKey, table, id);
        assert.deepEqual(bytes, value.plaintext);
        bytes.fill(0);
      }
    } finally { copiedKey.fill(0); }
    await assertUnchanged(value);
    assert.deepEqual(await readFile(value.sources.creator), sourceCreatorBytes);
    if (sourceWalBytes) assert.deepEqual(await readFile(`${value.sources.creator}-wal`), sourceWalBytes);
    const journalText = await readFile(join(result.directory, "journal.json"), "utf8");
    const journal = JSON.parse(journalText);
    assert.equal(journal.state, "sqlite_staging_ready");
    assert.equal(journal.activated, false);
    assert.equal(journalText.includes(value.plaintext.toString()), false);
    assert.equal(result.sharedMemory.length, 4);
    for (const file of [result.creator.path, result.vault.path, result.masterKey]) await verifyPrivateFile(file);
  } finally { await value.close(); }
});

test("SQLite staging rejects a damaged Vault tag and retains protected failed copies", async () => {
  const value = await fixture({ badTag: true });
  try {
    await assert.rejects(prepareSqliteStaging(value.lease, value), /authenticate|authenticat|Unsupported state/);
    const { journal, directory } = await failedJournal(value);
    assert.equal(journal.phase, "authenticate_vault");
    assert.match(journal.sourceHashes.sourceVault.main.sha256, /^[0-9a-f]{64}$/);
    await verifyPrivateFile(join(directory, "masterkey"));
    await assertUnchanged(value);
  } finally { await value.close(); }
});

for (const invalid of ["missing-key", "short-key", "key-junction", "changed-journal", "forged-recovery", "foreign-directory", "forged-path", "cancelled"] as const) test(`SQLite staging ${invalid} fails explicitly before creating SQLite targets`, async () => {
  const value = await fixture({ keyLength: invalid === "short-key" ? 31 : 32 });
  try {
    let sources = value.sources;
    let recovery = value.recovery;
    let signal: AbortSignal | undefined;
    const reason = new Error("requested cancellation");
    if (invalid === "missing-key") sources = { ...sources, masterKey: join(value.old, "missing-key") };
    if (invalid === "key-junction") {
      const alias = join(value.directory, "旧目录链接");
      await symlink(value.old, alias, process.platform === "win32" ? "junction" : "dir");
      sources = { ...sources, masterKey: join(alias, "masterkey") };
    }
    if (invalid === "changed-journal") {
      const path = join(recovery.directory, "journal.json");
      const journal = JSON.parse(await readFile(path, "utf8"));
      journal.state = "failed";
      await writeFile(path, JSON.stringify(journal));
    }
    if (invalid === "forged-recovery") {
      recovery = structuredClone(recovery);
      recovery.fingerprints.creator.schemaSha256 = "0".repeat(64);
    }
    if (invalid === "foreign-directory") recovery = { ...recovery, directory: value.old };
    if (invalid === "forged-path") recovery = { ...recovery, creator: { ...recovery.creator, path: sources.creator } };
    if (invalid === "cancelled") signal = AbortSignal.abort(reason);
    await assert.rejects(prepareSqliteStaging(value.lease, { recovery, sources, ...(signal ? { signal } : {}) }), invalid === "cancelled" ? error => error === reason : /ENOENT|32|journal|aliased|belong|layout/);
    const { journal, directory } = await failedJournal(value);
    if (invalid === "cancelled") assert.equal(journal.cancelled, true);
    assert.equal((await readdir(directory)).some(name => name === "creator" || name === "vault"), false);
    await assertUnchanged(value);
  } finally { await value.close(); }
});
