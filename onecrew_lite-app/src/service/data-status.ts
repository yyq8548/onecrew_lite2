import { createHash } from "node:crypto";
import { lstatSync } from "node:fs";
import { readFile, readdir } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { z } from "zod";
import { dataStartupStatus, type DataStartupStatus } from "@onecrew/runtime-contracts/status";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { inspectLegacyDatabase } from "../storage/legacy-sqlite.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import type { InstanceLease } from "./instance-lock.ts";

const manifest = z.strictObject({ format: z.literal(1), accountId: z.string().min(1), tenantId: z.string().min(1), baseId: z.string().min(1), database: z.literal("creator.sqlite") });
export interface UnifiedDataStatusOptions {
  lease: InstanceLease;
  generationRoot: string;
  vaultPath: string;
  originSource: string;
  candidate: boolean;
  activation?: { generationRoot: string; pointerSha256: string };
}

// 只读检查当前数据，不创建账号、不升级数据库、不把候选验收解释为生产激活。
export async function inspectUnifiedData(options: UnifiedDataStatusOptions): Promise<DataStartupStatus> {
  const { lease } = options;
  lease.assertOwned();
  const root = await verifyPrivateDirectory(options.generationRoot);
  await verifyPrivateFile(options.vaultPath);
  const vault = inspectLegacyDatabase(options.vaultPath, "vault");
  const targets: Array<{ store: "account" | "business"; from_version: number; to_version: number; impact: null }> = [{ store: "account", from_version: vault.version, to_version: vault.currentVersion, impact: null }];
  let current = vault.state === "current" && vault.schemaValidated;
  let accountCount = 0;
  for (const name of await readdir(root)) {
    if (!/^account-[a-f0-9]{64}$/u.test(name)) continue;
    const directory = await verifyPrivateDirectory(join(root, name));
    const metadata = join(directory, "account.json");
    await verifyPrivateFile(metadata);
    const owner = manifest.parse(parseApplicationJson(await readFile(metadata)));
    const expected = `account-${createHash("sha256").update(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId])).digest("hex")}`;
    if (name !== expected) throw new Error("data.account_manifest_mismatch");
    const path = join(directory, owner.database);
    await verifyPrivateFile(path);
    const inspected = inspectLegacyDatabase(path, "creator");
    targets.push({ store: "business", from_version: inspected.version, to_version: inspected.currentVersion, impact: null });
    current &&= inspected.state === "current" && inspected.schemaValidated;
    accountCount++;
  }
  if (options.activation) {
    if (relative(resolve(options.activation.generationRoot), resolve(root)) !== "") throw new Error("data.active_generation_changed");
    const pointer = join(lease.directory, "current.json");
    await verifyPrivateFile(pointer);
    if (createHash("sha256").update(await readFile(pointer)).digest("hex") !== options.activation.pointerSha256) throw new Error("data.active_generation_changed");
  }
  const trustedRoot = options.originSource === "new-install" && resolve(root) === resolve(lease.directory) || options.activation !== undefined || options.candidate;
  const blocked = !current || !trustedRoot || accountCount === 0 && options.originSource !== "new-install";
  // 既有旧布局必须经过迁移流程；不能把没有账号命名空间当成空的新安装。
  const unadopted = ["creator.sqlite", "creators.sqlite"].some(name => lstatSync(join(root, name), { throwIfNoEntry: false }) !== undefined);
  const ready = !blocked && !unadopted;
  lease.assertOwned();
  return dataStartupStatus.parse({
    phase: ready ? "ready" : "failed",
    diagnostic_code: ready ? options.candidate ? "data_candidate_inspected" : null : "data_migration_required",
    failure: null, upgrade: null, postflight: null,
    preflight: {
      state: ready ? accountCount === 0 ? "fresh" : "ready" : "blocked", targets,
      recoverable_backup: false, remote_data_deleted: false,
      blocked_store: ready ? null : !current && vault.state !== "current" ? "account" : "business",
      title: ready ? null : "Local data requires migration",
      message: ready ? null : "Inspect and migrate the existing data before starting account operations.",
      primary_action: ready ? null : "inspect_data", secondary_action: null,
      failure_kind: ready ? null : "unsafe_data", support_code: ready ? null : "data_migration_required",
    },
  });
}
