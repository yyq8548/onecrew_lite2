import { createHmac, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import { lstat, open, readFile, rename } from "node:fs/promises";
import { join } from "node:path";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import { verifyPrivateFile } from "../../storage/private-backup.ts";
import { readVaultMasterKey } from "../../storage/vault-reader.ts";
import { parseApplicationJson } from "../../contracts/application-json.ts";
import { browserBindingManifestSchema, verifyAccountOwnedChromeDirectory, type BrowserAccountDirectoryResolver, type TrustedExtensionContextProofVerifier } from "./binding.ts";
import { VerifiedManagedExtensionContext } from "./extension-challenge.ts";

type ProofInput = Parameters<TrustedExtensionContextProofVerifier["verify"]>[0];
function owner(context: AccountContext) { return { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId }; }
function sameOwner(left: ReturnType<typeof owner>, right: ReturnType<typeof owner>) { return left.accountId === right.accountId && left.tenantId === right.tenantId && left.baseId === right.baseId; }
function signingBytes(input: Omit<ProofInput, "proof">, identity: string): string {
  return JSON.stringify(["onecrew-managed-browser-v1", input.accountId, input.tenantId, input.baseId, input.deviceId, input.contextId, input.profileDirectory, identity]);
}

export class ManagedBrowserOwnership implements TrustedExtensionContextProofVerifier {
  readonly #accounts: AccountContextService;
  readonly #directories: BrowserAccountDirectoryResolver;
  readonly #pending = new Map<string, Promise<unknown>>();
  constructor(accounts: AccountContextService, directories: BrowserAccountDirectoryResolver) { this.#accounts = accounts; this.#directories = directories; }
  async verify(input: ProofInput): Promise<void> {
    const context = this.#accounts.current(), expected = owner(context);
    if (!sameOwner(input, expected) || input.deviceId !== context.principal.deviceId || !/^v1\.[A-Za-z0-9_-]{43}$/u.test(input.proof)) throw new Error("browser.extension_owner_mismatch");
    const account = await this.#directories.accountDirectory(context);
    if (!sameOwner(account.owner, expected)) throw new Error("browser.extension_owner_mismatch");
    const directory = await verifyAccountOwnedChromeDirectory(account.directory, input.profileDirectory);
    const key = await readVaultMasterKey(join(account.directory, "browser-binding.key"));
    try {
      const actual = Buffer.from(input.proof.slice(3), "base64url"), expectedProof = createHmac("sha256", key).update(signingBytes({ ...input, profileDirectory: directory.path }, directory.identity)).digest();
      if (actual.length !== expectedProof.length || !timingSafeEqual(actual, expectedProof)) throw new Error("browser.extension_proof_invalid");
      account.assertOwned(); this.#accounts.assertCurrent(context);
    } finally { key.fill(0); }
  }
  async register(context: AccountContext, verified: VerifiedManagedExtensionContext): Promise<Readonly<{ bindingId: string; contextId: string }>> {
    this.#accounts.assertCurrent(context);
    const id = context.principal.user.id, previous = this.#pending.get(id);
    const operation = (async () => { if (previous) await previous; return this.#register(context, verified); })();
    this.#pending.set(id, operation);
    try { return await operation; } finally { if (this.#pending.get(id) === operation) this.#pending.delete(id); }
  }
  async #register(context: AccountContext, verified: VerifiedManagedExtensionContext): Promise<Readonly<{ bindingId: string; contextId: string }>> {
    this.#accounts.assertCurrent(context); VerifiedManagedExtensionContext.prototype.assertCurrent.call(verified);
    const account = await this.#directories.accountDirectory(context), expected = owner(context);
    if (!sameOwner(account.owner, expected)) throw new Error("browser.extension_owner_mismatch");
    const directory = await verifyAccountOwnedChromeDirectory(account.directory, verified.profileDirectory);
    const manifestPath = join(account.directory, "browser-bindings.json"), keyPath = join(account.directory, "browser-binding.key");
    const original = await lstat(manifestPath).then(info => info, error => { if (error.code === "ENOENT") return undefined; throw error; });
    let bindings: { bindingId: string; contextId: string; deviceId: string; profileDirectory: string; extensionProof: string }[] = [];
    if (original) {
      await verifyPrivateFile(manifestPath); const bytes = await readFile(manifestPath);
      if (bytes.length > 256 * 1024) throw new Error("browser.binding_manifest_too_large");
      const current = browserBindingManifestSchema.parse(parseApplicationJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes)));
      if (current.format !== 2 || !sameOwner(current.owner, expected)) throw new Error("browser.binding_manifest_migration_required");
      bindings = current.bindings;
    }
    const keyExists = await lstat(keyPath).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
    if (!keyExists) {
      if (original) throw new Error("browser.binding_key_missing");
      const generated = randomBytes(32);
      try { await using file = await open(keyPath, "wx", 0o600); await file.writeFile(generated); await file.sync(); } finally { generated.fill(0); }
    }
    const key = await readVaultMasterKey(keyPath);
    try {
      const input = { ...expected, contextId: verified.contextId, deviceId: context.principal.deviceId, profileDirectory: directory.path };
      const existing = bindings.find(value => value.contextId === input.contextId);
      if (existing && existing.profileDirectory !== input.profileDirectory) throw new Error("browser.context_already_owned");
      const bindingId = existing?.bindingId ?? randomUUID(), extensionProof = `v1.${createHmac("sha256", key).update(signingBytes(input, directory.identity)).digest("base64url")}`;
      const updated = browserBindingManifestSchema.parse({ format: 2, owner: expected, bindings: [...bindings.filter(value => value.contextId !== input.contextId), { bindingId, contextId: input.contextId, deviceId: input.deviceId, profileDirectory: input.profileDirectory, extensionProof }] });
      const temporary = join(account.directory, `.browser-bindings-${randomUUID()}.json`);
      { await using file = await open(temporary, "wx", 0o600); await file.writeFile(JSON.stringify(updated)); await file.sync(); }
      await verifyPrivateFile(temporary);
      const latest = await lstat(manifestPath).then(info => info, error => { if (error.code === "ENOENT") return undefined; throw error; });
      if (original ? !latest || latest.dev !== original.dev || latest.ino !== original.ino || latest.mtimeMs !== original.mtimeMs || latest.size !== original.size : latest) throw new Error("browser.binding_manifest_changed");
      account.assertOwned(); this.#accounts.assertCurrent(context); verified.consume();
      await rename(temporary, manifestPath);
      return Object.freeze({ bindingId, contextId: input.contextId });
    } finally { key.fill(0); }
  }
}
