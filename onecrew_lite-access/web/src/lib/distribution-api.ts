import { z } from "zod";
import { AccessContractError } from "./access-api";
import { currentAccessLanguage } from "./request-locale";

export const cliReleaseSchema = z.object({status: z.enum(["published", "not_published", "unavailable"]), version: z.string().nullable(), targets: z.array(z.enum(["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc"]))}).strict();
const cliInstallLinkSchema = z.object({url: z.string().regex(/^\/cli\/install\/v1\.[A-Za-z0-9_-]{60,1536}$/), expiresAt: z.number().int().positive().safe(), prompt: z.string().min(1).max(4096)}).strict();
export type CliInstallLink = z.infer<typeof cliInstallLinkSchema>;
const applicationSchema = z.object({
  applicationsOpen: z.boolean(),
  cliRelease: cliReleaseSchema,
  settingRevision: z.number().int(),
  latestRelease: z.object({ version: z.string(), notes: z.string(), pubDate: z.string() }).strict().nullable(),
  application: z.object({ id: z.string(), displayName: z.string(), purpose: z.string(), company: z.string().nullable(), reviewReason: z.string().max(2000).nullable(), status: z.enum(["pending", "approved", "rejected"]), revision: z.number().int(), createdAt: z.number(), updatedAt: z.number() }).strict().nullable(),
  entitlement: z.object({ status: z.enum(["active", "revoked"]), revision: z.number().int() }).strict().nullable(),
  email: z.string().email(),
}).strict();
const linkSchema = z.object({ url: z.string().regex(/^\/d\/[A-Za-z0-9_-]{43}$/), expiresAt: z.number() }).strict();
const previewSchema = z.object({
  expiresAt: z.number(),
  release: z.object({ version: z.string(), notes: z.string(), pubDate: z.string() }).strict(),
  assets: z.array(z.object({ id: z.string(), filename: z.string(), platform: z.enum(["darwin", "windows", "linux"]), arch: z.enum(["aarch64", "x86_64"]), kind: z.enum(["installer", "updater"]), size: z.number().nonnegative(), downloadUrl: z.string() }).strict()),
}).strict();
export type DistributionApplication = z.infer<typeof applicationSchema>;
export type DistributionLink = z.infer<typeof linkSchema>;
export type DistributionPreview = z.infer<typeof previewSchema>;
export type ApplicationInput = { displayName: string; purpose: string; company?: string };
const errorCodeSchema = z.enum(["authentication_required", "email_verification_required", "application_exists", "applications_closed", "invalid_request", "idempotency_key_required", "link_not_found", "link_expired", "entitlement_revoked", "release_unavailable", "rate_limited", "idempotency_conflict"]);
export class DistributionApiError extends Error {
  constructor(readonly status: number, readonly code?: z.infer<typeof errorCodeSchema>, readonly retryAfter?: number) { super("Distribution request failed"); }
}
async function request<T>(path: string, schema: z.ZodType<T>, input?: unknown, idempotencyKey?: string): Promise<T> {
  const response = await fetch(path, { method: input === undefined ? "GET" : "POST", cache: "no-store", credentials: "include", headers: { Accept: "application/json", "Accept-Language": currentAccessLanguage(), ...(input !== undefined ? { "Content-Type": "application/json", "Idempotency-Key": idempotencyKey! } : {}) }, ...(input !== undefined ? { body: JSON.stringify(input) } : {}) });
  if (!response.ok) {
    const envelope: unknown = await response.json().catch(() => null);
    const parsed = z.object({ error: errorCodeSchema }).safeParse(envelope);
    throw new DistributionApiError(response.status, parsed.success ? parsed.data.error : undefined, response.headers.get("Retry-After") && /^\d+$/.test(response.headers.get("Retry-After")!) ? Number(response.headers.get("Retry-After")) : undefined);
  }
  const parsed = schema.safeParse(await response.json().catch(() => null));
  if (!parsed.success) throw new AccessContractError();
  return parsed.data;
}
export const getDistributionApplication = () => request("/api/distribution/application", applicationSchema);
export const submitDistributionApplication = (input: ApplicationInput, key: string) => request("/api/distribution/application", applicationSchema, input, key);
export const createDistributionLink = (key: string) => request("/api/distribution/download-links", linkSchema, {}, key);
export const createCliInstallLink = () => request("/api/cli/install-links", cliInstallLinkSchema, {}, crypto.randomUUID());
export const restoreCliInstallLink = () => request("/api/cli/install-links", z.object({link: cliInstallLinkSchema.nullable()}).strict());
export async function getDistributionPreview(token: string) {
  if (!/^[A-Za-z0-9_-]{43}$/.test(token)) throw new AccessContractError();
  const result = await request(`/api/distribution/download-links/${token}`, previewSchema);
  // The backend chooses the asset; the browser only accepts this protected same-origin path.
  for (const asset of result.assets) {
    if (asset.downloadUrl !== `/api/distribution/download-links/${token}/assets/${encodeURIComponent(asset.id)}`) throw new AccessContractError();
  }
  return result;
}
