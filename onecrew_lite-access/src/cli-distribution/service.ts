import { DEFAULT_EMAIL_LOCALE } from "../auth/email-locale";
import { installPrompt, type InstallLocale } from "./prompts";
import type { Bindings } from "../types/bindings";
import { DistributionError } from "../distribution/contracts";
import { hash } from "../distribution/crypto";
import { entitlement } from "../distribution/service";
import { manifestSchema, legacyTicketPattern, versionSchema, type CliManifest } from "./contracts";
import { verifyDeviceCredential } from "../devices/credential-identity";
import { parseAccessOrigin } from "../config/env";
import { ENTRY_LIFETIME_MS, DOWNLOAD_LIFETIME_MS, openCredential, sealCredential, type EntryCredential, type InstallCredential } from "./credentials";

// Reuse the installation credential verifier; never accept browser cookies or
// an install ticket as updater authentication.
export async function authorizeUpdate(env: Bindings, request: Request) {
  const owner = await verifyDeviceCredential(env, env.DB, request);
  const user = await env.DB.prepare('SELECT "emailVerified" AS verified FROM "user" WHERE id=?').bind(owner.userId).first<{verified: number}>();
  if (!user?.verified) throw new DistributionError("email_verification_required", 403);
  const grant = await entitlement(env.DB, owner.userId);
  if (grant?.status !== "active") throw new DistributionError("entitlement_revoked", 403);
  return release(env);
}

export async function release(env: Bindings, version = env.CLI_RELEASE_VERSION) {
  if (!versionSchema.safeParse(version).success || !env.RELEASES) throw new DistributionError("release_unavailable", 409);
  const object = await env.RELEASES.get(`releases/${version}/cli-manifest.json`);
  if (!object || object.size > 16384) throw new DistributionError("release_unavailable", 409);
  const parsed = manifestSchema.safeParse(await object.json().catch(() => null));
  if (!parsed.success || parsed.data.version !== version) throw new DistributionError("release_unavailable", 409);
  await Promise.all(parsed.data.assets.map(async asset => {
    const head = await env.RELEASES.head(asset.key);
    if (!head || head.size !== asset.size || head.customMetadata?.sha256 !== asset.sha256) throw new DistributionError("release_unavailable", 409);
  }));
  return parsed.data;
}
async function verifiedGrant(env: Bindings, userId: string, revision?: number) {
  const user = await env.DB.prepare('SELECT "emailVerified" AS verified FROM "user" WHERE id=?').bind(userId).first<{verified: number}>();
  if (!user?.verified) throw new DistributionError("email_verification_required", 403);
  const grant = await entitlement(env.DB, userId);
  if (grant?.status !== "active" || (revision !== undefined && grant.revision !== revision)) throw new DistributionError("entitlement_revoked", 403);
  return grant;
}
function installLink(env: Bindings, token: string, expiresAt: number, locale: InstallLocale) {
  const url = `/cli/install/${token}`;
  const prompt = installPrompt(`${parseAccessOrigin(env)}${url}`, locale);
  return { url, expiresAt, prompt };
}
export async function restoreInstallLink(env: Bindings, userId: string, token?: string, locale: InstallLocale = DEFAULT_EMAIL_LOCALE) {
  const grant = await verifiedGrant(env, userId);
  if (!token) return null;
  let credential: InstallCredential;
  try { credential = await openCredential(env, token); }
  catch (error) {
    if (error instanceof DistributionError && (error.code === "link_not_found" || error.code === "link_expired")) return null;
    throw error;
  }
  if (credential.kind !== "entry" || credential.subject !== userId || credential.entitlementRevision !== grant.revision) return null;
  return installLink(env, token, credential.expiresAt, locale);
}
export async function issueInstallLink(env: Bindings, userId: string, existingToken?: string, locale: InstallLocale = DEFAULT_EMAIL_LOCALE) {
  const existing = await restoreInstallLink(env, userId, existingToken, locale);
  if (existing) return existing;
  const grant = await verifiedGrant(env, userId);
  await release(env); // Do not hand out a new install entry with no usable release.
  const now = Date.now();
  const credential: EntryCredential = { v: 1, kind: "entry", subject: userId, entitlementRevision: grant.revision,
    issuedAt: now, expiresAt: now + ENTRY_LIFETIME_MS };
  return installLink(env, await sealCredential(env, credential), credential.expiresAt, locale);
}
export async function authorizeInstallEntry(env: Bindings, token: string) {
  const entry = await openCredential(env, token);
  if (entry.kind !== "entry") throw new DistributionError("link_not_found", 404);
  await verifiedGrant(env, entry.subject, entry.entitlementRevision);
  return entry;
}
export async function beginDownload(env: Bindings, entry: EntryCredential) {
  const manifest = await release(env);
  const now = Date.now(), expiresAt = Math.min(entry.expiresAt, now + DOWNLOAD_LIFETIME_MS);
  const token = await sealCredential(env, { ...entry, kind: "download", issuedAt: now, expiresAt,
    version: manifest.version, manifestHash: await hash(JSON.stringify(manifest)) });
  return { manifest, expiresAt, token };
}
export async function authorizeDownload(env: Bindings, token: string) {
  const download = await openCredential(env, token);
  if (download.kind !== "download") throw new DistributionError("link_not_found", 404);
  await verifiedGrant(env, download.subject, download.entitlementRevision);
  if (!versionSchema.safeParse(env.CLI_RELEASE_VERSION).success) throw new DistributionError("release_unavailable", 409);
  const manifest = await release(env, download.version);
  if (await hash(JSON.stringify(manifest)) !== download.manifestHash) throw new DistributionError("release_unavailable", 409);
  return { manifest, expiresAt: download.expiresAt };
}

// Drain only: no new legacy tickets are issued. Remove this reader and the old
// table after the last pre-cutover ticket has expired in the deployed database.
export async function authorizeLegacyTicket(env: Bindings, token: string) {
  if (!legacyTicketPattern.test(token)) throw new DistributionError("link_not_found", 404);
  const ticket = await env.DB.prepare('SELECT t.*, u."emailVerified" AS verified FROM cli_install_ticket t JOIN "user" u ON u.id=t.user_id WHERE token_hash=?').bind(await hash(token)).first<{user_id: string; version: string; manifest_hash: string; entitlement_revision: number; expires_at: number; verified: number}>();
  if (!ticket) throw new DistributionError("link_not_found", 404);
  if (ticket.expires_at <= Date.now()) throw new DistributionError("link_expired", 410);
  const grant = await entitlement(env.DB, ticket.user_id);
  if (!ticket.verified || grant?.status !== "active" || grant.revision !== ticket.entitlement_revision) throw new DistributionError("entitlement_revoked", 403);
  // Tickets remain pinned to their original immutable release after a newer release is selected.
  if (!versionSchema.safeParse(env.CLI_RELEASE_VERSION).success) throw new DistributionError("release_unavailable", 409);
  const manifest = await release(env, ticket.version);
  if (await hash(JSON.stringify(manifest)) !== ticket.manifest_hash) throw new DistributionError("release_unavailable", 409);
  return { manifest, expiresAt: ticket.expires_at };
}
export async function streamBinary(env: Bindings, manifest: CliManifest, target: string, headOnly: boolean) {
  const asset = manifest.assets.find(a => a.target === target);
  if (!asset) throw new DistributionError("not_found", 404);
  if (headOnly) {
    const head = await env.RELEASES.head(asset.key);
    if (!head || head.size !== asset.size || head.customMetadata?.sha256 !== asset.sha256) throw new DistributionError("release_unavailable", 409);
    return new Response(null, {headers: {"Content-Type": "application/octet-stream", "Content-Length": String(asset.size)}});
  }
  const object = await env.RELEASES.get(asset.key);
  if (!object || object.size !== asset.size || object.customMetadata?.sha256 !== asset.sha256) throw new DistributionError("release_unavailable", 409);
  return new Response(object.body, { headers: { "Content-Type": "application/octet-stream", "Content-Length": String(asset.size), "Content-Disposition": `attachment; filename="${asset.key.split("/").at(-1)}"` } });
}

export type CliReleaseProjection = { status: "published" | "not_published" | "unavailable"; version: string | null; targets: CliManifest["assets"][number]["target"][] };
export async function cliReleaseProjection(env: Bindings): Promise<CliReleaseProjection> {
  if (!env.CLI_RELEASE_VERSION) return { status: "not_published", version: null, targets: [] };
  try {
    const manifest = await release(env);
    return { status: "published", version: manifest.version, targets: manifest.assets.map(asset => asset.target) };
  } catch {
    return { status: "unavailable", version: versionSchema.safeParse(env.CLI_RELEASE_VERSION).success ? env.CLI_RELEASE_VERSION : null, targets: [] };
  }
}

export async function cleanExpiredLegacyTickets(env: Bindings) {
  // Bounded batches also keep expiry cleanup from monopolizing the scheduled job.
  await env.DB.prepare("DELETE FROM cli_install_ticket WHERE token_hash IN (SELECT token_hash FROM cli_install_ticket WHERE expires_at<=? LIMIT 500)").bind(Date.now()).run();
}
