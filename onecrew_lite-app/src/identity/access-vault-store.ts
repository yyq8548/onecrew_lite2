import { createCipheriv, createHash, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import { lstat } from "node:fs/promises";
import { lstatSync, type BigIntStats } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import type { InstanceLease } from "../service/instance-lock.ts";
import { verifyPrivateFile } from "../storage/private-backup.ts";
import { decryptVaultEnvelope, readVaultMasterKey } from "../storage/vault-reader.ts";
import { validateVaultSchema } from "../storage/vault-schema.ts";
import { fingerprintSqlite } from "../migration/sqlite-fingerprint.ts";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { AccountChangedError } from "./account-context.ts";
import { accessCredentialMetadataSchema, readStoredAccessCredential, validateAccessBearer } from "./legacy-access-credential.ts";
import { VerifiedMigrationCredential } from "./access-verification.ts";

const rowSchema = z.object({ id: z.guid(), label: z.literal("creator-sync"), metadata_json: z.string(), encrypted_secret: z.instanceof(Uint8Array),
  disabled: z.union([z.literal(0), z.literal(1)]), cooldown_until: z.null(), last_error: z.null(), created_at: z.string(), updated_at: z.string() });
const now = () => BigInt(Math.floor(Date.now() / 1000));
const revision = (value: string) => { if (!/^\d+$/u.test(value) || BigInt(value) >= 18446744073709551615n) throw new Error("access.credential_revision_invalid"); return (BigInt(value) + 1n > now() ? BigInt(value) + 1n : now()).toString(); };

const preparedWriteIssuer = Symbol("prepared-access-vault-write");
type PreparedVaultFile = Readonly<{ path: string; stat: BigIntStats | undefined }>;
function sameFileStat(current: BigIntStats | undefined, expected: BigIntStats | undefined): boolean {
  if (!current || !expected) return current === expected;
  return current.isFile() && !current.isSymbolicLink() && current.nlink === 1n && current.dev === expected.dev && current.ino === expected.ino && current.size === expected.size &&
    current.mtimeNs === expected.mtimeNs && current.ctimeNs === expected.ctimeNs && current.mode === expected.mode;
}
export class PreparedAccessVaultWrite {
  readonly #store: AccessVaultStore;
  readonly #files: readonly PreparedVaultFile[];
  readonly masterKeySha256: string;
  #used = false;
  constructor(issuer: symbol, store: AccessVaultStore, files: readonly PreparedVaultFile[], masterKeySha256: string) {
    if (issuer !== preparedWriteIssuer) throw new Error("access.prepared_write_unissued");
    this.#store = store; this.#files = files; this.masterKeySha256 = masterKeySha256; Object.freeze(this);
  }
  consume(store: AccessVaultStore): void {
    if (store !== this.#store || this.#used) throw new Error("access.prepared_write_invalid");
    store.lease.assertOwned();
    for (const { path, stat } of this.#files) {
      const current = lstatSync(path, { bigint: true, throwIfNoEntry: false });
      // 自己的 BEGIN IMMEDIATE 可创建空 WAL；任何已有事务帧仍必须保持原身份。
      const ownEmptyWal = !stat && path === `${store.vaultPath}-wal` && current?.isFile() && !current.isSymbolicLink() && current.nlink === 1n && current.size === 0n;
      if (!ownEmptyWal && !sameFileStat(current, stat)) throw new AccountChangedError();
    }
    this.#used = true;
  }
  toJSON(): never { throw new Error("Prepared Vault write authority cannot be serialized"); }
}

// 只写调用者提供的私有 Vault；首次保存必须持有真实远端签发的短期证明。
export class AccessVaultStore {
  readonly lease: InstanceLease;
  readonly vaultPath: string;
  readonly masterKeyPath: string;
  #writes: Promise<unknown> = Promise.resolve();
  constructor(lease: InstanceLease, vaultPath: string, masterKeyPath: string) { this.lease = lease; this.vaultPath = vaultPath; this.masterKeyPath = masterKeyPath; }
  load() { this.lease.assertOwned(); return readStoredAccessCredential(this.vaultPath, this.masterKeyPath).then(value => value, () => { throw new Error("access.credential_store_unavailable"); }); }
  fingerprint() { this.lease.assertOwned(); return fingerprintSqlite(this.vaultPath); }
  async prepareMigrationSave(expected: ReturnType<typeof fingerprintSqlite>): Promise<PreparedAccessVaultWrite> {
    await verifyPrivateFile(this.vaultPath);
    const files = [this.vaultPath, `${this.vaultPath}-wal`, this.masterKeyPath].map(path => ({ path, stat: lstatSync(path, { bigint: true, throwIfNoEntry: false }) }));
    for (const { stat } of files) if (stat && (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1n)) throw new AccountChangedError();
    const key = await readVaultMasterKey(this.masterKeyPath);
    using releaseKey = { [Symbol.dispose]() { key.fill(0); } };
    if (!isDeepStrictEqual(this.fingerprint(), expected)) throw new AccountChangedError();
    for (const { path, stat } of files) if (!sameFileStat(lstatSync(path, { bigint: true, throwIfNoEntry: false }), stat)) throw new AccountChangedError();
    return new PreparedAccessVaultWrite(preparedWriteIssuer, this, files, createHash("sha256").update(key).digest("hex"));
  }
  async savePreparedMigration(proof: VerifiedMigrationCredential, prepared: PreparedAccessVaultWrite): Promise<void> {
    if (!(proof instanceof VerifiedMigrationCredential) || !(prepared instanceof PreparedAccessVaultWrite)) throw new Error("access.verified_credential_required");
    return this.#exclusive(() => this.#save(proof, prepared)).then(value => value, error => { if (error instanceof AccountChangedError) throw error; throw new Error("access.credential_store_unavailable"); });
  }
  #exclusive<T>(operation: () => Promise<T>): Promise<T> { const next = this.#writes.then(operation); this.#writes = next.then(() => undefined, () => undefined); return next; }
  async #open() {
    this.lease.assertOwned();
    await verifyPrivateFile(this.vaultPath);
    const identity = await lstat(this.vaultPath, { bigint: true });
    const key = await readVaultMasterKey(this.masterKeyPath);
    let ready = false;
    using release = { [Symbol.dispose]() { if (!ready) key.fill(0); } };
    const db = new DatabaseSync(this.vaultPath);
    using releaseDb = { [Symbol.dispose]() { if (!ready) db.close(); } };
    db.exec("PRAGMA busy_timeout=5000");
    if (validateVaultSchema(db) !== 4) throw new Error("access.vault_upgrade_required");
    const assertCurrent = async () => {
      this.lease.assertOwned();
      await verifyPrivateFile(this.vaultPath);
      const current = await lstat(this.vaultPath, { bigint: true });
      if (current.dev !== identity.dev || current.ino !== identity.ino || current.nlink !== 1n || current.isSymbolicLink()) throw new AccountChangedError();
      const currentKey = await readVaultMasterKey(this.masterKeyPath);
      using releaseKey = { [Symbol.dispose]() { currentKey.fill(0); } };
      if (!timingSafeEqual(key, currentKey)) throw new AccountChangedError();
    };
    await assertCurrent();
    ready = true;
    return { db, key, assertCurrent, [Symbol.dispose]() { key.fill(0); db.close(); } };
  }
  #row(db: DatabaseSync, key: Buffer) {
    const rows = db.prepare("SELECT id,label,metadata_json,encrypted_secret,disabled,cooldown_until,last_error,created_at,updated_at FROM service_credentials WHERE provider='creator_sync' ORDER BY id").all();
    if (!rows.length) return undefined;
    if (rows.length !== 1) throw new Error("access.credential_ambiguous");
    const row = rowSchema.safeParse(rows[0]);
    if (!row.success) throw new Error("access.credential_invalid");
    const metadata = accessCredentialMetadataSchema.safeParse(parseStoredJson(row.data.metadata_json, true));
    if (!metadata.success || (metadata.data.status === "active") !== (row.data.disabled === 0)) throw new Error("access.credential_invalid");
    const plaintext = decryptVaultEnvelope(key, row.data.encrypted_secret);
    using release = { [Symbol.dispose]() { plaintext.fill(0); } };
    const secret = z.object({ value: z.string() }).safeParse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(plaintext)));
    if (!secret.success) throw new Error("access.credential_invalid");
    validateAccessBearer(metadata.data.credential_id, secret.data.value);
    return { ...row.data, metadata: metadata.data };
  }
  async save(proof: VerifiedMigrationCredential, expected: ReturnType<typeof fingerprintSqlite>): Promise<void> {
    if (!(proof instanceof VerifiedMigrationCredential)) throw new Error("access.verified_credential_required");
    return this.#exclusive(() => this.#save(proof, expected)).then(value => value, error => { if (error instanceof AccountChangedError) throw error; throw new Error("access.credential_store_unavailable"); });
  }
  async refresh(proof: VerifiedMigrationCredential): Promise<void> {
    if (!(proof instanceof VerifiedMigrationCredential)) throw new Error("access.verified_credential_required");
    return this.#exclusive(() => this.#save(proof, this.fingerprint(), true)).then(value => value, error => { if (error instanceof AccountChangedError) throw error; throw new Error("access.credential_store_unavailable"); });
  }
  async #save(proof: VerifiedMigrationCredential, expected: ReturnType<typeof fingerprintSqlite> | PreparedAccessVaultWrite, refresh = false): Promise<void> {
    proof.assertFresh();
    using opened = await this.#open();
    const { db, key } = opened;
    db.exec("BEGIN IMMEDIATE");
    let committed = false;
    using transaction = { [Symbol.dispose]() { if (!committed) db.exec("ROLLBACK"); } };
    if (expected instanceof PreparedAccessVaultWrite) expected.consume(this);
    else if (!isDeepStrictEqual(this.fingerprint(), expected)) throw new AccountChangedError();
    const prior = this.#row(db, key);
    if (refresh && (!prior || createHash("sha256").update(prior.id.toLowerCase()).update("\0").update(Buffer.from(prior.metadata_json)).update(prior.encrypted_secret).digest("hex") !== proof.sourceFingerprint)) throw new AccountChangedError();
    const metadata = accessCredentialMetadataSchema.parse(refresh ? { ...proof.metadata,
      accepted_bundle: { ...proof.metadata.accepted_bundle, format_version: prior!.metadata.accepted_bundle.format_version } } : proof.metadata);
    if (metadata.status !== "active") throw new Error("access.credential_invalid");
    const plaintext = Buffer.from(JSON.stringify({ value: proof.exposeBearer() }));
    using release = { [Symbol.dispose]() { plaintext.fill(0); } };
    const nonce = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, nonce);
    const encrypted = Buffer.concat([nonce, cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]);
    const id = prior?.id ?? randomUUID(), created = prior?.created_at ?? now().toString(), updated = prior ? revision(prior.updated_at) : created;
    db.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,disabled,cooldown_until,last_used_at,last_error,created_at,updated_at) VALUES(?,'creator_sync','creator-sync',?,?,0,NULL,NULL,NULL,?,?) ON CONFLICT(id) DO UPDATE SET label=excluded.label,metadata_json=excluded.metadata_json,encrypted_secret=excluded.encrypted_secret,disabled=0,cooldown_until=NULL,last_error=NULL,updated_at=excluded.updated_at")
      .run(id, JSON.stringify(metadata), encrypted, created, updated);
    const stored = this.#row(db, key);
    if (!stored || stored.id !== id || !isDeepStrictEqual(stored.metadata, metadata) || !Buffer.from(stored.encrypted_secret).equals(encrypted)) throw new Error("access.credential_readback_failed");
    await opened.assertCurrent(); proof.assertFresh(); this.lease.assertOwned();
    db.exec("COMMIT"); committed = true;
  }
  clear(expectedCredentialFingerprint?: string): Promise<void> { return this.#exclusive(() => this.#clear(expectedCredentialFingerprint)).then(value => value, error => { if (error instanceof AccountChangedError) throw error; throw new Error("access.credential_store_unavailable"); }); }
  async #clear(expectedCredentialFingerprint?: string): Promise<void> {
    using opened = await this.#open();
    const { db, key } = opened;
    db.exec("BEGIN IMMEDIATE");
    let committed = false;
    using transaction = { [Symbol.dispose]() { if (!committed) db.exec("ROLLBACK"); } };
    const row = this.#row(db, key);
    if (!row) { db.exec("COMMIT"); committed = true; return; }
    if (expectedCredentialFingerprint !== undefined && createHash("sha256").update(row.id.toLowerCase()).update("\0").update(Buffer.from(row.metadata_json)).update(row.encrypted_secret).digest("hex") !== expectedCredentialFingerprint) throw new AccountChangedError();
    const metadata = { ...row.metadata, status: "invalidated" as const,
      updated_at: row.metadata.status === "invalidated" ? row.metadata.updated_at : Math.max(row.metadata.updated_at + 1, Number(now())) };
    accessCredentialMetadataSchema.parse(metadata);
    if (row.metadata.status !== "invalidated") db.prepare("UPDATE service_credentials SET metadata_json=?,disabled=1,cooldown_until=NULL,last_error=NULL,updated_at=? WHERE id=? AND provider='creator_sync' AND label='creator-sync'")
      .run(JSON.stringify(metadata), String(metadata.updated_at), row.id);
    const invalidated = this.#row(db, key);
    if (!invalidated || invalidated.id !== row.id || !isDeepStrictEqual(invalidated.metadata, metadata) || invalidated.disabled !== 1 || !Buffer.from(invalidated.encrypted_secret).equals(Buffer.from(row.encrypted_secret))) throw new Error("access.credential_invalidation_failed");
    await opened.assertCurrent(); this.lease.assertOwned(); db.exec("COMMIT"); committed = true;
    // 原实现先提交失效墓碑，再独立提交删除；删除失败时仍保持不可用。
    db.exec("BEGIN IMMEDIATE"); committed = false;
    if (db.prepare("DELETE FROM service_credentials WHERE id=? AND provider='creator_sync' AND label='creator-sync' AND disabled=1").run(row.id).changes !== 1 || db.prepare("SELECT 1 FROM service_credentials WHERE provider='creator_sync'").get()) throw new Error("access.credential_deletion_failed");
    await opened.assertCurrent(); this.lease.assertOwned(); db.exec("COMMIT"); committed = true;
  }
}
