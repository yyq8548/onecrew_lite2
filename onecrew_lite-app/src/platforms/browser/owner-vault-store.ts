import { createCipheriv, createHash, randomBytes, randomUUID } from "node:crypto";
import { closeSync, constants, fsyncSync, lstatSync, openSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";
import { z } from "zod";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import type { AccountDatabases } from "../../identity/account-databases.ts";
import type { VerifiedMailIdentityProvider } from "../../mail/gmail-access.ts";
import type { InstanceLease } from "../../service/instance-lock.ts";
import { parseApplicationJson } from "../../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../../storage/private-backup.ts";
import { decryptVaultEnvelope, readVaultMasterKey } from "../../storage/vault-reader.ts";
import { validateVaultSchema } from "../../storage/vault-schema.ts";
import { readStorageSql } from "../../storage/sql-resource.ts";
import { PlatformVaultReader, platformProfileSecretSchema } from "./platform-vault.ts";
import { platformAccountManifestSchema, type OwnerPlatformManifest, type PlatformAccountManifest } from "./platform-manifest.ts";
import { proxyConfigSchema } from "./proxy.ts";

export interface OwnerPlatformReadScope { vault: DatabaseSync; creator: DatabaseSync; key: Buffer; manifest: PlatformAccountManifest }
export interface OwnerPlatformWriteScope extends OwnerPlatformReadScope { manifest: OwnerPlatformManifest }
export interface OwnerPlatformMetadataScope { creator: DatabaseSync; manifest: PlatformAccountManifest; profiles: Record<string, unknown>[]; services: Record<string, unknown>[] }
const sha = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
const journalSchema = z.strictObject({ format: z.literal(1), state: z.enum(["prepared", "completed", "aborted"]), oldManifest: z.string().nullable(), newManifest: z.string(), receiptKey: z.string().startsWith("platform-vault-store:"), changeset: z.string() });
function existing(path: string): Buffer | null { const stat = lstatSync(path, { throwIfNoEntry: false }); if (!stat) return null; if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) throw new Error("platform_store.invalid_file"); return readFileSync(path); }
function syncFile(path: string): void { const fd = openSync(path, constants.O_RDWR | (constants.O_NOFOLLOW ?? 0)); try { fsyncSync(fd); } finally { closeSync(fd); } }
function synchronous<T>(value: T): T { if (value !== null && typeof value === "object" && "then" in value) throw new Error("platform_store.synchronous_callback_required"); return value; }
export function encryptOwnerVaultSecret(key: Uint8Array, plaintext: Uint8Array): Buffer { if (key.length !== 32) throw new Error("platform_store.invalid_key"); const nonce = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, nonce); return Buffer.concat([nonce, cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]); }
function mapped(scope: OwnerPlatformReadScope, id: string, service: boolean) {
  const entry = service ? scope.manifest.services.find(value => value.accountRefId === id) : scope.manifest.profiles.find(value => value.accountRefId === id);
  if (!entry) throw new Error("platform_store.owner_mapping_required");
  const ref = scope.creator.prepare("SELECT platform,kind FROM account_refs WHERE id=?").get(id);
  const platform = "provider" in entry ? entry.provider : entry.platform;
  if (!ref || ref.platform !== platform || ref.kind !== (service ? "service_credential" : "profile")) throw new Error("platform_store.creator_mapping_mismatch");
  return entry;
}
export function assertOwnedService(scope: OwnerPlatformReadScope, id: string) { return mapped(scope, id, true) as PlatformAccountManifest["services"][number]; }
export function assertOwnedProfile(scope: OwnerPlatformReadScope, id: string) { return mapped(scope, id, false) as PlatformAccountManifest["profiles"][number]; }
function validateCreatorMappings(creator: DatabaseSync, manifest: PlatformAccountManifest): void {
  for (const entry of [...manifest.profiles, ...manifest.services]) {
    const ref = creator.prepare("SELECT platform,kind FROM account_refs WHERE id=?").get(entry.accountRefId);
    if (!ref || ref.platform !== ("provider" in entry ? entry.provider : entry.platform) || ref.kind !== ("provider" in entry ? "service_credential" : "profile")) throw new Error("platform_store.creator_mapping_mismatch");
  }
}

// 旧全局凭据只依据既有归属复制；每次写入发布独立加密代，保留旧代和全局 Access 数据。
export class OwnerPlatformVaultStore {
  readonly #lease: InstanceLease; readonly #accounts: AccountContextService; readonly #identities: VerifiedMailIdentityProvider; readonly #directories: AccountDatabases; readonly #global: PlatformVaultReader;
  readonly #pending = new Map<string, Promise<unknown>>();
  constructor(lease: InstanceLease, accounts: AccountContextService, identities: VerifiedMailIdentityProvider, directories: AccountDatabases, globalVault: PlatformVaultReader) { this.#lease = lease; this.#accounts = accounts; this.#identities = identities; this.#directories = directories; this.#global = globalVault; }
  async #serialized<T>(context: AccountContext, operation: () => Promise<T>): Promise<T> { const id = context.principal.user.id, previous = this.#pending.get(id); const result = (async () => { if (previous) await previous.then(() => undefined, () => undefined); return operation(); })(); this.#pending.set(id, result); try { return await result; } finally { if (this.#pending.get(id) === result) this.#pending.delete(id); } }
  async #prepare(context: AccountContext) {
    this.#lease.assertOwned(); this.#accounts.assertCurrent(context);
    const directory = await this.#directories.accountDirectory(context), connection = await this.#directories.platformStore(context), identity = await this.#identities.snapshot(directory.owner);
    if (identity.accessUserId !== context.principal.user.id || ["accountId", "tenantId", "baseId"].some(field => identity.owner[field as keyof typeof identity.owner] !== directory.owner[field as keyof typeof directory.owner])) throw new Error("platform_store.identity_mismatch");
    const manifestPath = join(directory.directory, "platform-accounts.json");
    await OwnerPlatformVaultStore.recoverStorage(directory.directory, connection.database);
    const initial = existing(manifestPath); if (initial) await verifyPrivateFile(manifestPath);
    const manifest: PlatformAccountManifest = initial ? platformAccountManifestSchema.parse(parseApplicationJson(initial)) : { format: 1, owner: { ...directory.owner }, deviceId: context.principal.deviceId, profiles: [], services: [] };
    if (manifest.owner.accountId !== directory.owner.accountId || manifest.owner.tenantId !== directory.owner.tenantId || manifest.owner.baseId !== directory.owner.baseId || manifest.deviceId !== context.principal.deviceId) throw new Error("platform_store.owner_mismatch");
    const assertCurrent = async () => { this.#lease.assertOwned(); connection.assertOwned(); directory.assertOwned(); this.#accounts.assertCurrent(context); await this.#identities.assertCurrent(identity); if (!existing(manifestPath)?.equals(initial ?? Buffer.alloc(0)) && !(initial === null && existing(manifestPath) === null)) throw new Error("platform_store.manifest_changed"); };
    await assertCurrent(); return { directory: directory.directory, creator: connection.database, manifestPath, manifest, initial, assertCurrent };
  }
  static async recoverStorage(directory: string, creator: DatabaseSync) {
    await verifyPrivateDirectory(directory); const manifestPath = join(directory, "platform-accounts.json");
    const journalDirectory = join(directory, "platform-vault-journals"); if (!lstatSync(journalDirectory, { throwIfNoEntry: false })) return;
    await verifyPrivateDirectory(journalDirectory);
    for (const name of await readdir(journalDirectory)) {
      if (!/^[-a-f0-9]{36}\.json$/u.test(name)) throw new Error("platform_store.invalid_journal_name");
      const path = join(journalDirectory, name); await verifyPrivateFile(path); const journal = journalSchema.parse(parseApplicationJson(await readFile(path)));
      if (journal.state !== "prepared") continue;
      const current = existing(manifestPath)?.toString("base64") ?? null, receipt = creator.prepare("SELECT action,state,request_fingerprint FROM idempotency_records WHERE key=?").get(journal.receiptKey), expected = sha(Buffer.from(journal.newManifest, "base64"));
      if (current === journal.oldManifest && !receipt) journal.state = "aborted";
      else if (current === journal.newManifest) {
        const manifest = platformAccountManifestSchema.parse(parseApplicationJson(Buffer.from(journal.newManifest, "base64")));
        if (manifest.format !== 2) throw new Error("platform_store.recovery_owner_vault_required");
        const vaultRoot = await verifyPrivateDirectory(join(directory, manifest.storage.directory)), vaultPath = join(vaultRoot, "vault.sqlite"); await verifyPrivateFile(vaultPath);
        const key = await readVaultMasterKey(join(vaultRoot, "masterkey")), vault = new DatabaseSync(vaultPath, { readOnly: true });
        try {
          if (validateVaultSchema(vault) !== 4) throw new Error("platform_store.schema4_required");
          for (const [table, entries] of [["profiles", manifest.profiles], ["service_credentials", manifest.services]] as const) for (const entry of entries) {
            const row = vault.prepare(`SELECT encrypted_secret FROM ${table} WHERE id=?`).get("profileId" in entry ? entry.profileId : entry.credentialId);
            if (!(row?.encrypted_secret instanceof Uint8Array) || sha(row.encrypted_secret) !== entry.vaultRevision) throw new Error("platform_store.recovery_vault_changed");
            const clear = decryptVaultEnvelope(key, row.encrypted_secret); try { parseApplicationJson(clear); } finally { clear.fill(0); }
          }
        } finally { key.fill(0); vault.close(); }
        if (!receipt) { creator.exec("BEGIN IMMEDIATE"); try { if (!creator.applyChangeset(Buffer.from(journal.changeset, "base64"))) throw new Error("platform_store.changeset_conflict"); validateCreatorMappings(creator, manifest); creator.exec("COMMIT"); } catch (error) { creator.exec("ROLLBACK"); throw error; } }
        validateCreatorMappings(creator, manifest);
        const actual = creator.prepare("SELECT action,state,request_fingerprint FROM idempotency_records WHERE key=?").get(journal.receiptKey);
        if (!actual || actual.action !== "platform_vault_commit" || actual.state !== "completed" || actual.request_fingerprint !== expected) throw new Error("platform_store.receipt_mismatch");
        journal.state = "completed";
      } else throw new Error("platform_store.recovery_conflict");
      writeFileSync(path, JSON.stringify(journal)); syncFile(path);
    }
  }
  async #copy(manifest: PlatformAccountManifest, directory: string, target: DatabaseSync, targetKey: Buffer) {
    const paths = manifest.format === 1 ? this.#global.storagePaths() : { vaultPath: join(directory, manifest.storage.directory, "vault.sqlite"), masterKeyPath: join(directory, manifest.storage.directory, "masterkey") };
    if (manifest.profiles.length + manifest.services.length === 0) return () => undefined;
    await verifyPrivateFile(paths.vaultPath); const vaultIdentity = lstatSync(paths.vaultPath, { bigint: true }), keyIdentity = lstatSync(paths.masterKeyPath, { bigint: true }), key = await readVaultMasterKey(paths.masterKeyPath), source = new DatabaseSync(paths.vaultPath, { readOnly: true });
    const records: { table: string; id: string; bytes: string }[] = [];
    try {
      if (validateVaultSchema(source) !== 4) throw new Error("platform_store.schema4_required");
      for (const [table, entries] of [["profiles", manifest.profiles], ["service_credentials", manifest.services]] as const) for (const entry of entries) {
        const id = "profileId" in entry ? entry.profileId : entry.credentialId, statement = source.prepare(`SELECT * FROM ${table} WHERE id=?`); statement.setReadBigInts(true); const row = statement.get(id);
        if (!row || !(row.encrypted_secret instanceof Uint8Array) || sha(row.encrypted_secret) !== entry.vaultRevision || ("profileId" in entry && (row.platform !== entry.platform || row.provider_account_id !== entry.providerAccountId)) || ("credentialId" in entry && row.provider !== "youtube")) throw new Error("platform_store.source_mapping_changed");
        const clear = decryptVaultEnvelope(key, row.encrypted_secret);
        try { const value = parseApplicationJson(clear); if ("profileId" in entry && (value === null || typeof value !== "object" || !("provider_account_id" in value) || value.provider_account_id !== entry.providerAccountId)) throw new Error("platform_store.secret_identity_mismatch"); records.push({ table, id, bytes: JSON.stringify(row, (_, item) => typeof item === "bigint" ? `${item}n` : item) }); const copied: Record<string, SQLInputValue> = { ...row, encrypted_secret: encryptOwnerVaultSecret(targetKey, clear) }; const columns = Object.keys(copied); target.prepare(`INSERT INTO ${table} (${columns.join(",")}) VALUES (${columns.map(() => "?").join(",")})`).run(...columns.map(column => copied[column]!)); }
        finally { clear.fill(0); }
      }
    } finally { key.fill(0); source.close(); }
    return () => {
      const actualVault = lstatSync(paths.vaultPath, { bigint: true }), actualKey = lstatSync(paths.masterKeyPath, { bigint: true });
      if (!actualVault.isFile() || actualVault.isSymbolicLink() || actualVault.nlink !== 1n || actualVault.dev !== vaultIdentity.dev || actualVault.ino !== vaultIdentity.ino || !actualKey.isFile() || actualKey.isSymbolicLink() || actualKey.nlink !== 1n || actualKey.dev !== keyIdentity.dev || actualKey.ino !== keyIdentity.ino || actualKey.size !== keyIdentity.size || actualKey.mtimeNs !== keyIdentity.mtimeNs || actualKey.ctimeNs !== keyIdentity.ctimeNs) throw new Error("platform_store.source_identity_changed");
      const check = new DatabaseSync(paths.vaultPath, { readOnly: true }); try { for (const record of records) { const query = check.prepare(`SELECT * FROM ${record.table} WHERE id=?`); query.setReadBigInts(true); if (JSON.stringify(query.get(record.id), (_, item) => typeof item === "bigint" ? `${item}n` : item) !== record.bytes) throw new Error("platform_store.source_changed"); } } finally { check.close(); }
    };
  }
  // 恢复待办属于业务库；仅在实际执行恢复时读取和校验平台凭据。
  async readCreator<T>(context: AccountContext, callback: (creator: DatabaseSync) => T): Promise<T> {
    return this.#serialized(context, async () => {
      this.#lease.assertOwned(); this.#accounts.assertCurrent(context);
      const connection = await this.#directories.platformStore(context), directory = await this.#directories.accountDirectory(context);
      await OwnerPlatformVaultStore.recoverStorage(directory.directory, connection.database);
      connection.assertOwned(); directory.assertOwned(); this.#accounts.assertCurrent(context);
      const value = synchronous(callback(connection.database));
      connection.assertOwned(); directory.assertOwned(); this.#accounts.assertCurrent(context);
      return value;
    });
  }
  async read<T>(context: AccountContext, callback: (scope: OwnerPlatformReadScope) => T): Promise<T> { return this.#serialized(context, async () => { const prepared = await this.#prepare(context), key = randomBytes(32), vault = new DatabaseSync(":memory:"); try { vault.exec(readStorageSql("vault-schema.sql") + ";PRAGMA user_version=4"); const check = await this.#copy(prepared.manifest, prepared.directory, vault, key); await prepared.assertCurrent(); check(); const value = synchronous(callback({ vault, key, creator: prepared.creator, manifest: prepared.manifest })); await prepared.assertCurrent(); return value; } finally { key.fill(0); vault.close(); } }); }
  async inspect<T>(context: AccountContext, callback: (scope: OwnerPlatformMetadataScope) => T): Promise<T> { return this.#serialized(context, async () => {
    const prepared = await this.#prepare(context), manifest = prepared.manifest, scope: OwnerPlatformMetadataScope = { creator: prepared.creator, manifest, profiles: [], services: [] };
    if (manifest.profiles.length + manifest.services.length > 0) {
      const paths = manifest.format === 1 ? this.#global.storagePaths() : { vaultPath: join(prepared.directory, manifest.storage.directory, "vault.sqlite"), masterKeyPath: join(prepared.directory, manifest.storage.directory, "masterkey") };
      await verifyPrivateFile(paths.vaultPath); const before = lstatSync(paths.vaultPath, { bigint: true }), key = await readVaultMasterKey(paths.masterKeyPath), database = new DatabaseSync(paths.vaultPath, { readOnly: true });
      try {
        database.exec("PRAGMA query_only=ON; BEGIN"); if (validateVaultSchema(database) !== 4) throw new Error("platform_store.schema4_required"); validateCreatorMappings(prepared.creator, manifest);
        for (const [table, entries, output] of [["profiles", manifest.profiles, scope.profiles], ["service_credentials", manifest.services, scope.services]] as const) for (const entry of entries) {
          const statement = database.prepare(`SELECT * FROM ${table} WHERE id=?`); statement.setReadBigInts(true); const row = statement.get("profileId" in entry ? entry.profileId : entry.credentialId);
          if (!row || !(row.encrypted_secret instanceof Uint8Array) || sha(row.encrypted_secret) !== entry.vaultRevision || ("profileId" in entry && (row.platform !== entry.platform || row.provider_account_id !== entry.providerAccountId)) || ("credentialId" in entry && row.provider !== "youtube")) throw new Error("platform_store.source_mapping_changed");
          const { encrypted_secret, ...metadata } = row; let readable = false, providerId: string | undefined, profileName: string | null = null, fingerprintSeed: string | null = null, proxyEndpoint: string | null = null, proxyKind: string | null = null, proxyEgress: string | null = null, clear: Buffer | undefined;
          try { clear = decryptVaultEnvelope(key, encrypted_secret); const decoded = parseApplicationJson(clear); if ("profileId" in entry) { const parsed = platformProfileSecretSchema.safeParse(decoded); readable = parsed.success; if (parsed.success) { providerId = parsed.data.provider_account_id; profileName = parsed.data.profile_name; fingerprintSeed = parsed.data.fingerprint_seed; proxyEgress = parsed.data.proxy_egress ?? null; if (parsed.data.proxy != null) { const proxy = proxyConfigSchema.parse(parsed.data.proxy); proxyEndpoint = `${proxy.host}:${proxy.port}`; proxyKind = proxy.kind; } } } else readable = z.strictObject({ value: z.string().trim().min(1).refine(value => !/\p{Cc}/u.test(value)) }).safeParse(decoded).success; }
          catch { readable = false; profileName = null; fingerprintSeed = null; proxyEndpoint = null; proxyKind = null; proxyEgress = null; }
          finally { clear?.fill(0); }
          if (readable && "profileId" in entry && providerId !== entry.providerAccountId) throw new Error("platform_store.secret_identity_mismatch");
          output.push({ ...metadata, accountRefId: entry.accountRefId, credentialReadable: readable, ...("profileId" in entry ? { profileName, fingerprintSeed, proxyEndpoint, proxyKind, proxyEgress } : {}) });
        }
        const after = lstatSync(paths.vaultPath, { bigint: true }); if (after.dev !== before.dev || after.ino !== before.ino || after.nlink !== 1n || after.isSymbolicLink()) throw new Error("platform_store.source_changed");
      } finally { key.fill(0); database.close(); }
    }
    await prepared.assertCurrent(); return synchronous(callback(scope));
  }); }
  async mutate<T>(context: AccountContext, callback: (scope: OwnerPlatformWriteScope) => T): Promise<T> { return this.#serialized(context, async () => {
    const prepared = await this.#prepare(context), generation = `platform-vault-${randomUUID()}`, root = await createPrivateDirectory(join(prepared.directory, generation)), vaultPath = join(root, "vault.sqlite"), keyPath = join(root, "masterkey"), key = randomBytes(32);
    writeFileSync(keyPath, key, { flag: "wx", mode: 0o600 }); await verifyPrivateFile(keyPath); syncFile(keyPath);
    writeFileSync(vaultPath, "", { flag: "wx", mode: 0o600 });
    const vault = new DatabaseSync(vaultPath); let vaultOpen = true, creatorTransaction = false, published = false;
    const manifest: OwnerPlatformManifest = { ...structuredClone(prepared.manifest), format: 2, storage: { kind: "account_vault", directory: generation } };
    try {
      vault.exec(readStorageSql("vault-schema.sql") + ";PRAGMA user_version=4"); await verifyPrivateFile(vaultPath);
      const checkSource = await this.#copy(prepared.manifest, prepared.directory, vault, key), journals = join(prepared.directory, "platform-vault-journals");
      if (!lstatSync(journals, { throwIfNoEntry: false })) await createPrivateDirectory(journals); else await verifyPrivateDirectory(journals);
      const id = randomUUID(), journalPath = join(journals, `${id}.json`), temporary = join(prepared.directory, `.platform-accounts-${id}.json`), receiptKey = `platform-vault-store:${id}`;
      writeFileSync(journalPath, JSON.stringify({ format: 1, state: "aborted", oldManifest: prepared.initial?.toString("base64") ?? null, newManifest: "", receiptKey, changeset: "" }), { flag: "wx", mode: 0o600 }); syncFile(journalPath); writeFileSync(temporary, "", { flag: "wx", mode: 0o600 }); await verifyPrivateFile(journalPath); await verifyPrivateFile(temporary); await prepared.assertCurrent(); checkSource();
      const session = prepared.creator.createSession();
      try {
        prepared.creator.exec("BEGIN IMMEDIATE"); creatorTransaction = true; vault.exec("BEGIN IMMEDIATE");
        const scope = { vault, key, creator: prepared.creator, manifest }, result = synchronous(callback(scope));
        for (const entry of manifest.profiles) { assertOwnedProfile(scope, entry.accountRefId); const row = vault.prepare("SELECT platform,provider_account_id,encrypted_secret FROM profiles WHERE id=?").get(entry.profileId); if (!row || row.platform !== entry.platform || row.provider_account_id !== entry.providerAccountId || !(row.encrypted_secret instanceof Uint8Array)) throw new Error("platform_store.profile_mismatch"); entry.vaultRevision = sha(row.encrypted_secret); const clear = decryptVaultEnvelope(key, row.encrypted_secret); try { const value = parseApplicationJson(clear); if (!value || typeof value !== "object" || !("provider_account_id" in value) || value.provider_account_id !== entry.providerAccountId) throw new Error("platform_store.secret_identity_mismatch"); } finally { clear.fill(0); } }
        for (const entry of manifest.services) { assertOwnedService(scope, entry.accountRefId); const row = vault.prepare("SELECT provider,encrypted_secret FROM service_credentials WHERE id=?").get(entry.credentialId); if (!row || row.provider !== "youtube" || !(row.encrypted_secret instanceof Uint8Array)) throw new Error("platform_store.service_mismatch"); entry.vaultRevision = sha(row.encrypted_secret); const clear = decryptVaultEnvelope(key, row.encrypted_secret); try { const value = z.strictObject({ value: z.string().min(1) }).parse(parseApplicationJson(clear)); if (!value.value.trim()) throw new Error("platform_store.empty_service_key"); } finally { clear.fill(0); } }
        const count = vault.prepare("SELECT (SELECT count(*) FROM profiles)+(SELECT count(*) FROM service_credentials) AS n").get()?.n;
        if (count !== manifest.profiles.length + manifest.services.length) throw new Error("platform_store.unmapped_records");
        const bytes = Buffer.from(JSON.stringify(platformAccountManifestSchema.parse(manifest))), fingerprint = sha(bytes), now = Math.floor(Date.now() / 1000);
        vault.exec("COMMIT"); vault.close(); vaultOpen = false; syncFile(vaultPath);
        prepared.creator.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,result_json,created_at,updated_at) VALUES (?,'platform_vault_commit',?,'completed',?,?,?)").run(receiptKey, fingerprint, JSON.stringify({ format: 1, manifest_sha256: fingerprint }), now, now);
        const journal = { format: 1 as const, state: "prepared" as "prepared" | "completed", oldManifest: prepared.initial?.toString("base64") ?? null, newManifest: bytes.toString("base64"), receiptKey, changeset: Buffer.from(session.changeset()).toString("base64") };
        writeFileSync(journalPath, JSON.stringify(journal)); syncFile(journalPath); writeFileSync(temporary, bytes); syncFile(temporary); renameSync(temporary, prepared.manifestPath); published = true;
        prepared.creator.exec("COMMIT"); creatorTransaction = false; journal.state = "completed"; writeFileSync(journalPath, JSON.stringify(journal)); syncFile(journalPath); return result;
      } finally { session.close(); }
    } catch (error) { if (creatorTransaction) prepared.creator.exec("ROLLBACK"); if (published) throw new Error("platform_store.publication_recovery_required", { cause: error }); throw error; }
    finally { key.fill(0); if (vaultOpen) vault.close(); }
  }); }
}
