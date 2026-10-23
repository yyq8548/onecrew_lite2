import { createHash } from "node:crypto";
import { lstat } from "node:fs/promises";
import { lstatSync } from "node:fs";
import { resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { readVaultMasterKey, decryptVaultEnvelope } from "../../storage/vault-reader.ts";
import { validateVaultSchema } from "../../storage/vault-schema.ts";
import { verifyPrivateFile } from "../../storage/private-backup.ts";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { youtubeHealthOutcome, type YoutubeHealthFailure } from "./youtube-health.ts";

const string = z.string().refine(value => value.isWellFormed());
const profileSchema = z.object({ id: z.uuid(), platform: z.enum(["x", "instagram", "telegram"]), provider_account_id: string.refine(value => !!value.trim()), username: string, created_at: string, updated_at: string }).strict();
const cookieSchema = z.object({ name: string, value: string, domain: string, path: string, secure: z.boolean(), http_only: z.boolean() }).strict();
const secretSchema = z.object({ provider_account_id: string.refine(value => !!value.trim()), cookies: z.array(cookieSchema), x_chat_passcode: string.nullable(), instagram_session_settings: string.nullable(), telegram_api_id: z.number().int().nullable(), telegram_api_hash: string.nullable(), telegram_session: string.nullable(), profile_name: string.default(""), browser_profile_dir: string.default(""), fingerprint_seed: string.default(""), proxy: z.unknown().optional(), proxy_egress: string.nullable().optional() }).strict();
export type PlatformProfileSecret = z.infer<typeof secretSchema>;
export const platformProfileSecretSchema = secretSchema;
export type PlatformVaultProfile = Readonly<z.infer<typeof profileSchema> & { revision: string; browserProfileDirectory: string }>;
export interface YoutubeVaultCredential { readonly id: string; readonly revision: string; readonly healthRevision: string; readonly disabled: boolean; readonly cooldownUntil: number | null }

// 路径只来自受信启动配置；读取本身不证明该全局 Vault 记录属于当前 OneCrew 账号。
export class PlatformVaultReader {
  readonly #vaultPath: string;
  readonly #keyPath: string;
  #observation = 0n;
  readonly #youtubeHealth = new Map<string, { material: string; revision: string; observation: bigint }>();
  constructor(vaultPath: string, masterKeyPath: string) { this.#vaultPath = resolve(vaultPath); this.#keyPath = resolve(masterKeyPath); }
  storagePaths(): Readonly<{ vaultPath: string; masterKeyPath: string }> { return Object.freeze({ vaultPath: this.#vaultPath, masterKeyPath: this.#keyPath }); }
  async withProfile<T>(profileId: string, consume: (profile: PlatformVaultProfile, secret: PlatformProfileSecret) => Promise<T>): Promise<T> {
    z.uuid().parse(profileId);
    await verifyPrivateFile(this.#vaultPath);
    const before = await lstat(this.#vaultPath, { bigint: true });
    if (before.nlink !== 1n) throw new Error("Platform Vault must have exactly one hard link");
    const key = await readVaultMasterKey(this.#keyPath);
    let clear: Buffer | undefined;
    try {
      const database = new DatabaseSync(this.#vaultPath, { readOnly: true });
      let profile: z.infer<typeof profileSchema>; let revision: string;
      try {
        database.exec("PRAGMA query_only=ON; PRAGMA busy_timeout=5000; BEGIN");
        if (validateVaultSchema(database) !== 4) throw new Error("Platform Vault requires schema version 4");
        const row = database.prepare("SELECT id,platform,provider_account_id,username,created_at,updated_at,encrypted_secret FROM profiles WHERE id=?").get(profileId);
        if (!row || !(row.encrypted_secret instanceof Uint8Array)) throw new Error("Platform Vault profile is unavailable");
        const { encrypted_secret, ...metadata } = row;
        profile = profileSchema.parse(metadata);
        revision = createHash("sha256").update(encrypted_secret).digest("hex");
        clear = decryptVaultEnvelope(key, encrypted_secret);
      } finally { database.close(); }
      const secret = secretSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(clear), true));
      if (profile.provider_account_id !== secret.provider_account_id) throw new Error("Platform Vault stable provider identity does not match");
      await verifyPrivateFile(this.#vaultPath);
      const after = await lstat(this.#vaultPath, { bigint: true });
      if (after.dev !== before.dev || after.ino !== before.ino || after.nlink !== 1n) throw new Error("Platform Vault file identity changed");
      return await consume(Object.freeze({ ...profile, revision, browserProfileDirectory: secret.browser_profile_dir }), secret);
    } finally { key.fill(0); clear?.fill(0); }
  }
  async profile(profileId: string): Promise<PlatformVaultProfile> { return this.withProfile(profileId, async profile => profile); }
  async youtubeMetadata(credentialId: string) {
    return this.withYoutubeCredential(credentialId, async credential => {
      using database = new DatabaseSync(this.#vaultPath, { readOnly: true });
      const value = z.strictObject({ label: string, metadata_json: string, last_error: string.nullable(), last_used_at: z.number().int().safe().nullable(), created_at: string, updated_at: string, encrypted_secret: z.instanceof(Uint8Array) }).parse(database.prepare("SELECT label,metadata_json,last_error,last_used_at,created_at,updated_at,encrypted_secret FROM service_credentials WHERE id=? AND provider='youtube'").get(credentialId));
      if (createHash("sha256").update(value.encrypted_secret).digest("hex") !== credential.revision) throw new Error("YouTube credential changed while reading metadata");
      return Object.freeze({ label: value.label, metadata: parseStoredJson(value.metadata_json, true), lastError: value.last_error, lastUsedAt: value.last_used_at, createdAt: value.created_at, updatedAt: value.updated_at });
    });
  }
  async withYoutubeCredential<T>(credentialId: string, consume: (credential: YoutubeVaultCredential, apiKey: string) => Promise<T>): Promise<T> {
    z.uuid().parse(credentialId); await verifyPrivateFile(this.#vaultPath);
    const before = await lstat(this.#vaultPath, { bigint: true }); if (before.nlink !== 1n) throw new Error("Platform Vault must have exactly one hard link");
    const key = await readVaultMasterKey(this.#keyPath); let clear: Buffer | undefined;
    try {
      const database = new DatabaseSync(this.#vaultPath, { readOnly: true });
      let credential: YoutubeVaultCredential;
      try {
        database.exec("PRAGMA query_only=ON; PRAGMA busy_timeout=5000; BEGIN");
        if (validateVaultSchema(database) !== 4) throw new Error("Platform Vault requires schema version 4");
        const row = database.prepare("SELECT id,provider,updated_at,disabled,cooldown_until,encrypted_secret FROM service_credentials WHERE id=?").get(credentialId);
        const parsed = z.object({ id: z.literal(credentialId), provider: z.literal("youtube"), updated_at: z.string().regex(/^\d+$/u), disabled: z.union([z.literal(0), z.literal(1)]), cooldown_until: z.number().int().safe().nullable(), encrypted_secret: z.instanceof(Uint8Array) }).safeParse(row);
        if (!parsed.success) throw new Error("YouTube credential is unavailable");
        credential = Object.freeze({ id: credentialId, revision: createHash("sha256").update(parsed.data.encrypted_secret).digest("hex"), healthRevision: parsed.data.updated_at, disabled: parsed.data.disabled === 1, cooldownUntil: parsed.data.cooldown_until });
        clear = decryptVaultEnvelope(key, parsed.data.encrypted_secret);
      } finally { database.close(); }
      const secret = z.strictObject({ value: string.refine(value => !!value.trim() && !/\p{Cc}/u.test(value)) }).parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(clear), true));
      await verifyPrivateFile(this.#vaultPath); const after = await lstat(this.#vaultPath, { bigint: true });
      if (after.dev !== before.dev || after.ino !== before.ino || after.nlink !== 1n) throw new Error("Platform Vault file identity changed");
      return await consume(credential, secret.value);
    } finally { key.fill(0); clear?.fill(0); }
  }
  nextYoutubeObservation(): bigint { return ++this.#observation; }
  async recordYoutubeHealth(credential: YoutubeVaultCredential, observation: bigint, failure: YoutubeHealthFailure | null): Promise<"applied" | "superseded"> {
    z.uuid().parse(credential.id); if (observation <= 0n || observation > this.#observation) throw new Error("YouTube health observation is invalid");
    await verifyPrivateFile(this.#vaultPath); const before = await lstat(this.#vaultPath, { bigint: true });
    if (before.nlink !== 1n) throw new Error("Platform Vault must have exactly one hard link");
    const database = new DatabaseSync(this.#vaultPath);
    try {
      database.exec("PRAGMA busy_timeout=5000; BEGIN IMMEDIATE");
      if (validateVaultSchema(database) !== 4) throw new Error("Platform Vault requires schema version 4");
      const row = database.prepare("SELECT updated_at,encrypted_secret FROM service_credentials WHERE id=? AND provider='youtube'").get(credential.id);
      if (!row) return "superseded";
      if (typeof row.updated_at !== "string" || !/^\d+$/u.test(row.updated_at) || !(row.encrypted_secret instanceof Uint8Array)) throw new Error("YouTube health metadata is invalid");
      const material = createHash("sha256").update(row.encrypted_secret).digest("hex"); const state = this.#youtubeHealth.get(credential.id);
      if (material !== credential.revision || state?.material === material && observation <= state.observation || row.updated_at !== credential.healthRevision && !(state?.material === material && state.revision === row.updated_at)) return "superseded";
      const now = Math.floor(Date.now() / 1000), current = BigInt(row.updated_at);
      if (current >= 18446744073709551615n) throw new Error("YouTube credential revision is exhausted");
      const revision = (current + 1n > BigInt(now) ? current + 1n : BigInt(now)).toString();
      const changed = failure === null
        ? database.prepare("UPDATE service_credentials SET cooldown_until=CASE WHEN cooldown_until IS NOT NULL AND cooldown_until>?2 THEN cooldown_until ELSE NULL END,last_error=CASE WHEN disabled OR (cooldown_until IS NOT NULL AND cooldown_until>?2) THEN last_error ELSE NULL END,last_used_at=?2,updated_at=?3 WHERE id=?1 AND updated_at=?4").run(credential.id, now, revision, row.updated_at)
        : (() => { const outcome = youtubeHealthOutcome(failure, now); return database.prepare("UPDATE service_credentials SET disabled=?2,cooldown_until=?3,last_error=?4,updated_at=?5 WHERE id=?1 AND updated_at=?6").run(credential.id, Number(outcome.disabled), outcome.cooldownUntil, outcome.message, revision, row.updated_at); })();
      if (changed.changes !== 1) throw new Error("YouTube health update lost its revision");
      const after = lstatSync(this.#vaultPath, { bigint: true });
      if (after.dev !== before.dev || after.ino !== before.ino || after.nlink !== 1n) throw new Error("Platform Vault file identity changed");
      database.exec("COMMIT"); this.#youtubeHealth.set(credential.id, { material, revision, observation }); return "applied";
    } finally { database.close(); }
  }
}
