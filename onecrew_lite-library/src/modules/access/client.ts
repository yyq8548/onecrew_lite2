import type { collectSystemInfo } from '../../system.js';
import { z } from 'zod';

// Contract checked against access/docs/contracts/desktop-pairing-admission-v1.md
// and sync-worker/src/contracts/{installations,credential-probe}.ts.
const installationSchema = z
  .object({
    installation_id: z.uuid(),
    credential_id: z.uuid(),
    access_user_id: z.string().min(1).max(128),
    access_device_id: z.string().regex(/^dev_[A-Za-z0-9_-]{24}$/),
    access_pairing_id: z.string().regex(/^pair_[A-Za-z0-9_-]{24}$/),
    source_instance_id: z.uuid(),
    client_platform: z.enum(['macos', 'windows']),
    app_version: z.string(),
    status: z.enum(['active', 'revoked']),
    revision: z.number().int().positive(),
    created_at: z.number().int().nonnegative(),
    updated_at: z.number().int().nonnegative(),
    last_seen_at: z.number().nullable(),
    revoked_at: z.number().nullable(),
  })
  .strict();
export const credentialSchema = z
  .object({
    bearer: z.string().min(80).max(165),
    installation: installationSchema,
  })
  .strict();
export type Credential = z.infer<typeof credentialSchema>;
export type Candidate = {
  installation_id: string;
  credential_id: string;
  bearer: string;
  client_platform: 'macos' | 'windows';
  app_version: string;
};
export class AccessError extends Error {
  constructor(
    public readonly code: string,
    public readonly status = 503,
  ) {
    super(code);
  }
}
export class AccessClient {
  constructor(
    private readonly syncOrigin = 'https://api.onecrewai.io',
    readonly accessOrigin = 'https://access.onecrewai.io',
  ) {}
  private async request(path: string, init: RequestInit) {
    let response: Response;
    try {
      response = await fetch(`${this.syncOrigin}${path}`, {
        ...init,
        redirect: 'error',
        signal: AbortSignal.timeout(10000),
      });
    } catch {
      throw new AccessError('access.remote_unavailable');
    }
    if (!response.ok)
      throw new AccessError(
        response.status === 401
          ? 'access.unauthorized'
          : response.status === 403
            ? 'access.forbidden'
            : 'access.remote_rejected',
        response.status,
      );
    // Responses never go into logs or error messages.
    const reader = response.body?.getReader();
    if (!reader) throw new AccessError('access.invalid_response');
    const chunks: Uint8Array[] = [];
    let size = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > 64 * 1024) {
        await reader.cancel();
        throw new AccessError('access.invalid_response');
      }
      chunks.push(value);
    }
    try {
      return JSON.parse(Buffer.concat(chunks).toString('utf8')) as unknown;
    } catch {
      throw new AccessError('access.invalid_response');
    }
  }
  async enroll(
    candidate: Candidate,
    code: string,
    verifier: string,
    requestId: string,
  ): Promise<Credential> {
    // A one-time grant is submitted once; ambiguous outcomes require a new authorization.
    const response = await this.request('/installations', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Idempotency-Key': requestId,
      },
      body: JSON.stringify({ ...candidate, pairing_code: code, code_verifier: verifier }),
    });
    const result = z
      .object({ installation: installationSchema, replayed: z.boolean() })
      .strict()
      .safeParse(response);
    if (!result.success) throw new AccessError('access.invalid_response');
    const i = result.data.installation;
    if (
      i.installation_id !== candidate.installation_id ||
      i.credential_id !== candidate.credential_id ||
      i.client_platform !== candidate.client_platform ||
      i.app_version !== candidate.app_version ||
      i.status !== 'active'
    )
      throw new AccessError('access.invalid_response');
    const credential = { bearer: candidate.bearer, installation: i };
    await this.validate(credential);
    return credential;
  }
  async reportSystemInfo(
    credential: Credential,
    info: Awaited<ReturnType<typeof collectSystemInfo>>,
  ) {
    const result = z
      .object({ ok: z.literal(true) })
      .strict()
      .safeParse(
        await this.request('/device-info', {
          method: 'PUT',
          headers: {
            Authorization: `Bearer ${credential.bearer}`,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify(info),
        }),
      );
    if (!result.success) throw new AccessError('access.invalid_response');
  }
  async profile(credential: Credential) {
    const result = z
      .object({ id: z.string().min(1), email: z.email() })
      .strict()
      .safeParse(
        await this.request('/me', { headers: { Authorization: `Bearer ${credential.bearer}` } }),
      );
    if (!result.success || result.data.id !== credential.installation.access_user_id)
      throw new AccessError('access.invalid_response');
    return result.data;
  }
  async validate(credential: Credential) {
    const headers = { Authorization: `Bearer ${credential.bearer}` };
    const response = await this.request(
      `/installations/${credential.installation.installation_id}`,
      { headers },
    );
    const result = z.object({ installation: installationSchema }).strict().safeParse(response);
    if (!result.success) throw new AccessError('access.invalid_response');
    const expected = credential.installation;
    const actual = result.data.installation;
    if (actual.status !== 'active') throw new AccessError('access.revoked', 401);
    const keys = [
      'installation_id',
      'credential_id',
      'access_user_id',
      'access_device_id',
      'access_pairing_id',
      'source_instance_id',
    ] as const;
    if (keys.some((key) => actual[key] !== expected[key]))
      throw new AccessError('access.invalid_response');
    const probe = z
      .object({
        protocol: z.literal('reacher.creator_sync'),
        protocol_version: z.number().int().positive(),
        installation_id: z.string(),
        credential_id: z.string(),
        access_user_id: z.string(),
        access_device_id: z.string(),
        access_pairing_id: z.string(),
        source_instance_id: z.string(),
        capabilities: z.tuple([z.literal('creator_snapshots:write_chunked_quarantine')]),
        accepted_bundle: z
          .object({
            format: z.string(),
            format_version: z.number().int().positive(),
            profile: z.literal('portable_public'),
            max_snapshot_bytes: z.number().positive(),
            max_chunk_bytes: z.number().positive(),
            max_chunks: z.number().positive(),
            max_namespace_staged_bytes: z.number().positive(),
            max_access_user_staged_bytes: z.number().positive(),
          })
          .strict(),
      })
      .strict()
      .safeParse(await this.request('/creator-sync/credential', { headers }));
    if (!probe.success || keys.some((key) => probe.data[key] !== expected[key]))
      throw new AccessError('access.invalid_response');
  }
}
