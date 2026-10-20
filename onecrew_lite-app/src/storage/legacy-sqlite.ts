import { open, realpath } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { backup, DatabaseSync } from "node:sqlite";
import { createPrivateDirectory, verifyPrivateFile } from "./private-backup.ts";
import { validateVaultSchema } from "./vault-schema.ts";
import { validateCreatorSchema } from "./creator-schema.ts";

export type LegacyDatabaseKind = "creator" | "vault";

const versions = {
  creator: { current: 91, earliest: 27 },
  vault: { current: 4, earliest: 2 },
} as const;

export interface LegacyDatabaseInspection {
  kind: LegacyDatabaseKind;
  version: number;
  currentVersion: number;
  state: "empty" | "current" | "upgrade-required";
  tables: string[];
  // 旧 Creator 版本仍需升级；当前版本与 Vault 已校验冻结结构。
  schemaValidated: boolean;
}

function inspectConnection(db: DatabaseSync, kind: LegacyDatabaseKind): LegacyDatabaseInspection {
  const version = Number(db.prepare("PRAGMA user_version").get()!.user_version);
  const policy = versions[kind];
  if (version > policy.current) throw new Error(`Future ${kind} schema version ${version}; supported through ${policy.current}`);
  const checks = db.prepare("PRAGMA integrity_check").all();
  if (checks.length !== 1 || checks[0]!.integrity_check !== "ok") {
    throw new Error(`${kind} database integrity check failed`);
  }
  const tables = db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
    .all().map((row) => String(row.name));
  const empty = version === 0 && tables.length === 0;
  if (!empty && version < policy.earliest) throw new Error(`Unsupported ${kind} schema version ${version}`);
  if (kind === "vault" && !empty) validateVaultSchema(db);
  if (kind === "creator" && (version === 90 || version === 91)) validateCreatorSchema(db);
  if (db.prepare("PRAGMA foreign_key_check").all().length !== 0) {
    throw new Error(`${kind} database foreign key check failed`);
  }
  return {
    kind, version, currentVersion: policy.current, tables, schemaValidated: !empty && (kind === "vault" || version === 90 || version === 91),
    state: empty ? "empty" : version === policy.current ? "current" : "upgrade-required",
  };
}

export function inspectLegacyDatabase(path: string, kind: LegacyDatabaseKind): LegacyDatabaseInspection {
  const db = new DatabaseSync(path, { readOnly: true });
  try {
    db.exec("PRAGMA query_only = ON; PRAGMA busy_timeout = 5000; BEGIN");
    return inspectConnection(db, kind);
  } finally {
    db.close();
  }
}

export async function backupLegacyDatabase(source: string, destination: string, kind: LegacyDatabaseKind) {
  const sourcePath = await realpath(source);
  const destinationPath = resolve(destination);
  if (sourcePath.toLowerCase() === destinationPath.toLowerCase()) throw new Error("Backup destination must differ from source");
  const db = new DatabaseSync(sourcePath, { readOnly: true });
  try {
    db.exec("PRAGMA query_only = ON; PRAGMA busy_timeout = 5000");
    inspectConnection(db, kind);
    // 要求调用者指定尚不存在的专用目录，绝不更改已有目录权限。
    await createPrivateDirectory(dirname(destinationPath));
    // 独占创建，禁止覆盖已有备份；失败留下的文件不作为成功备份返回。
    const reservation = await open(destinationPath, "wx", 0o600);
    await reservation.close();
    await verifyPrivateFile(destinationPath);
    const pages = await backup(db, destinationPath);
    await verifyPrivateFile(destinationPath);
    const inspection = inspectLegacyDatabase(destinationPath, kind);
    return { path: destinationPath, pages, inspection };
  } finally {
    db.close();
  }
}
