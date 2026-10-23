import { createHash } from "node:crypto";
import { lstat, readFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import type { VerifiedMailIdentityProvider } from "../../mail/gmail-access.ts";
import { verifyPrivateFile } from "../../storage/private-backup.ts";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import type { BrowserAccountDirectoryResolver } from "./binding.ts";
import { PlatformVaultReader } from "./platform-vault.ts";
import { YoutubeApiReadError } from "./youtube-health.ts";
import type { PlatformProfileSecret, PlatformVaultProfile, YoutubeVaultCredential } from "./platform-vault.ts";
import { proxyConfigSchema } from "./proxy.ts";
import { YoutubeRequestBudget, type YoutubeRequest } from "./youtube-request-budget.ts";
import { platformAccountManifestSchema, type PlatformAccountManifest } from "./platform-manifest.ts";
import { verifyPrivateDirectory } from "../../storage/private-backup.ts";
import type { OwnerPlatformVaultStore } from "./owner-vault-store.ts";
export { platformAccountManifestSchema } from "./platform-manifest.ts";

type Platform = "x" | "instagram" | "telegram";
function emptyInventory() { return { x: [] as { operator_account_id: string; username: string; label: string; availability: "available" | "unavailable" }[], instagram: [] as { operator_account_id: string; username: string; label: string; availability: "available" | "unavailable" }[], telegram: [] as { operator_account_id: string; username: string; label: string; availability: "available" | "unavailable" }[], youtube: [] as { credential_id: string; label: string; availability: "available" | "unavailable" }[] }; }
export interface OwnedPlatformAccountResolver {
  withProfile<T>(context: AccountContext, accountRefId: string, platform: Platform, consume: (profile: PlatformVaultProfile, secret: PlatformProfileSecret, assertCurrent: () => Promise<void>) => Promise<T>): Promise<T>;
  withYoutubeCredential<T>(context: AccountContext, accountRefId: string, consume: (credential: YoutubeVaultCredential, apiKey: string, assertCurrent: () => Promise<void>, request: YoutubeRequest) => Promise<T>): Promise<T>;
}
// 此文件由 M05 在选定已验证账号并完成导入时签发；运行时只读，旧全局 Vault 不会自动认领。
export class OwnedPlatformVaultResolver implements OwnedPlatformAccountResolver {
  readonly #accounts: AccountContextService;
  readonly #identities: VerifiedMailIdentityProvider;
  readonly #directories: BrowserAccountDirectoryResolver;
  readonly #vault: PlatformVaultReader;
  readonly #store: OwnerPlatformVaultStore | undefined;
  readonly #youtubeRequests = new Map<string, YoutubeRequestBudget>();
  readonly #ownerVaults = new Map<string, PlatformVaultReader>();
  youtubeRequest(credentialId: string): YoutubeRequest { z.uuid().parse(credentialId); const budget = this.#youtubeRequests.get(credentialId) ?? new YoutubeRequestBudget(); this.#youtubeRequests.set(credentialId, budget); return budget.run; }
  async #reader(manifest: PlatformAccountManifest, directory: string): Promise<PlatformVaultReader> {
    if (manifest.format === 1) return this.#vault;
    const root = await verifyPrivateDirectory(join(directory, manifest.storage.directory)), path = join(root, "vault.sqlite"), key = join(root, "masterkey");
    await verifyPrivateFile(path); await verifyPrivateFile(key);
    const existing = this.#ownerVaults.get(root); if (existing) return existing;
    const reader = new PlatformVaultReader(path, key); this.#ownerVaults.set(root, reader); return reader;
  }
  constructor(accounts: AccountContextService, identities: VerifiedMailIdentityProvider, directories: BrowserAccountDirectoryResolver, vault: PlatformVaultReader, store?: OwnerPlatformVaultStore) { this.#accounts = accounts; this.#identities = identities; this.#directories = directories; this.#vault = vault; this.#store = store; }
  async profiles(context: AccountContext) {
    if (this.#store) return this.#store.inspect(context, scope => scope.profiles.map(row => ({ id: z.string().parse(row.id), platform: z.enum(["x", "instagram", "telegram"]).parse(row.platform), provider_account_id: z.string().parse(row.provider_account_id), username: z.string().parse(row.username), profile_name: z.string().nullable().parse(row.profileName) ?? "", fingerprint_seed: z.string().nullable().parse(row.fingerprintSeed) ?? "", proxy: z.string().nullable().parse(row.proxyEndpoint), proxy_kind: z.string().nullable().parse(row.proxyKind), proxy_egress: z.string().nullable().parse(row.proxyEgress), updated_at: z.string().parse(row.updated_at) })));
    const inventory = await this.inventory(context), rows: { id: string; platform: Platform; provider_account_id: string; username: string; profile_name: string; fingerprint_seed: string; proxy: string | null; proxy_kind: string | null; proxy_egress: string | null; updated_at: string }[] = [];
    for (const platform of ["x", "instagram", "telegram"] as const) for (const item of inventory[platform]) await this.#withProfile(context, item.operator_account_id, platform, async (profile, secret) => {
      const proxy = secret.proxy === undefined || secret.proxy === null ? null : proxyConfigSchema.parse(secret.proxy);
      rows.push({ id: profile.id, platform, provider_account_id: profile.provider_account_id, username: profile.username, profile_name: secret.profile_name, fingerprint_seed: secret.fingerprint_seed, proxy: proxy ? `${proxy.host}:${proxy.port}` : null, proxy_kind: proxy?.kind ?? null, proxy_egress: secret.proxy_egress ?? null, updated_at: profile.updated_at });
    }, false);
    return rows;
  }
  async youtubeCredentials(context: AccountContext) {
    if (this.#store) return this.#store.inspect(context, scope => scope.services.map(row => ({ id: z.string().parse(row.id), provider: "youtube" as const, label: z.string().parse(row.label), metadata: parseStoredJson(z.string().parse(row.metadata_json)), disabled: row.disabled !== 0 && row.disabled !== 0n, cooldown_until: row.cooldown_until, last_used_at: row.last_used_at, last_error: row.last_error === null ? null : "YouTube credential has a recorded failure", created_at: z.string().parse(row.created_at), updated_at: z.string().parse(row.updated_at) })));
    const inventory = await this.inventory(context), rows: { id: string; provider: "youtube"; label: string; metadata: unknown; disabled: boolean; cooldown_until: number | null; last_used_at: number | null; last_error: string | null; created_at: string; updated_at: string }[] = [];
    for (const item of inventory.youtube) {
      const admitted = await this.#admit(context, item.credential_id, "youtube", false), record = admitted.manifest.services.find(value => value.accountRefId === item.credential_id);
      if (!record) throw new Error("YouTube credential owner mapping is missing");
      await admitted.vault.withYoutubeCredential(record.credentialId, async credential => {
        const metadata = await admitted.vault.youtubeMetadata(credential.id);
        if (credential.revision !== record.vaultRevision) throw new Error("YouTube credential changed");
        rows.push({ id: credential.id, provider: "youtube", label: metadata.label, metadata: metadata.metadata, disabled: credential.disabled, cooldown_until: credential.cooldownUntil, last_used_at: metadata.lastUsedAt, last_error: metadata.lastError === null ? null : "YouTube credential has a recorded failure", created_at: metadata.createdAt, updated_at: metadata.updatedAt });
      });
      await admitted.assertCurrent();
    }
    return rows;
  }
  async inventory(context: AccountContext) {
    if (this.#store) return this.#store.inspect(context, scope => {
      const result = emptyInventory();
      const available = (id: string) => { const ref = scope.creator.prepare("SELECT active,availability FROM account_refs WHERE id=?").get(id); return (ref?.active === 1 || ref?.active === 1n) && ref.availability === "available"; };
      for (const row of scope.profiles) { const platform = z.enum(["x", "instagram", "telegram"]).parse(row.platform), id = z.string().parse(row.accountRefId), username = z.string().parse(row.username), label = z.string().nullable().parse(row.profileName)?.trim() || username; result[platform].push({ operator_account_id: id, username, label, availability: row.credentialReadable === true && available(id) ? "available" : "unavailable" }); }
      for (const row of scope.services) { const id = z.string().parse(row.accountRefId), metadata = z.object({ api: z.literal("youtube_data_api_v3") }).passthrough().safeParse(parseStoredJson(z.string().parse(row.metadata_json))), cooldown = z.union([z.number().int().safe(), z.bigint()]).nullable().parse(row.cooldown_until); result.youtube.push({ credential_id: id, label: z.string().parse(row.label), availability: row.credentialReadable === true && available(id) && (row.disabled === 0 || row.disabled === 0n) && row.last_error === null && (cooldown === null || BigInt(cooldown) <= BigInt(Math.floor(Date.now() / 1000))) && metadata.success ? "available" : "unavailable" }); }
      for (const values of [result.x, result.instagram, result.telegram]) values.sort((a, b) => Buffer.compare(Buffer.from(a.label), Buffer.from(b.label)) || Buffer.compare(Buffer.from(a.operator_account_id), Buffer.from(b.operator_account_id)));
      result.youtube.sort((a, b) => Buffer.compare(Buffer.from(a.label), Buffer.from(b.label)) || Buffer.compare(Buffer.from(a.credential_id), Buffer.from(b.credential_id))); return result;
    });
    this.#accounts.assertCurrent(context);
    const expected = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId }, identity = await this.#identities.snapshot(expected), directory = await this.#directories.accountDirectory(context);
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (identity.owner[field] !== expected[field] || directory.owner[field] !== expected[field]) throw new Error("Platform inventory owner mismatch");
    if (identity.accessUserId !== expected.accountId) throw new Error("Platform inventory Access identity mismatch");
    const path = join(directory.directory, "platform-accounts.json");
    const exists = await lstat(path).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
    const empty = emptyInventory();
    if (!exists) { directory.assertOwned(); this.#accounts.assertCurrent(context); await this.#identities.assertCurrent(identity); return empty; }
    await verifyPrivateFile(path); const bytes = await readFile(path);
    if (bytes.length > 1024 * 1024) throw new Error("Platform account manifest is too large");
    const manifest = platformAccountManifestSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes), true));
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (manifest.owner[field] !== expected[field]) throw new Error("Platform inventory manifest owner mismatch");
    if (manifest.deviceId !== context.principal.deviceId) throw new Error("Platform inventory manifest device mismatch");
    for (const item of manifest.profiles) {
      const admitted = await this.#admit(context, item.accountRefId, item.platform, false);
      await this.#withProfile(context, item.accountRefId, item.platform, async (profile, secret) => { empty[item.platform].push({ operator_account_id: item.accountRefId, username: profile.username, label: secret.profile_name.trim() || profile.username, availability: admitted.available ? "available" : "unavailable" }); }, false);
      await admitted.assertCurrent();
    }
    for (const item of manifest.services) {
      const admitted = await this.#admit(context, item.accountRefId, "youtube", false);
      await admitted.vault.withYoutubeCredential(item.credentialId, async credential => {
        if (credential.revision !== item.vaultRevision) throw new Error("YouTube inventory credential changed");
        const metadata = await admitted.vault.youtubeMetadata(credential.id);
        const shape = z.object({ api: z.literal("youtube_data_api_v3") }).passthrough().safeParse(metadata.metadata);
        empty.youtube.push({ credential_id: item.accountRefId, label: metadata.label, availability: admitted.available && !credential.disabled && metadata.lastError === null && (credential.cooldownUntil === null || credential.cooldownUntil <= Math.floor(Date.now() / 1000)) && shape.success ? "available" : "unavailable" });
      });
      await admitted.assertCurrent();
    }
    for (const values of [empty.x, empty.instagram, empty.telegram]) values.sort((a, b) => Buffer.compare(Buffer.from(a.label), Buffer.from(b.label)) || Buffer.compare(Buffer.from(a.operator_account_id), Buffer.from(b.operator_account_id)));
    empty.youtube.sort((a, b) => Buffer.compare(Buffer.from(a.label), Buffer.from(b.label)) || Buffer.compare(Buffer.from(a.credential_id), Buffer.from(b.credential_id)));
    if (!(await readFile(path)).equals(bytes)) throw new Error("Platform inventory manifest changed");
    directory.assertOwned(); this.#accounts.assertCurrent(context); await this.#identities.assertCurrent(identity); return empty;
  }
  async #admit(context: AccountContext, id: string, platform: Platform | "youtube", requireAvailable = true) {
    this.#accounts.assertCurrent(context); z.uuid().parse(id);
    const expected = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
    const identity = await this.#identities.snapshot(expected); const directory = await this.#directories.accountDirectory(context);
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (identity.owner[field] !== expected[field] || directory.owner[field] !== expected[field]) throw new Error("Platform account owner does not match the verified identity");
    if (identity.accessUserId !== expected.accountId) throw new Error("Platform Access identity does not match");
    const manifestPath = join(directory.directory, "platform-accounts.json"); const databasePath = join(directory.directory, "creator.sqlite");
    const read = async () => {
      directory.assertOwned(); this.#accounts.assertCurrent(context); await this.#identities.assertCurrent(identity);
      await verifyPrivateFile(manifestPath); const bytes = await readFile(manifestPath);
      if (bytes.length > 1024 * 1024) throw new Error("Platform account manifest is too large");
      const manifest = platformAccountManifestSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes), true));
      for (const field of ["accountId", "tenantId", "baseId"] as const) if (manifest.owner[field] !== expected[field]) throw new Error("Platform account manifest belongs to another account");
      if (manifest.deviceId !== context.principal.deviceId) throw new Error("Platform account manifest belongs to another device");
      await verifyPrivateFile(databasePath); const database = new DatabaseSync(databasePath, { readOnly: true }); let available = false;
      try {
        const record = database.prepare("SELECT platform,kind,active,availability FROM account_refs WHERE id=?").get(id);
        if (!record || record.platform !== platform || record.kind !== (platform === "youtube" ? "service_credential" : "profile")) throw new Error("Platform account reference ownership is invalid");
        available = record.active === 1 && record.availability === "available";
        if (requireAvailable && !available) throw new Error("Platform account reference is unavailable");
      } finally { database.close(); }
      directory.assertOwned(); this.#accounts.assertCurrent(context); await this.#identities.assertCurrent(identity);
      return { manifest, available, hash: createHash("sha256").update(bytes).digest("hex") };
    };
    const initial = await read();
    return { manifest: initial.manifest, available: initial.available, vault: await this.#reader(initial.manifest, directory.directory), assertCurrent: async () => { const current = await read(); if (current.hash !== initial.hash || current.available !== initial.available) throw new Error("Platform account ownership mapping changed"); } };
  }
  async withProfile<T>(context: AccountContext, accountRefId: string, platform: Platform, consume: (profile: PlatformVaultProfile, secret: PlatformProfileSecret, assertCurrent: () => Promise<void>) => Promise<T>): Promise<T> {
    return this.#withProfile(context, accountRefId, platform, consume, true);
  }
  async #withProfile<T>(context: AccountContext, accountRefId: string, platform: Platform, consume: (profile: PlatformVaultProfile, secret: PlatformProfileSecret, assertCurrent: () => Promise<void>) => Promise<T>, requireAvailable: boolean): Promise<T> {
    const admitted = await this.#admit(context, accountRefId, platform, requireAvailable); const record = admitted.manifest.profiles.find(value => value.accountRefId === accountRefId && value.platform === platform);
    if (!record) throw new Error("Platform account has no verified owner mapping");
    return admitted.vault.withProfile(record.profileId, async (profile, secret) => {
      const assertCurrent = async () => { await admitted.assertCurrent(); const current = await admitted.vault.profile(record.profileId); if (current.platform !== platform || current.revision !== record.vaultRevision || current.provider_account_id !== record.providerAccountId) throw new Error("Platform Vault identity changed"); };
      if (profile.platform !== platform || profile.revision !== record.vaultRevision || profile.provider_account_id !== record.providerAccountId) throw new Error("Platform Vault identity does not match its owner mapping");
      await assertCurrent(); const result = await consume(profile, secret, assertCurrent); await assertCurrent(); return result;
    });
  }
  async withYoutubeCredential<T>(context: AccountContext, accountRefId: string, consume: (credential: YoutubeVaultCredential, apiKey: string, assertCurrent: () => Promise<void>, request: YoutubeRequest) => Promise<T>): Promise<T> {
    const admitted = await this.#admit(context, accountRefId, "youtube"); const record = admitted.manifest.services.find(value => value.accountRefId === accountRefId);
    if (!record) throw new Error("YouTube credential has no verified owner mapping");
    return admitted.vault.withYoutubeCredential(record.credentialId, async (credential, apiKey) => {
      const check = (value: YoutubeVaultCredential) => { if (value.revision !== record.vaultRevision || value.disabled || value.cooldownUntil !== null && value.cooldownUntil > Math.floor(Date.now() / 1000)) throw new Error("YouTube credential is unavailable or changed"); };
      const assertCurrent = async () => { await admitted.assertCurrent(); await admitted.vault.withYoutubeCredential(record.credentialId, async value => check(value)); };
      check(credential); await assertCurrent(); const observation = admitted.vault.nextYoutubeObservation();
      const outcome = await consume(credential, apiKey, assertCurrent, this.youtubeRequest(record.credentialId)).then(value => ({ value }), error => ({ error }));
      await assertCurrent();
      if ("error" in outcome) {
        if (outcome.error instanceof YoutubeApiReadError) await admitted.vault.recordYoutubeHealth(credential, observation, outcome.error.health);
        throw outcome.error;
      }
      await admitted.vault.recordYoutubeHealth(credential, observation, null); return outcome.value;
    });
  }
  async close(): Promise<void> { await Promise.all([...this.#youtubeRequests.values()].map(budget => budget.close())); }
}
