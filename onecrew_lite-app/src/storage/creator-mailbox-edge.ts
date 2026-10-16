import { randomUUID } from "node:crypto";
import { readStorageSql } from "./sql-resource.ts";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { rebuildCreatorTable } from "./creator-schema.ts";

function uuidBytes(value: string): Buffer | undefined {
  if (value.length === 32) return z.hex().length(32).safeParse(value).success ? Buffer.from(value, "hex") : undefined;
  const unwrapped = value.length === 38 && value.startsWith("{") && value.endsWith("}") ? value.slice(1, -1)
    : value.length === 45 && value.startsWith("urn:uuid:") ? value.slice(9) : value;
  return z.guid().safeParse(unwrapped).success ? Buffer.from(unwrapped.replaceAll("-", ""), "hex") : undefined;
}

export function legacyResendAuthConnectionId(value: string): string | undefined {
  const bytes = uuidBytes(value);
  if (!bytes) return undefined;
  const mask = Buffer.from("524553454e442d415554482d434f4e4e", "hex");
  for (let index = 0; index < bytes.length; index++) bytes[index] = bytes[index]! ^ mask[index]!;
  const hex = bytes.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

// 只构造旧邮箱的本地认证映射；无效历史 UUID 保留记录并明确标为不可用。
export function migrateCreatorMailboxEdge(db: DatabaseSync): void {
  rebuildCreatorTable(db, "mailbox_refs", "provider = 'resend'", "provider IN ('resend','smtp','imap')");
  db.exec(readStorageSql("creator-31-to-32.sql"));
  const query = db.prepare("SELECT id,provider,label,available,created_at,updated_at FROM mailbox_refs ORDER BY id");
  query.setReadBigInts(true);
  for (const row of query.all()) {
    if (typeof row.id !== "string" || typeof row.label !== "string" || typeof row.available !== "bigint") throw new Error("Legacy mailbox identity, label or availability has an invalid SQLite type");
    const id = row.id;
    if (row.provider !== "resend") throw new Error(`Legacy mailbox ${id} provider is unsupported`);
    if (typeof row.created_at !== "bigint" || row.created_at < 0n || typeof row.updated_at !== "bigint" || row.updated_at < 0n) throw new Error("Legacy mailbox timestamps are not unsigned integers");
    const derived = legacyResendAuthConnectionId(id);
    const auth = derived ?? randomUUID();
    const checkpoint = derived === undefined ? `email-checkpoint-${randomUUID()}` : `email-checkpoint-${id}`;
    db.prepare("INSERT INTO email_auth_connections(id,provider_kind,secret_record_id,label,available,created_at,updated_at) VALUES(?,'resend',?,?,?,?,?)")
      .run(auth, id, row.label, row.available !== 0n && derived !== undefined ? 1 : 0, row.created_at, row.updated_at);
    db.prepare("INSERT INTO mailbox_identity_mappings(mailbox_ref_id,auth_connection_id,provider_kind,projection_kind,capability_projection_revision,checkpoint_namespace,created_at,updated_at) VALUES(?,?,'resend','mailbox',1,?,?,?)")
      .run(id, auth, checkpoint, row.created_at, row.updated_at);
  }
}
