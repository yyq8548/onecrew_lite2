import { createHash, randomUUID } from "node:crypto";
import { constants } from "node:fs";
import { chmod, copyFile, lstat, open, readFile, readdir, realpath } from "node:fs/promises";
import { isDeepStrictEqual } from "node:util";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { acquireDatabaseLock } from "onecrew_lite_tables/runtime";
import { z } from "zod";
import type { InstanceLease } from "../service/instance-lock.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile, verifyPrivateTree } from "../storage/private-backup.ts";
import { writeMigrationJournal } from "./journal.ts";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { LegacyQuiescenceLease } from "./legacy-quiescence.ts";

async function directoryFingerprint(directory: string): Promise<{ path: string; bytes: string; sha256: string }[]> {
  const rows: { path: string; bytes: string; sha256: string }[] = [];
  async function visit(current: string): Promise<void> {
    const entries = await readdir(current, { withFileTypes: true });
    entries.sort((a, b) => Buffer.compare(Buffer.from(a.name), Buffer.from(b.name)));
    for (const entry of entries) {
      const path = join(current, entry.name);
      const before = await lstat(path, { bigint: true });
      if (before.isSymbolicLink()) throw new Error("Library recovery source contains a symbolic link");
      if (before.isDirectory()) { await visit(path); continue; }
      if (!before.isFile() || before.nlink !== 1n) throw new Error("Library recovery requires regular independent files");
      const file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
      try {
        const opened = await file.stat({ bigint: true });
        if (opened.dev !== before.dev || opened.ino !== before.ino || !opened.isFile()) throw new Error("Library recovery file changed while opening");
        const hash = createHash("sha256");
        for await (const chunk of file.createReadStream({ autoClose: false })) hash.update(chunk);
        const after = await file.stat({ bigint: true }), currentFile = await lstat(path, { bigint: true });
        if (after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs || currentFile.dev !== opened.dev || currentFile.ino !== opened.ino || currentFile.isSymbolicLink()) throw new Error("Library recovery file changed while reading");
        rows.push({ path: relative(directory, path).replaceAll("\\", "/"), bytes: String(opened.size), sha256: hash.digest("hex") });
      } finally { await file.close(); }
    }
  }
  await visit(directory);
  return rows;
}
async function copyPrivateTree(source: string, destination: string): Promise<void> {
  await createPrivateDirectory(destination);
  for (const entry of await readdir(source, { withFileTypes: true })) {
    const sourcePath = join(source, entry.name), destinationPath = join(destination, entry.name);
    const info = await lstat(sourcePath);
    if (info.isSymbolicLink()) throw new Error("Library recovery source contains a symbolic link");
    if (info.isDirectory()) await copyPrivateTree(sourcePath, destinationPath);
    else {
      if (!info.isFile() || info.nlink !== 1) throw new Error("Library recovery requires regular independent files");
      await copyFile(sourcePath, destinationPath, constants.COPYFILE_EXCL);
      if (process.platform !== "win32") await chmod(destinationPath, 0o600);
    }
  }
}

// 原 PGlite writer 的原生锁覆盖整个复制期间；不会打开或升级源数据库。
export async function withLibraryRecovery<T>(lease: InstanceLease, sourceDirectory: string, use: (recovery: { readonly id: string; readonly state: "library_recovery_ready"; readonly activated: false; readonly directory: string; readonly files: { path: string; bytes: string; sha256: string }[] }) => Promise<T>, quiescence?: LegacyQuiescenceLease): Promise<T> {
  lease.assertOwned();
  const source = await realpath(sourceDirectory);
  const requested = resolve(sourceDirectory);
  if (process.platform === "win32" ? source.toLowerCase() !== requested.toLowerCase() : source !== requested) throw new Error("Library recovery source must be canonical");
  const destinationWithinSource = relative(source, lease.directory);
  if (destinationWithinSource === "" || !isAbsolute(destinationWithinSource) && destinationWithinSource !== ".." && !destinationWithinSource.startsWith(`..${sep}`)) throw new Error("Library recovery destination must be outside the source directory");
  const sourceInfo = await lstat(source);
  const engineDirectory = join(source, "pglite");
  const engineInfo = await lstat(engineDirectory);
  const lockInfo = await lstat(join(source, "database.lock"));
  if (!sourceInfo.isDirectory() || sourceInfo.isSymbolicLink() || !engineInfo.isDirectory() || engineInfo.isSymbolicLink() || !lockInfo.isFile() || lockInfo.isSymbolicLink()) throw new Error("Library recovery requires an existing regular database and lock");
  if (quiescence !== undefined) {
    if (!(quiescence instanceof LegacyQuiescenceLease)) throw new Error("migration.legacy_quiescence_unavailable");
    quiescence.assertLibraryRecovery(lease, source);
  }
  const databaseLock = quiescence === undefined ? await acquireDatabaseLock(source) : undefined;
  if (quiescence === undefined && !databaseLock) throw new Error("library_database_busy");
  await using releaseSource = { async [Symbol.asyncDispose]() { await databaseLock?.close(); } };
  const id = randomUUID(), directory = await createPrivateDirectory(join(lease.directory, `library-recovery-${id}`));
  await writeMigrationJournal(directory, { id, activated: false, state: "preparing" });
  let completed = false;
  await using releaseJournal = {
    async [Symbol.asyncDispose]() {
      if (!completed) await writeMigrationJournal(directory, { id, activated: false, state: "failed", reason: "library_recovery_incomplete" });
    },
  };
  const before = await directoryFingerprint(engineDirectory);
  if (!before.some(file => file.path === "PG_VERSION")) throw new Error("Library recovery source is not an initialized PGlite database");
  lease.assertOwned();
  await copyPrivateTree(engineDirectory, join(directory, "pglite"));
  lease.assertOwned();
  const copied = await directoryFingerprint(join(directory, "pglite"));
  if (!isDeepStrictEqual(before, copied) || !isDeepStrictEqual(before, await directoryFingerprint(engineDirectory))) throw new Error("Library recovery source changed while copying");
  await verifyPrivateTree(join(directory, "pglite"));
  const manifest = { id, state: "library_recovery_ready", activated: false, directory, files: copied } as const;
  await writeMigrationJournal(directory, manifest);
  completed = true;
  const result = await use(manifest);
  if (!isDeepStrictEqual(before, await directoryFingerprint(engineDirectory))) throw new Error("Library recovery source changed while migrating");
  quiescence?.assertLibraryRecovery(lease, source);
  return result;
}

export async function prepareLibraryRecovery(lease: InstanceLease, sourceDirectory: string) {
  return withLibraryRecovery(lease, sourceDirectory, async recovery => recovery);
}

const recoveryManifest = z.object({ id: z.uuid(), state: z.literal("library_recovery_ready"), activated: z.literal(false), directory: z.string(), files: z.array(z.object({ path: z.string(), bytes: z.string().regex(/^\d+$/), sha256: z.string().regex(/^[a-f0-9]{64}$/) }).strict()) }).strict();

export async function prepareLibraryStaging(lease: InstanceLease, recovery: Awaited<ReturnType<typeof prepareLibraryRecovery>>) {
  lease.assertOwned();
  const parsed = recoveryManifest.parse(recovery);
  const expectedDirectory = join(lease.directory, `library-recovery-${parsed.id}`);
  if (parsed.directory !== expectedDirectory) throw new Error("Library recovery directory does not match its manifest identity");
  await verifyPrivateDirectory(expectedDirectory);
  const journalPath = join(expectedDirectory, "journal.json");
  await verifyPrivateFile(journalPath);
  const stored = recoveryManifest.parse(parseApplicationJson(await readFile(journalPath)));
  if (!isDeepStrictEqual(stored, parsed)) throw new Error("Library recovery journal changed");
  const source = join(expectedDirectory, "pglite");
  await verifyPrivateTree(source);
  if (!isDeepStrictEqual(parsed.files, await directoryFingerprint(source))) throw new Error("Library recovery data changed");
  const id = randomUUID(), directory = await createPrivateDirectory(join(lease.directory, `library-staging-${id}`));
  await writeMigrationJournal(directory, { id, recovery_id: parsed.id, activated: false, state: "preparing" });
  let completed = false;
  await using failure = { async [Symbol.asyncDispose]() {
    if (!completed) await writeMigrationJournal(directory, { id, recovery_id: parsed.id, activated: false, state: "failed", reason: "library_staging_incomplete" });
  } };
  lease.assertOwned();
  await copyPrivateTree(source, join(directory, "pglite"));
  lease.assertOwned();
  await verifyPrivateTree(join(directory, "pglite"));
  if (!isDeepStrictEqual(parsed.files, await directoryFingerprint(join(directory, "pglite"))) || !isDeepStrictEqual(parsed.files, await directoryFingerprint(source))) throw new Error("Library staging copy did not preserve recovery data");
  const latest = recoveryManifest.parse(parseApplicationJson(await readFile(journalPath)));
  if (!isDeepStrictEqual(latest, stored)) throw new Error("Library recovery journal changed while staging");
  const result = { id, recovery_id: parsed.id, state: "library_staging_ready", activated: false, directory } as const;
  await writeMigrationJournal(directory, result);
  completed = true;
  return result;
}
