import { createHash, randomUUID } from "node:crypto";
import { lstatSync } from "node:fs";
import { lstat, open, readFile, readdir, rename } from "node:fs/promises";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { fingerprintSqlite } from "../migration/sqlite-fingerprint.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { upgradeCreatorStagingCopy } from "./creator-upgrade.ts";
import { backupLegacyDatabase, inspectLegacyDatabase } from "./legacy-sqlite.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "./private-backup.ts";

export class LocalAccountUpgradeError extends Error {
  readonly code: string;
  constructor(code: string, message = code) {
    super(message);
    this.name = "LocalAccountUpgradeError";
    this.code = code;
  }
}

const ownerManifest = z.strictObject({ format: z.literal(1), accountId: z.string().min(1), tenantId: z.string().min(1), baseId: z.string().min(1), database: z.literal("creator.sqlite") });
const identity = async (path: string) => { const value = await lstat(path, { bigint: true }); return { dev: value.dev, ino: value.ino }; };

function preservesRetiredReadLimitMigration(source: string, staging: string, table: string): boolean {
  const ordering = table === "discovery_campaigns" ? "id" : table === "discovery_platform_runs" ? "discovery_campaign_id,platform,profile_ref" : null;
  if (ordering === null) return false;
  using before = new DatabaseSync(source, { readOnly: true });
  using after = new DatabaseSync(staging, { readOnly: true });
  const original = before.prepare(`SELECT * FROM ${table} ORDER BY ${ordering}`), updated = after.prepare(`SELECT * FROM ${table} ORDER BY ${ordering}`);
  original.setReadBigInts(true); updated.setReadBigInts(true);
  const expected = original.all().map(row => table === "discovery_campaigns" ? { ...row, max_calls: null } : row.status === "budget_exhausted" ? { ...row, status: "completed", stop_code: null, active_query: null } : { ...row });
  return isDeepStrictEqual(expected, updated.all().map(row => ({ ...row })));
}

async function fileState(path: string) {
  await verifyPrivateFile(path);
  const before = await lstat(path, { bigint: true });
  if (before.nlink !== 1n) throw new LocalAccountUpgradeError("data_upgrade_file_not_independent", `data_upgrade_file_not_independent: ${path}`);
  const sha256 = createHash("sha256").update(await readFile(path)).digest("hex"), after = await lstat(path, { bigint: true });
  if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs || after.nlink !== 1n) throw new LocalAccountUpgradeError("data_upgrade_file_changed", `data_upgrade_file_changed: ${path}`);
  return { dev: after.dev, ino: after.ino, size: after.size, mtimeNs: after.mtimeNs, ctimeNs: after.ctimeNs, sha256 };
}

function assertQuiescent(path: string): void {
  for (const suffix of ["-wal", "-journal"]) {
    const sidecar = lstatSync(`${path}${suffix}`, { throwIfNoEntry: false });
    if (sidecar && (!sidecar.isFile() || sidecar.isSymbolicLink() || sidecar.size !== 0)) throw new LocalAccountUpgradeError("data_upgrade_database_not_quiescent", `data_upgrade_database_not_quiescent: ${path}${suffix}`);
  }
}

function assertRootLayout(lease: InstanceLease): void {
  lease.assertOwned();
  if (lstatSync(join(lease.directory, "current.json"), { throwIfNoEntry: false })) throw new LocalAccountUpgradeError("data_upgrade_active_generation_requires_supported_migration");
}

async function readAccount(root: string, name: string) {
  if (!/^account-[a-f0-9]{64}$/u.test(name)) throw new LocalAccountUpgradeError("data_upgrade_account_directory_invalid", `data_upgrade_account_directory_invalid: ${name}`);
  const directory = await verifyPrivateDirectory(join(root, name)), directoryIdentity = await identity(directory);
  const manifestPath = join(directory, "account.json"), manifestState = await fileState(manifestPath);
  const owner = ownerManifest.parse(parseApplicationJson(await readFile(manifestPath)));
  const key = createHash("sha256").update(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId])).digest("hex");
  if (name !== `account-${key}`) throw new LocalAccountUpgradeError("data_upgrade_account_owner_mismatch", `data_upgrade_account_owner_mismatch: ${directory}`);
  const path = join(directory, "creator.sqlite");
  assertQuiescent(path);
  const sourceState = await fileState(path), inspection = inspectLegacyDatabase(path, "creator");
  if (inspection.state === "empty" || inspection.version < 90) throw new LocalAccountUpgradeError("data_upgrade_account_requires_supported_migration", `data_upgrade_account_requires_supported_migration: ${directory}`);
  const fingerprint = fingerprintSqlite(path);
  if (!isDeepStrictEqual(await fileState(manifestPath), manifestState) || !isDeepStrictEqual(await fileState(path), sourceState)) throw new LocalAccountUpgradeError("data_upgrade_account_changed", `data_upgrade_account_changed: ${directory}`);
  return { name, directory, directoryIdentity, manifestPath, manifestState, path, sourceState, fingerprint, from_version: inspection.version, to_version: inspection.currentVersion };
}

type Account = Awaited<ReturnType<typeof readAccount>>;
type AccountResult = { directory: string; from_version: number; to_version: number; backup_path?: string };

async function assertAccountUnchanged(lease: InstanceLease, account: Account): Promise<void> {
  assertRootLayout(lease);
  await verifyPrivateDirectory(account.directory);
  assertQuiescent(account.path);
  if (!isDeepStrictEqual(await identity(account.directory), account.directoryIdentity) || !isDeepStrictEqual(await fileState(account.manifestPath), account.manifestState) || !isDeepStrictEqual(await fileState(account.path), account.sourceState) || !isDeepStrictEqual(fingerprintSqlite(account.path), account.fingerprint)) throw new LocalAccountUpgradeError("data_upgrade_account_changed", `data_upgrade_account_changed: ${account.directory}`);
}

export async function upgradeLocalAccountDatabases(lease: InstanceLease): Promise<{ state: "upgraded" | "current"; accounts: AccountResult[] }> {
  assertRootLayout(lease);
  const root = await verifyPrivateDirectory(lease.directory), rootIdentity = await identity(root), accounts: Account[] = [];
  for (const name of (await readdir(root)).filter(value => value.startsWith("account-")).sort()) accounts.push(await readAccount(root, name));
  const pending = accounts.filter(account => account.from_version !== account.to_version);
  if (!pending.length) return { state: "current", accounts: accounts.map(({ directory, from_version, to_version }) => ({ directory, from_version, to_version })) };

  const operation = await createPrivateDirectory(join(root, `creator-upgrade-${randomUUID()}`));
  const records = pending.map(account => ({ directory: account.directory, from_version: account.from_version, to_version: account.to_version, backup_path: join(operation, `${account.name}-original`, "creator.sqlite"), staging_path: join(operation, `${account.name}-staging`, "creator.sqlite"), state: "pending" }));
  const state = { format: 1, phase: "preparing", accounts: records };
  const writeState = async () => {
    const temporary = join(operation, `${randomUUID()}.state`);
    { await using handle = await open(temporary, "wx", 0o600); await handle.writeFile(JSON.stringify(state)); await handle.sync(); }
    await rename(temporary, join(operation, "state.json"));
  };
  await writeState();
  const prepared: { account: Account; record: typeof records[number]; backupState: Awaited<ReturnType<typeof fileState>>; stagingState: Awaited<ReturnType<typeof fileState>> }[] = [];
  for (const [index, account] of pending.entries()) {
    lease.assertOwned();
    const record = records[index]!;
    await assertAccountUnchanged(lease, account);
    await backupLegacyDatabase(account.path, record.backup_path, "creator");
    if (!isDeepStrictEqual(fingerprintSqlite(record.backup_path), account.fingerprint)) throw new LocalAccountUpgradeError("data_upgrade_backup_changed", `data_upgrade_backup_changed: ${record.backup_path}`);
    const backupState = await fileState(record.backup_path);
    record.state = "backed_up"; await writeState();
    await backupLegacyDatabase(record.backup_path, record.staging_path, "creator");
    const upgraded = await upgradeCreatorStagingCopy({ sourcePath: record.backup_path, stagingPath: record.staging_path });
    const inspection = inspectLegacyDatabase(record.staging_path, "creator"), fingerprint = fingerprintSqlite(record.staging_path);
    if (upgraded.fromVersion !== account.from_version || upgraded.toVersion !== account.to_version || inspection.state !== "current" || account.fingerprint.tables.some(table => !isDeepStrictEqual(fingerprint.tables.find(value => value.name === table.name), table) && !(account.from_version < 94 && preservesRetiredReadLimitMigration(record.backup_path, record.staging_path, table.name)))) throw new LocalAccountUpgradeError("data_upgrade_existing_records_changed", `data_upgrade_existing_records_changed: ${account.directory}`);
    assertQuiescent(record.staging_path);
    prepared.push({ account, record, backupState, stagingState: await fileState(record.staging_path) });
    record.state = "prepared"; await writeState();
  }

  // 全部副本准备完成后统一复查；原库从未作为迁移写入目标。
  for (const value of prepared) await assertAccountUnchanged(lease, value.account);
  state.phase = "publishing"; await writeState();
  for (const value of prepared) {
    assertRootLayout(lease);
    if (!isDeepStrictEqual(await identity(root), rootIdentity)) throw new LocalAccountUpgradeError("data_upgrade_root_changed");
    await assertAccountUnchanged(lease, value.account);
    assertQuiescent(value.record.staging_path);
    if (!isDeepStrictEqual(await fileState(value.record.backup_path), value.backupState) || !isDeepStrictEqual(await fileState(value.record.staging_path), value.stagingState)) throw new LocalAccountUpgradeError("data_upgrade_prepared_copy_changed", `data_upgrade_prepared_copy_changed: ${operation}`);
    await rename(value.record.staging_path, value.account.path);
    value.record.state = "upgraded"; await writeState();
  }
  state.phase = "completed"; await writeState();
  return { state: "upgraded", accounts: accounts.map(account => {
    const record = records.find(value => value.directory === account.directory);
    return { directory: account.directory, from_version: account.from_version, to_version: account.to_version, ...(record ? { backup_path: record.backup_path } : {}) };
  }) };
}
