import { createHash, randomUUID, timingSafeEqual } from "node:crypto";
import { constants, lstatSync } from "node:fs";
import { lstat, open, realpath, rename } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { backupLegacyDatabase, inspectLegacyDatabase } from "../storage/legacy-sqlite.ts";
import { upgradeCreatorStagingCopy } from "../storage/creator-upgrade.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { decryptVaultEnvelope, readVaultMasterKey } from "../storage/vault-reader.ts";
import { upgradeVaultStagingCopy, validateVaultSchema } from "../storage/vault-schema.ts";
import { fingerprintSqlite } from "./sqlite-fingerprint.ts";
import type { prepareSqliteRecovery } from "./sqlite-recovery.ts";

type Recovery = Awaited<ReturnType<typeof prepareSqliteRecovery>>;
type Sources = { creator: string; vault: string; masterKey: string };
const samePath = (left: string, right: string) => relative(resolve(left), resolve(right)) === "";
const contains = (parent: string, child: string) => {
  const part = relative(parent, child);
  return part === "" || part !== ".." && !part.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) && !isAbsolute(part);
};

async function canonicalFile(path: string): Promise<string> {
  const absolute = resolve(path);
  const canonical = await realpath(absolute);
  const metadata = await lstat(absolute, { bigint: true });
  if (!samePath(absolute, canonical) || !metadata.isFile() || metadata.isSymbolicLink() || metadata.nlink !== 1n) throw new Error("Migration source must be a regular file without links or aliased ancestors");
  return canonical;
}

async function hashFile(path: string, required: boolean) {
  const before = lstatSync(path, { bigint: true, throwIfNoEntry: false });
  if (!before) {
    if (required) throw new Error("Required SQLite artifact is missing");
    return null;
  }
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n) throw new Error("SQLite artifact must be a regular file without links");
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.dev !== before.dev || opened.ino !== before.ino || opened.nlink !== 1n) throw new Error("SQLite artifact identity changed while opening");
  const hash = createHash("sha256");
  const bytes = Buffer.alloc(256 * 1024);
  for (;;) {
    const read = await file.read(bytes);
    if (read.bytesRead === 0) break;
    hash.update(bytes.subarray(0, read.bytesRead));
  }
  const after = await file.stat({ bigint: true });
  const current = await lstat(path, { bigint: true });
  if (!current.isFile() || current.isSymbolicLink() || current.dev !== opened.dev || current.ino !== opened.ino || current.nlink !== 1n ||
      after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs) throw new Error("SQLite artifact changed while hashing");
  return { sha256: hash.digest("hex"), bytes: after.size.toString(), device: after.dev.toString(), inode: after.ino.toString() };
}

async function sqliteArtifacts(path: string) {
  return { main: await hashFile(path, true), wal: await hashFile(`${path}-wal`, false), shm: await hashFile(`${path}-shm`, false) };
}

async function readPrivateJournal(path: string): Promise<unknown> {
  await verifyPrivateFile(path);
  const before = await lstat(path, { bigint: true });
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.dev !== before.dev || opened.ino !== before.ino || opened.nlink !== 1n || opened.size > 1024n * 1024n) throw new Error("Recovery journal identity or size is invalid");
  const text = await file.readFile();
  const after = await file.stat({ bigint: true });
  const current = await lstat(path, { bigint: true });
  if (!current.isFile() || current.isSymbolicLink() || current.nlink !== 1n || current.dev !== opened.dev || current.ino !== opened.ino || after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs) throw new Error("Recovery journal changed while reading");
  await verifyPrivateFile(path);
  return parseApplicationJson(text);
}

async function writeJournal(directory: string, value: unknown): Promise<void> {
  const temporary = join(directory, `journal-${randomUUID()}.json`);
  {
    await using file = await open(temporary, "wx", 0o600);
    await file.writeFile(JSON.stringify(value));
    await file.sync();
  }
  await verifyPrivateFile(temporary);
  await rename(temporary, join(directory, "journal.json"));
}

function authenticateVault(path: string, key: Uint8Array, signal?: AbortSignal) {
  using db = new DatabaseSync(path, { readOnly: true });
  db.exec("PRAGMA query_only=ON; PRAGMA busy_timeout=5000; BEGIN");
  if (validateVaultSchema(db) !== 4) throw new Error("Staged Vault authentication requires schema4");
  const counts = { profiles: 0, service_credentials: 0 };
  for (const table of ["profiles", "service_credentials"] as const) {
    for (const row of db.prepare(`SELECT encrypted_secret FROM ${table}`).iterate()) {
      signal?.throwIfAborted();
      if (!(row.encrypted_secret instanceof Uint8Array)) throw new Error("Vault envelope must have BLOB storage type");
      const plaintext = decryptVaultEnvelope(key, row.encrypted_secret);
      plaintext.fill(0);
      counts[table]++;
    }
  }
  return counts;
}

// 仅构造隔离 SQLite staging；账号、Library、旧 writer 与安装激活由后续门槛管理。
export async function prepareSqliteStaging(lease: InstanceLease, input: { recovery: Recovery; sources: Sources; signal?: AbortSignal }) {
  lease.assertOwned();
  const id = randomUUID();
  const directory = await createPrivateDirectory(join(lease.directory, `staging-${id}`));
  const initial = { id, activated: false, directory, recoveryId: input.recovery.id, createdAt: new Date().toISOString() } as const;
  let phase = "validate_recovery";
  let completed = false;
  let sourceHashes: Record<string, Awaited<ReturnType<typeof sqliteArtifacts>>> | undefined;
  await writeJournal(directory, { ...initial, state: "preparing", phase });
  await using journal = {
    async [Symbol.asyncDispose]() {
      if (!completed) await writeJournal(directory, { ...initial, state: "failed", phase, cancelled: input.signal?.aborted ?? false, ...(sourceHashes ? { sourceHashes } : {}) });
    },
  };
  input.signal?.throwIfAborted();
  const recovery = input.recovery;
  z.guid().parse(recovery.id);
  z.iso.datetime().parse(recovery.createdAt);
  const expectedRecovery = join(lease.directory, `recovery-${recovery.id}`);
  if (recovery.state !== "sqlite_recovery_ready" || recovery.activated !== false || !samePath(recovery.directory, expectedRecovery)) throw new Error("Recovery does not belong to the current lease directory");
  await verifyPrivateDirectory(expectedRecovery);
  const creatorRecovery = join(expectedRecovery, "creator", "creator.sqlite");
  const vaultRecovery = join(expectedRecovery, "vault", "vault.sqlite");
  if (!samePath(recovery.creator.path, creatorRecovery) || !samePath(recovery.vault.path, vaultRecovery)) throw new Error("Recovery database paths differ from the journal layout");
  if (!isDeepStrictEqual(await readPrivateJournal(join(expectedRecovery, "journal.json")), recovery)) throw new Error("Private recovery journal differs from the supplied recovery record");
  const sources = { creator: await canonicalFile(input.sources.creator), vault: await canonicalFile(input.sources.vault), masterKey: await canonicalFile(input.sources.masterKey) };
  await verifyPrivateFile(creatorRecovery);
  await verifyPrivateFile(vaultRecovery);
  for (const path of [sources.creator, sources.vault, sources.masterKey, creatorRecovery, vaultRecovery]) {
    if (contains(dirname(path), directory) || contains(directory, dirname(path))) throw new Error("Staging and source directories must not contain each other");
  }
  if (samePath(sources.creator, sources.vault) || samePath(sources.creator, sources.masterKey) || samePath(sources.vault, sources.masterKey)) throw new Error("Migration sources must be distinct files");
  const before = {
    sourceCreator: await sqliteArtifacts(sources.creator), sourceVault: await sqliteArtifacts(sources.vault),
    recoveryCreator: await sqliteArtifacts(creatorRecovery), recoveryVault: await sqliteArtifacts(vaultRecovery),
  };
  sourceHashes = before;
  const logical = { creator: fingerprintSqlite(sources.creator), vault: fingerprintSqlite(sources.vault) };
  if (!isDeepStrictEqual(logical, recovery.fingerprints) ||
      !isDeepStrictEqual(fingerprintSqlite(creatorRecovery), logical.creator) || !isDeepStrictEqual(fingerprintSqlite(vaultRecovery), logical.vault) ||
      !isDeepStrictEqual(inspectLegacyDatabase(creatorRecovery, "creator"), recovery.creator.inspection) ||
      !isDeepStrictEqual(inspectLegacyDatabase(vaultRecovery, "vault"), recovery.vault.inspection)) throw new Error("Source or recovery database no longer matches the recovery journal");
  phase = "copy_private_master_key";
  const key = await readVaultMasterKey(sources.masterKey, "legacy-source");
  using secret = { [Symbol.dispose]() { key.fill(0); } };
  const masterKey = join(directory, "masterkey");
  {
    await using file = await open(masterKey, "wx", 0o600);
    await verifyPrivateFile(masterKey);
    await file.writeFile(key);
    await file.sync();
  }
  await verifyPrivateFile(masterKey);
  phase = "copy_sqlite_staging";
  lease.assertOwned();
  input.signal?.throwIfAborted();
  const creatorPath = join(directory, "creator", "creator.sqlite");
  const vaultPath = join(directory, "vault", "vault.sqlite");
  await backupLegacyDatabase(creatorRecovery, creatorPath, "creator");
  await backupLegacyDatabase(vaultRecovery, vaultPath, "vault");
  await writeJournal(directory, { ...initial, state: "preparing", phase: "upgrade_sqlite_staging", sourceHashes: before });
  phase = "upgrade_creator";
  lease.assertOwned();
  input.signal?.throwIfAborted();
  const creatorUpgrade = await upgradeCreatorStagingCopy({ sourcePath: creatorRecovery, stagingPath: creatorPath });
  phase = "upgrade_vault";
  input.signal?.throwIfAborted();
  const vaultUpgrade = await upgradeVaultStagingCopy({ sourcePath: vaultRecovery, stagingPath: vaultPath });
  phase = "authenticate_vault";
  const envelopes = authenticateVault(vaultPath, key, input.signal);
  phase = "verify_unchanged_sources";
  lease.assertOwned();
  input.signal?.throwIfAborted();
  const copiedKey = await readVaultMasterKey(masterKey);
  using copiedSecret = { [Symbol.dispose]() { copiedKey.fill(0); } };
  const originalKey = await readVaultMasterKey(sources.masterKey, "legacy-source");
  using originalSecret = { [Symbol.dispose]() { originalKey.fill(0); } };
  if (!timingSafeEqual(key, copiedKey) || !timingSafeEqual(key, originalKey)) throw new Error("Vault master key changed or copied bytes differ");
  if (!isDeepStrictEqual(fingerprintSqlite(sources.creator), logical.creator) || !isDeepStrictEqual(fingerprintSqlite(sources.vault), logical.vault) ||
      !isDeepStrictEqual(fingerprintSqlite(creatorRecovery), logical.creator) || !isDeepStrictEqual(fingerprintSqlite(vaultRecovery), logical.vault)) throw new Error("Source or recovery logical data changed during staging");
  const after = {
    sourceCreator: await sqliteArtifacts(sources.creator), sourceVault: await sqliteArtifacts(sources.vault),
    recoveryCreator: await sqliteArtifacts(creatorRecovery), recoveryVault: await sqliteArtifacts(vaultRecovery),
  };
  const sharedMemory = Object.keys(before).map(name => {
    const key = name as keyof typeof before;
    if (!isDeepStrictEqual(before[key].main, after[key].main) || !isDeepStrictEqual(before[key].wal, after[key].wal)) throw new Error("Source or recovery SQLite main/WAL changed during staging");
    return { database: name, changed: !isDeepStrictEqual(before[key].shm, after[key].shm), before: before[key].shm, after: after[key].shm };
  });
  phase = "verify_recovery_journal";
  if (!isDeepStrictEqual(await readPrivateJournal(join(expectedRecovery, "journal.json")), recovery)) throw new Error("Recovery journal changed during staging");
  const creatorInspection = inspectLegacyDatabase(creatorPath, "creator");
  const vaultInspection = inspectLegacyDatabase(vaultPath, "vault");
  if (creatorInspection.version !== creatorInspection.currentVersion || !creatorInspection.schemaValidated || vaultInspection.version !== 4 || !vaultInspection.schemaValidated) throw new Error("Staged SQLite versions are not fully validated");
  const result = { ...initial, state: "sqlite_staging_ready", creator: { path: creatorPath, inspection: creatorInspection, upgrade: creatorUpgrade },
    vault: { path: vaultPath, inspection: vaultInspection, upgrade: vaultUpgrade }, masterKey, envelopes, sourceHashes: before, sharedMemory,
    fingerprints: { creator: fingerprintSqlite(creatorPath), vault: fingerprintSqlite(vaultPath) } } as const;
  lease.assertOwned();
  input.signal?.throwIfAborted();
  await writeJournal(directory, result);
  input.signal?.throwIfAborted();
  lease.assertOwned();
  completed = true;
  return result;
}
