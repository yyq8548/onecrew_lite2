import { createHash } from "node:crypto";
import { inspect } from "node:util";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { verifyPrivateFile } from "../storage/private-backup.ts";
import { decryptVaultEnvelope, readVaultMasterKey } from "../storage/vault-reader.ts";
import { validateVaultSchema } from "../storage/vault-schema.ts";

const unsigned = z.number().int().safe().nonnegative();
const positive = unsigned.min(1);
const uuid = z.guid().transform(value => value.toLowerCase());
const identifier = z.string().refine(value => value.isWellFormed() && value.length >= 1 && value.length <= 128);
export const creatorSyncLimits = Object.freeze({ max_snapshot_bytes: 32 * 1024 * 1024, max_chunk_bytes: 1024 * 1024, max_chunks: 128, max_namespace_staged_bytes: 100 * 1024 * 1024, max_access_user_staged_bytes: 400 * 1024 * 1024 });
export const accessCredentialMetadataSchema = z.object({
  installation_id: uuid, credential_id: uuid, access_user_id: identifier, access_device_id: z.string().regex(/^dev_[A-Za-z0-9_-]{24}$/u),
  source_instance_id: uuid, access_pairing_id: z.string().regex(/^pair_[A-Za-z0-9_-]{24}$/u),
  client_platform: z.enum(["macos", "windows"]), app_version: z.string().regex(/^[A-Za-z0-9][A-Za-z0-9.+-]{0,63}$/u),
  status: z.enum(["active", "revoked", "invalidated"]), revision: positive,
  created_at: unsigned, updated_at: unsigned, last_seen_at: unsigned.nullable().optional(), revoked_at: unsigned.nullable().optional(),
  protocol: z.literal("reacher.creator_sync"), protocol_version: z.literal(1),
  capabilities: z.tuple([z.literal("creator_snapshots:write_chunked_quarantine")]),
  accepted_bundle: z.object({
    format: z.literal("reacher.creator_bundle"), format_version: z.union([z.literal(1), z.literal(2), z.literal(3)]), profile: z.literal("portable_public"),
    max_snapshot_bytes: z.literal(creatorSyncLimits.max_snapshot_bytes), max_chunk_bytes: z.literal(creatorSyncLimits.max_chunk_bytes), max_chunks: z.literal(creatorSyncLimits.max_chunks),
    max_namespace_staged_bytes: z.literal(creatorSyncLimits.max_namespace_staged_bytes), max_access_user_staged_bytes: z.literal(creatorSyncLimits.max_access_user_staged_bytes),
  }).strict(),
}).strict().refine(value => value.installation_id !== value.credential_id && value.updated_at >= value.created_at &&
  !(value.status === "active" && value.revoked_at != null) && !(value.status === "revoked" && value.revoked_at == null));

export type StoredAccessMetadata = z.infer<typeof accessCredentialMetadataSchema>;

export function validateAccessBearer(credentialId: string, bearer: string): void {
  const prefix = `${credentialId}_`;
  if (!bearer.startsWith(prefix) || !/^[A-Za-z0-9_-]{43,128}$/u.test(bearer.slice(prefix.length)) || Buffer.byteLength(bearer) < 80 || Buffer.byteLength(bearer) > 165) {
    throw new Error("Stored Access credential bearer is invalid");
  }
}

// 仅表示本地格式验证通过；远端安装验证成功后才能建立 AccountContext。
export class StoredAccessCredential {
  readonly metadata: Readonly<StoredAccessMetadata>;
  readonly fingerprint: string;
  #bearer: Buffer;
  #disposed = false;
  constructor(metadata: StoredAccessMetadata, fingerprint: string, bearer: string) {
    Object.freeze(metadata.capabilities);
    Object.freeze(metadata.accepted_bundle);
    this.metadata = Object.freeze(metadata);
    this.fingerprint = fingerprint;
    this.#bearer = Buffer.from(bearer, "utf8");
  }
  exposeBearer(): string {
    if (this.#disposed) throw new Error("Stored Access credential has been disposed");
    return this.#bearer.toString("utf8");
  }
  dispose(): void {
    this.#bearer.fill(0);
    this.#disposed = true;
  }
  toJSON(): never {
    throw new Error("Stored Access credentials cannot be serialized");
  }
  [inspect.custom](): string {
    return "[StoredAccessCredential: redacted]";
  }
}

export type StoredAccessState =
  | { state: "missing" }
  | { state: "inactive"; metadata: Readonly<StoredAccessMetadata> }
  | { state: "active"; credential: StoredAccessCredential };

export async function readStoredAccessCredential(vaultPath: string, masterKeyPath: string): Promise<StoredAccessState> {
  await verifyPrivateFile(vaultPath);
  const database = new DatabaseSync(vaultPath, { readOnly: true });
  let key: Buffer | undefined;
  try {
    key = await readVaultMasterKey(masterKeyPath);
    database.exec("PRAGMA query_only=ON; PRAGMA busy_timeout=5000; BEGIN");
    if (validateVaultSchema(database) !== 4) throw new Error("Stored Access reading requires Vault schema 4");
    const rows = database.prepare("SELECT id,label,CAST(metadata_json AS BLOB) AS metadata_bytes,encrypted_secret,disabled,cooldown_until,last_error FROM service_credentials WHERE provider='creator_sync' ORDER BY id").all();
    if (rows.length === 0) return { state: "missing" };
    if (rows.length !== 1) throw new Error("Creator Sync credential identity is ambiguous");
    const row = z.object({ id: uuid, label: z.literal("creator-sync"), metadata_bytes: z.instanceof(Uint8Array),
      encrypted_secret: z.instanceof(Uint8Array), disabled: z.union([z.literal(0), z.literal(1)]),
      cooldown_until: z.null(), last_error: z.null() }).parse(rows[0]);
    const metadata = accessCredentialMetadataSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(row.metadata_bytes), true));
    if ((metadata.status === "active") !== (row.disabled === 0)) throw new Error("Stored Access credential lifecycle is inconsistent");
    if (metadata.status !== "active") return { state: "inactive", metadata: Object.freeze(metadata) };
    const plaintext = decryptVaultEnvelope(key, row.encrypted_secret);
    try {
      const secret = z.object({ value: z.string() }).parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(plaintext)));
      validateAccessBearer(metadata.credential_id, secret.value);
      const fingerprint = createHash("sha256").update(row.id).update("\0").update(row.metadata_bytes).update(row.encrypted_secret).digest("hex");
      return { state: "active", credential: new StoredAccessCredential(metadata, fingerprint, secret.value) };
    } finally {
      plaintext.fill(0);
    }
  } finally {
    key?.fill(0);
    database.close();
  }
}
