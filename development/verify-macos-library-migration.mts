import assert from "node:assert/strict";
import { constants, lstatSync } from "node:fs";
import { chmod, copyFile, open, readFile, readdir, rename } from "node:fs/promises";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { fingerprintLibraryCopy } from "../onecrew_lite-library/dist/migration-fingerprint.js";
import { acquireInstanceLock } from "../onecrew_lite-app/src/service/instance-lock.ts";
import { loadOrCreateServiceProfile } from "../onecrew_lite-app/src/service/profile.ts";
import { createPrivateDirectory, verifyPrivateDirectory } from "../onecrew_lite-app/src/storage/private-backup.ts";
import { backupLegacyDatabase } from "../onecrew_lite-app/src/storage/legacy-sqlite.ts";
import { fingerprintSqlite } from "../onecrew_lite-app/src/migration/sqlite-fingerprint.ts";
import { LegacyQuiescenceLease } from "../onecrew_lite-app/src/migration/legacy-quiescence.ts";
import { prepareSqliteRecovery } from "../onecrew_lite-app/src/migration/sqlite-recovery.ts";
import { prepareLibraryStaging, withLibraryRecovery } from "../onecrew_lite-app/src/migration/library-recovery.ts";
import { migrateMacLibrary } from "../onecrew_lite-app/src/migration/macos-library-migration.ts";
import { readActiveGeneration } from "../onecrew_lite-app/src/migration/activation.ts";
import { readStoredAccessCredential } from "../onecrew_lite-app/src/identity/legacy-access-credential.ts";

const { values } = parseArgs({ strict: true, options: {
  "legacy-data-dir": { type: "string" }, "library-data-dir": { type: "string" }, "creator-db": { type: "string" },
  vault: { type: "string" }, "master-key": { type: "string" }, output: { type: "string" },
  "web-assets": { type: "string" }, "library-web-assets": { type: "string" }, "scoped-data-dir": { type: "string" }, "scoped-from-migrated": { type: "boolean" }, "reuse-snapshot": { type: "boolean" },
} });
for (const option of ["legacy-data-dir", "library-data-dir", "creator-db", "vault", "master-key", "output", "web-assets"] as const) assert(values[option], `--${option} required`);
const output = values["reuse-snapshot"] ? await verifyPrivateDirectory(resolve(values.output!)) : await createPrivateDirectory(resolve(values.output!));
const priorReport = values["reuse-snapshot"] ? JSON.parse(await readFile(join(output, "verification.json"), "utf8")) : undefined;
const report: Record<string, unknown> = { format: 1, input: "genuine-local-installation-copies", search_resumed: false, checks: [] };
async function saveReport() {
  const path = join(output, "verification.json");
  await using file = await open(path, "w", 0o600);
  await file.writeFile(JSON.stringify(report, null, 2)); await file.sync();
}
let completed = false;
await using failure = { async [Symbol.asyncDispose]() { if (!completed) { report.state = "failed"; await saveReport(); } } };
const snapshot = values["reuse-snapshot"] ? await verifyPrivateDirectory(join(output, "snapshot")) : await createPrivateDirectory(join(output, "snapshot"));
const snapshotLease = await acquireInstanceLock(snapshot);
await using closeSnapshot = { [Symbol.asyncDispose]: () => snapshotLease.close() };
const actualSources = { creator: resolve(values["creator-db"]!), vault: resolve(values.vault!), masterKey: resolve(values["master-key"]!), libraryDirectory: resolve(values["library-data-dir"]!) };
const original = { creator: fingerprintSqlite(actualSources.creator), vault: fingerprintSqlite(actualSources.vault) };
const legacy = values["reuse-snapshot"] ? await verifyPrivateDirectory(join(output, "legacy")) : await createPrivateDirectory(join(output, "legacy"));
const library = join(output, "standalone-library");
console.log("phase=snapshot_genuine_stopped_installation");
if (!values["reuse-snapshot"]) {
  const quiescence = await LegacyQuiescenceLease.acquire(snapshotLease, { serviceDirectory: resolve(values["legacy-data-dir"]!), sources: actualSources });
  await using close = { [Symbol.asyncDispose]: () => quiescence.close() };
  const recovery = await prepareSqliteRecovery(snapshotLease, actualSources);
  await backupLegacyDatabase(recovery.creator.path, join(legacy, "creator", "creator.sqlite"), "creator");
  await backupLegacyDatabase(recovery.vault.path, join(legacy, "vault", "vault.sqlite"), "vault");
  for (const [source, target] of [[actualSources.masterKey, join(legacy, "masterkey")],
    [join(values["legacy-data-dir"]!, ".onecrew_lite-service-owner.lock"), join(legacy, ".onecrew_lite-service-owner.lock")]]) {
    await copyFile(source!, target!, constants.COPYFILE_EXCL); await chmod(target!, 0o600);
  }
  const sync = join(values["legacy-data-dir"]!, "dynamic-table-sync.json");
  if (lstatSync(sync, { throwIfNoEntry: false })) { await copyFile(sync, join(legacy, "dynamic-table-sync.json"), constants.COPYFILE_EXCL); await chmod(join(legacy, "dynamic-table-sync.json"), 0o600); }
  await withLibraryRecovery(snapshotLease, actualSources.libraryDirectory, async recovery => {
    const staged = await prepareLibraryStaging(snapshotLease, recovery);
    await rename(staged.directory, library);
    for (const name of ["server.lock", "database.lock"]) { await copyFile(join(actualSources.libraryDirectory, name), join(library, name), constants.COPYFILE_EXCL); await chmod(join(library, name), 0o600); }
  }, quiescence);
  assert.deepEqual(fingerprintSqlite(actualSources.creator), original.creator);
  assert.deepEqual(fingerprintSqlite(actualSources.vault), original.vault);
}
report.original_sources_unchanged = true;
assert.deepEqual(fingerprintSqlite(join(legacy, "creator", "creator.sqlite")), original.creator);
assert.deepEqual(fingerprintSqlite(join(legacy, "vault", "vault.sqlite")), original.vault);
const target = values["reuse-snapshot"] ? await verifyPrivateDirectory(join(output, "unified")) : await createPrivateDirectory(join(output, "unified"));
const lease = await acquireInstanceLock(target);
await using closeTarget = { [Symbol.asyncDispose]: () => lease.close() };
const portOwner = await loadOrCreateServiceProfile(snapshotLease);
const options = { legacyDirectory: legacy, creatorPath: join(legacy, "creator", "creator.sqlite"), libraryDirectory: library,
  vaultPath: join(legacy, "vault", "vault.sqlite"), masterKeyPath: join(legacy, "masterkey"), legacyPort: portOwner.port,
  webAssetsDirectory: resolve(values["web-assets"]!), ...(values["library-web-assets"] ? { libraryWebAssetsDirectory: resolve(values["library-web-assets"]) } : {}), signal: AbortSignal.timeout(15 * 60_000), onProgress: (step: string) => console.log(`phase=${step}`) };
console.log("phase=reject_running_original_owner");
const previouslyActivated = !!lstatSync(join(target, "current.json"), { throwIfNoEntry: false });
if (!previouslyActivated) {
  const held = await LegacyQuiescenceLease.acquire(snapshotLease, { serviceDirectory: legacy, sources: { creator: options.creatorPath, vault: options.vaultPath, masterKey: options.masterKeyPath, libraryDirectory: library } });
  await using closeHeld = { [Symbol.asyncDispose]: () => held.close() };
  await assert.rejects(migrateMacLibrary(lease, options), /migration.legacy_writer_running/u);
  assert.equal(lstatSync(join(target, "current.json"), { throwIfNoEntry: false }), undefined);
}
console.log("phase=migrate_legacy_root");
const began = performance.now();
const migrated = await migrateMacLibrary(lease, options);
assert.equal(migrated.state, previouslyActivated ? "current" : "migrated");
const active = await readActiveGeneration(lease); assert(active);
const candidate = JSON.parse(await readFile(join(active.generationRoot, "candidate.json"), "utf8"));
assert.deepEqual(fingerprintSqlite(active.creatorPath), candidate.fingerprints.creator);
const receipt = JSON.parse(await readFile(join(active.tablesDirectory, "library-migration.json"), "utf8"));
assert.deepEqual(await fingerprintLibraryCopy(active.tablesDirectory, receipt.library_tables), receipt.library_tables);
const access = await readStoredAccessCredential(active.vaultPath, active.masterKeyPath);
using releaseAccess = { [Symbol.dispose]() { if (access.state === "active") access.credential.dispose(); } };
assert.equal(access.state, "active");
if (access.state !== "active") throw new Error("Original account inactive after migration");
assert.equal(access.credential.metadata.installation_id, migrated.installation_id);
assert.equal(access.credential.metadata.access_user_id, active.owner.accountId);
assert.deepEqual(fingerprintSqlite(options.creatorPath), original.creator);
assert.deepEqual(fingerprintSqlite(options.vaultPath), original.vault);
const repeated = await migrateMacLibrary(lease, options);
assert.equal(repeated.state, "current");
assert.equal(repeated.tables_directory, migrated.tables_directory);
report.legacy_root = { state: "passed", duration_ms: previouslyActivated ? priorReport.legacy_root.duration_ms : Math.round(performance.now() - began), original_creator_version: original.creator.version,
  upgraded_creator_version: fingerprintSqlite(active.creatorPath).version, original_sqlite_tables: original.creator.tables.length,
  creator_tables_and_rows_preserved: true, library_tables: migrated.library_tables, installation_id_preserved: true,
  original_sources_unchanged: true, running_writer_rejected: true, retry_idempotent: true };
await saveReport();

if (values["scoped-from-migrated"] && lstatSync(join(output, "scoped"), { throwIfNoEntry: false })) {
  console.log("phase=reject_genuine_conflicting_tenant_scope");
  const conflicting = await acquireInstanceLock(join(output, "scoped"));
  await using closeConflicting = { [Symbol.asyncDispose]: () => conflicting.close() };
  await assert.rejects(migrateMacLibrary(conflicting, { libraryDirectory: library, vaultPath: join(conflicting.directory, "vault", "vault.sqlite"), masterKeyPath: join(conflicting.directory, "masterkey"), signal: options.signal }), /migration.library_owner_mapping_required/u);
  assert.equal(lstatSync(join(conflicting.directory, "tables"), { throwIfNoEntry: false }), undefined);
  report.conflicting_tenant_scope = { state: "rejected_before_publication", original_results_preserved: true };
}
if (values["scoped-data-dir"] || values["scoped-from-migrated"]) {
  console.log("phase=snapshot_genuine_scoped_accounts");
  const source = values["scoped-from-migrated"] ? active.generationRoot : resolve(values["scoped-data-dir"]!);
  const scopedName = values["scoped-from-migrated"] ? "scoped-compatible" : "scoped";
  const reusedScoped = values["reuse-snapshot"] && !!lstatSync(join(output, scopedName), { throwIfNoEntry: false });
  const scoped = reusedScoped ? await verifyPrivateDirectory(join(output, scopedName)) : await createPrivateDirectory(join(output, scopedName));
  const scopedVault = join(scoped, "vault", "vault.sqlite");
  if (!reusedScoped) {
  for (const name of (await readdir(source)).filter(value => /^account-[a-f0-9]{64}$/u.test(value))) {
    const originalPath = join(source, name, "creator.sqlite"), account = await createPrivateDirectory(join(scoped, name));
    const backupPath = join(output, `${scopedName}-backup-${name}`, "creator.sqlite");
    await backupLegacyDatabase(originalPath, backupPath, "creator");
    await copyFile(backupPath, join(account, "creator.sqlite"), constants.COPYFILE_EXCL); await chmod(join(account, "creator.sqlite"), 0o600);
    for (const file of ["account.json", "dynamic-table-sync.json", "platform-accounts.json"]) if (lstatSync(join(source, name, file), { throwIfNoEntry: false })) {
      await copyFile(join(source, name, file), join(account, file), constants.COPYFILE_EXCL); await chmod(join(account, file), 0o600);
    }
  }
  await backupLegacyDatabase(lstatSync(join(source, "vault.sqlite"), { throwIfNoEntry: false }) ? join(source, "vault.sqlite") : join(source, "vault", "vault.sqlite"), scopedVault, "vault");
  await copyFile(join(source, "masterkey"), join(scoped, "masterkey"), constants.COPYFILE_EXCL); await chmod(join(scoped, "masterkey"), 0o600);
  }
  const scopedLease = await acquireInstanceLock(scoped);
  await using releaseScoped = { [Symbol.asyncDispose]: () => scopedLease.close() };
  await loadOrCreateServiceProfile(scopedLease, portOwner.port);
  const scopedOptions = { libraryDirectory: library, vaultPath: scopedVault, masterKeyPath: join(scoped, "masterkey"), signal: AbortSignal.timeout(15 * 60_000) };
  console.log("phase=migrate_scoped_accounts");
  const started = performance.now(), saved = await migrateMacLibrary(scopedLease, scopedOptions);
  assert.equal(saved.state, "migrated");
  const savedReceipt = JSON.parse(await readFile(join(saved.tables_directory, "library-migration.json"), "utf8"));
  assert.deepEqual(await fingerprintLibraryCopy(saved.tables_directory, savedReceipt.library_tables), savedReceipt.library_tables);
  assert.equal((await migrateMacLibrary(scopedLease, scopedOptions)).state, "current");
  report.scoped = { state: "passed", input: values["scoped-from-migrated"] ? "genuine-migrated-account-layout" : "genuine-scoped-installation", duration_ms: Math.round(performance.now() - started), account_count: saved.account_ids.length,
    library_tables: saved.library_tables, original_login_preserved: true, account_results_preserved: true, retry_idempotent: true };
}
report.state = "passed"; await saveReport(); completed = true;
console.log(JSON.stringify({ state: "passed", report: join(output, "verification.json") }));
