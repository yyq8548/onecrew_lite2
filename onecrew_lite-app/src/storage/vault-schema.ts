import { readStorageSql } from "./sql-resource.ts";
import { lstat } from "node:fs/promises";
import { resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { verifyPrivateFile } from "./private-backup.ts";
import { normalizeCreatorSchemaSql } from "./creator-schema.ts";

// Rust local-vault 的冻结 v2、v3、v4 表和索引定义相同，仅版本标记与 provider 编码不同。
const schemaSql = readStorageSql("vault-schema.sql");

function schemaObjects(db: DatabaseSync): string {
  return JSON.stringify(db.prepare("SELECT type || ':' || name AS name, sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type, name")
    .all().map((row) => [row.name, normalizeCreatorSchemaSql(String(row.sql))]));
}

export function validateVaultSchema(db: DatabaseSync): 2 | 3 | 4 {
  const version = Number(db.prepare("PRAGMA user_version").get()!.user_version);
  if (version !== 2 && version !== 3 && version !== 4) throw new Error(`Unsupported vault schema version ${version}`);
  const reference = new DatabaseSync(":memory:");
  try {
    reference.exec(schemaSql);
    const tableSql = "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name";
    if (JSON.stringify(db.prepare(tableSql).all()) !== JSON.stringify(reference.prepare(tableSql).all()) || schemaObjects(db) !== schemaObjects(reference)) {
      throw new Error(`Vault schema version ${version} failed its frozen identity check`);
    }
  } finally {
    reference.close();
  }
  const checks = db.prepare("PRAGMA integrity_check").all();
  if (checks.length !== 1 || checks[0]!.integrity_check !== "ok" || db.prepare("PRAGMA foreign_key_check").all().length !== 0) {
    throw new Error("Vault integrity check failed");
  }
  return version;
}

function validateEmailMetadata(raw: string): void {
  const value: unknown = JSON.parse(raw);
  const validateJsonValues = (item: unknown): void => {
    if ((typeof item === "number" && !Number.isFinite(item)) || (typeof item === "string" && !item.isWellFormed())) {
      throw new Error("Email secret metadata contains a JSON value unsupported by Rust serde_json");
    }
    if (item !== null && typeof item === "object") {
      for (const [key, nested] of Object.entries(item)) {
        if (!key.isWellFormed()) throw new Error("Email secret metadata contains an invalid Unicode key");
        validateJsonValues(nested);
      }
    }
  };
  validateJsonValues(value);
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error("Email secret metadata must be an object");
  const forbidden = ["secret", "token", "password", "api_key", "private_key", "client_secret", "authorization"];
  const inspect = (object: object): void => {
    for (const [key, nested] of Object.entries(object)) {
      const lower = key.replace(/[A-Z]/g, (character) => character.toLowerCase());
      if (forbidden.some((part) => lower.includes(part))) throw new Error("Email secret metadata contains a secret-bearing field");
      // 与 Rust 保持一致：递归 JSON object，数组不递归。
      if (nested !== null && typeof nested === "object" && !Array.isArray(nested)) inspect(nested);
    }
  };
  inspect(value);
}

export interface VaultStagingUpgrade {
  sourcePath: string;
  stagingPath: string;
}

export async function upgradeVaultStagingCopy({ sourcePath, stagingPath }: VaultStagingUpgrade): Promise<{ fromVersion: 2 | 3 | 4; toVersion: 4 }> {
  const staging = resolve(stagingPath);
  const source = resolve(sourcePath);
  if (staging === source) throw new Error("Vault upgrade requires a distinct isolated staging copy");
  await verifyPrivateFile(staging);
  const sourceInfo = await lstat(source, { bigint: true });
  const stagingInfo = await lstat(staging, { bigint: true });
  if (!sourceInfo.isFile() || sourceInfo.isSymbolicLink() || !stagingInfo.isFile() || stagingInfo.nlink !== 1n ||
      (sourceInfo.dev === stagingInfo.dev && sourceInfo.ino === stagingInfo.ino)) {
    throw new Error("Vault staging copy must have an independent regular file identity");
  }
  const db = new DatabaseSync(staging);
  let committed = false;
  try {
    db.exec("PRAGMA busy_timeout = 5000; BEGIN IMMEDIATE");
    const fromVersion = validateVaultSchema(db);
    if (fromVersion < 4) {
      // Rust schema/upgrade.rs：v2→v3 无数据改动；v3→v4 仅变更旧 Resend provider 编码。
      const conflict = db.prepare("SELECT legacy.label FROM service_credentials legacy JOIN service_credentials typed ON typed.provider = 'email_resend_api_key' AND typed.label = legacy.label WHERE legacy.provider = 'resend' ORDER BY legacy.label LIMIT 1").get();
      if (conflict) throw new Error("Resend credential identity is ambiguous during typed Email upgrade");
      const metadata = db.prepare("SELECT metadata_json FROM service_credentials WHERE provider = 'resend' ORDER BY id").all();
      for (const row of metadata) {
        if (typeof row.metadata_json !== "string") throw new Error("Vault metadata_json must be text");
        validateEmailMetadata(row.metadata_json);
      }
      db.prepare("UPDATE service_credentials SET provider = 'email_resend_api_key' WHERE provider = ?").run("resend");
      db.exec("PRAGMA user_version = 4");
    }
    validateVaultSchema(db);
    await verifyPrivateFile(staging);
    const after = await lstat(staging, { bigint: true });
    if (after.dev !== stagingInfo.dev || after.ino !== stagingInfo.ino || after.nlink !== 1n) throw new Error("Vault staging file identity changed");
    db.exec("COMMIT");
    committed = true;
    return { fromVersion, toVersion: 4 };
  } finally {
    try {
      if (!committed && db.isTransaction) db.exec("ROLLBACK");
    } finally {
      db.close();
    }
  }
}
