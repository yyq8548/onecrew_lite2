import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { createAuth } from "../src/auth/auth";
import { privateAuthHeaders } from "../src/auth/request";
import { parseRuntimeConfig, RuntimeConfigError } from "../src/config/env";

const bindings = env as unknown as CloudflareBindings;

describe("runtime configuration", () => {
  it("accepts an explicit auth origin without Google", () => {
    const config = parseRuntimeConfig(bindings);

    expect(config.authOrigin).toBe("http://localhost:8787");
    expect(config.secureCookies).toBe(false);
    expect(config.webOrigin).toBe("http://localhost:8787");
    expect(config.google).toBeUndefined();
    expect(config.pairingHmacSecret).toBe(bindings.PAIRING_HMAC_SECRET);
  });

  it("enables Google only for a complete credential pair", () => {
    const config = parseRuntimeConfig({
      ...bindings,
      GOOGLE_CLIENT_ID: "google-client-id",
      GOOGLE_CLIENT_SECRET: "google-client-secret",
    });

    expect(config.google).toEqual({
      clientId: "google-client-id",
      clientSecret: "google-client-secret",
    });
    expect(createAuth(config).options.socialProviders?.google).toMatchObject({
      clientId: "google-client-id",
      clientSecret: "google-client-secret",
    });
  });

  it("fails closed for partial Google credentials", () => {
    expect(() =>
      parseRuntimeConfig({
        ...bindings,
        GOOGLE_CLIENT_ID: "google-client-id",
      }),
    ).toThrowError(new RuntimeConfigError("partial_google_credentials"));
  });

  it("enables the Better Auth email OTP plugin only with a complete Resend config", () => {
    const config = parseRuntimeConfig({
      ...bindings,
      RESEND_API_KEY: "re_test_key_0123456789",
      RESEND_FROM_EMAIL: "OneCrew <auth@example.com>",
    });
    expect(config.resend).toEqual({
      apiKey: "re_test_key_0123456789",
      from: "OneCrew <auth@example.com>",
    });
    const auth = createAuth(config);
    expect(auth.options.emailAndPassword).toMatchObject({
      autoSignIn: false,
      requireEmailVerification: true,
    });
    expect(auth.options.rateLimit?.customRules).toEqual({
      "/email-otp/send-verification-otp": { window: 60, max: 1 },
      "/email-otp/request-password-reset": { window: 60, max: 1 },
      "/email-otp/request-email-change": { window: 60, max: 1 },
    });
    expect(auth.options.plugins?.map((plugin) => plugin.id)).toContain("email-otp");
  });

  it("fails closed for partial Resend credentials", () => {
    expect(() => parseRuntimeConfig({
      ...bindings,
      RESEND_API_KEY: "re_test_key_0123456789",
    })).toThrowError(new RuntimeConfigError("partial_resend_credentials"));
  });

  it("requires explicit origins and a strong auth secret", () => {
    expect(() =>
      parseRuntimeConfig({ ...bindings, ACCESS_ORIGIN: undefined }),
    ).toThrow(RuntimeConfigError);
    expect(() =>
      parseRuntimeConfig({ ...bindings, BETTER_AUTH_URL: undefined }),
    ).toThrow(RuntimeConfigError);
    expect(() =>
      parseRuntimeConfig({ ...bindings, BETTER_AUTH_SECRET: "too-short" }),
    ).toThrowError(new RuntimeConfigError("invalid_better_auth_secret"));
    expect(() =>
      parseRuntimeConfig({ ...bindings, PAIRING_HMAC_SECRET: "too-short" }),
    ).toThrowError(new RuntimeConfigError("invalid_pairing_hmac_secret"));
    expect(() =>
      parseRuntimeConfig({
        ...bindings,
        PAIRING_HMAC_SECRET: bindings.BETTER_AUTH_SECRET,
      }),
    ).toThrowError(new RuntimeConfigError("reused_pairing_hmac_secret"));
    expect(() =>
      parseRuntimeConfig({
        ...bindings,
        BETTER_AUTH_SECRET:
          "placeholder_placeholder_placeholder_placeholder_1234567890",
      }),
    ).toThrowError(new RuntimeConfigError("invalid_better_auth_secret"));
    expect(() =>
      parseRuntimeConfig({ ...bindings, WEB_ORIGIN: "https://example.com/path" }),
    ).toThrowError(new RuntimeConfigError("invalid_web_origin"));
    expect(() =>
      parseRuntimeConfig({
        ...bindings,
        BETTER_AUTH_URL: "http://example.com",
      }),
    ).toThrowError(new RuntimeConfigError("insecure_better_auth_url"));
  });

  it("allows HTTP only for explicit loopback hosts", () => {
    expect(
      parseRuntimeConfig({
        ...bindings,
        ACCESS_ORIGIN: "http://127.0.0.1:8787",
        BETTER_AUTH_URL: "http://127.0.0.1:8787",
        WEB_ORIGIN: "http://127.0.0.1:8787",
      }).secureCookies,
    ).toBe(false);
    expect(
      parseRuntimeConfig({
        ...bindings,
        ACCESS_ORIGIN: "http://[::1]:8787",
        BETTER_AUTH_URL: "http://[::1]:8787",
        WEB_ORIGIN: "http://[::1]:8787",
      }).secureCookies,
    ).toBe(false);
    expect(() =>
      parseRuntimeConfig({
        ...bindings,
        WEB_ORIGIN: "http://192.0.2.1:5173",
      }),
    ).toThrowError(new RuntimeConfigError("insecure_web_origin"));
  });

  it("requires the Access, Better Auth and web origins to be identical", () => {
    expect(() =>
      parseRuntimeConfig({
        ...bindings,
        WEB_ORIGIN: "http://localhost:5173",
      }),
    ).toThrowError(new RuntimeConfigError("mismatched_access_origins"));
    expect(() =>
      parseRuntimeConfig({
        ...bindings,
        ACCESS_ORIGIN: "http://localhost:5173",
      }),
    ).toThrowError(new RuntimeConfigError("mismatched_access_origins"));
  });

  it("pins Better Auth cookie, IP, rate-limit and OAuth persistence policy", async () => {
    const config = parseRuntimeConfig({
      ...bindings,
      ACCESS_ORIGIN: "https://access.example.com",
      BETTER_AUTH_URL: "https://access.example.com",
      GOOGLE_CLIENT_ID: "google-client-id",
      GOOGLE_CLIENT_SECRET: "google-client-secret",
      WEB_ORIGIN: "https://access.example.com",
    });
    const auth = createAuth(config);

    expect(config.secureCookies).toBe(true);
    expect(auth.options.account?.encryptOAuthTokens).toBe(true);
    expect(auth.options.advanced?.cookiePrefix).toBe("__Host-onecrew");
    expect(auth.options.advanced?.defaultCookieAttributes).toMatchObject({
      httpOnly: true,
      path: "/",
      sameSite: "lax",
      secure: true,
    });
    expect(auth.options.advanced?.useSecureCookies).toBe(false);
    const authContext = await auth.$context;
    expect(authContext.authCookies.sessionToken.name).toBe(
      "__Host-onecrew.session_token",
    );
    expect(authContext.authCookies.sessionToken.attributes.domain).toBeUndefined();
    expect(authContext.authCookies.sessionToken.attributes).toMatchObject({
      httpOnly: true,
      path: "/",
      sameSite: "lax",
      secure: true,
    });
    expect(auth.options.advanced?.ipAddress?.ipAddressHeaders).toEqual([
      "cf-connecting-ip",
    ]);
    expect(auth.options.rateLimit).toMatchObject({
      enabled: true,
      storage: "database",
    });
    expect(auth.options.onAPIError?.errorURL).toBe(
      "https://access.example.com/login",
    );

    const account = {
      accessToken: "$ba$encrypted-access",
      accountId: "google-account",
      createdAt: new Date(),
      id: "account-id",
      idToken: "raw-id-token",
      issuer: "https://accounts.google.com",
      providerId: "google",
      refreshToken: "$ba$encrypted-refresh",
      updatedAt: new Date(),
      userId: "user-id",
    };
    const createHook = auth.options.databaseHooks?.account?.create?.before;
    const updateHook = auth.options.databaseHooks?.account?.update?.before;
    expect(createHook).toBeTypeOf("function");
    expect(updateHook).toBeTypeOf("function");

    const created = await createHook!(account);
    const updated = await updateHook!({ idToken: "new-raw-id-token" });
    expect(created).toMatchObject({
      data: {
        accessToken: "$ba$encrypted-access",
        idToken: null,
        refreshToken: "$ba$encrypted-refresh",
      },
    });
    expect(updated).toEqual({ data: { idToken: null } });
  });

  it("forwards only the private Better Auth request header allowlist", () => {
    const headers = privateAuthHeaders(
      new Headers({
        Authorization: "Bearer must-not-cross",
        Cookie: "session=cookie",
        Origin: "https://www.example.com",
        "User-Agent": "test-agent",
        "X-Forwarded-For": "198.51.100.1",
        "Accept-Language": "en-US",
        "cf-connecting-ip": "203.0.113.9",
      }),
      "application/json",
    );

    expect(Object.fromEntries(headers)).toEqual({
      "cf-connecting-ip": "203.0.113.9",
      "content-type": "application/json",
      cookie: "session=cookie",
      "accept-language": "en-US",
      origin: "https://www.example.com",
      "user-agent": "test-agent",
    });
    expect(headers.has("Authorization")).toBe(false);
    expect(headers.has("X-Forwarded-For")).toBe(false);
  });
});
