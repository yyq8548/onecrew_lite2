import { randomUUID } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import { join } from "node:path";
import type { InstanceLease } from "../service/instance-lock.ts";
import { backupLegacyDatabase } from "../storage/legacy-sqlite.ts";
import { createPrivateDirectory } from "../storage/private-backup.ts";
import { fingerprintSqlite } from "./sqlite-fingerprint.ts";
import { writeMigrationJournal as writeJournal } from "./journal.ts";

// 只准备 SQLite 恢复副本；Library、密钥及账号归属核验完成前不能激活。
export async function prepareSqliteRecovery(lease: InstanceLease, sources: { creator: string; vault: string }) {
  lease.assertOwned();
  const id = randomUUID();
  const directory = await createPrivateDirectory(join(lease.directory, `recovery-${id}`));
  const initial = { id, activated: false, createdAt: new Date().toISOString() } as const;
  await writeJournal(directory, { ...initial, state: "preparing" });
  let completed = false;
  // 清理记录再失败时，原生 SuppressedError 同时保留原错误与日志错误。
  await using journal = {
    async [Symbol.asyncDispose]() {
      if (!completed) await writeJournal(directory, { ...initial, state: "failed", reason: "sqlite_recovery_incomplete" });
    },
  };
  const creatorBefore = fingerprintSqlite(sources.creator);
  const vaultBefore = fingerprintSqlite(sources.vault);
  lease.assertOwned();
  const creator = await backupLegacyDatabase(sources.creator, join(directory, "creator", "creator.sqlite"), "creator");
  const vault = await backupLegacyDatabase(sources.vault, join(directory, "vault", "vault.sqlite"), "vault");
  lease.assertOwned();
  if (!isDeepStrictEqual(creatorBefore, fingerprintSqlite(creator.path)) ||
      !isDeepStrictEqual(vaultBefore, fingerprintSqlite(vault.path)) ||
      !isDeepStrictEqual(creatorBefore, fingerprintSqlite(sources.creator)) ||
      !isDeepStrictEqual(vaultBefore, fingerprintSqlite(sources.vault))) {
    throw new Error("Legacy SQLite source changed while preparing recovery; recovery is not ready");
  }
  const result = { ...initial, state: "sqlite_recovery_ready", directory, creator, vault, fingerprints: { creator: creatorBefore, vault: vaultBefore } } as const;
  await writeJournal(directory, result);
  completed = true;
  return result;
}
