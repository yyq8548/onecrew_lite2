import { createHash, randomUUID, timingSafeEqual } from "node:crypto";
import { constants, lstatSync, readdirSync, realpathSync, type BigIntStats } from "node:fs";
import { chmod, copyFile, lstat, open, readFile, readdir, realpath, rename } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { acquireDatabaseLock } from "onecrew_lite_tables/runtime";
import { AccountDatabases, PreparedCreatorAdoption } from "../identity/account-databases.ts";
import { VerifiedMigrationCredential } from "../identity/access-verification.ts";
import { readLibraryMigrationAccount } from "../identity/library-migration-account.ts";
import type { GmailBulkRepositoryOwner } from "../mail/gmail-bulk-account-binding.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { backupLegacyDatabase, inspectLegacyDatabase } from "../storage/legacy-sqlite.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile, verifyPrivateTree } from "../storage/private-backup.ts";
import { readVaultMasterKey } from "../storage/vault-reader.ts";
import { reviewMigrationAccounts, type MigrationAccountReview, type MigrationAccountSources } from "./account-review.ts";
import { fingerprintSqlite } from "./sqlite-fingerprint.ts";
import { writeMigrationJournal } from "./journal.ts";
import { LegacyQuiescenceLease, type LegacySourcePaths } from "./legacy-quiescence.ts";
import { startCandidateRuntime, type CandidateHealth } from "../service/candidate-runtime.ts";
import { readDevelopmentBuildIdentity } from "../service/build-identity.ts";
import { PreparedWindowsCutover } from "./windows-cutover.ts";
import { AccessVaultStore, PreparedAccessVaultWrite } from "../identity/access-vault-store.ts";
import { packagedFingerprintPaths } from "../packaging/runtime-context.ts";
import { prepareTableSyncStaging, verifyTableSyncStaging } from "./table-sync-staging.ts";
import { verifyLegacyPrivateDirectory } from "../storage/legacy-private-source.ts";
import { PreparedPlatformOwnership, prepareLegacyPlatformOwnership } from "../platforms/browser/ownership.ts";
import { readStoredAccessCredential } from "../identity/legacy-access-credential.ts";

const sha256 = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const digest = z.string().regex(/^[a-f0-9]{64}$/u);
const ownerSchema = z.object({ accountId: z.string().min(1), tenantId: z.string().min(1), baseId: z.string().min(1) }).strict();
const pointerSchema = z.object({ format: z.literal(1), generationId: z.guid(), manifestSha256: digest }).strict();
const manifestSchema = z.object({ format: z.literal(1), id: z.guid(), state: z.literal("data_generation_ready"), dataDirectory: z.string(),
  profileSha256: digest, owner: ownerSchema, creatorDirectory: z.string().regex(/^account-[a-f0-9]{64}$/u),
  masterKeySha256: digest, candidateSha256: digest, previousPointer: digest.nullable(), installerCutoverDirectory: z.string().regex(/^cutover-[a-f0-9-]{36}$/u),
  runtimeKind: z.enum(["source_development", "sea_candidate"]), productionReady: z.literal(false), runtimeExecutableSha256: digest, runtimeSourceSha256: digest,
  runtimeLockSha256: digest, webAssetsSha256: digest, tableSyncSourceSha256: digest.nullable() }).strict();
const samePath = (a: string, b: string) => relative(resolve(a), resolve(b)) === "";
const accountDirectoryName = (owner: GmailBulkRepositoryOwner) => `account-${sha256(JSON.stringify([owner.accountId, owner.tenantId, owner.baseId]))}`;

async function privateJson(path: string) {
  await verifyPrivateFile(path);
  const before = await lstat(path, { bigint: true });
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.ino !== before.ino || opened.dev !== before.dev || opened.nlink !== 1n || opened.size > 4n * 1024n * 1024n) throw new Error("migration.activation_metadata_invalid");
  const bytes = await file.readFile();
  const after = await file.stat({ bigint: true }), current = await lstat(path, { bigint: true });
  if (current.isSymbolicLink() || current.ino !== opened.ino || current.dev !== opened.dev || current.nlink !== 1n || after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs) throw new Error("migration.activation_metadata_changed");
  return { value: parseApplicationJson(bytes), bytes, sha256: sha256(bytes) };
}
async function profileIdentity(lease: InstanceLease) {
  const result = await privateJson(join(lease.directory, "profile.json"));
  const parsed = z.object({ format: z.literal(1), dataDirectory: z.string(), port: z.number().int().min(1).max(65535), originSource: z.enum(["new-install", "legacy-upgrade"]) }).strict().parse(result.value);
  if (!samePath(parsed.dataDirectory, lease.directory)) throw new Error("migration.profile_owner_mismatch");
  return { ...result, profile: parsed };
}
async function currentPointer(lease: InstanceLease) {
  const path = join(lease.directory, "current.json");
  if (!lstatSync(path, { throwIfNoEntry: false })) return null;
  const file = await privateJson(path);
  return { ...file, pointer: pointerSchema.parse(file.value) };
}

async function fileFingerprint(path: string, required = true) {
  const before = lstatSync(path, { bigint: true, throwIfNoEntry: false });
  if (!before) { if (required) throw new Error("migration.source_missing"); return null; }
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || !samePath(path, await realpath(path))) throw new Error("migration.source_alias_rejected");
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.ino !== before.ino || opened.dev !== before.dev || opened.nlink !== 1n) throw new Error("migration.source_changed");
  const hash = createHash("sha256");
  for await (const bytes of file.createReadStream({ autoClose: false })) hash.update(bytes);
  const after = await file.stat({ bigint: true }), current = await lstat(path, { bigint: true });
  if (after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs || current.size !== after.size || current.mtimeNs !== after.mtimeNs || current.ctimeNs !== after.ctimeNs || current.ino !== opened.ino || current.dev !== opened.dev || current.isSymbolicLink() || current.nlink !== 1n) throw new Error("migration.source_changed");
  return { sha256: hash.digest("hex"), bytes: after.size.toString(), device: after.dev.toString(), inode: after.ino.toString() };
}
async function treeFingerprint(directory: string) {
  const files: { path: string; bytes: string; sha256: string }[] = [];
  async function visit(path: string): Promise<void> {
    const entries = await readdir(path, { withFileTypes: true }); entries.sort((a, b) => Buffer.compare(Buffer.from(a.name), Buffer.from(b.name)));
    for (const entry of entries) {
      const child = join(path, entry.name), info = await lstat(child);
      if (info.isSymbolicLink()) throw new Error("migration.source_alias_rejected");
      if (info.isDirectory()) await visit(child);
      else { const value = (await fileFingerprint(child))!; files.push({ path: relative(directory, child).replaceAll("\\", "/"), bytes: value.bytes, sha256: value.sha256 }); }
    }
  }
  await visit(directory); return files;
}
async function copyTree(source: string, destination: string): Promise<void> {
  await createPrivateDirectory(destination);
  for (const entry of await readdir(source, { withFileTypes: true })) {
    const from = join(source, entry.name), to = join(destination, entry.name), info = await lstat(from);
    if (info.isSymbolicLink()) throw new Error("migration.source_alias_rejected");
    if (info.isDirectory()) await copyTree(from, to);
    else { if (!info.isFile() || info.nlink !== 1) throw new Error("migration.source_alias_rejected"); await copyFile(from, to, constants.COPYFILE_EXCL); if (process.platform !== "win32") await chmod(to, 0o600); }
  }
}
async function assertOriginalSources(lease: InstanceLease, sources: Omit<MigrationAccountSources, "signal">, paths: LegacySourcePaths) {
  for (const [field, recorded] of [["creator", sources.sqlite.sourceHashes.sourceCreator], ["vault", sources.sqlite.sourceHashes.sourceVault]] as const) {
    const actual = { main: await fileFingerprint(paths[field]), wal: await fileFingerprint(`${paths[field]}-wal`, false) };
    if (!recorded || !isDeepStrictEqual(actual.main, recorded.main) || !isDeepStrictEqual(actual.wal, recorded.wal)) throw new Error("migration.original_source_changed");
  }
  const originalKey = await readVaultMasterKey(paths.masterKey, "legacy-source");
  using releaseOriginal = { [Symbol.dispose]() { originalKey.fill(0); } };
  const copiedKey = await readVaultMasterKey(sources.sqlite.masterKey);
  using releaseCopy = { [Symbol.dispose]() { copiedKey.fill(0); } };
  if (!timingSafeEqual(originalKey, copiedKey)) throw new Error("migration.original_master_key_changed");
  if (sources.library.state !== "staged" || paths.libraryDirectory === null) throw new Error("migration.library_owner_mapping_required");
  const recoveryId = z.guid().parse(sources.library.receipt.recovery_id);
  const recoveryDirectory = join(lease.directory, `library-recovery-${recoveryId}`);
  const recovery = await privateJson(join(recoveryDirectory, "journal.json"));
  const manifest = z.object({ id: z.guid(), directory: z.string(), state: z.literal("library_recovery_ready"), activated: z.literal(false),
    files: z.array(z.object({ path: z.string(), bytes: z.string(), sha256: digest }).strict()) }).strict().parse(recovery.value);
  if (manifest.id !== recoveryId || !samePath(manifest.directory, recoveryDirectory) || !isDeepStrictEqual(manifest.files, await treeFingerprint(join(paths.libraryDirectory, "pglite")))) throw new Error("migration.original_library_changed");
}

export interface ActivationCandidateInput {
  legacyServiceDirectory: string;
  platformAccountRefIds?: readonly string[];
  sources: Omit<MigrationAccountSources, "signal">;
  sourcePaths: LegacySourcePaths;
  expectedReview: MigrationAccountReview;
  owner: GmailBulkRepositoryOwner;
  expectedCurrent: string | null;
  signal: AbortSignal;
}

// 准备阶段不需要、也不产生在线身份。所有输出仍在未激活的私有 generation 中。
export async function prepareActivationCandidate(lease: InstanceLease, input: ActivationCandidateInput) {
  lease.assertOwned(); input.signal.throwIfAborted();
  const profile = await profileIdentity(lease);
  if (profile.profile.originSource !== "legacy-upgrade") throw new Error("migration.legacy_profile_required");
  if ((await currentPointer(lease))?.sha256 !== (input.expectedCurrent ?? undefined)) throw new Error("migration.current_pointer_changed");
  const reviewed = await reviewMigrationAccounts(lease, { ...input.sources, signal: input.signal });
  if (!isDeepStrictEqual(reviewed.review, input.expectedReview)) throw new Error("migration.account_review_changed");
  const owner = ownerSchema.parse(input.owner), main = reviewed.review.candidates.find(value => value.source === "main");
  if (!main || !["present", "inactive"].includes(main.state) || main.userId !== owner.accountId) throw new Error("creator_owner_mapping_required");
  if (reviewed.decision.state !== "verify_existing" || reviewed.decision.userId !== owner.accountId) throw new Error(`migration.accounts.${reviewed.state}`);
  const tenant = reviewed.preserved.tenants.find(value => value.owner_user_id === owner.accountId && value.id === owner.tenantId && value.base_id === owner.baseId);
  if (!tenant || input.sources.library.state !== "staged") throw new Error("migration.library_owner_mapping_required");
  const sourcePaths = { creator: resolve(input.sourcePaths.creator), vault: resolve(input.sourcePaths.vault), masterKey: resolve(input.sourcePaths.masterKey), libraryDirectory: input.sourcePaths.libraryDirectory === null ? null : resolve(input.sourcePaths.libraryDirectory) };
  const legacyServiceDirectory = await verifyLegacyPrivateDirectory(input.legacyServiceDirectory);
  for (const source of [sourcePaths.creator, sourcePaths.vault, sourcePaths.masterKey]) {
    const child = relative(legacyServiceDirectory, source);
    if (!child || isAbsolute(child) || child === ".." || child.startsWith("..\\") || child.startsWith("../") || resolve(legacyServiceDirectory, child) !== source) throw new Error("migration.legacy_source_owner_mismatch");
  }
  await assertOriginalSources(lease, input.sources, sourcePaths);
  const id = randomUUID(), directory = await createPrivateDirectory(join(lease.directory, `generation-${id}`));
  let completed = false, phase = "copy_sqlite";
  await writeMigrationJournal(directory, { id, activated: false, state: "preparing", phase });
  await using failure = { async [Symbol.asyncDispose]() { if (!completed) await writeMigrationJournal(directory, { id, activated: false, state: "failed", phase }); } };
  const preparedCreator = await AccountDatabases.prepareValidatedAdoption(lease, directory, owner,
    { sourcePath: input.sources.sqlite.creator.path, expectedFingerprint: input.sources.sqlite.fingerprints.creator });
  const creator = preparedCreator.databasePath, vault = join(directory, "vault", "vault.sqlite"), masterKey = join(directory, "masterkey"), tables = join(directory, "tables");
  const tableSync = await prepareTableSyncStaging(lease, { sourceDirectory: legacyServiceDirectory, generationDirectory: directory, accountDirectory: dirname(creator), accountId: owner.accountId });
  await backupLegacyDatabase(input.sources.sqlite.vault.path, vault, "vault");
  const key = await readVaultMasterKey(input.sources.sqlite.masterKey);
  using releaseKey = { [Symbol.dispose]() { key.fill(0); } };
  { await using file = await open(masterKey, "wx", 0o600); await verifyPrivateFile(masterKey); await file.writeFile(key); await file.sync(); }
  phase = "copy_library";
  const librarySource = input.sources.library.receipt.directory;
  const lock = await acquireDatabaseLock(librarySource);
  if (!lock) throw new Error("migration.library_staging_busy");
  {
    await using releaseLock = { [Symbol.asyncDispose]: () => lock.close() };
    const before = await treeFingerprint(join(librarySource, "pglite"));
    await createPrivateDirectory(tables); await copyTree(join(librarySource, "pglite"), join(tables, "pglite"));
    await verifyPrivateTree(tables);
    if (!isDeepStrictEqual(before, await treeFingerprint(join(tables, "pglite"))) || !isDeepStrictEqual(before, await treeFingerprint(join(librarySource, "pglite")))) throw new Error("migration.library_staging_changed");
  }
  phase = "verify_candidate";
  const candidateLibrary = await readLibraryMigrationAccount(lease, tables);
  using releaseLibrary = { [Symbol.dispose]() { candidateLibrary.credential?.dispose(); } };
  if (candidateLibrary.sourceFingerprint !== input.expectedReview.sources.library) throw new Error("migration.library_staging_changed");
  const latest = await reviewMigrationAccounts(lease, { ...input.sources, signal: input.signal });
  if (!isDeepStrictEqual(latest.review, reviewed.review)) throw new Error("migration.account_review_changed");
  await assertOriginalSources(lease, input.sources, sourcePaths);
  const fingerprints = { creator: fingerprintSqlite(creator), vault: fingerprintSqlite(vault) };
  if (!isDeepStrictEqual(fingerprints, input.sources.sqlite.fingerprints)) throw new Error("migration.sqlite_staging_changed");
  const result = { id, directory, state: "activation_candidate_ready", activated: false, profileSha256: profile.sha256,
    sources: input.sources, sourcePaths, review: reviewed.review, owner, selected: reviewed.decision.candidates[0]!, expectedCurrent: input.expectedCurrent,
    creator, vault, masterKey, tables, legacyServiceDirectory, tableSync, platformAccountRefIds: input.platformAccountRefIds === undefined ? null : z.array(z.uuid()).max(1000).parse(input.platformAccountRefIds), masterKeySha256: sha256(key), fingerprints, libraryFingerprint: candidateLibrary.sourceFingerprint } as const;
  input.signal.throwIfAborted(); lease.assertOwned();
  { await using file = await open(join(directory, "candidate.json"), "wx", 0o600); await verifyPrivateFile(join(directory, "candidate.json")); await file.writeFile(JSON.stringify(result)); await file.sync(); }
  await writeMigrationJournal(directory, result); completed = true;
  return result;
}
export type ActivationCandidate = Awaited<ReturnType<typeof prepareActivationCandidate>>;

const publicationIssuer = Symbol("verified-activation-publication");
type FileIdentity = Readonly<{ path: string; stat: BigIntStats | undefined; children: readonly string[] | null }>;
function collectIdentity(path: string, output: FileIdentity[]): void {
  const stat = lstatSync(path, { bigint: true, throwIfNoEntry: false });
  if (!stat) { output.push({ path, stat, children: null }); return; }
  if (stat.isSymbolicLink() || (!stat.isFile() && !stat.isDirectory()) || !samePath(path, realpathSync(path)) || (stat.isFile() && stat.nlink !== 1n)) throw new Error("migration.publication_path_invalid");
  const children = stat.isDirectory() ? readdirSync(path).sort() : null;
  output.push({ path, stat, children });
  if (children) for (const child of children) collectIdentity(join(path, child), output);
}
function assertIdentity(expected: readonly FileIdentity[], additions = new Map<string, readonly string[]>()): void {
  for (const item of expected) {
    const current = lstatSync(item.path, { bigint: true, throwIfNoEntry: false });
    if (!item.stat) { if (current) throw new Error("migration.publication_data_changed"); continue; }
    const added = additions.get(item.path);
    if (!current || current.isSymbolicLink() || current.dev !== item.stat.dev || current.ino !== item.stat.ino || (!added && (current.size !== item.stat.size ||
        current.mtimeNs !== item.stat.mtimeNs || current.ctimeNs !== item.stat.ctimeNs)) || current.mode !== item.stat.mode ||
        (current.isFile() && current.nlink !== 1n) || !samePath(item.path, realpathSync(item.path))) throw new Error("migration.publication_data_changed");
    if (item.children && !isDeepStrictEqual(readdirSync(item.path).sort(), [...item.children, ...(added ?? [])].sort())) throw new Error("migration.publication_data_changed");
  }
}

// 大库哈希、候选启动和完整结构校验先完成；远端证明签发后的发布只执行身份 CAS。
export class PreparedActivationPublication {
  readonly #lease: InstanceLease;
  readonly #candidate: ActivationCandidate;
  #identities: readonly FileIdentity[];
  readonly #quiescence: LegacyQuiescenceLease;
  readonly #adoption: PreparedCreatorAdoption;
  readonly #vaultStore: AccessVaultStore;
  readonly #vaultWrite: PreparedAccessVaultWrite;
  readonly #platformOwnership: PreparedPlatformOwnership | undefined;
  #credentialInstalled = false;
  readonly candidateSha256: string;
  readonly #health: CandidateHealth;
  constructor(issuer: symbol, lease: InstanceLease, candidate: ActivationCandidate, sha256: string, identities: readonly FileIdentity[],
    quiescence: LegacyQuiescenceLease, adoption: PreparedCreatorAdoption, health: CandidateHealth, vaultStore: AccessVaultStore, vaultWrite: PreparedAccessVaultWrite, platformOwnership?: PreparedPlatformOwnership) {
    if (issuer !== publicationIssuer) throw new Error("migration.publication_authority_unissued");
    this.#lease = lease; this.#candidate = structuredClone(candidate); this.#identities = identities;
    this.#quiescence = quiescence; this.#adoption = adoption; this.candidateSha256 = sha256; this.#health = structuredClone(health);
    this.#vaultStore = vaultStore; this.#vaultWrite = vaultWrite;
    this.#platformOwnership = platformOwnership;
    Object.freeze(this);
  }
  get health(): Readonly<CandidateHealth> { return structuredClone(this.#health); }
  assertPrepared(lease: InstanceLease, quiescence: LegacyQuiescenceLease) {
    this.#lease.assertOwned();
    if (lease !== this.#lease || quiescence !== this.#quiescence) throw new Error("migration.publication_lease_changed");
    quiescence.assertQuiescent(this.#candidate.sourcePaths);
    this.#adoption.assertPrepared(lease); assertIdentity(this.#identities);
    return { candidate: structuredClone(this.#candidate), adoption: this.#adoption };
  }
  assertBeforePublish(lease: InstanceLease, quiescence: LegacyQuiescenceLease): void {
    this.#lease.assertOwned();
    if (lease !== this.#lease || quiescence !== this.#quiescence) throw new Error("migration.publication_lease_changed");
    if (!this.#credentialInstalled) throw new Error("migration.selected_credential_not_installed");
    quiescence.assertQuiescent(this.#candidate.sourcePaths); this.#adoption.assertPrepared(lease);
    assertIdentity(this.#identities, new Map([[this.#candidate.directory, ["generation.json", ...(this.#candidate.expectedCurrent ? ["previous-current.json"] : [])]], [dirname(this.#candidate.creator), ["account.json"]]]));
  }
  async installVerifiedCredential(lease: InstanceLease, quiescence: LegacyQuiescenceLease, proof: VerifiedMigrationCredential): Promise<void> {
    if (this.#credentialInstalled) throw new Error("migration.publication_already_used");
    this.assertPrepared(lease, quiescence);
    if (!(proof instanceof VerifiedMigrationCredential) || proof.sourceFingerprint !== this.#candidate.selected.fingerprint || proof.user.id !== this.#candidate.owner.accountId) throw new Error("migration.verified_owner_mismatch");
    if (this.#platformOwnership) {
      await this.#platformOwnership.issue(lease, proof);
      const directory = dirname(this.#candidate.creator), updated: FileIdentity[] = [];
      collectIdentity(directory, updated);
      this.#identities = [...this.#identities.filter(item => !samePath(item.path, directory) && !item.path.startsWith(directory + (process.platform === "win32" ? "\\" : "/"))), ...updated];
    }
    await this.#vaultStore.savePreparedMigration(proof, this.#vaultWrite);
    const vaultDirectory = dirname(this.#candidate.vault), updated: FileIdentity[] = [];
    collectIdentity(vaultDirectory, updated);
    this.#identities = [...this.#identities.filter(item => !samePath(item.path, vaultDirectory) && !samePath(item.path, this.#candidate.vault) && !item.path.startsWith(vaultDirectory + (process.platform === "win32" ? "\\" : "/"))), ...updated];
    this.#credentialInstalled = true;
  }
  toJSON(): never { throw new Error("Publication authority cannot be serialized"); }
}

export async function prepareActivationPublication(lease: InstanceLease, input: { candidate: ActivationCandidate; quiescence: LegacyQuiescenceLease;
  webAssetsDirectory: string; libraryWebAssetsDirectory?: string; signal: AbortSignal }): Promise<PreparedActivationPublication> {
  lease.assertOwned(); input.signal.throwIfAborted();
  if (!(input.quiescence instanceof LegacyQuiescenceLease)) throw new Error("migration.legacy_quiescence_unavailable");
  if (!samePath(input.quiescence.serviceDirectory, input.candidate.legacyServiceDirectory)) throw new Error("migration.legacy_source_owner_mismatch");
  input.quiescence.assertQuiescent(input.candidate.sourcePaths);
  const initial = await readActivationCandidate(lease, input.candidate.id);
  if (!isDeepStrictEqual(initial.candidate, input.candidate)) throw new Error("migration.candidate_data_changed");
  const runtime = await startCandidateRuntime(lease, { generationRoot: initial.candidate.directory, webAssetsDirectory: input.webAssetsDirectory, ...(input.libraryWebAssetsDirectory ? { libraryWebAssetsDirectory: input.libraryWebAssetsDirectory } : {}) });
  let health: CandidateHealth;
  try { health = await runtime.health(); } finally { await runtime.close(); }
  input.signal.throwIfAborted();
  const checked = await readActivationCandidate(lease, input.candidate.id), candidate = checked.candidate;
  if (checked.sha256 !== initial.sha256) throw new Error("migration.candidate_data_changed");
  const reviewed = await reviewMigrationAccounts(lease, { ...candidate.sources, signal: input.signal });
  if (!isDeepStrictEqual(reviewed.review, candidate.review) || reviewed.decision.state !== "verify_existing" || reviewed.decision.userId !== candidate.owner.accountId ||
      !isDeepStrictEqual(reviewed.decision.candidates[0], candidate.selected)) throw new Error("migration.account_review_changed");
  await assertOriginalSources(lease, candidate.sources, candidate.sourcePaths);
  const adoption = await AccountDatabases.prepareValidatedAdoption(lease, candidate.directory, candidate.owner,
    { sourcePath: candidate.creator, expectedFingerprint: candidate.fingerprints.creator, existing: true });
  const vaultStore = new AccessVaultStore(lease, candidate.vault, candidate.masterKey), vaultWrite = await vaultStore.prepareMigrationSave(candidate.fingerprints.vault);
  if (vaultWrite.masterKeySha256 !== candidate.masterKeySha256) throw new Error("migration.candidate_key_changed");
  let platformOwnership: PreparedPlatformOwnership | undefined;
  if (candidate.platformAccountRefIds !== null) {
    const access = await readStoredAccessCredential(candidate.vault, candidate.masterKey);
    using releaseAccess = { [Symbol.dispose]() { if (access.state === "active") access.credential.dispose(); } };
    if (access.state === "missing") throw new Error("platform_ownership.access_identity_missing");
    const metadata = access.state === "active" ? access.credential.metadata : access.metadata;
    if (metadata.access_user_id !== candidate.owner.accountId) throw new Error("platform_ownership.access_owner_mismatch");
    platformOwnership = await prepareLegacyPlatformOwnership(lease, { creatorPath: candidate.creator, vaultPath: candidate.vault, masterKeyPath: candidate.masterKey, owner: candidate.owner, deviceId: metadata.access_device_id, selectedAccountRefIds: candidate.platformAccountRefIds });
  }
  const build = await readDevelopmentBuildIdentity();
  for (const field of ["kind", "executable_sha256", "source_tree_sha256", "dependency_lock_sha256"] as const) {
    if (build[field] !== health.resource_identity[field]) throw new Error("migration.candidate_build_changed");
  }
  const identities: FileIdentity[] = [];
  const sourceRoot = fileURLToPath(new URL("../../../", import.meta.url));
  const buildPaths = packagedFingerprintPaths() ?? [process.execPath, join(sourceRoot, "pnpm-lock.yaml"), join(sourceRoot, "onecrew_lite-app", "src"), join(sourceRoot, "onecrew_lite-contracts", "src"), join(sourceRoot, "onecrew_lite-library", "dist"), ...["onecrew_lite-app", "onecrew_lite-contracts", "onecrew_lite-library"].map(directory => join(sourceRoot, directory, "package.json"))];
  for (const path of [candidate.directory, join(lease.directory, "profile.json"), join(lease.directory, "current.json"),
    candidate.sourcePaths.creator, `${candidate.sourcePaths.creator}-wal`, candidate.sourcePaths.vault, `${candidate.sourcePaths.vault}-wal`, candidate.sourcePaths.masterKey,
    ...buildPaths, resolve(input.webAssetsDirectory), ...(input.libraryWebAssetsDirectory ? [resolve(input.libraryWebAssetsDirectory)] : []), candidate.tableSync.sourcePath,
    candidate.sources.sqlite.directory, ...(candidate.sources.library.state === "staged" ? [candidate.sources.library.receipt.directory] : []),
    ...(candidate.sourcePaths.libraryDirectory ? [candidate.sourcePaths.libraryDirectory] : [])]) collectIdentity(path, identities);
  input.signal.throwIfAborted(); input.quiescence.assertQuiescent(candidate.sourcePaths); lease.assertOwned();
  return new PreparedActivationPublication(publicationIssuer, lease, candidate, checked.sha256, identities, input.quiescence, adoption, health, vaultStore, vaultWrite, platformOwnership);
}

export async function readActivationCandidate(lease: InstanceLease, id: string) {
  lease.assertOwned(); z.guid().parse(id);
  const directory = join(lease.directory, `generation-${id}`);
  await verifyPrivateDirectory(directory);
  const stored = await privateJson(join(directory, "candidate.json"));
  const shape = z.object({ id: z.guid(), directory: z.string(), state: z.literal("activation_candidate_ready"), activated: z.literal(false),
    profileSha256: digest, masterKeySha256: digest, owner: ownerSchema, creator: z.string(), vault: z.string(), masterKey: z.string(), tables: z.string(),
    expectedCurrent: digest.nullable() }).passthrough().parse(stored.value);
  if (shape.id !== id || !samePath(shape.directory, directory) || !samePath(shape.creator, join(directory, accountDirectoryName(shape.owner), "creator.sqlite")) ||
      !samePath(shape.vault, join(directory, "vault", "vault.sqlite")) || !samePath(shape.masterKey, join(directory, "masterkey")) || !samePath(shape.tables, join(directory, "tables"))) throw new Error("migration.candidate_layout_changed");
  if (!isDeepStrictEqual((await privateJson(join(directory, "journal.json"))).value, stored.value)) throw new Error("migration.candidate_journal_changed");
  const candidate = stored.value as ActivationCandidate;
  if (!samePath(candidate.legacyServiceDirectory, candidate.tableSync.sourceDirectory)) throw new Error("migration.table_sync_owner_changed");
  await verifyTableSyncStaging(lease, candidate.tableSync, candidate.directory, dirname(candidate.creator), candidate.owner.accountId);
  if ((await profileIdentity(lease)).sha256 !== candidate.profileSha256) throw new Error("migration.profile_changed");
  for (const path of [candidate.creator, candidate.vault, candidate.masterKey]) await verifyPrivateFile(path);
  const creatorInspection = inspectLegacyDatabase(candidate.creator, "creator"), vaultInspection = inspectLegacyDatabase(candidate.vault, "vault");
  if (creatorInspection.version !== creatorInspection.currentVersion || vaultInspection.version !== 4 || !creatorInspection.schemaValidated || !vaultInspection.schemaValidated ||
      !isDeepStrictEqual({ creator: fingerprintSqlite(candidate.creator), vault: fingerprintSqlite(candidate.vault) }, candidate.fingerprints)) throw new Error("migration.candidate_data_changed");
  const key = await readVaultMasterKey(candidate.masterKey);
  using releaseKey = { [Symbol.dispose]() { key.fill(0); } };
  if (sha256(key) !== candidate.masterKeySha256) throw new Error("migration.candidate_key_changed");
  const library = await readLibraryMigrationAccount(lease, candidate.tables);
  using releaseLibrary = { [Symbol.dispose]() { library.credential?.dispose(); } };
  if (library.sourceFingerprint !== candidate.libraryFingerprint || !library.tenants.some(row => row.owner_user_id === candidate.owner.accountId && row.id === candidate.owner.tenantId && row.base_id === candidate.owner.baseId)) throw new Error("migration.candidate_library_changed");
  lease.assertOwned();
  return { candidate, sha256: stored.sha256 };
}

// 当前指针只定位数据，不构造已验证账号；启动后的 Access 仍由唯一账号服务重新验证。
export async function readActiveGeneration(lease: InstanceLease) {
  lease.assertOwned();
  const current = await currentPointer(lease); if (!current) return null;
  const root = join(lease.directory, `generation-${current.pointer.generationId}`);
  await verifyPrivateDirectory(root);
  const stored = await privateJson(join(root, "generation.json"));
  if (stored.sha256 !== current.pointer.manifestSha256) throw new Error("migration.generation_manifest_changed");
  const manifest = manifestSchema.parse(stored.value), profile = await profileIdentity(lease);
  if (manifest.id !== current.pointer.generationId || !samePath(manifest.dataDirectory, lease.directory) || manifest.profileSha256 !== profile.sha256 || manifest.creatorDirectory !== accountDirectoryName(manifest.owner)) throw new Error("migration.generation_owner_mismatch");
  if (manifest.previousPointer !== null) {
    const previous = await privateJson(join(root, "previous-current.json"));
    if (previous.sha256 !== manifest.previousPointer) throw new Error("migration.previous_pointer_changed");
    pointerSchema.parse(previous.value);
  }
  const candidate = await privateJson(join(root, "candidate.json"));
  if (candidate.sha256 !== manifest.candidateSha256) throw new Error("migration.generation_candidate_changed");
  if (manifest.tableSyncSourceSha256 !== null) {
    const backup = join(root, "history", "dynamic-table-sync.json"); await verifyPrivateFile(backup);
    if ((await fileFingerprint(backup))?.sha256 !== manifest.tableSyncSourceSha256) throw new Error("migration.table_sync_history_changed");
  }
  const vaultPath = join(root, "vault", "vault.sqlite"), masterKeyPath = join(root, "masterkey"), tablesDirectory = join(root, "tables");
  const creatorPath = join(root, manifest.creatorDirectory, "creator.sqlite");
  await verifyPrivateFile(vaultPath); await verifyPrivateFile(creatorPath); await verifyPrivateDirectory(tablesDirectory);
  const account = await privateJson(join(root, manifest.creatorDirectory, "account.json"));
  if (!isDeepStrictEqual(account.value, { format: 1, ...manifest.owner, database: "creator.sqlite" })) throw new Error("migration.generation_owner_mismatch");
  const key = await readVaultMasterKey(masterKeyPath);
  using releaseKey = { [Symbol.dispose]() { key.fill(0); } };
  const creatorInspection = inspectLegacyDatabase(creatorPath, "creator"), vaultInspection = inspectLegacyDatabase(vaultPath, "vault");
  if (sha256(key) !== manifest.masterKeySha256 || creatorInspection.version !== creatorInspection.currentVersion || vaultInspection.version !== 4 || !creatorInspection.schemaValidated || !vaultInspection.schemaValidated) throw new Error("migration.active_generation_invalid");
  const library = await readLibraryMigrationAccount(lease, tablesDirectory);
  using releaseLibrary = { [Symbol.dispose]() { library.credential?.dispose(); } };
  if (!library.tenants.some(value => value.owner_user_id === manifest.owner.accountId && value.id === manifest.owner.tenantId && value.base_id === manifest.owner.baseId)) throw new Error("migration.generation_owner_mismatch");
  lease.assertOwned();
  if ((await currentPointer(lease))?.sha256 !== current.sha256) throw new Error("migration.current_pointer_changed");
  return Object.freeze({ generationRoot: root, vaultPath, masterKeyPath, tablesDirectory, creatorPath, owner: manifest.owner, pointerSha256: current.sha256 });
}

export async function commitActivation(lease: InstanceLease, input: { candidate: ActivationCandidate; proof: VerifiedMigrationCredential;
  quiescence: LegacyQuiescenceLease; publication?: PreparedActivationPublication; cutover?: PreparedWindowsCutover; expectedCurrent: string | null; signal: AbortSignal }) {
  lease.assertOwned(); input.signal.throwIfAborted();
  if (!(input.proof instanceof VerifiedMigrationCredential)) throw new Error("migration.verified_credential_required");
  input.proof.assertFresh();
  if (!(input.quiescence instanceof LegacyQuiescenceLease)) throw new Error("migration.legacy_quiescence_unavailable");
  if (!(input.publication instanceof PreparedActivationPublication)) throw new Error("migration.publication_authority_required");
  if (!(input.cutover instanceof PreparedWindowsCutover)) throw new Error("migration.installer_cutover_required");
  const prepared = input.publication.assertPrepared(lease, input.quiescence), candidate = prepared.candidate;
  if (!samePath(input.quiescence.serviceDirectory, candidate.legacyServiceDirectory)) throw new Error("migration.legacy_source_owner_mismatch");
  if (!isDeepStrictEqual(candidate, input.candidate) || candidate.expectedCurrent !== input.expectedCurrent || (await currentPointer(lease))?.sha256 !== (input.expectedCurrent ?? undefined)) throw new Error("migration.current_pointer_changed");
  if (input.proof.sourceFingerprint !== candidate.selected.fingerprint || input.proof.user.id !== candidate.owner.accountId || input.proof.metadata.access_user_id !== candidate.owner.accountId) throw new Error("migration.verified_owner_mismatch");
  await input.cutover.verifyForCommit(lease, input.quiescence, candidate.sourcePaths);
  input.signal.throwIfAborted(); input.proof.assertFresh(); input.publication.assertPrepared(lease, input.quiescence); lease.assertOwned();
  let published = false;
  await using failure = { async [Symbol.asyncDispose]() {
    if (!published) await writeMigrationJournal(candidate.directory, { state: "activation_failed", activated: false, candidateId: candidate.id });
  } };
  await input.publication.installVerifiedCredential(lease, input.quiescence, input.proof);
  await AccountDatabases.adoptPrepared(lease, input.proof, prepared.adoption);
  if (input.expectedCurrent !== null) {
    const previous = await currentPointer(lease);
    if (previous?.sha256 !== input.expectedCurrent) throw new Error("migration.current_pointer_changed");
    const path = join(candidate.directory, "previous-current.json");
    await using file = await open(path, "wx", 0o600); await verifyPrivateFile(path); await file.writeFile(previous.bytes); await file.sync();
  }
  const manifest = manifestSchema.parse({ format: 1, id: candidate.id, state: "data_generation_ready", dataDirectory: lease.directory,
    profileSha256: candidate.profileSha256, owner: candidate.owner, creatorDirectory: accountDirectoryName(candidate.owner), masterKeySha256: candidate.masterKeySha256,
    candidateSha256: input.publication.candidateSha256, previousPointer: input.expectedCurrent, installerCutoverDirectory: basename(input.cutover.directory),
    runtimeKind: input.publication.health.resource_identity.kind, productionReady: false, runtimeExecutableSha256: input.publication.health.resource_identity.executable_sha256,
    runtimeSourceSha256: input.publication.health.resource_identity.source_tree_sha256, runtimeLockSha256: input.publication.health.resource_identity.dependency_lock_sha256,
    webAssetsSha256: input.publication.health.web_assets_sha256, tableSyncSourceSha256: candidate.tableSync.source?.sha256 ?? null });
  const manifestBytes = Buffer.from(JSON.stringify(manifest));
  { const path = join(candidate.directory, "generation.json"); await using file = await open(path, "wx", 0o600); await verifyPrivateFile(path); await file.writeFile(manifestBytes); await file.sync(); }
  const pointer = pointerSchema.parse({ format: 1, generationId: candidate.id, manifestSha256: sha256(manifestBytes) });
  const temporary = join(lease.directory, `current-${randomUUID()}.json`);
  { await using file = await open(temporary, "wx", 0o600); await verifyPrivateFile(temporary); await file.writeFile(JSON.stringify(pointer)); await file.sync(); }
  input.signal.throwIfAborted(); input.proof.assertFresh(); input.quiescence.assertQuiescent(candidate.sourcePaths); lease.assertOwned();
  if ((await currentPointer(lease))?.sha256 !== (input.expectedCurrent ?? undefined) || (await profileIdentity(lease)).sha256 !== candidate.profileSha256) throw new Error("migration.current_pointer_changed");
  input.publication.assertBeforePublish(lease, input.quiescence);
  input.proof.assertFresh(); input.signal.throwIfAborted();
  // 同目录 rename 是唯一发布点；该点之后取消不再把已发布结果报告为失败。
  await rename(temporary, join(lease.directory, "current.json")); published = true;
  return Object.freeze({ activated: true as const, generationId: candidate.id, generationRoot: candidate.directory,
    pointerSha256: sha256(Buffer.from(JSON.stringify(pointer))), runtimeKind: input.publication.health.resource_identity.kind, productionReady: false as const });
}
