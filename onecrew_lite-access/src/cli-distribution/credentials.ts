import { z } from "zod";
import type { Bindings } from "../types/bindings";
import { parseAccessOrigin } from "../config/env";
import { DistributionError } from "../distribution/contracts";
import { linkSecret } from "../distribution/crypto";
import { credentialPattern, versionSchema } from "./contracts";

export const ENTRY_LIFETIME_MS = 24 * 60 * 60 * 1000;
export const DOWNLOAD_LIFETIME_MS = 60 * 60 * 1000;
export const INSTALL_COOKIE = "__Host-onecrew-install";
const domain = "onecrew.cli-install.credential.v1";
const encoder = new TextEncoder();
const fields = {
  v: z.literal(1),
  subject: z.string().min(1).max(256),
  entitlementRevision: z.number().int().positive().safe(),
  issuedAt: z.number().int().nonnegative().safe(),
  expiresAt: z.number().int().positive().safe(),
};
const schema = z.discriminatedUnion("kind", [
  z.object({ ...fields, kind: z.literal("entry") }).strict(),
  z.object({ ...fields, kind: z.literal("download"), version: versionSchema,
    manifestHash: z.string().regex(/^[A-Za-z0-9_-]{43}$/) }).strict(),
]);
export type InstallCredential = z.infer<typeof schema>;
export type EntryCredential = Extract<InstallCredential, { kind: "entry" }>;
export type DownloadCredential = Extract<InstallCredential, { kind: "download" }>;

function encode(bytes: Uint8Array) {
  return btoa(String.fromCharCode(...bytes)).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
function decode(value: string) {
  const bytes = Uint8Array.from(atob(value.replaceAll("-", "+").replaceAll("_", "/")), c => c.charCodeAt(0));
  if (encode(bytes) !== value) throw new Error("noncanonical encoding");
  return bytes;
}
async function key(env: Bindings) {
  const material = await crypto.subtle.importKey("raw", linkSecret(env), "HKDF", false, ["deriveKey"]);
  return crypto.subtle.deriveKey({ name: "HKDF", hash: "SHA-256", salt: encoder.encode(domain),
    info: encoder.encode(parseAccessOrigin(env)) }, material, { name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
}
function validTimes(value: InstallCredential, now: number) {
  const maximum = value.kind === "entry" ? ENTRY_LIFETIME_MS : DOWNLOAD_LIFETIME_MS;
  // Allow small clock differences between edges without extending expiresAt.
  return value.issuedAt <= now + 30_000 && value.expiresAt > value.issuedAt && value.expiresAt - value.issuedAt <= maximum;
}
export async function sealCredential(env: Bindings, value: InstallCredential) {
  const parsed = schema.parse(value);
  if (!validTimes(parsed, Date.now()) || parsed.expiresAt <= Date.now()) throw new DistributionError("link_expired", 410);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: encoder.encode(domain) },
    await key(env), encoder.encode(JSON.stringify(parsed))));
  const token = `v1.${encode(new Uint8Array([...iv, ...ciphertext]))}`;
  if (!credentialPattern.test(token)) throw new DistributionError("configuration_required", 503);
  return token;
}
export async function openCredential(env: Bindings, token: string): Promise<InstallCredential> {
  if (!credentialPattern.test(token)) throw new DistributionError("link_not_found", 404);
  // Resolve configuration outside the untrusted-input catch: missing keys are a
  // service configuration error, never an excuse to mint a replacement cookie.
  const cryptoKey = await key(env);
  let parsed: InstallCredential;
  try {
    const bytes = decode(token.slice(3));
    const plaintext = await crypto.subtle.decrypt({ name: "AES-GCM", iv: bytes.slice(0, 12), additionalData: encoder.encode(domain) },
      cryptoKey, bytes.slice(12));
    parsed = schema.parse(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(plaintext)));
    if (!validTimes(parsed, Date.now())) throw new Error("invalid lifetime");
  } catch { throw new DistributionError("link_not_found", 404); }
  if (parsed.expiresAt <= Date.now()) throw new DistributionError("link_expired", 410);
  return parsed;
}
