import { z } from "zod";
import * as oidc from "openid-client";
import { decode, encode } from "../../../shared/crypto";
import { IntegrationError } from "../../../../devices/errors";
import type {
  MailOAuthAdapter,
  MailOAuthConfig,
  MailOAuthRefresh,
  MailOAuthTokens,
} from "../types";

export const GOOGLE_MAIL_CALLBACK_PATH = "/integrations/gmail/callback";
export const GOOGLE_MAIL_SCOPES = [
  "openid",
  "https://www.googleapis.com/auth/userinfo.email",
  "https://www.googleapis.com/auth/gmail.send",
  "https://www.googleapis.com/auth/gmail.readonly",
] as const;

type GoogleMailConfig = MailOAuthConfig & {
  provider: "google";
  clientId: string;
  clientSecret: string;
  redirectUri: string;
};

const GOOGLE_ISSUER = new URL("https://accounts.google.com");
const PROVIDER_TIMEOUT_MS = 12_000;
const productionConfigurations = new Map<string, Promise<oidc.Configuration>>();
const configSchema = z.object({
  GMAIL_CLIENT_ID: z.string().min(1).max(512),
  GMAIL_CLIENT_SECRET: z.string().min(1).max(4096),
  THIRD_PARTY_TOKEN_ENCRYPTION_KEY: z.string().regex(/^[A-Za-z0-9_-]{43}$/),
});

function parseGoogleConfig(config: MailOAuthConfig): GoogleMailConfig {
  if (config.provider !== "google" || !("clientId" in config) || !("clientSecret" in config) || !("redirectUri" in config)) {
    throw new IntegrationError("configuration_required", 503);
  }
  return config as GoogleMailConfig;
}

function boundedFetch(fetcher: typeof fetch): oidc.CustomFetch {
  return async (input, init) => {
    const timeout = AbortSignal.timeout(PROVIDER_TIMEOUT_MS);
    const signal = init?.signal ? AbortSignal.any([init.signal, timeout]) : timeout;
    const { body, ...options } = init ?? {};
    const response = await fetcher.call(globalThis, input, {
      ...options,
      ...(body !== undefined ? { body: body as BodyInit } : {}),
      signal,
      redirect: "manual",
    });
    if (response.redirected || (response.status >= 300 && response.status < 400)) {
      await response.body?.cancel().catch(() => undefined);
      throw new Error("provider_redirect_rejected");
    }
    return response;
  };
}

function createConfiguration(config: GoogleMailConfig, fetcher: typeof fetch) {
  return oidc.discovery(
    GOOGLE_ISSUER,
    config.clientId,
    { client_secret: config.clientSecret },
    oidc.ClientSecretPost(config.clientSecret),
    { [oidc.customFetch]: boundedFetch(fetcher) },
  ).then((configuration) => {
    oidc.enableNonRepudiationChecks(configuration);
    return configuration;
  });
}

function getConfiguration(config: GoogleMailConfig) {
  let existing = productionConfigurations.get(config.clientId);
  if (!existing) {
    existing = createConfiguration(config, fetch);
    productionConfigurations.set(config.clientId, existing);
    void existing.catch(() => {
      if (productionConfigurations.get(config.clientId) === existing) {
        productionConfigurations.delete(config.clientId);
      }
    });
  }
  return existing;
}

export class GoogleMailOAuthAdapter implements MailOAuthAdapter {
  readonly provider = "google" as const;
  readonly supported = true;
  readonly callbackPath = GOOGLE_MAIL_CALLBACK_PATH;

  constructor(private readonly fetcher: typeof fetch = fetch, private readonly production = true) {}

  createConfig(bindings: unknown, authOrigin: string): GoogleMailConfig | null {
    const parsed = configSchema.safeParse(bindings);
    if (!parsed.success) return null;
    try {
      const encryptionKey = parsed.data.THIRD_PARTY_TOKEN_ENCRYPTION_KEY;
      const keyBytes = decode(encryptionKey);
      if (keyBytes.length !== 32 || encode(keyBytes) !== encryptionKey) return null;
      const origin = new URL(authOrigin);
      if (origin.protocol !== "https:" && origin.hostname !== "localhost") return null;
      return {
        provider: "google",
        clientId: parsed.data.GMAIL_CLIENT_ID,
        clientSecret: parsed.data.GMAIL_CLIENT_SECRET,
        redirectUri: new URL(GOOGLE_MAIL_CALLBACK_PATH, origin).href,
        encryptionKey,
      };
    } catch {
      return null;
    }
  }

  private configuration(config: MailOAuthConfig) {
    const google = parseGoogleConfig(config);
    return this.production
      ? getConfiguration(google)
      : createConfiguration(google, this.fetcher);
  }

  async authorizationUrl(config: MailOAuthConfig, state: string, nonce: string, codeVerifier: string) {
    const google = parseGoogleConfig(config);
    const codeChallenge = await oidc.calculatePKCECodeChallenge(codeVerifier);
    return oidc.buildAuthorizationUrl(await this.configuration(config), {
      redirect_uri: google.redirectUri,
      response_type: "code",
      scope: [...GOOGLE_MAIL_SCOPES].join(" "),
      state,
      nonce,
      code_challenge: codeChallenge,
      code_challenge_method: "S256",
      access_type: "offline",
      prompt: "consent",
      include_granted_scopes: "true",
    });
  }

  callbackUrl(config: MailOAuthConfig, parameters: URLSearchParams) {
    const url = new URL(parseGoogleConfig(config).redirectUri);
    parameters.forEach((value, key) => url.searchParams.append(key, value));
    return url;
  }

  async exchangeCode(config: MailOAuthConfig, callbackUrl: URL, state: string, nonce: string, codeVerifier: string): Promise<MailOAuthTokens> {
    const response = await oidc.authorizationCodeGrant(await this.configuration(config), callbackUrl, {
      expectedState: state,
      expectedNonce: nonce,
      pkceCodeVerifier: codeVerifier,
    });
    const claims = response.claims();
    const expiresIn = response.expiresIn();
    const scopes = (response.scope ?? "").split(/\s+/u).filter(Boolean);
    if (
      !response.access_token || !response.refresh_token || !claims ||
      typeof claims.sub !== "string" || typeof claims.email !== "string" ||
      claims.email_verified !== true || expiresIn === undefined ||
      !Number.isFinite(expiresIn) || expiresIn <= 0
    ) {
      throw new IntegrationError("provider_response_invalid", 502);
    }
    return {
      access_token: response.access_token,
      refresh_token: response.refresh_token,
      expires_in: expiresIn,
      subject: claims.sub,
      email: claims.email,
      scopes,
    };
  }

  async refresh(config: MailOAuthConfig, refreshToken: string): Promise<MailOAuthRefresh> {
    const response = await oidc.refreshTokenGrant(await this.configuration(config), refreshToken);
    const expiresIn = response.expiresIn();
    if (!response.access_token || expiresIn === undefined || !Number.isFinite(expiresIn) || expiresIn <= 0 || (response.scope !== undefined && response.scope.trim().length === 0)) {
      throw new IntegrationError("provider_response_invalid", 502);
    }
    return {
      access_token: response.access_token,
      ...(response.refresh_token ? { refresh_token: response.refresh_token } : {}),
      expires_in: expiresIn,
      ...(response.scope === undefined ? {} : { scopes: response.scope.split(/\s+/u).filter(Boolean) }),
    };
  }

  async revoke(config: MailOAuthConfig, refreshToken: string) {
    await oidc.tokenRevocation(await this.configuration(config), refreshToken, { token_type_hint: "refresh_token" });
  }

  isReauthRequired(error: unknown) {
    return error instanceof oidc.ResponseBodyError && error.error === "invalid_grant";
  }

  capabilities(scopes: string[]) {
    return {
      can_send: scopes.includes("https://www.googleapis.com/auth/gmail.send"),
      can_read_thread: scopes.includes("https://www.googleapis.com/auth/gmail.readonly"),
      can_read_attachment: false as const,
    };
  }
}

export const googleMailOAuthAdapter = new GoogleMailOAuthAdapter();
