import { createHash } from "node:crypto";
import { lstatSync, type BigIntStats } from "node:fs";
import { open } from "node:fs/promises";
import { basename, dirname, join, relative, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import type { InstanceLease } from "../../service/instance-lock.ts";
import { VerifiedMigrationCredential } from "../../identity/access-verification.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../../storage/private-backup.ts";
import { validateCreatorSchema90 } from "../../storage/creator-schema.ts";
import { fingerprintSqlite } from "../../migration/sqlite-fingerprint.ts";
import { platformAccountManifestSchema } from "./owned-accounts.ts";
import { platformAccountOwnerSchema } from "./platform-manifest.ts";
import { PlatformVaultReader } from "./platform-vault.ts";

const issuer = Symbol("prepared-legacy-platform-owner");
type Manifest = z.infer<typeof platformAccountManifestSchema>;
const ownerSchema = platformAccountOwnerSchema;
const key = (owner: Manifest["owner"]) => createHash("sha256").update(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId])).digest("hex");
function identity(path: string): BigIntStats | null {
  const value = lstatSync(path, { bigint: true, throwIfNoEntry: false });
  if (!value) return null;
  if (!value.isFile() || value.isSymbolicLink() || value.nlink !== 1n) throw new Error("platform_ownership.file_invalid");
  return value;
}
function sameIdentity(a: BigIntStats | null, b: BigIntStats | null): boolean {
  return a === null || b === null ? a === b : a.dev === b.dev && a.ino === b.ino && a.size === b.size && a.mtimeNs === b.mtimeNs && a.ctimeNs === b.ctimeNs;
}

export class PreparedPlatformOwnership {
  readonly manifestPath: string;
  readonly #lease: InstanceLease;
  readonly #manifest: Manifest;
  readonly #identities: ReadonlyMap<string, BigIntStats | null>;
  #issued = false;
  constructor(token: symbol, lease: InstanceLease, manifestPath: string, manifest: Manifest, identities: ReadonlyMap<string, BigIntStats | null>) {
    if (token !== issuer) throw new Error("platform_ownership.private_preparation_required");
    this.#lease = lease; this.manifestPath = manifestPath; this.#manifest = structuredClone(manifest); this.#identities = new Map(identities); Object.freeze(this);
  }
  get selection(): Readonly<Manifest> { return structuredClone(this.#manifest); }
  assertPrepared(lease: InstanceLease): void {
    this.#lease.assertOwned();
    if (lease !== this.#lease || this.#issued) throw new Error("platform_ownership.preparation_unavailable");
    for (const [path, recorded] of this.#identities) if (!sameIdentity(recorded, identity(path))) throw new Error("platform_ownership.source_changed");
    if (identity(this.manifestPath) !== null) throw new Error("platform_ownership.manifest_already_exists");
  }
  async issue(lease: InstanceLease, proof: VerifiedMigrationCredential): Promise<void> {
    if (!(proof instanceof VerifiedMigrationCredential)) throw new Error("platform_ownership.verified_credential_required");
    proof.assertFresh(); this.assertPrepared(lease);
    if (proof.user.id !== this.#manifest.owner.accountId || proof.metadata.access_user_id !== this.#manifest.owner.accountId || proof.metadata.access_device_id !== this.#manifest.deviceId) throw new Error("platform_ownership.verified_owner_mismatch");
    await using file = await open(this.manifestPath, "wx", 0o600);
    await verifyPrivateFile(this.manifestPath); proof.assertFresh();
    await file.writeFile(JSON.stringify(this.#manifest)); await file.sync();
    proof.assertFresh(); lease.assertOwned(); this.#issued = true;
  }
  toJSON(): never { throw new Error("platform_ownership.authority_not_serializable"); }
}

// 只迁入明确选择的旧凭据；全局 Vault 中其余记录仍保留为历史，不获得当前账号权限。
export async function prepareLegacyPlatformOwnership(lease: InstanceLease, input: {
  creatorPath: string; vaultPath: string; masterKeyPath: string; owner: Manifest["owner"]; deviceId: string; selectedAccountRefIds: readonly string[];
}): Promise<PreparedPlatformOwnership> {
  lease.assertOwned(); const owner = ownerSchema.parse(input.owner), creatorPath = resolve(input.creatorPath), directory = dirname(creatorPath), generation = dirname(directory);
  if (relative(resolve(lease.directory), dirname(generation)) !== "" || !/^generation-[a-f0-9-]{36}$/u.test(basename(generation)) || basename(directory) !== `account-${key(owner)}` || basename(creatorPath) !== "creator.sqlite" || resolve(input.vaultPath) !== join(generation, "vault", "vault.sqlite") || resolve(input.masterKeyPath) !== join(generation, "masterkey")) throw new Error("platform_ownership.staging_owner_mismatch");
  await verifyPrivateDirectory(generation); await verifyPrivateDirectory(directory);
  for (const path of [creatorPath, input.vaultPath, input.masterKeyPath]) await verifyPrivateFile(path);
  const manifestPath = join(directory, "platform-accounts.json");
  if (identity(manifestPath) !== null) throw new Error("platform_ownership.manifest_already_exists");
  const selected = z.array(z.uuid()).max(1000).parse(input.selectedAccountRefIds);
  if (new Set(selected).size !== selected.length) throw new Error("platform_ownership.duplicate_selection");
  const paths = [creatorPath, `${creatorPath}-wal`, input.vaultPath, `${input.vaultPath}-wal`, input.masterKeyPath];
  const identities = new Map(paths.map(path => [path, identity(path)]));
  const before = { creator: fingerprintSqlite(creatorPath), vault: fingerprintSqlite(input.vaultPath) };
  const vault = new PlatformVaultReader(input.vaultPath, input.masterKeyPath);
  const manifest: Manifest = { format: 1, owner, deviceId: z.string().min(1).parse(input.deviceId), profiles: [], services: [] };
  using database = new DatabaseSync(creatorPath, { readOnly: true }); validateCreatorSchema90(database);
  for (const id of selected) {
    const row = database.prepare("SELECT id,platform,kind,active,availability FROM account_refs WHERE id=?").get(id);
    const reference = z.object({ id: z.literal(id), platform: z.enum(["x", "instagram", "telegram", "youtube"]), kind: z.enum(["profile", "service_credential"]), active: z.literal(1), availability: z.literal("available") }).strict().parse(row);
    if (reference.kind === "profile" && reference.platform !== "youtube") {
      const profile = await vault.profile(id);
      if (profile.platform !== reference.platform) throw new Error("platform_ownership.vault_identity_mismatch");
      manifest.profiles.push({ accountRefId: id, profileId: id, platform: profile.platform, providerAccountId: profile.provider_account_id, vaultRevision: profile.revision });
    } else if (reference.kind === "service_credential" && reference.platform === "youtube") {
      await vault.withYoutubeCredential(id, async credential => {
        if (credential.disabled) throw new Error("platform_ownership.credential_disabled");
        manifest.services.push({ accountRefId: id, credentialId: id, provider: "youtube", vaultRevision: credential.revision });
      });
    } else throw new Error("platform_ownership.reference_kind_mismatch");
  }
  if (!isDeepStrictEqual(before, { creator: fingerprintSqlite(creatorPath), vault: fingerprintSqlite(input.vaultPath) })) throw new Error("platform_ownership.source_changed");
  const prepared = new PreparedPlatformOwnership(issuer, lease, manifestPath, platformAccountManifestSchema.parse(manifest), identities); prepared.assertPrepared(lease); return prepared;
}
