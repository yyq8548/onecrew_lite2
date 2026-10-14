export const DEVICE_ID_PATTERN = /^dev_[A-Za-z0-9_-]{24}$/;

function base64Url(bytes: Uint8Array) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

export function createDeviceId() {
  const bytes = new Uint8Array(18);
  crypto.getRandomValues(bytes);
  return `dev_${base64Url(bytes)}`;
}
