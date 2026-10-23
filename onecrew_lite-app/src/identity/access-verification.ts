import { z } from "zod";
import { inspect, isDeepStrictEqual } from "node:util";
import { setTimeout as delay } from "node:timers/promises";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { accessCredentialMetadataSchema, type StoredAccessMetadata } from "./legacy-access-credential.ts";

const fields = accessCredentialMetadataSchema.shape;
const identityFields = {
  installation_id: fields.installation_id, credential_id: fields.credential_id,
  access_user_id: fields.access_user_id, access_device_id: fields.access_device_id,
  access_pairing_id: fields.access_pairing_id, source_instance_id: fields.source_instance_id,
};
const installationSchema = z.object({ installation: z.object({
  ...identityFields, client_platform: fields.client_platform, app_version: z.string(),
  status: z.enum(["active", "revoked"]), revision: fields.revision,
  created_at: fields.created_at, updated_at: fields.updated_at,
  last_seen_at: z.number().finite().nullable(), revoked_at: z.number().finite().nullable(),
}).strict() }).strict();
const probeSchema = z.object({
  ...identityFields, protocol: fields.protocol, protocol_version: fields.protocol_version,
  capabilities: fields.capabilities, accepted_bundle: fields.accepted_bundle,
}).strict();
const identityKeys = ["installation_id", "credential_id", "access_user_id", "access_device_id", "access_pairing_id", "source_instance_id"] as const;
export interface AccessInstallationCredential {
  readonly metadata: Readonly<Pick<StoredAccessMetadata, typeof identityKeys[number] | "status">>;
  readonly fingerprint: string;
  exposeBearer(): string;
}
const migrationIssuer = Symbol("verified-access-migration");

// 迁移证明只由真实远端验证产生；不会把本地有效凭据称为已验证账号。
export class VerifiedMigrationCredential {
  readonly metadata: Readonly<StoredAccessMetadata>;
  readonly user: Readonly<{ id: string; email: string }>;
  readonly sourceFingerprint: string;
  readonly #verifiedAt = performance.now();
  readonly #bearer: Buffer;
  #disposed = false;
  constructor(issuer: symbol, metadata: StoredAccessMetadata, user: Readonly<{ id: string; email: string }>, credential: AccessInstallationCredential) {
    if (issuer !== migrationIssuer) throw new Error("Migration credentials require online verification");
    Object.freeze(metadata.capabilities);
    Object.freeze(metadata.accepted_bundle);
    this.metadata = Object.freeze(metadata);
    this.user = Object.freeze({ ...user });
    this.sourceFingerprint = credential.fingerprint;
    this.#bearer = Buffer.from(credential.exposeBearer());
    Object.freeze(this);
  }
  assertFresh(): void {
    if (this.#disposed || performance.now() - this.#verifiedAt > 30_000) throw new AccessVerificationError("access.migration_verification_expired", 409);
  }
  exposeBearer(): string { this.assertFresh(); return this.#bearer.toString("utf8"); }
  dispose(): void { this.#bearer.fill(0); this.#disposed = true; }
  toJSON(): never { throw new Error("Verified migration credentials cannot be serialized"); }
  [inspect.custom](): string { return "VerifiedMigrationCredential [redacted]"; }
}

export class AccessVerificationError extends Error {
  readonly code: string;
  readonly status: number;
  readonly details: Readonly<{ endpoint: string; http_status?: number; elapsed_ms?: number; transport_code?: string }> | undefined;
  constructor(code: string, status = 503, details?: AccessVerificationError["details"]) {
    super(code);
    this.code = code;
    this.status = status;
    this.details = details;
  }
}

export class AccessNonActiveError extends AccessVerificationError {
  readonly sourceFingerprint: string;
  readonly remoteState: "revoked" | "unauthorized";
  constructor(sourceFingerprint: string, remoteState: "revoked" | "unauthorized") {
    super(`access.${remoteState}`, 401);
    this.sourceFingerprint = sourceFingerprint; this.remoteState = remoteState;
  }
}

export function classifyAccessHttpFailure(status: number, installationRead: boolean): "non_active" | "unavailable" | "protocol" {
  if (status === 401 || status === 404 && installationRead) return "non_active";
  return status >= 500 ? "unavailable" : "protocol";
}

// 只重试只读验证的临时网络故障；登录失效、协议错误和取消立即停止。
export function accessVerificationRetryDelay(error: unknown, attempt: number, signal: AbortSignal): number | null {
  return !signal.aborted && attempt === 0 && error instanceof AccessVerificationError && error.code === "access.remote_unavailable" ? 500 : null;
}

// 只验证现有安装身份；不注册设备、不上传系统信息、不启动 OAuth。
export class AccessInstallationVerifier {
  readonly #onNonActive: (() => void) | undefined;
  constructor(onNonActive?: () => void) { this.#onNonActive = onNonActive; }
  async #response(path: string, credential: AccessInstallationCredential, signal: AbortSignal): Promise<{ status: number; value?: unknown }> {
    signal.throwIfAborted();
    const started = performance.now(), endpoint = path.startsWith("/installations/") ? "/installations/{id}" : path;
    const unavailable = (error: unknown) => {
      const cause = error instanceof Error ? error.cause : undefined;
      const code = cause && typeof cause === "object" && "code" in cause ? cause.code : undefined;
      return new AccessVerificationError("access.remote_unavailable", 503, { endpoint, elapsed_ms: Math.round(performance.now() - started),
        ...(typeof code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/u.test(code) ? { transport_code: code } :
          error instanceof Error && ["AbortError", "TimeoutError", "TypeError"].includes(error.name) ? { transport_code: error.name } : {}) });
    };
    const response = await fetch(`https://api.onecrewai.io${path}`, {
      headers: { authorization: `Bearer ${credential.exposeBearer()}`, accept: "application/json", "cache-control": "no-store",
        ...(path === "/creator-sync/credential" ? { "X-OneCrew-Creator-Bundle-Version": "3" } : {}) },
      redirect: "manual", signal: AbortSignal.any([signal, AbortSignal.timeout(10_000)]),
    }).then(value => value, error => { throw unavailable(error); });
    if (!response.ok) {
      await response.body?.cancel();
      return { status: response.status };
    }
    if (!response.body) throw new AccessVerificationError("access.invalid_response");
    const reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    try {
      for (;;) {
        const part = await reader.read().then(value => value, error => { throw unavailable(error); });
        if (part.done) break;
        size += part.value.length;
        if (size > 64 * 1024) throw new AccessVerificationError("access.invalid_response");
        chunks.push(part.value);
      }
    } finally {
      await reader.cancel();
      reader.releaseLock();
    }
    const value = await Promise.resolve().then(() => parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks)), true))
      .then(value => value, () => { throw new AccessVerificationError("access.invalid_response"); });
    return { status: response.status, value };
  }
  async #nonActive(credential: AccessInstallationCredential, known?: "revoked"): Promise<never> {
    this.#onNonActive?.();
    let state: "revoked" | "unauthorized" = known ?? "unauthorized";
    if (!known) {
      const observed = await this.#response(`/installations/${credential.metadata.installation_id}`, credential, new AbortController().signal).then(value => value, () => undefined);
      const parsed = observed?.status === 200 ? installationSchema.safeParse(observed.value) : undefined;
      if (parsed?.success && parsed.data.installation.installation_id === credential.metadata.installation_id && parsed.data.installation.status === "revoked") state = "revoked";
    }
    // 401 已证明该凭据非活跃，额外回读失败不恢复其准入权限。
    throw new AccessNonActiveError(credential.fingerprint, state);
  }
  async #requestOnce(path: string, credential: AccessInstallationCredential, signal: AbortSignal): Promise<unknown> {
    const response = await this.#response(path, credential, signal);
    if (response.status === 200) return response.value;
    const failure = classifyAccessHttpFailure(response.status, path.startsWith("/installations/"));
    if (failure === "non_active") return this.#nonActive(credential);
    throw new AccessVerificationError(failure === "unavailable" ? "access.remote_unavailable" : "access.protocol_mismatch", 503,
      { endpoint: path.startsWith("/installations/") ? "/installations/{id}" : path, http_status: response.status });
  }

  async #request(path: string, credential: AccessInstallationCredential, signal: AbortSignal): Promise<unknown> {
    for (let attempt = 0; ; attempt++) {
      signal.throwIfAborted();
      const outcome = await this.#requestOnce(path, credential, signal).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
      if (outcome.ok) return outcome.value;
      signal.throwIfAborted();
      const wait = accessVerificationRetryDelay(outcome.error, attempt, signal);
      if (wait === null) throw outcome.error;
      await delay(wait, undefined, { signal });
    }
  }

  async #verify(credential: AccessInstallationCredential, signal: AbortSignal) {
    signal.throwIfAborted();
    if (credential.metadata.status !== "active") throw new AccessVerificationError("access.revoked", 401);
    if (JSON.stringify(credential.metadata).includes(credential.exposeBearer())) throw new AccessVerificationError("access.invalid_response");
    const [installationResponse, probeResponse, profileResponse] = await Promise.all([
      this.#request(`/installations/${credential.metadata.installation_id}`, credential, signal),
      this.#request("/creator-sync/credential", credential, signal),
      this.#request("/me", credential, signal),
    ]);
    const installation = installationSchema.safeParse(installationResponse);
    if (!installation.success) throw new AccessVerificationError("access.invalid_response");
    if (identityKeys.some(key => installation.data.installation[key] !== credential.metadata[key])) throw new AccessVerificationError("access.invalid_response");
    if (installation.data.installation.status !== "active") return this.#nonActive(credential, "revoked");
    const probe = probeSchema.safeParse(probeResponse);
    if (!probe.success || identityKeys.some(key => probe.data[key] !== credential.metadata[key])) throw new AccessVerificationError("access.invalid_response");
    const profile = z.object({ id: z.string().min(1), email: z.email() }).strict().safeParse(profileResponse);
    if (!profile.success || profile.data.id !== credential.metadata.access_user_id) throw new AccessVerificationError("access.invalid_response");
    const bearer = credential.exposeBearer();
    if (profile.data.id.includes(bearer) || profile.data.email.includes(bearer)) throw new AccessVerificationError("access.invalid_response");
    const metadata = accessCredentialMetadataSchema.safeParse({ ...installation.data.installation, ...probe.data });
    if (!metadata.success) throw new AccessVerificationError("access.invalid_response");
    const stored = accessCredentialMetadataSchema.safeParse(credential.metadata);
    if (stored.success && (!isDeepStrictEqual({ ...metadata.data, last_seen_at: stored.data.last_seen_at ?? null, revoked_at: metadata.data.revoked_at ?? null,
      accepted_bundle: { ...metadata.data.accepted_bundle, format_version: stored.data.accepted_bundle.format_version } },
      { ...stored.data, last_seen_at: stored.data.last_seen_at ?? null, revoked_at: stored.data.revoked_at ?? null }) ||
      stored.data.last_seen_at != null && (metadata.data.last_seen_at == null || metadata.data.last_seen_at < stored.data.last_seen_at))) throw new AccessVerificationError("access.protocol_mismatch");
    return { user: Object.freeze(profile.data), metadata: metadata.data };
  }
  async verify(credential: AccessInstallationCredential, signal: AbortSignal): Promise<Readonly<{ id: string; email: string }>> {
    return (await this.#verify(credential, signal)).user;
  }
  async verifyForMigration(credential: AccessInstallationCredential, signal: AbortSignal): Promise<VerifiedMigrationCredential> {
    const verified = await this.#verify(credential, signal);
    signal.throwIfAborted();
    return new VerifiedMigrationCredential(migrationIssuer, verified.metadata, verified.user, credential);
  }
}
