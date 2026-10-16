import { collectSystemInfo, getDeviceLabel } from '../../system.js';
import { createHash, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import { AccessClient, AccessError, type Candidate, type Credential } from './client.js';
import { APP_VERSION } from '../../version.js';

export interface CredentialStore {
  load(): Promise<Credential | undefined>;
  save(value: Credential): Promise<void>;
  clear(): Promise<void>;
}

type Pending = {
  state: string;
  verifier: string;
  expires: number;
  candidate: Candidate;
  requestId: string;
};
type Verification =
  | { state: 'signed_in'; user: { id: string; email: string } }
  | { state: 'signed_out' | 'unavailable'; issue: string };
type VerificationCache = Verification & {
  credential: Credential;
  epoch: number;
  expiresAt: number;
};
export type AccessStatus = {
  state: 'signed_out' | 'authorizing' | 'signed_in' | 'unavailable';
  user: { id: string; email: string } | null;
  issue?: string;
};
export class AccessService {
  private credential?: Credential;
  private pending?: Pending;
  private loaded = false;
  private epoch = 0;
  private completing = false;
  private verificationCache?: VerificationCache;
  private verificationInFlight?: {
    credential: Credential;
    epoch: number;
    promise: Promise<Verification>;
  };
  private issue?: string;
  private queue = Promise.resolve();
  private reportedCredential?: string;
  private nextReportAt = 0;
  constructor(
    private readonly store: CredentialStore,
    private readonly client = new AccessClient(),
    private readonly platform = process.platform,
  ) {}
  private exclusive<T>(fn: () => Promise<T>): Promise<T> {
    const task = this.queue.then(fn);
    this.queue = task.then(
      () => undefined,
      () => undefined,
    );
    return task;
  }
  private async load() {
    await this.exclusive(async () => {
      if (!this.loaded) {
        try {
          this.credential = await this.store.load();
          this.loaded = true;
        } catch {
          throw new AccessError('access.credential_store_unavailable');
        }
      }
    });
  }
  private async verify(credential: Credential) {
    try {
      await this.client.validate(credential);
      const user = await this.client.profile(credential);
      return { state: 'signed_in', user } as const;
    } catch (error) {
      const issue = error instanceof AccessError ? error.code : 'access.remote_unavailable';
      return {
        state: ['access.unauthorized', 'access.forbidden', 'access.revoked'].includes(issue)
          ? 'signed_out'
          : 'unavailable',
        issue,
      } as const;
    }
  }
  async status(): Promise<AccessStatus> {
    try {
      await this.load();
    } catch {
      return { state: 'unavailable', user: null, issue: 'access.credential_store_unavailable' };
    }
    if (this.pending && this.pending.expires <= Date.now()) {
      this.pending = undefined;
      this.issue = 'access.expired';
    }
    if (this.pending || this.completing) return { state: 'authorizing', user: null };
    const credential = this.credential;
    if (!credential)
      return { state: 'signed_out', user: null, ...(this.issue ? { issue: this.issue } : {}) };
    const epoch = this.epoch;
    let verification: Verification;
    const cached = this.verificationCache;
    if (
      cached &&
      cached.credential === credential &&
      cached.epoch === epoch &&
      cached.expiresAt > Date.now()
    ) {
      verification = cached;
    } else {
      let inFlight = this.verificationInFlight;
      if (!inFlight || inFlight.credential !== credential || inFlight.epoch !== epoch) {
        const promise = this.verify(credential);
        inFlight = { credential, epoch, promise };
        this.verificationInFlight = inFlight;
        void promise.then((result) => {
          if (this.credential === credential && this.epoch === epoch) {
            this.verificationCache = {
              ...result,
              credential,
              epoch,
              expiresAt: Date.now() + (result.state === 'signed_in' ? 30000 : 3000),
            };
          }
          if (this.verificationInFlight === inFlight) this.verificationInFlight = undefined;
        });
      }
      verification = await inFlight.promise;
      if (this.credential !== credential || this.epoch !== epoch) return this.status();
    }
    if (verification.state !== 'signed_in')
      return { state: verification.state, user: null, issue: verification.issue };
    if (
      this.reportedCredential !== credential.installation.credential_id &&
      Date.now() >= this.nextReportAt
    ) {
      this.nextReportAt = Date.now() + 5 * 60000;
      try {
        const info = await collectSystemInfo();
        if (this.credential === credential) {
          await this.client.reportSystemInfo(credential, info);
          this.reportedCredential = credential.installation.credential_id;
        }
      } catch {
        // Diagnostics are best-effort and never turn a valid login into a failure.
      }
    }
    if (this.credential !== credential) return this.status();
    return { state: 'signed_in', user: verification.user };
  }
  async begin(port: number) {
    await this.load();
    const credential = this.credential;
    const status = credential ? await this.status() : undefined;
    return this.exclusive(async () => {
      if (this.credential !== credential) throw new AccessError('access.already_signed_in', 409);
      if (credential && status?.state !== 'signed_out')
        throw new AccessError(
          status?.state === 'unavailable'
            ? (status.issue ?? 'access.remote_unavailable')
            : 'access.already_signed_in',
          status?.state === 'unavailable' ? 503 : 409,
        );
      if (this.platform !== 'darwin' && this.platform !== 'win32')
        throw new AccessError('access.unsupported_platform', 422);
      if (!Number.isInteger(port) || port < 1024 || port > 65535)
        throw new AccessError('access.invalid_port', 422);
      if (this.completing) throw new AccessError('access.already_pending', 409);
      const state = randomBytes(32).toString('base64url');
      const verifier = randomBytes(32).toString('base64url');
      const credentialId = randomUUID();
      const candidate: Candidate = {
        installation_id: randomUUID(),
        credential_id: credentialId,
        bearer: `${credentialId}_${randomBytes(32).toString('base64url')}`,
        client_platform: this.platform === 'darwin' ? 'macos' : 'windows',
        app_version: APP_VERSION,
      };
      this.epoch++;
      this.issue = undefined;
      this.pending = {
        state,
        verifier,
        expires: Date.now() + 5 * 60000,
        candidate,
        requestId: randomBytes(24).toString('base64url'),
      };
      const url = new URL('/pairing/authorize', this.client.accessOrigin);
      url.search = new URLSearchParams({
        state,
        code_challenge: createHash('sha256').update(verifier).digest('base64url'),
        code_challenge_method: 'S256',
        callback_port: String(port),
        platform: candidate.client_platform,
        app_version: candidate.app_version,
        label: await getDeviceLabel(),
      }).toString();
      return { authorize_url: url.toString() };
    });
  }
  async complete(code: string, state: string) {
    const pending = this.pending;
    if (
      !pending ||
      pending.expires <= Date.now() ||
      !/^[A-Za-z0-9_-]{43}$/.test(state) ||
      !timingSafeEqual(Buffer.from(state), Buffer.from(pending.state))
    )
      throw new AccessError('access.invalid_callback', 400);
    if (!/^reacher_pairing_v1\.[A-Za-z0-9_-]{43}$/.test(code))
      throw new AccessError('access.invalid_callback', 400);
    this.pending = undefined;
    this.completing = true;
    const epoch = this.epoch;
    try {
      const credential = await this.client.enroll(
        pending.candidate,
        code,
        pending.verifier,
        pending.requestId,
      );
      await this.exclusive(async () => {
        if (this.epoch !== epoch) throw new AccessError('access.canceled', 409);
        try {
          await this.store.save(credential);
        } catch {
          throw new AccessError('access.credential_store_unavailable');
        }
        this.credential = credential;
        this.verificationCache = undefined;
        this.issue = undefined;
      });
    } catch (error) {
      if (this.epoch === epoch)
        this.issue = error instanceof AccessError ? error.code : 'access.remote_unavailable';
      throw error instanceof AccessError ? error : new AccessError('access.remote_unavailable');
    } finally {
      if (this.epoch === epoch) this.completing = false;
    }
  }
  async logout() {
    this.epoch++;
    this.pending = undefined;
    this.completing = false;
    await this.exclusive(async () => {
      try {
        if (this.credential || !this.loaded) await this.store.clear();
      } catch {
        throw new AccessError('access.credential_store_unavailable');
      }
      this.credential = undefined;
      this.loaded = true;
      this.verificationCache = undefined;
      this.reportedCredential = undefined;
      this.nextReportAt = 0;
      this.issue = undefined;
    });
    return this.status();
  }
}
