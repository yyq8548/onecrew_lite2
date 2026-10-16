import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { createHash } from "node:crypto";
import { z } from "zod";
import type { InstanceLease } from "../service/instance-lock.ts";
import { verifyLegacyPrivateDirectory, verifyLegacyPrivateFile } from "../storage/legacy-private-source.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { partitionLegacyTableSync } from "../sync/legacy-table-sync.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";

const digest = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
const hashSchema = z.string().regex(/^[a-f0-9]{64}$/u);
const identitySchema = z.object({ device: z.string(), inode: z.string(), bytes: z.string(), mtime: z.string(), ctime: z.string(), sha256: hashSchema }).strict();
const receiptSchema = z.object({ format: z.literal(1), sourceDirectory: z.string(), sourcePath: z.string(), accountId: z.string().min(1),
  source: identitySchema.nullable(), backupPath: z.string().nullable(), selectedPath: z.string().nullable(), selectedSha256: hashSchema.nullable(), otherUserIds: z.array(z.string()) }).strict();
export type TableSyncStagingReceipt = z.infer<typeof receiptSchema>;
function descendant(root: string, child: string): void {
  const value = relative(resolve(root), resolve(child));
  if (!value || isAbsolute(value) || value === ".." || value.startsWith(`..${sep}`)) throw new Error("migration.table_sync_path_owner_mismatch");
}
async function readSource(path: string, legacy: boolean) {
  const before = await lstat(path, { bigint: true }).then(value => value, error => { if (error.code === "ENOENT") return null; throw error; });
  if (!before) return null;
  if (legacy) await verifyLegacyPrivateFile(path); else await verifyPrivateFile(path);
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || before.size > 512n * 1024n * 1024n || resolve(path) !== await realpath(path)) throw new Error("migration.table_sync_source_invalid");
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.dev !== before.dev || opened.ino !== before.ino) throw new Error("migration.table_sync_source_changed");
  const bytes = await file.readFile(), after = await file.stat({ bigint: true }), current = await lstat(path, { bigint: true });
  if (after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs || current.dev !== opened.dev || current.ino !== opened.ino || current.isSymbolicLink() || current.nlink !== 1n || current.size !== after.size || current.mtimeNs !== after.mtimeNs || current.ctimeNs !== after.ctimeNs) throw new Error("migration.table_sync_source_changed");
  return { bytes, identity: { device: after.dev.toString(), inode: after.ino.toString(), bytes: after.size.toString(), mtime: after.mtimeNs.toString(), ctime: after.ctimeNs.toString(), sha256: digest(bytes) } };
}
async function privateWrite(path: string, bytes: Uint8Array): Promise<void> {
  await using file = await open(path, "wx", 0o600); await verifyPrivateFile(path); await file.writeFile(bytes); await file.sync();
}

export async function prepareTableSyncStaging(lease: InstanceLease, input: { sourceDirectory: string; generationDirectory: string; accountDirectory: string; accountId: string }): Promise<TableSyncStagingReceipt> {
  lease.assertOwned();
  const sourceDirectory = await verifyLegacyPrivateDirectory(input.sourceDirectory), sourcePath = join(sourceDirectory, "dynamic-table-sync.json");
  descendant(lease.directory, input.generationDirectory); descendant(input.generationDirectory, input.accountDirectory);
  await verifyPrivateDirectory(input.generationDirectory); await verifyPrivateDirectory(input.accountDirectory);
  const loaded = await readSource(sourcePath, true);
  if (!loaded) return receiptSchema.parse({ format: 1, sourceDirectory, sourcePath, accountId: input.accountId, source: null, backupPath: null, selectedPath: null, selectedSha256: null, otherUserIds: [] });
  const partition = await partitionLegacyTableSync(loaded.bytes, input.accountId);
  const history = await createPrivateDirectory(join(input.generationDirectory, "history")), backupPath = join(history, "dynamic-table-sync.json"), selectedPath = join(input.accountDirectory, "dynamic-table-sync.json");
  await privateWrite(backupPath, loaded.bytes);
  const selectedBytes = Buffer.from(stringifyApplicationJson(partition.selected));
  await privateWrite(selectedPath, selectedBytes);
  const receipt = receiptSchema.parse({ format: 1, sourceDirectory, sourcePath, accountId: input.accountId, source: loaded.identity, backupPath, selectedPath, selectedSha256: digest(selectedBytes), otherUserIds: partition.otherUserIds });
  await verifyTableSyncStaging(lease, receipt, input.generationDirectory, input.accountDirectory, input.accountId);
  return receipt;
}

export async function verifyTableSyncStaging(lease: InstanceLease, value: unknown, generation: string, accountDirectory: string, accountId: string): Promise<TableSyncStagingReceipt> {
  lease.assertOwned(); const receipt = receiptSchema.parse(value);
  descendant(lease.directory, generation); descendant(generation, accountDirectory);
  await verifyLegacyPrivateDirectory(receipt.sourceDirectory);
  if (receipt.accountId !== accountId || receipt.sourcePath !== join(receipt.sourceDirectory, "dynamic-table-sync.json")) throw new Error("migration.table_sync_owner_changed");
  const original = await readSource(receipt.sourcePath, true);
  if (JSON.stringify(original?.identity ?? null) !== JSON.stringify(receipt.source)) throw new Error("migration.table_sync_source_changed");
  if (!original) {
    if (receipt.backupPath !== null || receipt.selectedPath !== null || receipt.selectedSha256 !== null || receipt.otherUserIds.length) throw new Error("migration.table_sync_absence_changed");
    return receipt;
  }
  if (receipt.backupPath !== join(generation, "history", "dynamic-table-sync.json") || receipt.selectedPath !== join(accountDirectory, "dynamic-table-sync.json")) throw new Error("migration.table_sync_copy_owner_changed");
  const backup = await readSource(receipt.backupPath, false), selected = await readSource(receipt.selectedPath, false);
  const partition = await partitionLegacyTableSync(original.bytes, accountId), expected = Buffer.from(stringifyApplicationJson(partition.selected));
  if (!backup || !selected || backup.identity.sha256 !== original.identity.sha256 || selected.identity.sha256 !== receipt.selectedSha256 || selected.identity.sha256 !== digest(expected) || JSON.stringify(receipt.otherUserIds) !== JSON.stringify(partition.otherUserIds)) throw new Error("migration.table_sync_copy_changed");
  lease.assertOwned(); return receipt;
}
