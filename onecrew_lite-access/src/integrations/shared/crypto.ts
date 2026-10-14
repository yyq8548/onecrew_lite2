export function encode(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}
export function decode(value: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(
    atob(value.replaceAll("-", "+").replaceAll("_", "/")),
    (c) => c.charCodeAt(0),
  );
}
export function randomState() {
  return encode(crypto.getRandomValues(new Uint8Array(32)));
}
export async function stateHash(state: string) {
  return encode(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(state)),
    ),
  );
}
async function key(secret: string) {
  const bytes = decode(secret);
  if (bytes.length !== 32 || encode(bytes) !== secret)
    throw new Error("invalid_integration_key");
  return crypto.subtle.importKey("raw", bytes, "AES-GCM", false, [
    "encrypt",
    "decrypt",
  ]);
}
function aad(userId: string, provider: string, id: string) {
  return new TextEncoder().encode(
    JSON.stringify(["third-party-token", 1, userId, provider, id]),
  );
}
export async function encryptToken(
  secret: string,
  token: string,
  userId: string,
  provider: string,
  id: string,
) {
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: nonce, additionalData: aad(userId, provider, id) },
    await key(secret),
    new TextEncoder().encode(token),
  );
  return {
    ciphertext: encode(new Uint8Array(ciphertext)),
    nonce: encode(nonce),
    keyVersion: 1,
  };
}
export async function decryptToken(
  secret: string,
  encrypted: { ciphertext: string; nonce: string; keyVersion: number },
  userId: string,
  provider: string,
  id: string,
) {
  if (encrypted.keyVersion !== 1)
    throw new Error("unsupported_integration_key_version");
  return new TextDecoder().decode(
    await crypto.subtle.decrypt(
      {
        name: "AES-GCM",
        iv: decode(encrypted.nonce),
        additionalData: aad(userId, provider, id),
      },
      await key(secret),
      decode(encrypted.ciphertext),
    ),
  );
}
