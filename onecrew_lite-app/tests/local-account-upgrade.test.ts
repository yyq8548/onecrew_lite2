import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmod, copyFile, lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test, { before } from "node:test";
import { fileURLToPath } from "node:url";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { fingerprintSqlite } from "../src/migration/sqlite-fingerprint.ts";
import { inspectLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { LocalAccountUpgradeError, upgradeLocalAccountDatabases } from "../src/storage/local-account-upgrade.ts";
import { createPrivateDirectory, verifyPrivateFile } from "../src/storage/private-backup.ts";

const scratch = fileURLToPath(new URL("../../.scratch/local-account-upgrade-tests/", import.meta.url));
before(async () => { await mkdir(scratch, { recursive: true }); });

async function fixture(t: test.TestContext, label = "fixture-owner") {
  const temporary = await mkdtemp(join(scratch, "local-account-upgrade-"));
  t.after(() => rm(temporary, { recursive: true }));
  const root = await createPrivateDirectory(join(temporary, "private"));
  const owner = { accountId: label, tenantId: "fixture-tenant", baseId: "fixture-base" };
  const key = createHash("sha256").update(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId])).digest("hex");
  const directory = await createPrivateDirectory(join(root, `account-${key}`)), database = join(directory, "creator.sqlite"), manifest = join(directory, "account.json");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), database); await chmod(database, 0o600);
  await writeFile(manifest, JSON.stringify({ format: 1, ...owner, database: "creator.sqlite" }), { flag: "wx", mode: 0o600 });
  return { root, directory, database, manifest, owner };
}

test("真实历史库在实例锁内独立备份升级，全部旧表记录和账号身份保留，current只读幂等", async t => {
  const value = await fixture(t), original = fingerprintSqlite(value.database), ownerBytes = await readFile(value.manifest), originalIdentity = await lstat(value.database, { bigint: true });
  assert.ok(original.tables.find(table => table.name === "discovery_campaigns")!.rows > 0);
  assert.ok(original.tables.find(table => table.name === "discovery_search_hits")!.rows > 0);
  const lease = await acquireInstanceLock(value.root); t.after(() => lease.close());
  const result = await upgradeLocalAccountDatabases(lease);
  assert.equal(result.state, "upgraded"); assert.equal(result.accounts.length, 1);
  const account = result.accounts[0]!;
  assert.equal(account.directory, value.directory); assert.equal(account.from_version, 90); assert.equal(account.to_version, 96); assert.ok(account.backup_path);
  await verifyPrivateFile(account.backup_path);
  assert.deepEqual(fingerprintSqlite(account.backup_path), original);
  const current = fingerprintSqlite(value.database);
  assert.equal(current.version, 96); assert.equal(inspectLegacyDatabase(value.database, "creator").state, "current");
  for (const table of original.tables.filter(table => table.name !== "discovery_campaigns")) assert.deepEqual(current.tables.find(row => row.name === table.name), table);
  using oldDatabase = new DatabaseSync(account.backup_path, { readOnly: true });
  using newDatabase = new DatabaseSync(value.database, { readOnly: true });
  const expectedCampaigns = oldDatabase.prepare("SELECT * FROM discovery_campaigns ORDER BY id").all().map(row => ({ ...row, max_calls: null }));
  assert.deepEqual(newDatabase.prepare("SELECT * FROM discovery_campaigns ORDER BY id").all().map(row => ({ ...row })), expectedCampaigns);
  assert.deepEqual(await readFile(value.manifest), ownerBytes);
  assert.notEqual((await lstat(value.database, { bigint: true })).ino, originalIdentity.ino);
  const publishedBytes = await readFile(value.database), entries = await readdir(value.root);
  assert.deepEqual(await upgradeLocalAccountDatabases(lease), { state: "current", accounts: [{ directory: value.directory, from_version: 96, to_version: 96 }] });
  assert.deepEqual(await readFile(value.database), publishedBytes); assert.deepEqual(await readdir(value.root), entries);
  const operation = entries.find(name => name.startsWith("creator-upgrade-")); assert.ok(operation);
  const state = JSON.parse(await readFile(join(value.root, operation, "state.json"), "utf8"));
  assert.equal(state.phase, "completed"); assert.equal(state.accounts[0].state, "upgraded");
});

test("真实SQLite未检查点WAL明确拒绝，原库和WAL保持完整且不创建升级备份", async t => {
  const value = await fixture(t), database = new DatabaseSync(value.database);
  t.after(() => database.close());
  database.exec("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; VACUUM");
  const wal = await readFile(`${value.database}-wal`), before = fingerprintSqlite(value.database);
  assert.ok(wal.length > 0);
  const lease = await acquireInstanceLock(value.root); t.after(() => lease.close());
  await assert.rejects(upgradeLocalAccountDatabases(lease), error => error instanceof LocalAccountUpgradeError && error.code === "data_upgrade_database_not_quiescent" && error.message.includes(`${value.database}-wal`));
  assert.deepEqual(await readFile(`${value.database}-wal`), wal); assert.deepEqual(fingerprintSqlite(value.database), before);
  assert.equal((await readdir(value.root)).some(name => name.startsWith("creator-upgrade-")), false);
});

test("account.json归属哈希不一致时保留原库，释放的实例锁拒绝升级", async t => {
  const value = await fixture(t), bytes = await readFile(value.database);
  await writeFile(value.manifest, JSON.stringify({ format: 1, ...value.owner, accountId: "different-fixture-owner", database: "creator.sqlite" }));
  const lease = await acquireInstanceLock(value.root);
  await assert.rejects(upgradeLocalAccountDatabases(lease), error => error instanceof LocalAccountUpgradeError && error.code === "data_upgrade_account_owner_mismatch" && error.message.includes(value.directory));
  assert.deepEqual(await readFile(value.database), bytes);
  await lease.close();
  await assert.rejects(upgradeLocalAccountDatabases(lease), /Instance ownership has been released/);
});

test("active generation指针存在时明确要求受支持迁移且不访问账号数据库", async t => {
  const value = await fixture(t), bytes = await readFile(value.database);
  await writeFile(join(value.root, "current.json"), "{}", { flag: "wx", mode: 0o600 });
  const lease = await acquireInstanceLock(value.root); t.after(() => lease.close());
  await assert.rejects(upgradeLocalAccountDatabases(lease), error => error instanceof LocalAccountUpgradeError && error.code === "data_upgrade_active_generation_requires_supported_migration");
  assert.deepEqual(await readFile(value.database), bytes);
  assert.equal((await readdir(value.root)).some(name => name.startsWith("creator-upgrade-")), false);
});
