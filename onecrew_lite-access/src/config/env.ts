const MINIMUM_AUTH_SECRET_BYTES = 32;
const MINIMUM_AUTH_SECRET_CHARACTERS = 43;
const MINIMUM_DISTINCT_SECRET_CHARACTERS = 16;
const PLACEHOLDER_SECRET_FRAGMENTS = [
  "better-auth-secret",
  "changeme",
  "example",
  "placeholder",
  "replace",
] as const;

export class RuntimeConfigError extends Error {
  constructor(readonly code: string) {
    super(code);
    this.name = "RuntimeConfigError";
  }
}

export type GoogleAuthConfig = {
  clientId: string;
  clientSecret: string;
};

export type ResendAuthConfig = {
  apiKey: string;
  from: string;
};

export type RuntimeConfig = {
  authOrigin: string;
  authSecret: string;
  db: D1Database;
  google?: GoogleAuthConfig;
  resend?: ResendAuthConfig;
  pairingHmacSecret: string;
  secureCookies: boolean;
  webOrigin: string;
};

type ConfigBindings = {
  ACCESS_ORIGIN?: unknown;
  BETTER_AUTH_SECRET?: unknown;
  BETTER_AUTH_URL?: unknown;
  DB?: unknown;
  GOOGLE_CLIENT_ID?: unknown;
  GOOGLE_CLIENT_SECRET?: unknown;
  RESEND_API_KEY?: unknown;
  RESEND_FROM_EMAIL?: unknown;
  PAIRING_HMAC_SECRET?: unknown;
  WEB_ORIGIN?: unknown;
};

type RequiredStringBinding =
  | "ACCESS_ORIGIN"
  | "BETTER_AUTH_SECRET"
  | "BETTER_AUTH_URL"
  | "PAIRING_HMAC_SECRET"
  | "WEB_ORIGIN";

function requiredString(bindings: ConfigBindings, name: RequiredStringBinding): string {
  const value = bindings[name];
  if (typeof value !== "string" || value.length === 0) {
    throw new RuntimeConfigError(`missing_${String(name).toLowerCase()}`);
  }
  if (value !== value.trim()) {
    throw new RuntimeConfigError(`invalid_${String(name).toLowerCase()}`);
  }
  return value;
}

function optionalString(
  bindings: ConfigBindings,
  name:
    | "GOOGLE_CLIENT_ID"
    | "GOOGLE_CLIENT_SECRET"
    | "RESEND_API_KEY"
    | "RESEND_FROM_EMAIL",
): string | undefined {
  const value = bindings[name];
  if (value === undefined) return undefined;
  if (typeof value !== "string" || value.length === 0 || value !== value.trim()) {
    throw new RuntimeConfigError(`invalid_${name.toLowerCase()}`);
  }
  return value;
}

function requiredOrigin(
  bindings: ConfigBindings,
  name: "ACCESS_ORIGIN" | "BETTER_AUTH_URL" | "WEB_ORIGIN",
) {
  const value = requiredString(bindings, name);
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new RuntimeConfigError(`invalid_${name.toLowerCase()}`);
  }

  if (
    (url.protocol !== "https:" && url.protocol !== "http:") ||
    url.username.length > 0 ||
    url.password.length > 0 ||
    url.pathname !== "/" ||
    url.search.length > 0 ||
    url.hash.length > 0
  ) {
    throw new RuntimeConfigError(`invalid_${name.toLowerCase()}`);
  }

  const isLoopback =
    url.hostname === "localhost" ||
    url.hostname === "127.0.0.1" ||
    url.hostname === "[::1]";
  if (url.protocol === "http:" && !isLoopback) {
    throw new RuntimeConfigError(`insecure_${name.toLowerCase()}`);
  }

  return url.origin;
}

export function parseAccessOrigin(bindings: ConfigBindings) {
  const accessOrigin = requiredOrigin(bindings, "ACCESS_ORIGIN");
  const authOrigin = requiredOrigin(bindings, "BETTER_AUTH_URL");
  const webOrigin = requiredOrigin(bindings, "WEB_ORIGIN");
  if (accessOrigin !== authOrigin || accessOrigin !== webOrigin) {
    throw new RuntimeConfigError("mismatched_access_origins");
  }
  return accessOrigin;
}

export function parseWebOrigin(bindings: ConfigBindings) {
  return parseAccessOrigin(bindings);
}

function isStrongBase64UrlSecret(value: string) {
  if (
    value.length < MINIMUM_AUTH_SECRET_CHARACTERS ||
    !/^[A-Za-z0-9_-]+$/.test(value) ||
    value.length % 4 === 1 ||
    new Set(value).size < MINIMUM_DISTINCT_SECRET_CHARACTERS ||
    PLACEHOLDER_SECRET_FRAGMENTS.some((fragment) =>
      value.toLowerCase().includes(fragment),
    )
  ) {
    return false;
  }

  try {
    const padding = "=".repeat((4 - (value.length % 4)) % 4);
    const decoded = atob(value.replaceAll("-", "+").replaceAll("_", "/") + padding);
    const canonical = btoa(decoded)
      .replaceAll("+", "-")
      .replaceAll("/", "_")
      .replace(/=+$/, "");
    return decoded.length >= MINIMUM_AUTH_SECRET_BYTES && canonical === value;
  } catch {
    return false;
  }
}

function requiredDatabase(bindings: ConfigBindings): D1Database {
  const db = bindings.DB;
  if (
    typeof db !== "object" ||
    db === null ||
    typeof (db as D1Database).prepare !== "function" ||
    typeof (db as D1Database).batch !== "function" ||
    typeof (db as D1Database).exec !== "function"
  ) {
    throw new RuntimeConfigError("missing_db_binding");
  }
  return db as D1Database;
}

export function parseRuntimeConfig(bindings: ConfigBindings): RuntimeConfig {
  const authSecret = requiredString(bindings, "BETTER_AUTH_SECRET");
  if (!isStrongBase64UrlSecret(authSecret)) {
    throw new RuntimeConfigError("invalid_better_auth_secret");
  }
  const pairingHmacSecret = requiredString(bindings, "PAIRING_HMAC_SECRET");
  if (!isStrongBase64UrlSecret(pairingHmacSecret)) {
    throw new RuntimeConfigError("invalid_pairing_hmac_secret");
  }
  if (pairingHmacSecret === authSecret) {
    throw new RuntimeConfigError("reused_pairing_hmac_secret");
  }

  const googleClientId = optionalString(bindings, "GOOGLE_CLIENT_ID");
  const googleClientSecret = optionalString(bindings, "GOOGLE_CLIENT_SECRET");
  if ((googleClientId === undefined) !== (googleClientSecret === undefined)) {
    throw new RuntimeConfigError("partial_google_credentials");
  }
  const resendApiKey = optionalString(bindings, "RESEND_API_KEY");
  const resendFromEmail = optionalString(bindings, "RESEND_FROM_EMAIL");
  if ((resendApiKey === undefined) !== (resendFromEmail === undefined)) {
    throw new RuntimeConfigError("partial_resend_credentials");
  }

  const authOrigin = parseAccessOrigin(bindings);
  const config: RuntimeConfig = {
    authOrigin,
    authSecret,
    db: requiredDatabase(bindings),
    pairingHmacSecret,
    secureCookies: authOrigin.startsWith("https://"),
    webOrigin: authOrigin,
  };
  if (googleClientId !== undefined && googleClientSecret !== undefined) {
    config.google = {
      clientId: googleClientId,
      clientSecret: googleClientSecret,
    };
  }
  if (resendApiKey !== undefined && resendFromEmail !== undefined) {
    config.resend = { apiKey: resendApiKey, from: resendFromEmail };
  }
  return config;
}
