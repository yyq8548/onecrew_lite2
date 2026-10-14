import type { DesktopPairingAuthorizeRequest } from "./contracts";

const textEncoder = new TextEncoder();

export function base64Url(bytes: Uint8Array) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

export function decodeBase64Url(value: string) {
  const padding = "=".repeat((4 - (value.length % 4)) % 4);
  const binary = atob(value.replaceAll("-", "+").replaceAll("_", "/") + padding);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

export function randomOpaqueId(prefix: "pair") {
  const bytes = new Uint8Array(18);
  crypto.getRandomValues(bytes);
  return `${prefix}_${base64Url(bytes)}`;
}

export function randomPairingCode() {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return `reacher_pairing_v1.${base64Url(bytes)}`;
}

export async function pairingHmac(
  secret: string,
  purpose: "approval" | "code" | "request" | "session",
  value: string,
) {
  const key = await crypto.subtle.importKey(
    "raw",
    decodeBase64Url(secret),
    { hash: "SHA-256", name: "HMAC" },
    false,
    ["sign"],
  );
  const digest = await crypto.subtle.sign(
    "HMAC",
    key,
    textEncoder.encode(`reacher:${purpose}:v1\0${value}`),
  );
  return base64Url(new Uint8Array(digest));
}

export async function pkceChallenge(codeVerifier: string) {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    textEncoder.encode(codeVerifier),
  );
  return base64Url(new Uint8Array(digest));
}

export function constantTimeEqual(left: string, right: string) {
  if (left.length !== right.length) return false;
  try {
    const leftBytes = decodeBase64Url(left);
    const rightBytes = decodeBase64Url(right);
    if (base64Url(leftBytes) !== left || base64Url(rightBytes) !== right) {
      return false;
    }
    const subtle = crypto.subtle as SubtleCrypto & {
      timingSafeEqual(
        left: ArrayBuffer | ArrayBufferView,
        right: ArrayBuffer | ArrayBufferView,
      ): boolean;
    };
    return subtle.timingSafeEqual(
      leftBytes,
      rightBytes,
    );
  } catch {
    return false;
  }
}

export function canonicalPairingRequest(
  userId: string,
  request: DesktopPairingAuthorizeRequest,
) {
  return JSON.stringify({
    appVersion: request.appVersion,
    callbackPort: request.callbackPort,
    codeChallenge: request.codeChallenge,
    label: request.label,
    platform: request.platform,
    state: request.state,
    userId,
  });
}
