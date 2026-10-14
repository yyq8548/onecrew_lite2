import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { hc } from "hono/client";
import { beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";
import type { AppType } from "../src/app";
import app from "../src/index";
import { sanitizeOAuthCallbackResponse } from "../src/routes/auth";

type TestBindings = CloudflareBindings & {
  TEST_MIGRATIONS: D1Migration[];
};

const bindings = env as unknown as TestBindings;
const typedClient = hc<AppType>("http://localhost");

type HasUnknownApiRoute = "unknown" extends keyof typeof typedClient.api
  ? true
  : false;
type HasLegacyDeviceIssue = "$post" extends keyof typeof typedClient.api.devices
  ? true
  : false;

function apiRequest(
  path: string,
  init: RequestInit = {},
  overrides: Partial<TestBindings> = {},
) {
  const requestBindings = { ...bindings, ...overrides };
  return app.request(
    new URL(path, requestBindings.BETTER_AUTH_URL),
    init,
    requestBindings,
  );
}

function jsonHeaders(extra: HeadersInit = {}) {
  return {
    "Content-Type": "application/json",
    Origin: bindings.WEB_ORIGIN,
    ...extra,
  };
}

function sessionCookie(response: Response) {
  const setCookie = response.headers.get("Set-Cookie");
  expect(setCookie).toBeTruthy();
  if (!setCookie) throw new Error("expected session cookie");
  const cookie = setCookie.split(";", 1)[0];
  if (!cookie) throw new Error("expected non-empty session cookie");
  return cookie;
}

function expectNoTokenKeys(value: unknown): void {
  if (Array.isArray(value)) {
    for (const item of value) expectNoTokenKeys(item);
    return;
  }
  if (typeof value !== "object" || value === null) return;
  for (const [key, child] of Object.entries(value)) {
    expect(key.toLowerCase()).not.toContain("token");
    expectNoTokenKeys(child);
  }
}

async function register(email: string, name = "Reacher User") {
  return apiRequest("/api/session/register", {
    body: JSON.stringify({
      email,
      name,
      password: "correct-horse-battery-staple",
    }),
    headers: jsonHeaders(),
    method: "POST",
  });
}

beforeEach(async () => {
  await applyD1Migrations(bindings.DB, bindings.TEST_MIGRATIONS);
  await bindings.DB.batch([
    bindings.DB.prepare('DELETE FROM "activation_rate_limit"'),
    bindings.DB.prepare('DELETE FROM "device_activation"'),
    bindings.DB.prepare('DELETE FROM "access_device"'),
    bindings.DB.prepare('DELETE FROM "rateLimit"'),
    bindings.DB.prepare('DELETE FROM "verification"'),
    bindings.DB.prepare('DELETE FROM "account"'),
    bindings.DB.prepare('DELETE FROM "session"'),
    bindings.DB.prepare('DELETE FROM "user"'),
  ]);
});

describe("public API allowlist", () => {
  it("preserves mounted Hono routes in the exported app type", () => {
    expectTypeOf(typedClient.api.health.$get).toBeFunction();
    expectTypeOf(typedClient.api.account.settings.$get).toBeFunction();
    expectTypeOf(typedClient.api.devices.$get).toBeFunction();
    expectTypeOf(typedClient.api.pairing.authorize.$get).toBeFunction();
    expectTypeOf(typedClient.api.pairing.authorize.$post).toBeFunction();
    expectTypeOf<HasLegacyDeviceIssue>().toEqualTypeOf<false>();
    expectTypeOf<HasUnknownApiRoute>().toEqualTypeOf<false>();
  });

  it("reports bounded health and Google capability projections", async () => {
    const health = await apiRequest("/api/health");
    expect(health.status).toBe(200);
    expect(await health.json()).toEqual({ status: "ok" });

    const withoutGoogle = await apiRequest("/api/auth/capabilities");
    const withoutGoogleBody = await withoutGoogle.json();
    expect(withoutGoogleBody).toEqual({ googleSignInEnabled: false, emailOtpEnabled: false });
    expectNoTokenKeys(withoutGoogleBody);

    const withGoogle = await apiRequest(
      "/api/auth/capabilities",
      {},
      {
        GOOGLE_CLIENT_ID: "google-client-id",
        GOOGLE_CLIENT_SECRET: "google-client-secret",
      },
    );
    expect(await withGoogle.json()).toEqual({ googleSignInEnabled: true, emailOtpEnabled: false });
  });

  it("does not mount native Better Auth account, token or session routes", async () => {
    const paths = [
      "/api/auth/get-session",
      "/api/auth/list-accounts",
      "/api/auth/get-access-token",
      "/api/auth/refresh-token",
      "/api/auth/account-info",
      "/api/auth/sign-up/email",
      "/api/auth/sign-in/email",
      "/api/auth/sign-out",
    ];

    for (const path of paths) {
      const response = await apiRequest(path, {
        body: "{}",
        headers: jsonHeaders(),
        method: "POST",
      });
      expect(response.status, path).toBe(404);
    }
  });

  it("rejects unsupported social and callback methods at the Access boundary", async () => {
    const socialGet = await apiRequest("/api/auth/sign-in/social", {
      headers: { "cf-connecting-ip": "203.0.113.8" },
    });
    expect(socialGet.status).toBe(405);

    for (const method of ["POST", "PATCH"]) {
      const callback = await apiRequest("/api/auth/callback/google", { method });
      expect(callback.status, method).toBe(405);
    }
  });

  it("keeps Google start and callback disabled without credentials", async () => {
    const social = await apiRequest("/api/auth/sign-in/social", {
      body: JSON.stringify({ provider: "google" }),
      headers: jsonHeaders({ "cf-connecting-ip": "203.0.113.9" }),
      method: "POST",
    });
    expect(social.status).toBe(404);
    const socialBody = await social.json();
    expect(socialBody).toEqual({ error: "oauth_provider_not_found" });
    expectNoTokenKeys(socialBody);
    expect(social.headers.get("Set-Cookie")).toBeNull();

    const callback = await apiRequest(
      "/api/auth/callback/google?state=fake&code=fake&error_description=sensitive",
    );
    expect(callback.status).toBe(302);
    expect(callback.headers.get("Location")).toBe(
      `${bindings.WEB_ORIGIN}/login?error=oauth_provider_not_found`,
    );
    expect(callback.headers.get("Location")).not.toContain("sensitive");
    expect(callback.headers.get("Set-Cookie")).toBeNull();

    const persisted = await bindings.DB.prepare(
      `select
         (select count(*) from "account") as "accounts",
         (select count(*) from "session") as "sessions"`,
    ).first<{ accounts: number; sessions: number }>();
    expect(persisted).toEqual({ accounts: 0, sessions: 0 });
  });
});

describe("email session wrappers", () => {
  it("registers, restores, logs in and logs out without exposing tokens", async () => {
    const registration = await register("user@example.com");
    expect(registration.status).toBe(200);
    const registrationBody = await registration.json<{
      ok: boolean;
      user: Record<string, unknown>;
    }>();
    expect(registrationBody).toEqual({
      ok: true,
      user: {
        email: "user@example.com",
        id: expect.any(String),
        image: null,
        name: "Reacher User",
      },
    });
    expectNoTokenKeys(registrationBody);
    const cookie = sessionCookie(registration);

    const settings = await apiRequest("/api/account/settings", {
      headers: { Cookie: cookie },
    });
    expect(settings.status).toBe(200);
    expect(await settings.json()).toMatchObject({
      hasPassword: true,
      user: { email: "user@example.com" },
    });

    const restored = await apiRequest("/api/session", {
      headers: { Cookie: cookie },
    });
    expect(restored.status).toBe(200);
    expect(restored.headers.get("Cache-Control")).toBe("no-store");
    expect(restored.headers.get("Pragma")).toBe("no-cache");
    const restoredBody = await restored.json();
    expect(restoredBody).toMatchObject({
      authenticated: true,
      expiresAt: expect.any(String),
      user: { email: "user@example.com" },
    });
    expectNoTokenKeys(restoredBody);

    const login = await apiRequest("/api/session/login", {
      body: JSON.stringify({
        email: "user@example.com",
        password: "correct-horse-battery-staple",
      }),
      headers: jsonHeaders(),
      method: "POST",
    });
    expect(login.status).toBe(200);
    const loginBody = await login.json();
    expect(loginBody).toMatchObject({
      ok: true,
      user: { email: "user@example.com" },
    });
    expectNoTokenKeys(loginBody);

    const logout = await apiRequest("/api/session/logout", {
      body: "{}",
      headers: jsonHeaders({ Cookie: cookie }),
      method: "POST",
    });
    expect(logout.status).toBe(200);
    expect(await logout.json()).toEqual({ ok: true });
    expect(logout.headers.get("Set-Cookie")).toContain("Max-Age=0");

    const afterLogout = await apiRequest("/api/session", {
      headers: { Cookie: cookie },
    });
    expect(await afterLogout.json()).toEqual({
      authenticated: false,
      expiresAt: null,
      user: null,
    });
  });

  it("lets an unregistered email verify into a passwordless account", async () => {
    let otp: string | undefined;
    let emailHtml: string | undefined;
    const resend = vi.spyOn(globalThis, "fetch").mockImplementation(async (_input, init) => {
      const payload = JSON.parse(String(init?.body ?? "{}")) as { html?: string };
      emailHtml = payload.html;
      otp = payload.html?.match(/>(\d{6})<\/p>/)?.[1];
      return new Response(JSON.stringify({ id: "resend-test-id" }), {
        headers: { "Content-Type": "application/json" },
        status: 200,
      });
    });
    try {
      const overrides = {
        RESEND_API_KEY: "re_test_key_0123456789",
        RESEND_FROM_EMAIL: "OneCrew <auth@example.com>",
      } as unknown as Partial<TestBindings>;
      const send = await apiRequest("/api/session/login-otp/send", {
        body: JSON.stringify({ email: "otp-new-user@example.com" }),
        headers: jsonHeaders(),
        method: "POST",
      }, overrides);
      expect(send.status).toBe(200);
      expect(await send.json()).toEqual({ ok: true });
      expect(otp).toMatch(/^\d{6}$/);
      expect(emailHtml).toContain("cid:onecrew-mark");
      expect(emailHtml).toContain("验证码 5 分钟内有效。");

      const login = await apiRequest("/api/session/login-otp", {
        body: JSON.stringify({ email: "otp-new-user@example.com", otp }),
        headers: jsonHeaders({ Cookie: `__Host-onecrew-ga-context=${"a".repeat(64)}` }),
        method: "POST",
      }, overrides);
      expect(login.status).toBe(200);
      expect(await login.json()).toMatchObject({
        ok: true,
        user: { email: "otp-new-user@example.com", name: "" },
      });
      const cookie = sessionCookie(login);
      const claim = await apiRequest("/api/analytics/claim", {
        method: "POST", headers: jsonHeaders({ Cookie: `${cookie}; __Host-onecrew-ga-context=${"a".repeat(64)}` }), body: "{}",
      }, overrides);
      expect(await claim.json()).toEqual({ events: ["sign_up"] });
      const replay = await apiRequest("/api/analytics/claim", {
        method: "POST", headers: jsonHeaders({ Cookie: `${cookie}; __Host-onecrew-ga-context=${"a".repeat(64)}` }), body: "{}",
      }, overrides);
      expect(await replay.json()).toEqual({ events: [] });
      const settings = await apiRequest("/api/account/settings", {
        headers: { Cookie: cookie },
      }, overrides);
      expect(await settings.json()).toMatchObject({ hasPassword: false });

      const setPassword = await apiRequest("/api/account/password", {
        body: JSON.stringify({ newPassword: "correct-horse-battery-staple" }),
        headers: jsonHeaders({ Cookie: cookie }),
        method: "POST",
      }, overrides);
      expect(setPassword.status).toBe(200);
      expect(await setPassword.json()).toEqual({ ok: true });
      const passwordSettings = await apiRequest("/api/account/settings", {
        headers: { Cookie: cookie },
      }, overrides);
      expect(await passwordSettings.json()).toMatchObject({ hasPassword: true });
      expect(await bindings.DB.prepare(
        `SELECT COUNT(*) AS "count" FROM "account"
         WHERE "userId" = (SELECT "id" FROM "user" WHERE "email" = ?)
           AND "providerId" = 'credential'`,
      ).bind("otp-new-user@example.com").first<{ count: number }>()).toEqual({ count: 1 });
    } finally {
      resend.mockRestore();
    }
  });

  it("uses the request language for transactional OTP email content", async () => {
    let payload: { html?: string; subject?: string } | undefined;
    const resend = vi.spyOn(globalThis, "fetch").mockImplementation(async (_input, init) => {
      payload = JSON.parse(String(init?.body ?? "{}")) as { html?: string; subject?: string };
      return new Response(JSON.stringify({ id: "resend-english-test-id" }), {
        headers: { "Content-Type": "application/json" },
        status: 200,
      });
    });
    try {
      const overrides = {
        RESEND_API_KEY: "re_test_key_0123456789",
        RESEND_FROM_EMAIL: "OneCrew <auth@example.com>",
      } as unknown as Partial<TestBindings>;
      const response = await apiRequest("/api/session/login-otp/send", {
        body: JSON.stringify({ email: "english-otp@example.com" }),
        headers: jsonHeaders({ "Accept-Language": "en-US" }),
        method: "POST",
      }, overrides);

      expect(response.status).toBe(200);
      expect(payload?.subject).toBe("Your OneCrew sign-in code");
      expect(payload?.html).toContain("Sign in to OneCrew");
      expect(payload?.html).toContain("This code is valid for 5 minutes.");
      expect(payload?.html).not.toContain("登录 OneCrew");
    } finally {
      resend.mockRestore();
    }
  });

  it("resets a password through Better Auth email OTP", async () => {
    await register("forgot@example.com");
    let otp: string | undefined;
    const resend = vi.spyOn(globalThis, "fetch").mockImplementation(async (_input, init) => {
      const payload = JSON.parse(String(init?.body ?? "{}")) as { html?: string };
      otp = payload.html?.match(/>(\d{6})<\/p>/)?.[1];
      return new Response(JSON.stringify({ id: "resend-reset-test-id" }), {
        headers: { "Content-Type": "application/json" },
        status: 200,
      });
    });
    try {
      const overrides = {
        RESEND_API_KEY: "re_test_key_0123456789",
        RESEND_FROM_EMAIL: "OneCrew <auth@example.com>",
      } as unknown as Partial<TestBindings>;
      const send = await apiRequest("/api/session/forgot-password/send", {
        body: JSON.stringify({ email: "forgot@example.com" }),
        headers: jsonHeaders(),
        method: "POST",
      }, overrides);
      expect(send.status).toBe(200);
      expect(otp).toMatch(/^\d{6}$/);

      const reset = await apiRequest("/api/session/forgot-password", {
        body: JSON.stringify({
          email: "forgot@example.com",
          otp,
          password: "new-correct-horse-battery",
        }),
        headers: jsonHeaders(),
        method: "POST",
      }, overrides);
      expect(reset.status).toBe(200);
      expect(await reset.json()).toEqual({ ok: true });

      const login = await apiRequest("/api/session/login", {
        body: JSON.stringify({
          email: "forgot@example.com",
          password: "new-correct-horse-battery",
        }),
        headers: jsonHeaders(),
        method: "POST",
      }, overrides);
      expect(login.status).toBe(200);
      expect(await login.json()).toMatchObject({ ok: true });
    } finally {
      resend.mockRestore();
    }
  });

  it("changes email through Better Auth current-and-new OTP verification", async () => {
    const registration = await register("change-old@example.com");
    let currentOtp: string | undefined;
    let newOtp: string | undefined;
    const resend = vi.spyOn(globalThis, "fetch").mockImplementation(async (_input, init) => {
      const payload = JSON.parse(String(init?.body ?? "{}")) as { subject?: string; html?: string };
      const code = payload.html?.match(/>(\d{6})<\/p>/)?.[1];
      if (payload.subject?.includes("验证你的")) currentOtp = code;
      else newOtp = code;
      return new Response(JSON.stringify({ id: "resend-email-change-test-id" }), {
        headers: { "Content-Type": "application/json" },
        status: 200,
      });
    });
    try {
      const overrides = {
        RESEND_API_KEY: "re_test_key_0123456789",
        RESEND_FROM_EMAIL: "OneCrew <auth@example.com>",
      } as unknown as Partial<TestBindings>;
      const cookie = sessionCookie(registration);
      const current = await apiRequest("/api/account/email/current-otp/send", {
        body: "{}",
        headers: jsonHeaders({ Cookie: cookie }),
        method: "POST",
      }, overrides);
      expect(current.status).toBe(200);
      expect(currentOtp).toMatch(/^\d{6}$/);

      const currentVerified = await apiRequest("/api/account/email/current-otp/verify", {
        body: JSON.stringify({ otp: currentOtp }),
        headers: jsonHeaders({ Cookie: cookie }),
        method: "POST",
      }, overrides);
      expect(currentVerified.status).toBe(200);
      expect(await currentVerified.json()).toEqual({ ok: true });

      const requested = await apiRequest("/api/account/email/change/request", {
        body: JSON.stringify({ currentOtp, newEmail: "change-new@example.com" }),
        headers: jsonHeaders({ Cookie: cookie }),
        method: "POST",
      }, overrides);
      expect(requested.status).toBe(200);
      expect(newOtp).toMatch(/^\d{6}$/);

      const verified = await apiRequest("/api/account/email/change/verify", {
        body: JSON.stringify({ newEmail: "change-new@example.com", otp: newOtp }),
        headers: jsonHeaders({ Cookie: cookie }),
        method: "POST",
      }, overrides);
      expect(verified.status).toBe(200);
      expect(await verified.json()).toEqual({ ok: true });
      const updatedSettings = await apiRequest("/api/account/settings", {
        headers: { Cookie: sessionCookie(verified) },
      }, overrides);
      expect(await updatedSettings.json()).toMatchObject({
        user: { email: "change-new@example.com" },
      });
    } finally {
      resend.mockRestore();
    }
  });

  it("returns fixed safe errors for duplicates, malformed input and wrong passwords", async () => {
    expect((await register("duplicate@example.com")).status).toBe(200);

    const duplicate = await register("duplicate@example.com");
    expect(duplicate.status).toBe(422);
    expect(await duplicate.json()).toEqual({ error: "registration_failed" });

    const malformed = await apiRequest("/api/session/login", {
      body: "{",
      headers: jsonHeaders(),
      method: "POST",
    });
    expect(malformed.status).toBe(400);
    expect(await malformed.json()).toEqual({ error: "login_failed" });

    const wrongPassword = await apiRequest("/api/session/login", {
      body: JSON.stringify({
        email: "duplicate@example.com",
        password: "definitely-wrong",
      }),
      headers: jsonHeaders(),
      method: "POST",
    });
    expect(wrongPassword.status).toBe(401);
    const wrongPasswordBody = await wrongPassword.json();
    expect(wrongPasswordBody).toEqual({ error: "login_failed" });
    expectNoTokenKeys(wrongPasswordBody);
  });

  it("enforces strict JSON DTOs and canonicalizes registration names", async () => {
    const trimmed = await register("trimmed@example.com", "  Trimmed Name  ");
    expect(trimmed.status).toBe(200);
    expect(await trimmed.json()).toMatchObject({
      user: { name: "Trimmed Name" },
    });

    const invalidRegistrations = [
      {
        email: "invalid",
        name: "Name",
        password: "correct-horse-battery-staple",
      },
      {
        email: "extra@example.com",
        extra: true,
        name: "Name",
        password: "correct-horse-battery-staple",
      },
      {
        email: "blank@example.com",
        name: "   ",
        password: "correct-horse-battery-staple",
      },
      {
        email: "short@example.com",
        name: "Name",
        password: "short",
      },
    ];
    for (const body of invalidRegistrations) {
      const response = await apiRequest("/api/session/register", {
        body: JSON.stringify(body),
        headers: jsonHeaders(),
        method: "POST",
      });
      expect(response.status).toBe(400);
      expect(await response.json()).toEqual({ error: "registration_failed" });
    }

    const invalidLogin = await apiRequest("/api/session/login", {
      body: JSON.stringify({
        email: "trimmed@example.com",
        extra: true,
        password: "correct-horse-battery-staple",
      }),
      headers: jsonHeaders(),
      method: "POST",
    });
    expect(invalidLogin.status).toBe(400);
    expect(await invalidLogin.json()).toEqual({ error: "login_failed" });

    const invalidLogout = await apiRequest("/api/session/logout", {
      body: JSON.stringify({ callbackURL: "https://attacker.example" }),
      headers: jsonHeaders(),
      method: "POST",
    });
    expect(invalidLogout.status).toBe(400);
    expect(await invalidLogout.json()).toEqual({ error: "logout_failed" });

    const nonJson = await apiRequest("/api/session/login", {
      body: "email=trimmed%40example.com",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      method: "POST",
    });
    expect(nonJson.status).toBe(400);
  });

  it("forwards session renewal and stale-cookie cleanup headers", async () => {
    const registration = await register("refresh@example.com");
    const cookie = sessionCookie(registration);

    await bindings.DB.prepare(
      "UPDATE session SET expiresAt = ? WHERE userId = (SELECT id FROM user WHERE email = ?)",
    )
      .bind(Date.now() + 60_000, "refresh@example.com")
      .run();

    const renewed = await apiRequest("/api/session", {
      headers: { Cookie: cookie },
    });
    expect(renewed.status).toBe(200);
    expect(renewed.headers.get("Set-Cookie")).toContain(
      "better-auth.session_token=",
    );
    expect(renewed.headers.get("Cache-Control")).toBe("no-store");

    await bindings.DB.prepare(
      "DELETE FROM session WHERE userId = (SELECT id FROM user WHERE email = ?)",
    )
      .bind("refresh@example.com")
      .run();
    const stale = await apiRequest("/api/session", {
      headers: { Cookie: cookie },
    });
    expect(stale.status).toBe(200);
    expect(stale.headers.get("Set-Cookie")).toContain("Max-Age=0");
    expect(await stale.json()).toMatchObject({ authenticated: false });
  });
});

describe("Google OAuth boundary", () => {
  const googleBindings = {
    GOOGLE_CLIENT_ID: "google-client-id",
    GOOGLE_CLIENT_SECRET: "google-client-secret",
  } as const;

  it("blocks the ID-token branch and redirects missing state safely", async () => {
    const idTokenAttempt = await apiRequest(
      "/api/auth/sign-in/social",
      {
        body: JSON.stringify({
          idToken: { token: "browser-supplied-id-token" },
          provider: "google",
        }),
        headers: jsonHeaders(),
        method: "POST",
      },
      googleBindings,
    );
    expect(idTokenAttempt.status).toBe(400);
    const idTokenBody = await idTokenAttempt.json();
    expect(idTokenBody).toEqual({ error: "invalid_social_request" });
    expectNoTokenKeys(idTokenBody);

    const callback = await apiRequest(
      "/api/auth/callback/google?error_description=sensitive&unknown=discarded",
      {},
      googleBindings,
    );
    expect(callback.status).toBe(302);
    expect(callback.headers.get("Location")).toBe(
      `${bindings.WEB_ORIGIN}/login?error=state_not_found`,
    );
    expect(callback.headers.get("Location")).not.toContain("error_description");
    expect(callback.headers.get("Location")).not.toContain("unknown");
  });

  it("accepts only the closed social-start DTO", async () => {
    const forbiddenFields = [
      { callbackURL: "https://attacker.example" },
      { scopes: ["drive"] },
      { additionalParams: { prompt: "none" } },
      { additionalData: { role: "admin" } },
      { unknown: true },
    ];
    for (const extra of forbiddenFields) {
      const response = await apiRequest(
        "/api/auth/sign-in/social",
        {
          body: JSON.stringify({ provider: "google", ...extra }),
          headers: jsonHeaders(),
          method: "POST",
        },
        googleBindings,
      );
      expect(response.status).toBe(400);
      expect(await response.json()).toEqual({ error: "invalid_social_request" });
    }

    const valid = await apiRequest(
      "/api/auth/sign-in/social",
      {
        body: JSON.stringify({ provider: "google" }),
        headers: jsonHeaders(),
        method: "POST",
      },
      googleBindings,
    );
    expect(valid.status).toBe(200);
    expectNoTokenKeys(await valid.json());
  });

  it("maps duplicate and unexpected callback redirects to one safe error", async () => {
    const duplicateState = await apiRequest(
      "/api/auth/callback/google?state=one&state=two&error_description=secret",
      {},
      googleBindings,
    );
    expect(duplicateState.status).toBe(302);
    expect(duplicateState.headers.get("Location")).toBe(
      `${bindings.WEB_ORIGIN}/login?error=oauth_callback_error`,
    );

    const unexpected = sanitizeOAuthCallbackResponse(
      new Response(null, {
        headers: {
          Location:
            "https://attacker.example/callback?error=access_denied&error_description=secret",
        },
        status: 302,
      }),
      bindings.WEB_ORIGIN,
    );
    expect(unexpected.headers.get("Location")).toBe(
      `${bindings.WEB_ORIGIN}/login?error=oauth_callback_error`,
    );

    const allowedError = sanitizeOAuthCallbackResponse(
      new Response(null, {
        headers: {
          Location: `${bindings.WEB_ORIGIN}/login?error=access_denied&error_description=secret&unknown=discarded`,
        },
        status: 302,
      }),
      bindings.WEB_ORIGIN,
    );
    expect(allowedError.headers.get("Location")).toBe(
      `${bindings.WEB_ORIGIN}/login?error=access_denied`,
    );

    const allowedAccount = sanitizeOAuthCallbackResponse(
      new Response(null, {
        headers: { Location: `${bindings.WEB_ORIGIN}/account` },
        status: 302,
      }),
      bindings.WEB_ORIGIN,
    );
    expect(allowedAccount.headers.get("Location")).toBe(
      `${bindings.WEB_ORIGIN}/account`,
    );
  });

  it("carries only approved application and download destinations through Google login", async () => {
    const download = `/d/${"A".repeat(43)}`;
    for (const returnTo of ["/account", "/account?apply=1", download]) {
      const response = await apiRequest("/api/auth/sign-in/social", {
        body: JSON.stringify({ provider: "google", returnTo }),
        headers: jsonHeaders(),
        method: "POST",
      }, googleBindings);
      expect(response.status).toBe(200);
      const callback = sanitizeOAuthCallbackResponse(new Response(null, {
        status: 302,
        headers: { Location: `${bindings.WEB_ORIGIN}${returnTo}` },
      }), bindings.WEB_ORIGIN);
      expect(callback.headers.get("Location")).toBe(`${bindings.WEB_ORIGIN}${returnTo}`);
    }

    for (const returnTo of [
      "https://attacker.example/account", "//attacker.example/account", "/account?apply=2",
      "/account?panel=admin", "/account?panel=email&panel=devices", "/account?panel=email&redirect=https://attacker.example",
      `${download}?next=https://attacker.example`, `${download}#fragment`,
      `/d/${"A".repeat(42)}`, "/d/%2faccount", "/account/../admin",
    ]) {
      const response = await apiRequest("/api/auth/sign-in/social", {
        body: JSON.stringify({ provider: "google", returnTo }),
        headers: jsonHeaders(),
        method: "POST",
      }, googleBindings);
      expect(response.status).toBe(400);
      expect(await response.json()).toEqual({ error: "invalid_social_request" });
    }
  });

  it.each(["/account?panel=email&lng=en-US", "/account?panel=devices&lng=zh-CN"])("preserves %s through Google without changing the rate limit", async returnTo => {
    const response = await apiRequest("/api/auth/sign-in/social", {
      body: JSON.stringify({ provider: "google", returnTo }), headers: jsonHeaders(), method: "POST",
    }, googleBindings);
    expect(response.status).toBe(200);
    const callback = sanitizeOAuthCallbackResponse(new Response(null, { status: 302, headers: { Location: `${bindings.WEB_ORIGIN}${returnTo}` } }), bindings.WEB_ORIGIN);
    expect(callback.headers.get("Location")).toBe(`${bindings.WEB_ORIGIN}${returnTo}`);
  });

  it("preserves a safe retry destination without leaking arbitrary callback data", () => {
    const download = `/d/${"B".repeat(43)}`;
    const callback = new URL("/login", bindings.WEB_ORIGIN);
    callback.searchParams.set("error", "access_denied");
    callback.searchParams.set("returnTo", download);
    callback.searchParams.set("error_description", "private-provider-detail");
    const clean = sanitizeOAuthCallbackResponse(new Response(null, {
      status: 302, headers: { Location: callback.toString() },
    }), bindings.WEB_ORIGIN);
    const location = new URL(clean.headers.get("Location")!);
    expect(location.searchParams.get("returnTo")).toBe(download);
    expect(location.searchParams.get("error")).toBe("access_denied");
    expect(location.searchParams.has("error_description")).toBe(false);

    for (const value of ["https://attacker.example", "//attacker.example", `${download}#fragment`]) {
      callback.searchParams.set("returnTo", value);
      const rejected = sanitizeOAuthCallbackResponse(new Response(null, {
        status: 302, headers: { Location: callback.toString() },
      }), bindings.WEB_ORIGIN);
      expect(new URL(rejected.headers.get("Location")!).searchParams.has("returnTo")).toBe(false);
    }
    callback.searchParams.set("returnTo", download);
    callback.searchParams.append("returnTo", "/account?apply=1");
    const duplicate = sanitizeOAuthCallbackResponse(new Response(null, {
      status: 302, headers: { Location: callback.toString() },
    }), bindings.WEB_ORIGIN);
    expect(new URL(duplicate.headers.get("Location")!).searchParams.has("returnTo")).toBe(false);
  });

});

describe("email abuse limits", () => {
  it("rate-limits both private register and login handler calls in D1", async () => {
    for (let attempt = 0; attempt < 3; attempt += 1) {
      const registration = await apiRequest("/api/session/register", {
        body: JSON.stringify({
          email: `rate-register-${attempt}@example.com`,
          name: "Rate Limited Registration",
          password: "correct-horse-battery-staple",
        }),
        headers: jsonHeaders({ "cf-connecting-ip": "203.0.113.10" }),
        method: "POST",
      });
      expect(registration.status).toBe(200);
      expectNoTokenKeys(await registration.json());
    }
    const registerLimited = await apiRequest("/api/session/register", {
      body: JSON.stringify({
        email: "rate-register-limited@example.com",
        name: "Rate Limited Registration",
        password: "correct-horse-battery-staple",
      }),
      headers: jsonHeaders({ "cf-connecting-ip": "203.0.113.10" }),
      method: "POST",
    });
    expect(registerLimited.status).toBe(429);
    expect(await registerLimited.json()).toEqual({
      error: "registration_failed",
    });

    for (let attempt = 0; attempt < 3; attempt += 1) {
      const login = await apiRequest("/api/session/login", {
        body: JSON.stringify({
          email: "missing@example.com",
          password: "definitely-wrong",
        }),
        headers: jsonHeaders({ "cf-connecting-ip": "203.0.113.11" }),
        method: "POST",
      });
      expect(login.status).toBe(401);
      expect(await login.json()).toEqual({ error: "login_failed" });
    }
    const loginLimited = await apiRequest("/api/session/login", {
      body: JSON.stringify({
        email: "missing@example.com",
        password: "definitely-wrong",
      }),
      headers: jsonHeaders({ "cf-connecting-ip": "203.0.113.11" }),
      method: "POST",
    });
    expect(loginLimited.status).toBe(429);
    expect(await loginLimited.json()).toEqual({ error: "login_failed" });

    const rows = await bindings.DB.prepare(
      'SELECT "key", "count" FROM "rateLimit" ORDER BY "key"',
    ).all<{ count: number; key: string }>();
    expect(rows.results).toHaveLength(2);
    expect(rows.results.map((row) => row.count)).toEqual([3, 3]);
    expect(rows.results.map((row) => row.key)).toEqual(
      expect.arrayContaining([
        expect.stringContaining("/sign-in/email"),
        expect.stringContaining("/sign-up/email"),
      ]),
    );
  });
});

describe("request security and schema", () => {
  it("rejects an untrusted credentialed origin", async () => {
    const response = await apiRequest("/api/session/register", {
      body: JSON.stringify({
        email: "blocked@example.com",
        name: "Blocked",
        password: "correct-horse-battery-staple",
      }),
      headers: {
        "Content-Type": "application/json",
        Origin: "https://attacker.example",
      },
      method: "POST",
    });

    expect(response.status).toBe(403);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(response.headers.get("Pragma")).toBe("no-cache");
    expect(response.headers.get("Access-Control-Allow-Origin")).toBeNull();
    expect(await response.json()).toEqual({ error: "origin_not_allowed" });
  });

  it("allows only the configured credentialed CORS origin", async () => {
    const preflight = await apiRequest("/api/session/login", {
      headers: {
        "Access-Control-Request-Headers": "content-type",
        "Access-Control-Request-Method": "POST",
        Origin: bindings.WEB_ORIGIN,
      },
      method: "OPTIONS",
    });

    expect(preflight.status).toBe(204);
    expect(preflight.headers.get("Access-Control-Allow-Origin")).toBe(
      bindings.WEB_ORIGIN,
    );
    expect(preflight.headers.get("Access-Control-Allow-Credentials")).toBe(
      "true",
    );
    expect(preflight.headers.get("Cache-Control")).toBe("no-store");
  });

  it("keeps configuration failures non-cacheable", async () => {
    const response = await apiRequest(
      "/api/health",
      { headers: { Origin: bindings.WEB_ORIGIN } },
      { GOOGLE_CLIENT_ID: "partial-google-configuration" },
    );
    expect(response.status).toBe(500);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(response.headers.get("Pragma")).toBe("no-cache");
    expect(response.headers.get("Access-Control-Allow-Origin")).toBe(
      bindings.WEB_ORIGIN,
    );
    expect(await response.json()).toEqual({
      error: "service_configuration_error",
    });

    const invalidWebOrigin = await apiRequest(
      "/api/health",
      { headers: { Origin: bindings.WEB_ORIGIN } },
      {
        WEB_ORIGIN:
          "not-an-origin" as CloudflareBindings["WEB_ORIGIN"],
      },
    );
    expect(invalidWebOrigin.status).toBe(500);
    expect(
      invalidWebOrigin.headers.get("Access-Control-Allow-Origin"),
    ).toBeNull();
  });

  it("applies the pinned auth schema and append-only Access schemas", async () => {
    const tables = await bindings.DB.prepare(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '_cf_METADATA' ORDER BY name",
    ).all<{ name: string }>();
    expect(tables.results.map((row) => row.name)).toEqual([
      "access_device",
      "account",
      "activation_rate_limit",
      "analytics_success_event",
      "cli_install_ticket",
      "d1_migrations",
      "desktop_pairing_grant",
      "desktop_pairing_rate_limit",
      "device_activation",
      "distribution_application",
      "distribution_asset",
      "distribution_audit",
      "distribution_entitlement",
      "distribution_link",
      "distribution_mail",
      "distribution_operation",
      "distribution_release",
      "distribution_settings",
      "distribution_settings_audit",
      "mail_connection",
      "mail_oauth_transaction",
      "rateLimit",
      "session",
      "user",
      "verification",
    ]);

    const accountColumns = await bindings.DB.prepare(
      'PRAGMA table_info("account")',
    ).all<{ name: string }>();
    expect(accountColumns.results.map((row) => row.name)).toContain("issuer");

    const rateLimitColumns = await bindings.DB.prepare(
      'PRAGMA table_info("rateLimit")',
    ).all<{ name: string }>();
    expect(rateLimitColumns.results.map((row) => row.name)).toEqual([
      "id",
      "key",
      "count",
      "lastRequest",
    ]);

    const activationColumns = await bindings.DB.prepare(
      'PRAGMA table_info("device_activation")',
    ).all<{ name: string }>();
    expect(activationColumns.results.map((row) => row.name)).toEqual([
      "id",
      "device_id",
      "key_verifier",
      "created_at",
      "expires_at",
      "consumed_at",
      "revoked_at",
    ]);
    expect(activationColumns.results.map((row) => row.name)).not.toContain(
      "activation_key",
    );

    const pairingColumns = await bindings.DB.prepare(
      'PRAGMA table_info("desktop_pairing_grant")',
    ).all<{ name: string }>();
    expect(pairingColumns.results.map((row) => row.name)).toEqual([
      "id",
      "device_id",
      "request_hmac",
      "code_hmac",
      "code_challenge",
      "platform",
      "app_version",
      "created_at",
      "expires_at",
      "consumed_at",
      "attempt_count",
    ]);
    const pairingColumnNames = pairingColumns.results.map((row) => row.name);
    for (const forbidden of [
      "approval_token",
      "callback_url",
      "code",
      "code_verifier",
      "ip",
      "session_token",
      "state",
    ]) {
      expect(pairingColumnNames).not.toContain(forbidden);
    }

    const rateLimitIndexes = await bindings.DB.prepare(
      `SELECT name FROM sqlite_master
       WHERE type = 'index' AND name = 'activation_rate_limit_reset_idx'`,
    ).all<{ name: string }>();
    expect(rateLimitIndexes.results).toEqual([
      { name: "activation_rate_limit_reset_idx" },
    ]);

    const loginRiskIndexes = await bindings.DB.prepare(
      `SELECT name FROM sqlite_master
       WHERE type = 'index' AND name = 'access_session_user_latest_ip_idx'`,
    ).all<{ name: string }>();
    expect(loginRiskIndexes.results).toEqual([
      { name: "access_session_user_latest_ip_idx" },
    ]);
    const loginRiskPlan = await bindings.DB.prepare(
      `EXPLAIN QUERY PLAN
       SELECT "ipAddress"
       FROM "session"
       WHERE "userId" = ? AND "ipAddress" IS NOT NULL
       ORDER BY "updatedAt" DESC, "createdAt" DESC, "id" DESC
       LIMIT 1`,
    ).bind("index-plan-user").all<{ detail: string }>();
    expect(loginRiskPlan.results.some(({ detail }) =>
      detail.includes("access_session_user_latest_ip_idx"),
    )).toBe(true);

    const migrations = await bindings.DB.prepare(
      'SELECT name FROM "d1_migrations" ORDER BY id',
    ).all<{ name: string }>();
    expect(migrations.results.map((row) => row.name)).toEqual([
      "0001_better_auth.sql",
      "0002_activation_devices.sql",
      "0003_desktop_pairing_grants.sql",
      "0004_registration_ip.sql",
      "0005_device_pairing_ip.sql",
      "0006_login_risk_session_idx.sql",
      "0007_third_party_oauth.sql",
      "0008_desktop_integrations.sql",
      "0009_desktop_distribution.sql",
      "0010_cli_distribution.sql",
      "0011_mail_diagnostics.sql",
      "0012_analytics_success_events.sql",
      "0013_analytics_attribution_context.sql",
      "0014_mail_oauth.sql",
      "0015_retire_tkone_integrations.sql",
    ]);
  });
});
