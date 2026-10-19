import { inspect } from "node:util";
import { relative, resolve } from "node:path";
import type { GmailBulkRepositoryOwner } from "../mail/gmail-bulk-account-binding.ts";
import type { VerifiedMailIdentity, VerifiedMailIdentityProvider } from "../mail/gmail-access.ts";
import { AccountChangedError, AccountContextService, type AccountContext } from "./account-context.ts";
import { AccessInstallationVerifier, AccessNonActiveError, AccessVerificationError } from "./access-verification.ts";
import { AccessVaultStore } from "./access-vault-store.ts";
import { readStoredAccessCredential, type StoredAccessCredential } from "./legacy-access-credential.ts";

export interface VerifiedUserTenantResolver {
  resolve(user: Readonly<{ id: string; email: string }>): Promise<Readonly<{
    tenantId: string; baseId: string; user: Readonly<{ id: string; email: string }>;
  }>>;
}
interface IssuedIdentity { context: AccountContext; credential: StoredAccessCredential }

// 统一身份由同一 Vault 和远端安装验证产生；Library 仅解析该用户的 tenant。
export class UnifiedAccountService implements VerifiedMailIdentityProvider {
  readonly contexts = new AccountContextService();
  readonly #vaultPath: string;
  readonly #keyPath: string;
  readonly #tenants: VerifiedUserTenantResolver;
  readonly #verifier = new AccessInstallationVerifier(() => this.invalidate());
  #credentialStore: AccessVaultStore | undefined;
  readonly #issued = new WeakMap<VerifiedMailIdentity, IssuedIdentity>();
  #credential: StoredAccessCredential | undefined;
  #context: AccountContext | undefined;
  #verifiedAt = 0;
  #generation = 0;
  #controller = new AbortController();
  #refreshing: Promise<AccountContext> | undefined;

  constructor(vaultPath: string, masterKeyPath: string, tenants: VerifiedUserTenantResolver) {
    this.#vaultPath = vaultPath;
    this.#keyPath = masterKeyPath;
    this.#tenants = tenants;
  }

  // 仅允许主服务持有的实际私有 Vault owner；请求参数不能提供清理回调。
  bindCredentialStore(store: AccessVaultStore): AccessVaultStore {
    if (Object.getPrototypeOf(store) !== AccessVaultStore.prototype || relative(resolve(store.vaultPath), resolve(this.#vaultPath)) !== "" ||
        relative(resolve(store.masterKeyPath), resolve(this.#keyPath)) !== "") throw new Error("Access credential store owner mismatch");
    store.lease.assertOwned();
    if (this.#credentialStore) {
      if (this.#credentialStore.lease !== store.lease) throw new Error("Access credential store lease changed");
      return this.#credentialStore;
    }
    this.#credentialStore = store;
    return store;
  }

  // 清除运行中的授权上下文；此方法不代表已删除持久凭据或完成用户登出。
  invalidate(): void {
    this.#generation++;
    this.#controller.abort(new AccountChangedError());
    this.#controller = new AbortController();
    this.#discardCurrent();
  }

  #discardCurrent(): void {
    this.contexts.logout();
    this.#credential?.dispose();
    this.#credential = undefined;
    this.#context = undefined;
    this.#verifiedAt = 0;
  }

  refresh(): Promise<AccountContext> {
    if (this.#refreshing) return this.#refreshing;
    const pending = this.#refresh().finally(() => {
      if (this.#refreshing === pending) this.#refreshing = undefined;
    });
    this.#refreshing = pending;
    return pending;
  }

  async installationIdentity(): Promise<Readonly<{ credentialId: string; installationId: string }>> {
    const context = await this.refresh(), credential = this.#credential;
    this.contexts.assertCurrent(context);
    if (!credential || this.#context !== context || credential.metadata.access_user_id !== context.principal.user.id) throw new AccountChangedError();
    const result = Object.freeze({ credentialId: credential.metadata.credential_id, installationId: credential.metadata.installation_id });
    this.contexts.assertCurrent(context);
    return result;
  }

  async #refresh(): Promise<AccountContext> {
    const generation = this.#generation;
    let candidate: StoredAccessCredential | undefined;
    let retained = false;
    try {
      const local = await readStoredAccessCredential(this.#vaultPath, this.#keyPath);
      if (local.state !== "active") throw new AccessVerificationError("access.login_required", 401);
      candidate = local.credential;
      if (generation !== this.#generation) throw new AccountChangedError();
      if (this.#credential && this.#credential.fingerprint !== candidate.fingerprint) this.#discardCurrent();
      const continuingContext = this.#context && this.#credential?.fingerprint === candidate.fingerprint ? this.#context : undefined;
      if (this.#context && this.#credential?.fingerprint === candidate.fingerprint && performance.now() - this.#verifiedAt < 30_000) {
        this.contexts.assertCurrent(this.#context);
        return this.#context;
      }
      const proof = await this.#verifier.verifyForMigration(candidate, this.#controller.signal);
      using releaseProof = { [Symbol.dispose]() { proof.dispose(); } };
      const user = proof.user;
      if ((candidate.metadata.last_seen_at ?? null) !== (proof.metadata.last_seen_at ?? null)) {
        if (!this.#credentialStore) throw new AccessVerificationError("access.credential_store_unavailable");
        await this.#credentialStore.refresh(proof);
        const refreshed = await this.#credentialStore.load();
        if (refreshed.state !== "active") throw new AccountChangedError();
        candidate.dispose(); candidate = refreshed.credential;
      }
      const tenant = await this.#tenants.resolve(user);
      proof.assertFresh();
      if (tenant.user.id !== user.id || tenant.user.email !== user.email) throw new AccountChangedError();
      const latest = await readStoredAccessCredential(this.#vaultPath, this.#keyPath);
      try {
        if (generation !== this.#generation || latest.state !== "active" || latest.credential.fingerprint !== candidate.fingerprint) {
          throw new AccountChangedError();
        }
      } finally {
        if (latest.state === "active") latest.credential.dispose();
      }
      if (this.#context && this.#context === continuingContext &&
          this.#context.principal.user.id === user.id && this.#context.principal.user.email === user.email &&
          this.#context.principal.tenantId === tenant.tenantId && this.#context.principal.baseId === tenant.baseId &&
          this.#context.principal.deviceId === candidate.metadata.access_device_id) {
        this.contexts.assertCurrent(this.#context);
        // 同一凭据经过在线核实并更新 last_seen_at 后，保留已授权页面与专用浏览器的身份周期。
        if (this.#credential?.fingerprint !== candidate.fingerprint) {
          this.#credential?.dispose();
          this.#credential = candidate;
          retained = true;
        }
      } else {
        this.#credential?.dispose();
        this.#credential = candidate;
        retained = true;
        this.#context = this.contexts.activate({ user: { ...user }, tenantId: tenant.tenantId,
          baseId: tenant.baseId, deviceId: candidate.metadata.access_device_id, source: "access" });
      }
      this.#verifiedAt = performance.now();
      proof.assertFresh();
      return this.#context;
    } catch (error) {
      // 临时远端失败仍拒绝本次准入，但不撤销已验证的页面和运行中任务。
      if (!(error instanceof AccessVerificationError && error.code === "access.remote_unavailable")) this.invalidate();
      if (error instanceof AccessNonActiveError) {
        if (!this.#credentialStore) throw new AccessVerificationError("access.credential_cleanup_unavailable");
        await this.#credentialStore.clear(error.sourceFingerprint);
      }
      throw error;
    } finally {
      if (!retained) candidate?.dispose();
    }
  }

  async snapshot(owner: GmailBulkRepositoryOwner): Promise<VerifiedMailIdentity> {
    const context = await this.refresh();
    const credential = this.#credential;
    if (!credential || owner.accountId !== context.principal.user.id || owner.tenantId !== context.principal.tenantId || owner.baseId !== context.principal.baseId) {
      throw new AccountChangedError();
    }
    const identity = Object.freeze({
      owner: Object.freeze({ ...owner }), accessUserId: context.principal.user.id,
      exposeBearer: () => { this.contexts.assertCurrent(context); return credential.exposeBearer(); },
      toJSON: (): never => { throw new Error("Verified Access identities cannot be serialized"); },
      [inspect.custom]: () => "[VerifiedMailIdentity: redacted]",
    });
    this.#issued.set(identity, { context, credential });
    return identity;
  }

  async assertCurrent(identity: VerifiedMailIdentity): Promise<void> {
    const issued = this.#issued.get(identity);
    if (!issued || issued.credential !== this.#credential) throw new AccountChangedError();
    this.contexts.assertCurrent(issued.context);
    try {
      const latest = await readStoredAccessCredential(this.#vaultPath, this.#keyPath);
      try {
        this.contexts.assertCurrent(issued.context);
        if (latest.state !== "active" || latest.credential.fingerprint !== issued.credential.fingerprint) throw new AccountChangedError();
      } finally {
        if (latest.state === "active") latest.credential.dispose();
      }
    } catch (error) {
      if (issued.context === this.#context && issued.credential === this.#credential) this.invalidate();
      throw error;
    }
  }
}
