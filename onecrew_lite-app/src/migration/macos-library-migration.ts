import { createHash } from "node:crypto";
import { lstatSync } from "node:fs";
import { open, readFile, readdir, rename } from "node:fs/promises";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { fingerprintLibraryCopy, type LibraryTableFingerprint } from "onecrew_lite_tables/migration-fingerprint";
import { inspectLibraryMigrationCopy } from "onecrew_lite_tables/migration-copy";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { AccessInstallationVerifier } from "../identity/access-verification.ts";
import { readStoredAccessCredential } from "../identity/legacy-access-credential.ts";
import { acquireInstanceLock, type InstanceLease } from "../service/instance-lock.ts";
import { loadOrCreateServiceProfile } from "../service/profile.ts";
import { inspectLegacyDatabase } from "../storage/legacy-sqlite.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { prepareActivationCandidate, prepareActivationPublication, commitActivation, readActiveGeneration } from "./activation.ts";
import { reviewMigrationAccounts } from "./account-review.ts";
import { prepareLibraryStaging, withLibraryRecovery } from "./library-recovery.ts";
import { prepareMacCutover } from "./macos-cutover.ts";
import { prepareSqliteRecovery } from "./sqlite-recovery.ts";
import { prepareSqliteStaging } from "./sqlite-staging.ts";
import { fingerprintSqlite } from "./sqlite-fingerprint.ts";

const ownerSchema = z.strictObject({ format: z.literal(1), accountId: z.string().min(1), tenantId: z.string().min(1), baseId: z.string().min(1), database: z.literal("creator.sqlite") });
const tableSchema = z.strictObject({ schema: z.string().min(1), name: z.string().min(1), columns: z.array(z.string().min(1)).min(1), rows: z.number().int().nonnegative(), sha256: z.string().regex(/^[a-f0-9]{64}$/u) });
const receiptSchema = z.strictObject({
  format: z.literal(1), kind: z.literal("mac-library-migration"), layout: z.enum(["scoped", "legacy-root"]),
  data_directory: z.string(), source_library: z.string(), source_service: z.string(), recovery_directory: z.string(),
  account_ids: z.array(z.string()).min(1), installation_id: z.string().min(1),
  library_tables: z.array(tableSchema), verified_at: z.iso.datetime(),
});
type Receipt = z.infer<typeof receiptSchema>;
export interface MacLibraryMigrationOptions {
  libraryDirectory: string;
  vaultPath: string;
  masterKeyPath: string;
  legacyDirectory?: string;
  creatorPath?: string;
  legacyPort?: number;
  webAssetsDirectory?: string;
  libraryWebAssetsDirectory?: string;
  onProgress?: (step: string) => void;
  signal: AbortSignal;
}

async function saveReceipt(directory: string, receipt: Receipt): Promise<void> {
  const path = join(directory, "library-migration.json");
  await using file = await open(path, "wx", 0o600);
  await file.writeFile(JSON.stringify(receipt));
  await file.sync();
}

async function readReceipt(directory: string): Promise<Receipt> {
  const path = join(directory, "library-migration.json");
  await verifyPrivateFile(path);
  return receiptSchema.parse(parseApplicationJson(await readFile(path)));
}

function result(receipt: Receipt, state: "migrated" | "current", tablesDirectory: string, vaultPath: string, masterKeyPath: string) {
  return { state, layout: receipt.layout, data_directory: receipt.data_directory, tables_directory: tablesDirectory,
    vault_path: vaultPath, master_key_path: masterKeyPath, account_ids: receipt.account_ids, installation_id: receipt.installation_id,
    library_tables: receipt.library_tables.map(({ schema, name, rows }) => ({ schema, name, rows })),
    recovery_directory: receipt.recovery_directory, source_preserved: true, search_resumed: false };
}

async function assertLibraryPreserved(directory: string, original: readonly LibraryTableFingerprint[]) {
  if (!isDeepStrictEqual(await fingerprintLibraryCopy(directory, original), original)) throw new Error("migration.library_records_changed");
}

async function owners(lease: InstanceLease) {
  const accounts = [];
  for (const name of (await readdir(lease.directory)).filter(value => value.startsWith("account-")).sort()) {
    const directory = await verifyPrivateDirectory(join(lease.directory, name));
    const manifest = join(directory, "account.json");
    await verifyPrivateFile(manifest);
    const owner = ownerSchema.parse(parseApplicationJson(await readFile(manifest)));
    const key = createHash("sha256").update(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId])).digest("hex");
    if (name !== `account-${key}`) throw new Error("migration.account_directory_owner_mismatch");
    const creator = join(directory, "creator.sqlite");
    await verifyPrivateFile(creator);
    if (inspectLegacyDatabase(creator, "creator").state !== "current") throw new Error("migration.data_upgrade_required");
    accounts.push({ owner, manifest, manifest_sha256: createHash("sha256").update(await readFile(manifest)).digest("hex"), creator, fingerprint: fingerprintSqlite(creator) });
  }
  if (!accounts.length) throw new Error("migration.existing_account_required");
  return accounts;
}

async function migrateScoped(lease: InstanceLease, input: MacLibraryMigrationOptions) {
  const source = resolve(input.libraryDirectory), target = join(lease.directory, "tables");
  if (source === target) throw new Error("migration.library_source_equals_target");
  const accounts = await owners(lease);
  if (lstatSync(target, { throwIfNoEntry: false })) {
    const receipt = await readReceipt(target);
    if (receipt.layout !== "scoped" || receipt.data_directory !== lease.directory || receipt.source_library !== source || receipt.source_service !== lease.directory) throw new Error("migration.existing_tables_not_this_migration");
    return result(receipt, "current", target, input.vaultPath, input.masterKeyPath);
  }
  const profilePath = join(lease.directory, "profile.json");
  await verifyPrivateFile(profilePath);
  const profile = await readFile(profilePath);
  await loadOrCreateServiceProfile(lease);
  await verifyPrivateFile(input.vaultPath);
  await verifyPrivateFile(input.masterKeyPath);
  const vault = fingerprintSqlite(input.vaultPath), key = await readFile(input.masterKeyPath);
  using eraseKey = { [Symbol.dispose]() { key.fill(0); } };
  const access = await readStoredAccessCredential(input.vaultPath, input.masterKeyPath);
  using releaseAccess = { [Symbol.dispose]() { if (access.state === "active") access.credential.dispose(); } };
  if (access.state !== "active") throw new Error("migration.original_login_required");
  const accountId = access.credential.metadata.access_user_id;
  if (!accounts.some(value => value.owner.accountId === accountId)) throw new Error("migration.main_account_owner_mismatch");
  if (!lstatSync(join(source, "server.lock"), { throwIfNoEntry: false })) throw new Error("migration.library_service_lock_missing");
  const libraryLease = await acquireInstanceLock(source);
  await using releaseLibrary = { [Symbol.asyncDispose]: () => libraryLease.close() };
  return await withLibraryRecovery(lease, source, async recovery => {
    input.onProgress?.("verify_original_library_rows");
    const baseline = await prepareLibraryStaging(lease, recovery);
    const original = await fingerprintLibraryCopy(baseline.directory);
    const staged = await prepareLibraryStaging(lease, recovery);
    await inspectLibraryMigrationCopy(staged.directory, snapshot => {
      for (const { owner } of accounts) {
        if (!snapshot.tenants.some(tenant => tenant.owner_user_id === owner.accountId && tenant.id === owner.tenantId && tenant.base_id === owner.baseId)) throw new Error("migration.library_owner_mapping_required");
      }
    });
    await assertLibraryPreserved(staged.directory, original);
    const receipt: Receipt = { format: 1, kind: "mac-library-migration", layout: "scoped", data_directory: lease.directory,
      source_library: source, source_service: lease.directory, recovery_directory: recovery.directory,
      account_ids: accounts.map(value => value.owner.accountId), installation_id: access.credential.metadata.installation_id,
      library_tables: original, verified_at: new Date().toISOString() };
    await saveReceipt(staged.directory, receipt);
    input.signal.throwIfAborted();
    input.onProgress?.("verify_original_online_account");
    const proof = await new AccessInstallationVerifier().verifyForMigration(access.credential, input.signal);
    using releaseProof = { [Symbol.dispose]() { proof.dispose(); } };
    if (proof.user.id !== accountId) throw new Error("migration.main_account_owner_mismatch");
    for (const account of accounts) {
      if (!isDeepStrictEqual(fingerprintSqlite(account.creator), account.fingerprint) || createHash("sha256").update(await readFile(account.manifest)).digest("hex") !== account.manifest_sha256) throw new Error("migration.original_account_changed");
    }
    if (!isDeepStrictEqual(fingerprintSqlite(input.vaultPath), vault) || !key.equals(await readFile(input.masterKeyPath)) || !profile.equals(await readFile(profilePath))) throw new Error("migration.original_identity_changed");
    if (lstatSync(target, { throwIfNoEntry: false })) throw new Error("migration.target_appeared");
    proof.assertFresh(); input.signal.throwIfAborted(); lease.assertOwned(); libraryLease.assertOwned();
    input.onProgress?.("publish_verified_tables");
    await rename(staged.directory, target);
    return result(receipt, "migrated", target, input.vaultPath, input.masterKeyPath);
  });
}

async function migrateLegacyRoot(lease: InstanceLease, input: MacLibraryMigrationOptions) {
  if (!input.legacyDirectory || !input.creatorPath || input.legacyPort === undefined || !input.webAssetsDirectory) throw new Error("migration.legacy_paths_port_and_web_assets_required");
  const sourcePaths = { creator: resolve(input.creatorPath), vault: resolve(input.vaultPath), masterKey: resolve(input.masterKeyPath), libraryDirectory: resolve(input.libraryDirectory) };
  const legacyDirectory = resolve(input.legacyDirectory);
  if (lstatSync(join(lease.directory, "current.json"), { throwIfNoEntry: false })) {
    const active = await readActiveGeneration(lease);
    if (!active) throw new Error("migration.active_generation_missing");
    const receipt = await readReceipt(active.tablesDirectory);
    if (receipt.layout !== "legacy-root" || receipt.source_service !== legacyDirectory || receipt.source_library !== sourcePaths.libraryDirectory || receipt.data_directory !== lease.directory) throw new Error("migration.existing_generation_not_this_migration");
    return result(receipt, "current", active.tablesDirectory, active.vaultPath, active.masterKeyPath);
  }
  if ((await readdir(lease.directory)).some(name => name.startsWith("account-") || name === "tables")) throw new Error("migration.destination_contains_account_data");
  await loadOrCreateServiceProfile(lease, input.legacyPort);
  const cutover = await prepareMacCutover(lease, { legacyServiceDirectory: legacyDirectory, sources: sourcePaths, legacyPort: input.legacyPort });
  await using releaseCutover = { [Symbol.asyncDispose]: () => cutover.close() };
  return await withLibraryRecovery(lease, sourcePaths.libraryDirectory, async recovery => {
    input.onProgress?.("verify_original_library_rows");
    const baseline = await prepareLibraryStaging(lease, recovery);
    const original = await fingerprintLibraryCopy(baseline.directory);
    const library = await prepareLibraryStaging(lease, recovery);
    input.onProgress?.("backup_and_upgrade_sqlite");
    const sqliteRecovery = await prepareSqliteRecovery(lease, { creator: sourcePaths.creator, vault: sourcePaths.vault });
    const sqlite = await prepareSqliteStaging(lease, { recovery: sqliteRecovery, sources: sourcePaths, signal: input.signal });
    const sources = { sqlite, library: { state: "staged" as const, receipt: library } };
    input.onProgress?.("review_original_account_ownership");
    const reviewed = await reviewMigrationAccounts(lease, { ...sources, signal: input.signal });
    if (reviewed.decision.state !== "verify_existing") throw new Error(`migration.accounts.${reviewed.state}`);
    const access = await readStoredAccessCredential(sqlite.vault.path, sqlite.masterKey);
    using releaseAccess = { [Symbol.dispose]() { if (access.state === "active") access.credential.dispose(); } };
    if (access.state !== "active") throw new Error("migration.original_login_required");
    const accountId = access.credential.metadata.access_user_id;
    if (reviewed.decision.userId !== accountId) throw new Error("migration.main_account_owner_mismatch");
    const tenant = reviewed.preserved.tenants.filter(value => value.owner_user_id === accountId);
    if (tenant.length !== 1) throw new Error("migration.library_owner_mapping_required");
    await assertLibraryPreserved(library.directory, original);
    const owner = { accountId, tenantId: tenant[0]!.id, baseId: tenant[0]!.base_id };
    input.onProgress?.("prepare_unified_generation");
    const candidate = await prepareActivationCandidate(lease, { legacyServiceDirectory: legacyDirectory, sources, sourcePaths,
      expectedReview: reviewed.review, owner, expectedCurrent: null, signal: input.signal });
    await assertLibraryPreserved(candidate.tables, original);
    const receipt: Receipt = { format: 1, kind: "mac-library-migration", layout: "legacy-root", data_directory: lease.directory,
      source_library: sourcePaths.libraryDirectory, source_service: legacyDirectory, recovery_directory: recovery.directory,
      account_ids: reviewed.preserved.knownLibraryAccountIds.includes(accountId) ? [...reviewed.preserved.knownLibraryAccountIds] : [accountId, ...reviewed.preserved.knownLibraryAccountIds],
      installation_id: access.credential.metadata.installation_id, library_tables: original, verified_at: new Date().toISOString() };
    await saveReceipt(candidate.tables, receipt);
    input.onProgress?.("verify_candidate_service");
    using publication = await prepareActivationPublication(lease, { candidate, quiescence: cutover.quiescence,
      webAssetsDirectory: input.webAssetsDirectory!, ...(input.libraryWebAssetsDirectory ? { libraryWebAssetsDirectory: input.libraryWebAssetsDirectory } : {}), libraryFingerprint: original, verifyOriginalMain: true, signal: input.signal });
    input.onProgress?.("verify_original_online_account");
    const proof = await publication.verifyOriginalMainAccount(lease, cutover.quiescence, input.signal);
    using releaseProof = { [Symbol.dispose]() { proof.dispose(); } };
    input.onProgress?.("publish_verified_generation");
    await commitActivation(lease, { candidate, proof, quiescence: cutover.quiescence, cutover, publication, expectedCurrent: null, signal: input.signal });
    return result(receipt, "migrated", candidate.tables, candidate.vault, candidate.masterKey);
  }, cutover.quiescence);
}

// 两种既有布局分别迁移，所有表格来自完整副本；不创建空库代替旧数据。
export async function migrateMacLibrary(lease: InstanceLease, input: MacLibraryMigrationOptions) {
  if (process.platform !== "darwin") throw new Error("migration.macos_required");
  lease.assertOwned(); input.signal.throwIfAborted();
  return input.legacyDirectory === undefined ? migrateScoped(lease, input) : migrateLegacyRoot(lease, input);
}
