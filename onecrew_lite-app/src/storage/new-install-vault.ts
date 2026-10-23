import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { lstat, open, realpath, unlink } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import type { InstanceLease } from "../service/instance-lock.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "./private-backup.ts";
import { validateVaultSchema } from "./vault-schema.ts";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";

// 新安装只初始化两个同时缺失的文件；已有密钥或数据库缺失属于需要修复的状态。
export async function initializeNewInstallVault(lease: InstanceLease, vaultPath: string, masterKeyPath: string): Promise<void> {
  lease.assertOwned();
  const vault = await lstat(vaultPath).then(value => value, error => { if (error?.code === "ENOENT") return undefined; throw error; });
  const keyFile = await lstat(masterKeyPath).then(value => value, error => { if (error?.code === "ENOENT") return undefined; throw error; });
  if (vault && keyFile) return;
  if (vault || keyFile) throw new Error("Fresh Vault initialization requires both database and master key to be absent");
  const directory = await realpath(await verifyPrivateDirectory(lease.directory));
  if (resolve(vaultPath) === resolve(masterKeyPath) || await realpath(dirname(resolve(vaultPath))) !== directory || await realpath(dirname(resolve(masterKeyPath))) !== directory) throw new Error("Fresh Vault files must belong directly to the instance directory");
  const key = randomBytes(32);
  const created: { path: string; dev: bigint; ino: bigint }[] = [];
  let ready = false;
  await using cleanup = { async [Symbol.asyncDispose]() {
    if (ready) return;
    for (const item of created.toReversed()) {
      const current = await lstat(item.path, { bigint: true });
      if (current.dev !== item.dev || current.ino !== item.ino || !current.isFile() || current.isSymbolicLink()) throw new Error("Fresh Vault file ownership changed during cleanup");
      await unlink(item.path);
    }
  } };
  using releaseKey = { [Symbol.dispose]() { key.fill(0); } };
  await using keyHandle = await open(masterKeyPath, "wx", 0o600);
  const keyIdentity = await keyHandle.stat({ bigint: true }); created.push({ path: masterKeyPath, dev: keyIdentity.dev, ino: keyIdentity.ino });
  await verifyPrivateFile(masterKeyPath);
  await keyHandle.writeFile(key); await keyHandle.sync();
  await using databaseHandle = await open(vaultPath, "wx", 0o600);
  const databaseIdentity = await databaseHandle.stat({ bigint: true }); created.push({ path: vaultPath, dev: databaseIdentity.dev, ino: databaseIdentity.ino });
  await verifyPrivateFile(vaultPath);
  using database = new DatabaseSync(vaultPath);
  database.exec("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; BEGIN IMMEDIATE");
  let committed = false;
  using transaction = { [Symbol.dispose]() { if (!committed) database.exec("ROLLBACK"); } };
  database.exec(readFileSync(runtimeAssetPath("storage/vault-schema.sql", new URL("./vault-schema.sql", import.meta.url)), "utf8"));
  database.exec("PRAGMA user_version=4");
  validateVaultSchema(database);
  lease.assertOwned();
  database.exec("COMMIT"); committed = true;
  await databaseHandle.sync();
  ready = true;
}
