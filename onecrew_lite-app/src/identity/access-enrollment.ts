import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { AccessInstallationVerifier, AccessVerificationError } from "./access-verification.ts";
import { accessCredentialMetadataSchema, StoredAccessCredential } from "./legacy-access-credential.ts";

export const ACCESS_AUTHORIZE_ORIGIN = "https://access.onecrewai.io";
const API_ORIGIN = "https://api.onecrewai.io";
const fields = accessCredentialMetadataSchema.shape;
const installationSchema = z.object({ installation_id: fields.installation_id, credential_id: fields.credential_id, access_user_id: fields.access_user_id,
  access_device_id: z.string().regex(/^dev_[A-Za-z0-9_-]{24}$/u), access_pairing_id: z.string().regex(/^pair_[A-Za-z0-9_-]{24}$/u),
  source_instance_id: fields.source_instance_id, client_platform: fields.client_platform, app_version: fields.app_version,
  status: z.enum(["active", "revoked"]), revision: fields.revision, created_at: fields.created_at, updated_at: fields.updated_at,
  last_seen_at: fields.last_seen_at, revoked_at: fields.revoked_at }).strict();
export interface AccessPairingCandidate { installationId: string; credentialId: string; bearer: Buffer; platform: "windows" | "macos"; appVersion: string }
type HttpResult = { status: number; body: unknown } | { status: "transport" };
const fail = (code = "access.enrollment_recovery_required") => new AccessVerificationError(code);

async function request(path: string, init: RequestInit): Promise<HttpResult> {
  const response = await fetch(`${API_ORIGIN}${path}`, { ...init, redirect: "manual", signal: AbortSignal.timeout(20_000) })
    .then(value => value, () => undefined);
  if (!response) return { status: "transport" };
  if (response.status === 404 && init.method !== "POST") { await response.body?.cancel(); return { status: 404, body: null }; }
  if (!response.body) throw fail();
  const reader = response.body.getReader(), chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const part = await reader.read().then(value => value, () => { throw fail("access.remote_unavailable"); });
      if (part.done) break;
      size += part.value.length;
      if (size > 64 * 1024) throw fail();
      chunks.push(part.value);
    }
  } finally { await reader.cancel(); reader.releaseLock(); }
  const body = await Promise.resolve().then(() => parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks)), true))
    .then(value => value, () => { throw fail(); });
  return { status: response.status, body };
}

// 仅迁移原始已声明的协议；没有 OTP 接口，也不接受可配置的远端域名。
export async function enrollAccessInstallation(candidate: AccessPairingCandidate, pairingCode: string, verifier: string, state: string, idempotencyKey: string) {
  const secrets = [candidate.bearer.toString(), pairingCode, verifier, state];
  function rejectSecret(value: unknown) {
    if (typeof value === "string" && secrets.some(secret => value.includes(secret))) throw fail();
    if (Array.isArray(value)) value.forEach(rejectSecret);
    else if (value && typeof value === "object") Object.values(value).forEach(rejectSecret);
  }
  const headers = { authorization: `Bearer ${candidate.bearer.toString()}`, accept: "application/json", "cache-control": "no-store" };
  async function readback() {
    const result = await request(`/installations/${candidate.installationId}`, { headers });
    if (result.status === 404) return undefined;
    if (result.status !== 200 || !("body" in result)) throw fail(result.status === "transport" || typeof result.status === "number" && result.status >= 500 ? "access.remote_unavailable" : undefined);
    const parsed = z.object({ installation: installationSchema }).strict().safeParse(result.body);
    if (!parsed.success) throw fail();
    rejectSecret(parsed.data);
    const i = parsed.data.installation;
    if (i.installation_id !== candidate.installationId || i.credential_id !== candidate.credentialId || i.client_platform !== candidate.platform || i.app_version !== candidate.appVersion || i.status !== "active" || i.revoked_at != null) throw fail();
    return i;
  }
  const body = JSON.stringify({ pairing_code: pairingCode, code_verifier: verifier, installation_id: candidate.installationId,
    credential_id: candidate.credentialId, bearer: candidate.bearer.toString(), client_platform: candidate.platform, app_version: candidate.appVersion });
  if (Buffer.byteLength(body) > 4096) throw fail();
  const post = () => request("/installations", { method: "POST", headers: { "content-type": "application/json", "idempotency-key": idempotencyKey, "cache-control": "no-store" }, body });
  let posted = await post();
  if (posted.status !== 200 && posted.status !== 201) {
    const envelope = "body" in posted ? z.object({ error: z.string(), retry_safe: z.boolean().optional(), action: z.string().optional() }).strict().safeParse(posted.body) : undefined;
    const error = envelope?.success ? envelope.data : undefined;
    if (posted.status === 429 && ["enrollment_rate_limited", "enrollment_attempt_quota_exceeded"].includes(error?.error ?? "")) throw fail("access.enrollment_rate_limited");
    if ([403, 410, 422, 429].includes(Number(posted.status)) && ["pairing_consumed", "pairing_expired", "pairing_invalid", "pairing_rate_limited", "pairing_revoked"].includes(error?.error ?? "") || posted.status === 503 && error?.error === "pairing_unavailable" && error.retry_safe === false) throw fail("access.pairing_rejected");
    const retry = posted.status === 503 && error?.error === "enrollment_in_progress" && error.retry_safe === true && error.action === "retry_same_request_for_readback";
    const readOnly = posted.status === "transport" || typeof posted.status === "number" && (posted.status >= 500 || posted.status === 429);
    if (!retry && !readOnly) throw fail();
    const existing = await readback();
    if (!existing && retry) posted = await post();
    else if (!existing) throw fail();
    else posted = { status: 200, body: { installation: existing, replayed: true } };
  }
  if (!("body" in posted)) throw fail();
  const enrolled = z.object({ installation: installationSchema, replayed: z.boolean() }).strict().safeParse(posted.body);
  if (!enrolled.success || enrolled.data.replayed !== (posted.status === 200)) throw fail();
  rejectSecret(enrolled.data);
  const actual = await readback();
  if (!actual || !isDeepStrictEqual(actual, enrolled.data.installation)) throw fail();
  const probeResult = await request("/creator-sync/credential", { headers: { ...headers, "X-OneCrew-Creator-Bundle-Version": "3" } });
  if (probeResult.status !== 200 || !("body" in probeResult) || !probeResult.body || typeof probeResult.body !== "object" || Array.isArray(probeResult.body)) throw fail();
  rejectSecret(probeResult.body);
  const probe = z.object({ protocol: fields.protocol, protocol_version: fields.protocol_version, capabilities: fields.capabilities,
    accepted_bundle: fields.accepted_bundle, installation_id: fields.installation_id, credential_id: fields.credential_id,
    access_user_id: fields.access_user_id, access_device_id: fields.access_device_id, access_pairing_id: fields.access_pairing_id,
    source_instance_id: fields.source_instance_id }).strict().safeParse(probeResult.body);
  if (!probe.success) throw fail();
  const parsed = accessCredentialMetadataSchema.safeParse({ ...actual, ...probe.data });
  if (!parsed.success) throw fail();
  const metadata = parsed.data, accepted = metadata.accepted_bundle;
  for (const key of ["installation_id", "credential_id", "access_user_id", "access_device_id", "access_pairing_id", "source_instance_id"] as const) if (metadata[key] !== actual[key]) throw fail();
  if (metadata.protocol_version !== 1 || accepted.format !== "reacher.creator_bundle" || accepted.format_version > 3 ||
      accepted.max_snapshot_bytes !== 32 * 1024 * 1024 || accepted.max_chunk_bytes !== 1024 * 1024 || accepted.max_chunks !== 128 ||
      accepted.max_namespace_staged_bytes !== 100 * 1024 * 1024 || accepted.max_access_user_staged_bytes !== 400 * 1024 * 1024) throw fail();
  const credential = new StoredAccessCredential(metadata, createHash("sha256").update(JSON.stringify(metadata)).update(candidate.bearer).digest("hex"), candidate.bearer.toString());
  using release = { [Symbol.dispose]() { credential.dispose(); } };
  const proof = await new AccessInstallationVerifier().verifyForMigration(credential, new AbortController().signal);
  let retained = false;
  using releaseProof = { [Symbol.dispose]() { if (!retained) proof.dispose(); } };
  rejectSecret(proof.user);
  if (!isDeepStrictEqual({ ...proof.metadata, last_seen_at: metadata.last_seen_at }, metadata) ||
      metadata.last_seen_at != null && (proof.metadata.last_seen_at == null || proof.metadata.last_seen_at < metadata.last_seen_at)) throw fail();
  retained = true;
  return proof;
}
