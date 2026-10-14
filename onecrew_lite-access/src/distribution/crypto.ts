import type { Bindings } from "../types/bindings";
import { DistributionError } from "./contracts";
const encode = (v: Uint8Array) =>
  btoa(String.fromCharCode(...v))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
const decode = (v: string) =>
  Uint8Array.from(atob(v.replaceAll("-", "+").replaceAll("_", "/")), (c) =>
    c.charCodeAt(0),
  );
export async function hash(value: string) {
  return encode(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value)),
    ),
  );
}
export function linkSecret(env: Bindings) {
  const secret = env.RELEASE_LINK_SECRET;
  if (
    !secret ||
    !/^[A-Za-z0-9_-]{43}$/.test(secret) ||
    new Set(secret).size < 16 ||
    secret === env.BETTER_AUTH_SECRET ||
    secret === env.PAIRING_HMAC_SECRET
  )
    throw new DistributionError("configuration_required", 503);
  const bytes = decode(secret);
  if (bytes.length !== 32 || encode(bytes) !== secret)
    throw new DistributionError("configuration_required", 503);
  return bytes;
}
async function key(env: Bindings) {
  return crypto.subtle.importKey("raw", linkSecret(env), "AES-GCM", false, [
    "encrypt",
    "decrypt",
  ]);
}
export async function makeLink(env: Bindings) {
  const token = encode(crypto.getRandomValues(new Uint8Array(32)));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const encrypted = await crypto.subtle.encrypt(
    {
      name: "AES-GCM",
      iv,
      additionalData: new TextEncoder().encode("onecrew.release-link.v1"),
    },
    await key(env),
    new TextEncoder().encode(token),
  );
  return {
    token,
    tokenHash: await hash(token),
    encryptedToken: `${encode(iv)}.${encode(new Uint8Array(encrypted))}`,
  };
}
export async function decryptLink(env: Bindings, value: string) {
  const [iv, cipher] = value.split(".");
  if (!iv || !cipher)
    throw new DistributionError("configuration_required", 503);
  return new TextDecoder().decode(
    await crypto.subtle.decrypt(
      {
        name: "AES-GCM",
        iv: decode(iv),
        additionalData: new TextEncoder().encode("onecrew.release-link.v1"),
      },
      await key(env),
      decode(cipher),
    ),
  );
}
export async function publisherAuthorized(env: Bindings, request: Request) {
  const expected = env.RELEASE_PUBLISHER_TOKEN;
  if (
    !expected ||
    !/^[A-Za-z0-9_-]{43,128}$/.test(expected) ||
    new Set(expected).size < 16 ||
    expected === env.BETTER_AUTH_SECRET ||
    expected === env.PAIRING_HMAC_SECRET ||
    expected === env.RELEASE_LINK_SECRET
  )
    throw new DistributionError("configuration_required", 503);
  const actual = request.headers
    .get("Authorization")
    ?.match(/^Bearer (\S+)$/)?.[1];
  if (!actual || (await hash(actual)) !== (await hash(expected)))
    throw new DistributionError("publisher_unauthorized", 401);
}
