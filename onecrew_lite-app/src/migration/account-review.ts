import { createHash } from "node:crypto";
import { constants, lstatSync } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { AccessInstallationVerifier } from "../identity/access-verification.ts";
import { AccountChangedError } from "../identity/account-context.ts";
import { decideLegacyAccounts, type LegacyAccountChoice, type LegacyAccountSnapshot } from "../identity/legacy-account-selection.ts";
import { readStoredAccessCredential } from "../identity/legacy-access-credential.ts";
import { readLibraryMigrationAccount } from "../identity/library-migration-account.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { readVaultMasterKey } from "../storage/vault-reader.ts";
import { fingerprintSqlite } from "./sqlite-fingerprint.ts";
import type { prepareSqliteStaging } from "./sqlite-staging.ts";
import type { prepareLibraryStaging } from "./library-recovery.ts";

type SQLiteStagingReceipt = Awaited<ReturnType<typeof prepareSqliteStaging>>;
type LibraryStagingReceipt = Awaited<ReturnType<typeof prepareLibraryStaging>>;
export type MigrationAccountSources = {
  sqlite: SQLiteStagingReceipt;
  library: { state: "staged"; receipt: LibraryStagingReceipt } | { state: "absent"; sourceDirectory: string };
  choice?: LegacyAccountChoice;
  signal: AbortSignal;
};
const samePath = (a: string, b: string) => relative(resolve(a), resolve(b)) === "";

async function privateJournal(path: string): Promise<unknown> {
  await verifyPrivateFile(path);
  const before = await lstat(path, { bigint: true });
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.dev !== before.dev || opened.ino !== before.ino || opened.nlink !== 1n || opened.size > 1024n * 1024n) throw new Error("migration.accounts.invalid_journal");
  const value = parseApplicationJson(await file.readFile());
  const after = await file.stat({ bigint: true }), current = await lstat(path, { bigint: true });
  if (current.isSymbolicLink() || current.ino !== opened.ino || current.dev !== opened.dev || current.nlink !== 1n || after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs) throw new AccountChangedError();
  await verifyPrivateFile(path);
  return value;
}

async function validateReceipt(lease: InstanceLease, receipt: SQLiteStagingReceipt | LibraryStagingReceipt, kind: "sqlite" | "library") {
  z.guid().parse(receipt.id);
  const expected = join(lease.directory, `${kind === "sqlite" ? "staging" : "library-staging"}-${receipt.id}`);
  if (!samePath(receipt.directory, expected) || receipt.activated !== false || receipt.state !== `${kind}_staging_ready`) throw new Error("migration.accounts.invalid_staging");
  await verifyPrivateDirectory(expected);
  if (!isDeepStrictEqual(await privateJournal(join(expected, "journal.json")), receipt)) throw new AccountChangedError();
}

async function confirmAbsent(path: string): Promise<string> {
  const absolute = resolve(path);
  if (lstatSync(absolute, { throwIfNoEntry: false })) throw new Error("migration.accounts.library_source_exists");
  let parent = dirname(absolute);
  for (;;) {
    const info = lstatSync(parent, { throwIfNoEntry: false });
    if (info) {
      if (!info.isDirectory() || info.isSymbolicLink() || !samePath(parent, await realpath(parent))) throw new Error("migration.accounts.library_source_alias");
      return absolute;
    }
    const next = dirname(parent);
    if (next === parent) throw new Error("migration.accounts.library_parent_missing");
    parent = next;
  }
}

// 本地审阅仅给出选择和保留清单；凭据始终在内部释放，不认领任何历史数据。
async function inspectSources(lease: InstanceLease, input: MigrationAccountSources) {
  lease.assertOwned();
  input.signal.throwIfAborted();
  await validateReceipt(lease, input.sqlite, "sqlite");
  const sqlite = input.sqlite;
  if (!samePath(sqlite.creator.path, join(sqlite.directory, "creator", "creator.sqlite")) ||
      !samePath(sqlite.vault.path, join(sqlite.directory, "vault", "vault.sqlite")) || !samePath(sqlite.masterKey, join(sqlite.directory, "masterkey"))) throw new Error("migration.accounts.invalid_sqlite_layout");
  await verifyPrivateFile(sqlite.creator.path);
  await verifyPrivateFile(sqlite.vault.path);
  const fingerprints = { creator: fingerprintSqlite(sqlite.creator.path), vault: fingerprintSqlite(sqlite.vault.path) };
  if (!isDeepStrictEqual(fingerprints, sqlite.fingerprints)) throw new AccountChangedError();
  const key = await readVaultMasterKey(sqlite.masterKey);
  const keyFingerprint = createHash("sha256").update(key).digest("hex");
  key.fill(0);
  const main = await readStoredAccessCredential(sqlite.vault.path, sqlite.masterKey).then(value => value, () => { throw new Error("migration.accounts.invalid_main_credential"); });
  using releaseMain = { [Symbol.dispose]() { if (main.state === "active") main.credential.dispose(); } };
  if (main.state === "active" && JSON.stringify(main.credential.metadata).includes(main.credential.exposeBearer())) throw new Error("migration.accounts.invalid_main_credential");
  const mainSnapshot: LegacyAccountSnapshot = main.state === "active"
    ? { source: "main", state: "present", userId: main.credential.metadata.access_user_id, fingerprint: main.credential.fingerprint }
    : main.state === "inactive" ? { source: "main", state: "inactive", userId: main.metadata.access_user_id, fingerprint: createHash("sha256").update(JSON.stringify(main.metadata)).digest("hex") }
      : { source: "main", state: "missing" };
  let library: Awaited<ReturnType<typeof readLibraryMigrationAccount>> | undefined;
  using releaseLibrary = { [Symbol.dispose]() { library?.credential?.dispose(); } };
  let librarySnapshot: LegacyAccountSnapshot, libraryFingerprint: string;
  if (input.library.state === "staged") {
    await validateReceipt(lease, input.library.receipt, "library");
    library = await readLibraryMigrationAccount(lease, input.library.receipt.directory).then(value => value, () => { throw new Error("migration.accounts.invalid_library_credential"); });
    if (library.credential && JSON.stringify({ snapshot: library.snapshot, knownAccountIds: library.knownAccountIds, tenants: library.tenants }).includes(library.credential.exposeBearer())) throw new Error("migration.accounts.invalid_library_credential");
    librarySnapshot = library.snapshot;
    libraryFingerprint = library.sourceFingerprint;
  } else {
    const absent = await confirmAbsent(input.library.sourceDirectory);
    librarySnapshot = { source: "library", state: "missing" };
    libraryFingerprint = createHash("sha256").update(`absent\0${absent}`).digest("hex");
  }
  const choice = input.choice ? Object.freeze({ source: input.choice.source, fingerprint: input.choice.fingerprint }) : undefined;
  const decision = decideLegacyAccounts(mainSnapshot, librarySnapshot, choice);
  const review = { sources: { sqliteId: sqlite.id, creator: fingerprints.creator, vault: fingerprints.vault, masterKey: keyFingerprint,
    libraryId: input.library.state === "staged" ? input.library.receipt.id : null, library: libraryFingerprint },
    candidates: [mainSnapshot, librarySnapshot], ...(choice ? { choice } : {}) };
  lease.assertOwned();
  input.signal.throwIfAborted();
  return { review, decision, state: decision.state === "choose_account" ? "choice_required" as const : decision.state,
    preserved: { knownLibraryAccountIds: library?.knownAccountIds ?? [], tenants: library?.tenants ?? [] } };
}

export async function reviewMigrationAccounts(lease: InstanceLease, input: MigrationAccountSources) {
  return inspectSources(lease, input);
}
export type MigrationAccountReview = Awaited<ReturnType<typeof reviewMigrationAccounts>>["review"];

// 仅真实 Access 安装验证可产生证明；验证后重新读取双方，CAS 和 30 秒时效均通过才交付。
export async function verifyMigrationAccount(lease: InstanceLease, input: MigrationAccountSources & { expectedReview: MigrationAccountReview }) {
  const before = await inspectSources(lease, input);
  if (!isDeepStrictEqual(before.review, input.expectedReview)) throw new AccountChangedError();
  if (before.decision.state !== "verify_existing") throw new Error(`migration.accounts.${before.state}`);
  const selected = before.decision.candidates[0]!;
  let main: Awaited<ReturnType<typeof readStoredAccessCredential>> | undefined;
  let library: Awaited<ReturnType<typeof readLibraryMigrationAccount>> | undefined;
  using release = { [Symbol.dispose]() { if (main?.state === "active") main.credential.dispose(); library?.credential?.dispose(); } };
  if (selected.source === "main") main = await readStoredAccessCredential(input.sqlite.vault.path, input.sqlite.masterKey).then(value => value, () => { throw new Error("migration.accounts.invalid_main_credential"); });
  else if (input.library.state === "staged") library = await readLibraryMigrationAccount(lease, input.library.receipt.directory).then(value => value, () => { throw new Error("migration.accounts.invalid_library_credential"); });
  const credential = selected.source === "main" ? main?.state === "active" ? main.credential : undefined : library?.credential;
  if (!credential || credential.fingerprint !== selected.fingerprint || credential.metadata.access_user_id !== selected.userId) throw new AccountChangedError();
  lease.assertOwned();
  input.signal.throwIfAborted();
  const proof = await new AccessInstallationVerifier().verifyForMigration(credential, input.signal);
  let delivered = false;
  using releaseProof = { [Symbol.dispose]() { if (!delivered) proof.dispose(); } };
  const after = await inspectSources(lease, input);
  if (!isDeepStrictEqual(after.review, before.review) || proof.sourceFingerprint !== selected.fingerprint || proof.user.id !== selected.userId) throw new AccountChangedError();
  lease.assertOwned();
  input.signal.throwIfAborted();
  proof.assertFresh();
  delivered = true;
  return proof;
}
