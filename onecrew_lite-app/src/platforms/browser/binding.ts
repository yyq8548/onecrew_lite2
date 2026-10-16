import { createHash } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { z } from "zod";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import type { VerifiedMailIdentityProvider } from "../../mail/gmail-access.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../../storage/private-backup.ts";
import { validateOpenCliProfile } from "../opencli/registry.ts";
import { OpenCliError } from "../opencli/errors.ts";
import type { VerifiedOpenCliBrowserBinding } from "../opencli/reader.ts";
import { PlatformVaultReader } from "./platform-vault.ts";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { managedChromeProfilesDirectory } from "./profile-directory.ts";

const ownerSchema = z.strictObject({ accountId: z.string().min(1), tenantId: z.string().min(1), baseId: z.string().min(1) });
const recordSchema = z.strictObject({ bindingId: z.uuid(), profileId: z.uuid(), contextId: z.string().min(1), deviceId: z.string().min(1), providerAccountId: z.string().min(1), vaultRevision: z.string().regex(/^[a-f\d]{64}$/u), profileDirectory: z.string().min(1), extensionProof: z.string().min(1) });
const dedicatedRecordSchema = recordSchema.omit({ profileId: true, providerAccountId: true, vaultRevision: true });
export const browserBindingManifestSchema = z.discriminatedUnion("format", [
  z.strictObject({ format: z.literal(1), owner: ownerSchema, bindings: z.array(recordSchema).max(100) }),
  z.strictObject({ format: z.literal(2), owner: ownerSchema, bindings: z.array(dedicatedRecordSchema).max(100) }),
]).refine(value => new Set(value.bindings.map(binding => binding.bindingId)).size === value.bindings.length && new Set(value.bindings.map(binding => binding.contextId)).size === value.bindings.length);
export interface VerifiedBrowserAccountDirectory { readonly directory: string; readonly owner: z.infer<typeof ownerSchema>; assertOwned(): void }
export interface BrowserAccountDirectoryResolver { accountDirectory(context: AccountContext): Promise<VerifiedBrowserAccountDirectory> }
export interface VerifiedOwnedBrowserProfile extends VerifiedOpenCliBrowserBinding { readonly directory: string }
// 仅专用扩展注册流程可签发此证明。请求字段、alias 和连通状态都不能替代目录归属证明。
export interface TrustedExtensionContextProofVerifier {
  verify(input: Readonly<{ contextId: string; deviceId: string; profileDirectory: string; proof: string; accountId: string; tenantId: string; baseId: string }>): Promise<void>;
}
function sameOwner(a: z.infer<typeof ownerSchema>, b: z.infer<typeof ownerSchema>) { return a.accountId === b.accountId && a.tenantId === b.tenantId && a.baseId === b.baseId; }
export async function verifyOwnedChromeDirectory(root: string, directory: string): Promise<Readonly<{ path: string; identity: string }>> {
  const absoluteRoot = resolve(root); const absolute = resolve(directory); const difference = relative(absoluteRoot, absolute);
  if (!isAbsolute(directory) || !difference || isAbsolute(difference) || difference === ".." || difference.startsWith(`..${sep}`)) throw new Error("Chrome profile directory is outside the app-owned profile root");
  await verifyPrivateDirectory(absoluteRoot); await verifyPrivateDirectory(absolute);
  const canonicalRoot = await realpath(absoluteRoot); const canonical = await realpath(absolute);
  const canonicalDifference = relative(canonicalRoot, canonical);
  if (!canonicalDifference || isAbsolute(canonicalDifference) || canonicalDifference === ".." || canonicalDifference.startsWith(`..${sep}`)) throw new Error("Chrome profile resolves outside the app-owned profile root");
  const info = await lstat(absolute, { bigint: true });
  if (!info.isDirectory() || info.isSymbolicLink()) throw new Error("Chrome profile directory is unsafe");
  return Object.freeze({ path: canonical, identity: `${info.dev}:${info.ino}` });
}

export async function verifyAccountOwnedChromeDirectory(accountDirectory: string, directory: string) {
  const legacyRoot = join(accountDirectory, "profiles"), difference = relative(resolve(legacyRoot), resolve(directory));
  const legacy = difference && !isAbsolute(difference) && difference !== ".." && !difference.startsWith(`..${sep}`);
  const root = legacy ? legacyRoot : managedChromeProfilesDirectory(accountDirectory);
  if (!legacy && process.platform === "win32") await verifyPrivateDirectory(dirname(root));
  return verifyOwnedChromeDirectory(root, directory);
}

export class AccountBrowserBindingResolver {
  readonly accounts: AccountContextService;
  readonly identities: VerifiedMailIdentityProvider;
  readonly directories: BrowserAccountDirectoryResolver;
  readonly vault: PlatformVaultReader | undefined;
  readonly extensionProofs: TrustedExtensionContextProofVerifier | undefined;
  constructor(accounts: AccountContextService, identities: VerifiedMailIdentityProvider, directories: BrowserAccountDirectoryResolver, vault?: PlatformVaultReader, extensionProofs?: TrustedExtensionContextProofVerifier) { this.accounts = accounts; this.identities = identities; this.directories = directories; this.vault = vault; this.extensionProofs = extensionProofs; }
  async listOwned(context: AccountContext): Promise<Readonly<{ binding_id: string; context_id: string }>[]> {
    this.accounts.assertCurrent(context); const account = await this.directories.accountDirectory(context), path = join(account.directory, "browser-bindings.json");
    const exists = await lstat(path).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
    if (!exists) { account.assertOwned(); this.accounts.assertCurrent(context); return []; }
    await verifyPrivateFile(path); const bytes = await readFile(path); if (bytes.length > 256 * 1024) throw new Error("Browser binding manifest is too large");
    const manifest = browserBindingManifestSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes), true)), result: { binding_id: string; context_id: string }[] = [];
    if (!sameOwner(account.owner, manifest.owner)) throw new Error("Browser binding manifest belongs to another account");
    for (const item of manifest.bindings) { const profile = await this.resolveOwnedProfile(context, item.bindingId); await profile.assertCurrent(); result.push({ binding_id: item.bindingId, context_id: profile.profile }); }
    if (!(await readFile(path)).equals(bytes)) throw new Error("Browser binding manifest changed");
    account.assertOwned(); this.accounts.assertCurrent(context); return result;
  }
  async resolveProfileRef(context: AccountContext, profileRef: string): Promise<VerifiedOpenCliBrowserBinding> {
    this.accounts.assertCurrent(context); validateOpenCliProfile(profileRef);
    if (!this.extensionProofs) throw new OpenCliError("opencli_profile_unavailable");
    const account = await this.directories.accountDirectory(context), owner = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
    if (!sameOwner(account.owner, owner)) throw new Error("Browser directory belongs to another account");
    const path = join(account.directory, "browser-bindings.json"); account.assertOwned(); this.accounts.assertCurrent(context);
    await verifyPrivateFile(path); const bytes = await readFile(path);
    if (bytes.length > 256 * 1024) throw new Error("Browser binding manifest is too large");
    const manifest = browserBindingManifestSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes), true));
    if (!sameOwner(manifest.owner, owner)) throw new Error("Browser binding manifest belongs to another account");
    const binding = manifest.bindings.find(value => value.contextId === profileRef);
    if (!binding) throw new OpenCliError("opencli_profile_unavailable");
    account.assertOwned(); this.accounts.assertCurrent(context);
    const resolved = await this.resolve(context, binding.bindingId);
    if (resolved.profile !== profileRef) throw new OpenCliError("opencli_profile_unavailable");
    account.assertOwned(); this.accounts.assertCurrent(context); return resolved;
  }
  async resolve(context: AccountContext, bindingId: string): Promise<VerifiedOpenCliBrowserBinding> {
    const owned = await this.resolveOwnedProfile(context, bindingId);
    return Object.freeze({ profile: owned.profile, signal: owned.signal, assertCurrent: () => owned.assertCurrent() });
  }
  async resolveOwnedProfile(context: AccountContext, bindingId: string): Promise<VerifiedOwnedBrowserProfile> {
    this.accounts.assertCurrent(context); z.uuid().parse(bindingId);
    if (!this.extensionProofs) throw new OpenCliError("opencli_profile_unavailable");
    const proofVerifier = this.extensionProofs;
    const owner = Object.freeze({ accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId });
    const identity = await this.identities.snapshot(owner);
    if (!sameOwner(identity.owner, owner) || identity.accessUserId !== owner.accountId) throw new Error("Browser identity belongs to another account");
    const account = await this.directories.accountDirectory(context);
    if (!sameOwner(account.owner, owner)) throw new Error("Browser directory belongs to another account");
    const path = join(account.directory, "browser-bindings.json");
    const read = async () => {
      account.assertOwned(); this.accounts.assertCurrent(context); await this.identities.assertCurrent(identity);
      await verifyPrivateFile(path); const bytes = await readFile(path);
      if (bytes.length > 256 * 1024) throw new Error("Browser binding manifest is too large");
      const manifest = browserBindingManifestSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes), true));
      if (!sameOwner(manifest.owner, owner)) throw new Error("Browser binding manifest belongs to another account");
      const binding = manifest.bindings.find(value => value.bindingId === bindingId);
      if (!binding || binding.deviceId !== context.principal.deviceId) throw new OpenCliError("opencli_profile_unavailable");
      validateOpenCliProfile(binding.contextId);
      if (manifest.format === 1) {
        if (!this.vault) throw new OpenCliError("opencli_profile_unavailable");
        const legacy = manifest.bindings.find(value => value.bindingId === bindingId);
        if (!legacy) throw new OpenCliError("opencli_profile_unavailable");
        const profile = await this.vault.profile(legacy.profileId);
        if (profile.platform === "telegram" || profile.provider_account_id !== legacy.providerAccountId || profile.revision !== legacy.vaultRevision || resolve(profile.browserProfileDirectory) !== resolve(legacy.profileDirectory)) throw new OpenCliError("opencli_profile_unavailable");
      }
      const directory = await verifyAccountOwnedChromeDirectory(account.directory, binding.profileDirectory);
      await proofVerifier.verify({ ...owner, contextId: binding.contextId, deviceId: binding.deviceId, profileDirectory: directory.path, proof: binding.extensionProof });
      account.assertOwned(); this.accounts.assertCurrent(context); await this.identities.assertCurrent(identity);
      return { contextId: binding.contextId, directory: directory.path, fingerprint: createHash("sha256").update(bytes).update(directory.identity).digest("hex") };
    };
    const initial = await read();
    return Object.freeze({ profile: initial.contextId, directory: initial.directory, signal: context.signal, assertCurrent: async () => { const current = await read(); if (current.contextId !== initial.contextId || current.fingerprint !== initial.fingerprint) throw new OpenCliError("opencli_profile_unavailable"); } });
  }
}
