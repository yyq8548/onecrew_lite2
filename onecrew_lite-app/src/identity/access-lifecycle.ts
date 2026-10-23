import { createHash, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import { inspect } from "node:util";
import type { InstanceLease } from "../service/instance-lock.ts";
import { UnifiedAccountService } from "./account-service.ts";
import { AccessVerificationError } from "./access-verification.ts";
import { ACCESS_AUTHORIZE_ORIGIN, enrollAccessInstallation, type AccessPairingCandidate } from "./access-enrollment.ts";
import { AccessLoopback } from "./access-loopback.ts";
import { AccessVaultStore } from "./access-vault-store.ts";
import type { AccountContext } from "./account-context.ts";

interface Pending { id: string; started: number; state: Buffer; verifier: Buffer; idempotency: Buffer; candidate: AccessPairingCandidate; listener: AccessLoopback; expected: ReturnType<AccessVaultStore["fingerprint"]> }
export interface AccessLifecycleOptions { vaultPath: string; masterKeyPath: string; appVersion: string; platform: "windows" | "macos"; deviceLabel: string }
export type AccessLifecycleStatus = { state: "signed_out" | "authorizing" | "signed_in" | "unavailable"; user: Readonly<{ id: string; email: string }> | null; flowId?: string; issue?: string; credentialId?: string; installationId?: string };

// 主服务唯一配对 owner。Library 只复用 accountOwner 的已验证用户和 tenant。
export class AccessLifecycle {
  readonly #lease: InstanceLease;
  readonly #options: AccessLifecycleOptions;
  readonly #accounts: UnifiedAccountService;
  readonly #store: AccessVaultStore;
  #pending: Pending | undefined;
  #inFlight: Promise<void> | undefined;
  #cancelAfter = false;
  #issue: string | undefined;
  #completedPairing: { id: string; context: AccountContext } | undefined;
  #queue: Promise<unknown> = Promise.resolve();
  constructor(lease: InstanceLease, options: AccessLifecycleOptions, accountOwner: UnifiedAccountService) {
    this.#lease = lease; this.#options = { ...options }; this.#accounts = accountOwner;
    this.#store = accountOwner.bindCredentialStore(new AccessVaultStore(lease, options.vaultPath, options.masterKeyPath));
  }
  #exclusive<T>(operation: () => Promise<T>): Promise<T> { const next = this.#queue.then(operation); this.#queue = next.then(() => undefined, () => undefined); return next; }
  #dispose(pending: Pending) { pending.state.fill(0); pending.verifier.fill(0); pending.idempotency.fill(0); pending.candidate.bearer.fill(0); }
  #authorization(pending: Pending): string {
    const url = new URL("/pairing/authorize", ACCESS_AUTHORIZE_ORIGIN);
    url.search = new URLSearchParams({ app_version: this.#options.appVersion, callback_port: String(pending.listener.port), code_challenge: createHash("sha256").update(pending.verifier).digest("base64url"), code_challenge_method: "S256", label: this.#options.deviceLabel.trim(), platform: this.#options.platform, state: pending.state.toString() }).toString();
    return url.toString();
  }
  reopen(): Promise<{ flowId: string; authorizeUrl: string }> {
    return this.#exclusive(async () => {
      this.#lease.assertOwned();
      const pending = this.#pending;
      if (!pending || this.#inFlight || performance.now() - pending.started >= 300_000) throw new AccessVerificationError("access.pairing_not_pending", 409);
      return { flowId: pending.id, authorizeUrl: this.#authorization(pending) };
    });
  }
  pairingEvidence() { this.#lease.assertOwned(); return { flowId: this.#pending?.id ?? this.#completedPairing?.id ?? null, callbackReceived: this.#inFlight !== undefined || this.#completedPairing !== undefined }; }
  completedPairingContext(flowId: string): AccountContext | undefined {
    this.#lease.assertOwned();
    if (this.#completedPairing?.id !== flowId) return undefined;
    this.#accounts.contexts.assertCurrent(this.#completedPairing.context);
    return this.#completedPairing.context;
  }
  async status(): Promise<AccessLifecycleStatus> {
    this.#lease.assertOwned();
    if (this.#pending || this.#inFlight) return { state: "authorizing", user: null, ...(this.#pending ? { flowId: this.#pending.id } : {}) };
    const local = await this.#store.load().then(value => value, () => undefined);
    if (!local) return { state: "unavailable", user: null, issue: "access.credential_store_unavailable" };
    using release = { [Symbol.dispose]() { if (local.state === "active") local.credential.dispose(); } };
    if (local.state !== "active") return { state: "signed_out", user: null, ...(this.#issue ? { issue: this.#issue } : {}) };
    const verified = await this.#accounts.refresh().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, issue: error instanceof AccessVerificationError ? error.code : "access.remote_unavailable" }));
    this.#lease.assertOwned();
    if (verified.ok) {
      const installation = await this.#accounts.installationIdentity();
      this.#accounts.contexts.assertCurrent(verified.value);
      return { state: "signed_in", user: verified.value.principal.user, ...installation };
    }
    return ["access.revoked", "access.unauthorized"].includes(verified.issue) ? { state: "signed_out", user: null, issue: verified.issue } : { state: "unavailable", user: null, issue: verified.issue };
  }
  begin(): Promise<{ flowId: string; authorizeUrl: string }> {
    return this.#exclusive(async () => {
      this.#lease.assertOwned();
      if (this.#pending || this.#inFlight) throw new AccessVerificationError("access.already_pending", 409);
      const { appVersion, platform } = this.#options, label = this.#options.deviceLabel.trim();
      if (!/^[A-Za-z0-9](?:[A-Za-z0-9]|[.+-](?=[A-Za-z0-9])){0,31}$/u.test(appVersion)) throw new AccessVerificationError("access.invalid_app_version", 422);
      if (!["windows", "macos"].includes(platform)) throw new AccessVerificationError("access.unsupported_platform", 422);
      if (!label.isWellFormed() || !label.length || label.length > 80 || /[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/u.test(label)) throw new AccessVerificationError("access.device_label_invalid", 422);
      const local = await this.#store.load();
      using releaseLocal = { [Symbol.dispose]() { if (local.state === "active") local.credential.dispose(); } };
      if (local.state === "active") throw new AccessVerificationError("access.admission_validation_required", 409);
      const expected = this.#store.fingerprint();
      const state = Buffer.from(randomBytes(32).toString("base64url")), verifier = Buffer.from(randomBytes(32).toString("base64url"));
      const credentialId = randomUUID(), installationId = randomUUID();
      if (credentialId === installationId) throw new Error("access.random_identity_collision");
      const candidate = { installationId, credentialId, bearer: Buffer.from(`${credentialId}_${randomBytes(32).toString("base64url")}`), platform, appVersion };
      let retained = false;
      using release = { [Symbol.dispose]() { if (!retained) { state.fill(0); verifier.fill(0); candidate.bearer.fill(0); } } };
      const listener = await AccessLoopback.start((code, returnedState) => this.#complete(code, returnedState));
      const pending = { id: randomUUID(), started: performance.now(), state, verifier, candidate, listener, expected, idempotency: Buffer.from(randomBytes(32).toString("base64url")) };
      this.#pending = pending; this.#completedPairing = undefined; this.#cancelAfter = false; this.#issue = undefined; retained = true;
      this.#accounts.invalidate();
      void listener.finished.then(outcome => {
        if (this.#pending === pending && !this.#inFlight) {
          this.#pending = undefined; this.#dispose(pending);
          if (outcome !== "succeeded") this.#issue = `access.pairing_${outcome}`;
        }
      });
      return { flowId: pending.id, authorizeUrl: this.#authorization(pending) };
    });
  }
  async #complete(code: string, state: string): Promise<void> {
    const pending = this.#pending;
    if (!pending || this.#inFlight || performance.now() - pending.started >= 300_000 || !/^[A-Za-z0-9_-]{43}$/u.test(state) ||
        Buffer.from(state, "base64url").toString("base64url") !== state || !timingSafeEqual(Buffer.from(state), pending.state) ||
        !/^reacher_pairing_v1\.[A-Za-z0-9_-]{43}$/u.test(code)) throw new AccessVerificationError("access.invalid_callback", 400);
    this.#lease.assertOwned();
    const run = async () => {
      const proof = await enrollAccessInstallation(pending.candidate, code, pending.verifier.toString(), pending.state.toString(), pending.idempotency.toString());
      using releaseProof = { [Symbol.dispose]() { proof.dispose(); } };
      this.#lease.assertOwned();
      await this.#store.save(proof, pending.expected);
      this.#accounts.invalidate();
      if (!this.#cancelAfter) {
        const context = await this.#accounts.refresh();
        if (!this.#cancelAfter) this.#completedPairing = { id: pending.id, context };
      }
    };
    const work = run(); this.#inFlight = work;
    await work.then(() => { this.#issue = undefined; }, () => { this.#issue = "access.enrollment_recovery_required"; throw new AccessVerificationError(this.#issue); }).finally(() => {
      if (this.#pending === pending) this.#pending = undefined;
      this.#dispose(pending); this.#inFlight = undefined;
    });
  }
  cancel(): Promise<void> {
    const hadRegistration = this.#inFlight !== undefined;
    this.#cancelAfter = true; this.#completedPairing = undefined; this.#accounts.invalidate();
    return this.#exclusive(async () => {
      const pending = this.#pending, registering = this.#inFlight;
      if (pending) await pending.listener.cancel();
      if (registering) await registering.then(() => undefined, () => undefined);
      if (hadRegistration) await this.#store.clear();
      if (this.#pending === pending) this.#pending = undefined;
      if (pending) this.#dispose(pending);
      this.#issue = "access.pairing_cancelled"; this.#lease.assertOwned();
    });
  }
  logout(): Promise<void> {
    this.#cancelAfter = true; this.#completedPairing = undefined; this.#accounts.invalidate();
    return this.#exclusive(async () => {
      const pending = this.#pending;
      if (pending) await pending.listener.cancel();
      await this.#inFlight?.then(() => undefined, () => undefined);
      this.#accounts.invalidate();
      await this.#store.clear();
      if (pending) this.#dispose(pending);
      this.#pending = undefined; this.#issue = undefined; this.#lease.assertOwned();
    });
  }
  async close(): Promise<void> { await this.cancel(); }
  toJSON(): never { throw new Error("Access lifecycle secrets cannot be serialized"); }
  [inspect.custom](): string { return "AccessLifecycle [redacted]"; }
}
