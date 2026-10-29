import { createHash } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { gmailConnectionIdSchema, mailConnectionSchema } from "./access-mail-client.ts";
import type { MailConnection } from "./access-mail-client.ts";

export const GMAIL_SEND_SCOPE = "https://www.googleapis.com/auth/gmail.send";
export const GMAIL_READ_SCOPE = "https://www.googleapis.com/auth/gmail.readonly";
export function asciiLower(value: string): string { return value.replace(/[A-Z]/gu, letter => letter.toLowerCase()); }
export function gmailMailboxId(user: string, connection: string): string {
  gmailConnectionIdSchema.parse(connection);
  const hex = createHash("sha256").update(`onecrew-gmail\0${user}\0${connection}`).digest("hex").slice(0, 32);
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
function rustTrim(value: string): string { return value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, ""); }
export function gmailProjection(user: string, connection: MailConnection) {
  const parsed = mailConnectionSchema.parse(connection);
  const email = asciiLower(rustTrim(parsed.email));
  const parts = email.split("@");
  if (parts.length !== 2 || !parts[0] || !parts[1] || /\p{White_Space}/u.test(email)) throw new Error("mailbox email address is invalid");
  const label = rustTrim(parsed.email);
  if (!label || !user) throw new Error("gmail_projection_invalid");
  const available = parsed.status === "connected" && parsed.capabilities.can_send && parsed.capabilities.can_read_thread && parsed.scopes.includes(GMAIL_SEND_SCOPE) && parsed.scopes.includes(GMAIL_READ_SCOPE);
  return { id: gmailMailboxId(user, parsed.id), email, label, available,
    authConnectionId: `gmail:${user}:${parsed.id}`, secretRecordId: `access:${user}:${parsed.id}`, checkpointNamespace: `gmail-${parsed.id}` };
}

export interface GmailMailboxBinding {
  id: string; email: string; provider: string; available: boolean; capabilities: string[];
  authConnectionId: string; secretRecordId: string; providerKind: string; authAvailable: boolean;
}
const columns = {
  mailbox_refs: ["id", "provider", "label", "email_address", "display_name", "capabilities_json", "available", "last_error", "created_at", "updated_at"],
  email_auth_connections: ["id", "provider_kind", "secret_record_id", "label", "available", "created_at", "updated_at"],
  mailbox_identity_mappings: ["mailbox_ref_id", "auth_connection_id", "provider_kind", "projection_kind", "capability_projection_revision", "checkpoint_namespace", "created_at", "updated_at"],
} as const;

export class GmailMailboxRepository {
  readonly #db: DatabaseSync;
  constructor(db: DatabaseSync) {
    if (![90, 91, 92, 93, 94, 95, 96, 97].includes(Number(db.prepare("PRAGMA user_version").get()!.user_version)) || db.prepare("PRAGMA foreign_keys").get()!.foreign_keys !== 1) throw new Error("Gmail mailbox mapping requires a supported Creator schema with foreign keys");
    for (const [table, expected] of Object.entries(columns)) {
      if (db.prepare(`PRAGMA table_info(${table})`).all().map(row => row.name).join("|") !== expected.join("|")) throw new Error(`Incompatible ${table} mapping columns`);
    }
    const authIndexes = db.prepare("PRAGMA index_list(email_auth_connections)").all();
    for (const expected of ["id|provider_kind", "secret_record_id"]) {
      if (!authIndexes.some(index => index.unique === 1 && index.partial === 0 && db.prepare("SELECT name FROM pragma_index_info(?) ORDER BY seqno").all(String(index.name)).map(row => row.name).join("|") === expected)) throw new Error("Incompatible email auth connection identity constraints");
    }
    const foreignKeys = db.prepare("PRAGMA foreign_key_list(mailbox_identity_mappings)").all();
    if (foreignKeys.length !== 3 || !foreignKeys.some(row => row.table === "mailbox_refs" && row.from === "mailbox_ref_id" && row.to === "id" && row.on_delete === "RESTRICT") ||
      !foreignKeys.some(row => row.table === "email_auth_connections" && row.from === "auth_connection_id" && row.to === "id" && row.on_delete === "RESTRICT") ||
      !foreignKeys.some(row => row.table === "email_auth_connections" && row.from === "provider_kind" && row.to === "provider_kind" && row.on_delete === "RESTRICT")) throw new Error("Incompatible Gmail mailbox mapping foreign keys");
    this.#db = db;
  }

  reconcile(user: string, connections: readonly MailConnection[]): void {
    const projections = connections.filter(connection => connection.provider === "google").map(connection => gmailProjection(user, connection));
    if (this.#db.prepare("PRAGMA journal_mode").get()!.journal_mode !== "wal" || Number(this.#db.prepare("PRAGMA synchronous").get()!.synchronous) < 2) throw new Error("Gmail mapping writes require durable WAL storage");
    this.#db.exec("BEGIN IMMEDIATE");
    let committed = false;
    try {
      const now = Math.floor(Date.now() / 1000);
      this.#db.prepare("UPDATE email_auth_connections SET available=0,updated_at=? WHERE provider_kind='resend' AND available=1").run(now);
      this.#db.prepare("UPDATE mailbox_refs SET available=0,last_error='gmail_authorization_required',updated_at=? WHERE provider='gmail'").run(now);
      for (const projection of projections) {
        const existingAuth = this.#db.prepare("SELECT provider_kind,secret_record_id FROM email_auth_connections WHERE id=?").get(projection.authConnectionId);
        if (existingAuth && (existingAuth.provider_kind !== "remote" || existingAuth.secret_record_id !== projection.secretRecordId)) throw new Error("email auth connection identity is already bound to another secret");
        const existingMapping = this.#db.prepare("SELECT auth_connection_id,provider_kind FROM mailbox_identity_mappings WHERE mailbox_ref_id=?").get(projection.id);
        if (existingMapping && (existingMapping.auth_connection_id !== projection.authConnectionId || existingMapping.provider_kind !== "remote")) throw new Error("mailbox identity is already mapped to another auth connection");
        this.#db.prepare("INSERT INTO mailbox_refs(id,provider,label,email_address,display_name,capabilities_json,available,last_error,created_at,updated_at) VALUES (?,'gmail',?,?,NULL,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET provider=excluded.provider,label=excluded.label,email_address=excluded.email_address,display_name=excluded.display_name,capabilities_json=excluded.capabilities_json,available=excluded.available,last_error=excluded.last_error,updated_at=excluded.updated_at").run(projection.id, projection.label, projection.email, JSON.stringify(["send", "sent_readback", "inbox"]), Number(projection.available), projection.available ? null : "gmail_authorization_required", now, now);
        this.#db.prepare("INSERT INTO email_auth_connections(id,provider_kind,secret_record_id,label,available,created_at,updated_at) VALUES (?,'remote',?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET label=excluded.label,available=excluded.available,updated_at=excluded.updated_at").run(projection.authConnectionId, projection.secretRecordId, projection.label, Number(projection.available), now, now);
        this.#db.prepare("INSERT INTO mailbox_identity_mappings(mailbox_ref_id,auth_connection_id,provider_kind,projection_kind,capability_projection_revision,checkpoint_namespace,created_at,updated_at) VALUES (?,?,'remote','mailbox',2,?,?,?) ON CONFLICT(mailbox_ref_id) DO UPDATE SET projection_kind=excluded.projection_kind,capability_projection_revision=excluded.capability_projection_revision,checkpoint_namespace=excluded.checkpoint_namespace,updated_at=excluded.updated_at").run(projection.id, projection.authConnectionId, projection.checkpointNamespace, now, now);
      }
      this.#db.exec("COMMIT"); committed = true;
    } finally { if (!committed) this.#db.exec("ROLLBACK"); }
  }

  binding(id: string): GmailMailboxBinding {
    const row = this.#db.prepare("SELECT m.id,m.provider,m.email_address,m.available,m.capabilities_json,a.id AS auth_connection_id,a.secret_record_id,a.provider_kind,a.available AS auth_available FROM mailbox_refs m JOIN mailbox_identity_mappings p ON p.mailbox_ref_id=m.id JOIN email_auth_connections a ON a.id=p.auth_connection_id AND a.provider_kind=p.provider_kind WHERE m.id=?").get(id);
    if (!row) throw new Error("gmail_mailbox_unavailable");
    const parsed = z.object({ id: z.string(), provider: z.string(), email_address: z.string(), available: z.union([z.literal(0), z.literal(1)]), capabilities_json: z.string(), auth_connection_id: z.string(), secret_record_id: z.string(), provider_kind: z.string(), auth_available: z.union([z.literal(0), z.literal(1)]) }).parse(row);
    return { id: parsed.id, provider: parsed.provider, email: parsed.email_address, available: parsed.available === 1, capabilities: z.array(z.string()).parse(JSON.parse(parsed.capabilities_json)), authConnectionId: parsed.auth_connection_id, secretRecordId: parsed.secret_record_id, providerKind: parsed.provider_kind, authAvailable: parsed.auth_available === 1 };
  }
}
