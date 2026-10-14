import { env, exports as workerExports } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import type {
  ConsumePairingGrantResult,
  DesktopPairingAuthorizeRequest,
} from "../src/pairing/contracts";
import {
  consumePairingGrantResultSchema,
  normalizeConsumePairingGrantResult,
} from "../src/pairing/contracts";
import { base64Url, pairingHmac } from "../src/pairing/crypto";
import { createApprovalToken } from "../src/pairing/approval-token";
import { safelyConsumePairingGrant } from "../src/pairing/service";
import app from "../src/index";

type TestBindings = CloudflareBindings & {
  TEST_MIGRATIONS: D1Migration[];
  CREATOR_SYNC_DEVICE_REVOCATION_CONTROL: {
    getCalls(): Promise<Array<{ deviceId: string; pairingId: string; userId: string }>>;
    reset(): Promise<void>;
    setFailure(enabled: boolean): Promise<void>;
    setResult(value: unknown): Promise<void>;
  };
};
const bindings = env as unknown as TestBindings;
const RFC_VERIFIER = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const RFC_CHALLENGE = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const STATE = base64Url(new Uint8Array(32).fill(7));

function apiRequest(path: string, init: RequestInit = {}) {
  return app.request(new URL(path, bindings.BETTER_AUTH_URL), init, bindings);
}

function jsonHeaders(extra: HeadersInit = {}) {
  return {
    "Content-Type": "application/json",
    Origin: bindings.WEB_ORIGIN,
    "cf-connecting-ip": "203.0.113.70",
    ...extra,
  };
}

function sessionCookie(response: Response) {
  const setCookie = response.headers.get("Set-Cookie");
  expect(setCookie).toBeTruthy();
  const cookie = setCookie?.split(";", 1)[0];
  if (!cookie) throw new Error("expected session cookie");
  return cookie;
}

async function register(email: string) {
  const response = await apiRequest("/api/session/register", {
    body: JSON.stringify({
      email,
      name: "Pairing User",
      password: "correct-horse-battery-staple",
    }),
    headers: jsonHeaders(),
    method: "POST",
  });
  expect(response.status).toBe(200);
  const body = await response.clone().json<{ user: { id: string } }>();
  return { cookie: sessionCookie(response), userId: body.user.id };
}

async function login(email: string) {
  const response = await apiRequest("/api/session/login", {
    body: JSON.stringify({
      email,
      password: "correct-horse-battery-staple",
    }),
    headers: jsonHeaders(),
    method: "POST",
  });
  expect(response.status).toBe(200);
  return sessionCookie(response);
}

async function seedPairingDevices(userId: string, count: number) {
  const statements: D1PreparedStatement[] = [];
  const now = Date.now();
  for (let index = 0; index < count; index += 1) {
    const suffix = index.toString(36).padStart(24, "0");
    const digest = base64Url(new Uint8Array(32).fill(index + 1));
    const deviceId = `dev_${suffix}`;
    statements.push(
      bindings.DB.prepare(
        `insert into "access_device" ("id", "user_id", "label", "created_at")
         values (?, ?, ?, ?)`,
      ).bind(deviceId, userId, `Seed ${index}`, now + index),
      bindings.DB.prepare(
        `insert into "desktop_pairing_grant"
           ("id", "device_id", "request_hmac", "code_hmac", "code_challenge",
            "platform", "app_version", "created_at", "expires_at", "consumed_at")
         values (?, ?, ?, ?, ?, 'macos', '1.0.0', ?, ?, ?)`,
      ).bind(
        `pair_${suffix}`,
        deviceId,
        digest,
        base64Url(new Uint8Array(32).fill(index + 40)),
        RFC_CHALLENGE,
        now + index,
        now + index + 90_000,
        now + index + 1,
      ),
    );
  }
  await bindings.DB.batch(statements);
}

function pairingRequest(
  overrides: Partial<DesktopPairingAuthorizeRequest> = {},
): DesktopPairingAuthorizeRequest {
  return {
    appVersion: "1.2.3",
    callbackPort: "49152",
    codeChallenge: RFC_CHALLENGE,
    codeChallengeMethod: "S256",
    label: "Studio Mac",
    platform: "macos",
    state: STATE,
    ...overrides,
  };
}

function authorizePath(request = pairingRequest()) {
  const query = new URLSearchParams({
    app_version: request.appVersion,
    callback_port: request.callbackPort,
    code_challenge: request.codeChallenge,
    code_challenge_method: request.codeChallengeMethod,
    label: request.label,
    platform: request.platform,
    state: request.state,
  });
  return `/api/pairing/authorize?${query.toString()}`;
}

function authorizePagePath(request = pairingRequest()) {
  return authorizePath(request).replace("/api/pairing/authorize", "/pairing/authorize");
}

async function preview(cookie: string, request = pairingRequest()) {
  const response = await apiRequest(authorizePath(request), {
    headers: { Cookie: cookie, Origin: bindings.WEB_ORIGIN },
  });
  expect(response.status).toBe(200);
  return response.json<{
    approvalToken: string;
    expiresAt: string;
    request: {
      appName: string;
      appVersion: string;
      callbackPort: string;
      label: string;
      platform: string;
      platformLabel: string;
      bindingIp: string | null;
      bindingLocation: {
        country: string | null;
        region: string | null;
        city: string | null;
        colo: string | null;
      };
    };
  }>();
}

async function approve(cookie: string, approvalToken: string) {
  return apiRequest("/api/pairing/authorize", {
    body: new URLSearchParams({ approvalToken }).toString(),
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Cookie: cookie,
      Origin: bindings.WEB_ORIGIN,
      "Sec-Fetch-Site": "same-origin",
      "cf-connecting-ip": "203.0.113.70",
    },
    method: "POST",
    redirect: "manual",
  });
}

async function approvedPairing(email: string) {
  const owner = await register(email);
  const request = pairingRequest();
  const authorization = await preview(owner.cookie, request);
  const response = await approve(owner.cookie, authorization.approvalToken);
  expect(response.status).toBe(303);
  const location = response.headers.get("Location");
  if (!location) throw new Error("expected loopback Location");
  const callback = new URL(location);
  expect(`${callback.protocol}//${callback.hostname}:${callback.port}${callback.pathname}`).toBe(
    "http://127.0.0.1:49152/creator-sync/pairing-callback",
  );
  expect(callback.searchParams.get("state")).toBe(request.state);
  const code = callback.searchParams.get("code");
  if (!code) throw new Error("expected pairing code");
  const device = await bindings.DB.prepare(
    `select p."id" as "pairing_id", p."device_id"
     from "desktop_pairing_grant" p
     join "access_device" d on d."id" = p."device_id"
     where d."user_id" = ?`,
  )
    .bind(owner.userId)
    .first<{ device_id: string; pairing_id: string }>();
  if (!device) throw new Error("expected pairing row");
  return { ...owner, ...device, approvalToken: authorization.approvalToken, code };
}

async function consumedPairing(email: string) {
  const pairing = await approvedPairing(email);
  expect(
    await workerExports.DesktopPairingAdmission.consumePairingGrant({
      code: pairing.code,
      codeVerifier: RFC_VERIFIER,
    }),
  ).toMatchObject({ admission: "admitted", pairingId: pairing.pairing_id });
  return pairing;
}

beforeEach(async () => {
  await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.reset();
  await applyD1Migrations(bindings.DB, bindings.TEST_MIGRATIONS);
  await bindings.DB.batch([
    bindings.DB.prepare('DELETE FROM "desktop_pairing_rate_limit"'),
    bindings.DB.prepare('DELETE FROM "desktop_pairing_grant"'),
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

describe("Desktop browser pairing authorization", () => {
  it("serves the authorize shell with no-store and anti-framing headers", async () => {
    const response = await app.request(
      new URL("/pairing/authorize", bindings.BETTER_AUTH_URL),
      {},
      {
        ...bindings,
        ASSETS: {
          fetch: async () =>
            new Response("<!doctype html><title>Pairing</title>", {
              headers: { "Content-Type": "text/html" },
            }),
        } as unknown as Fetcher,
      },
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(response.headers.get("Pragma")).toBe("no-cache");
    expect(response.headers.get("Referrer-Policy")).toBe("origin");
    expect(response.headers.get("X-Frame-Options")).toBe("DENY");
    expect(response.headers.get("Content-Security-Policy")).toContain(
      "frame-ancestors 'none'",
    );
    expect(response.headers.get("Content-Security-Policy")).toContain(
      "form-action 'self' http://127.0.0.1:*",
    );
  });

  it("requires a session for the rate-limited result page", async () => {
    const anonymous = await apiRequest("/pairing/rate-limited", { redirect: "manual" });
    expect(anonymous.status).toBe(303);
    expect(anonymous.headers.get("Location")).toBe("/login");

    const owner = await register("limited-result-page@example.com");
    const authenticated = await app.request(
      new URL("/pairing/rate-limited", bindings.BETTER_AUTH_URL),
      { headers: { Cookie: owner.cookie } },
      {
        ...bindings,
        ASSETS: {
          fetch: async () => new Response("<!doctype html><title>Access</title>", {
            headers: { "Content-Type": "text/html" },
          }),
        } as unknown as Fetcher,
      },
    );
    expect(authenticated.status).toBe(200);
    expect(await authenticated.text()).toContain("<title>Access</title>");
  });

  it("previews without side effects and requires an explicit strict approval", async () => {
    const { cookie } = await register("preview@example.com");
    const result = await preview(cookie);
    expect(result.request).toEqual({
      appName: "Agent Platform",
      appVersion: "1.2.3",
      callbackPort: "49152",
      label: "Studio Mac",
      platform: "macos",
      platformLabel: "macOS",
      bindingIp: null,
      bindingLocation: { country: null, region: null, city: null, colo: null },
    });
    expect(result.approvalToken).not.toContain(STATE);
    expect(
      await bindings.DB.prepare('select count(*) as "count" from "access_device"')
        .first<{ count: number }>(),
    ).toEqual({ count: 0 });

    const missingOrigin = await apiRequest("/api/pairing/authorize", {
      body: JSON.stringify({ approvalToken: result.approvalToken }),
      headers: { "Content-Type": "application/json", Cookie: cookie },
      method: "POST",
    });
    expect(missingOrigin.status).toBe(403);
    expect(await missingOrigin.json()).toEqual({ error: "origin_not_allowed" });

    for (const fetchSite of [undefined, "same-site", "cross-site", "none"]) {
      const headers: Record<string, string> = {
        "Content-Type": "application/json",
        Cookie: cookie,
        Origin: bindings.WEB_ORIGIN,
      };
      if (fetchSite) headers["Sec-Fetch-Site"] = fetchSite;
      const rejected = await apiRequest("/api/pairing/authorize", {
        body: JSON.stringify({ approvalToken: result.approvalToken }),
        headers,
        method: "POST",
      });
      expect(rejected.status, fetchSite).toBe(403);
    }

    const jsonBody = await apiRequest("/api/pairing/authorize", {
      body: JSON.stringify({ approvalToken: result.approvalToken }),
      headers: jsonHeaders({
        Cookie: cookie,
        "Sec-Fetch-Site": "same-origin",
      }),
      method: "POST",
    });
    expect(jsonBody.status).toBe(415);
    expect(await jsonBody.json()).toEqual({ error: "invalid_content_type" });

    const duplicateToken = new URLSearchParams();
    duplicateToken.append("approvalToken", result.approvalToken);
    duplicateToken.append("approvalToken", result.approvalToken);
    const duplicate = await apiRequest("/api/pairing/authorize", {
      body: duplicateToken.toString(),
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Cookie: cookie,
        Origin: bindings.WEB_ORIGIN,
        "Sec-Fetch-Site": "same-origin",
      },
      method: "POST",
    });
    expect(duplicate.status).toBe(400);

    for (const [field, value] of [
      ["redirect_uri", "https://evil.example"],
      ["callback_port", "49153"],
      ["callback_url", "http://evil.example"],
      ["host", "evil.example"],
      ["origin", "https://evil.example"],
      ["path", "/other"],
    ] as const) {
      const extra = await apiRequest("/api/pairing/authorize", {
        body: new URLSearchParams({
          approvalToken: result.approvalToken,
          [field]: value,
        }).toString(),
        headers: {
          "Content-Type": "application/x-www-form-urlencoded",
          Cookie: cookie,
          Origin: bindings.WEB_ORIGIN,
          "Sec-Fetch-Site": "same-origin",
        },
        method: "POST",
      });
      expect(extra.status, field).toBe(400);
      expect(await extra.json()).toEqual({ error: "invalid_pairing_request" });
    }
  });

  it("rejects duplicate, unknown, non-S256 and attacker redirect query fields", async () => {
    const { cookie } = await register("strict-query@example.com");
    const missingPort = new URL(authorizePath(), bindings.BETTER_AUTH_URL);
    missingPort.searchParams.delete("callback_port");
    const cases = [
      `${missingPort.pathname}${missingPort.search}`,
      `${authorizePath()}&state=${STATE}`,
      `${authorizePath()}&callback_port=49153`,
      `${authorizePath()}&unknown=true`,
      authorizePath({ ...pairingRequest(), codeChallengeMethod: "plain" as "S256" }),
      ...["0", "01024", "+1024", " 1024", "1023", "65536", "49152/path", "49152\r\nX-Test:yes"].map(
        (callbackPort) => authorizePath({ ...pairingRequest(), callbackPort }),
      ),
      authorizePath({ ...pairingRequest(), label: "Unsafe\u202elabel" }),
      authorizePath({ ...pairingRequest(), state: `${"A".repeat(42)}B` }),
      `${authorizePath()}&redirect_uri=https%3A%2F%2Fevil.example`,
      `${authorizePath()}&callback_url=http%3A%2F%2Fevil.example`,
      `${authorizePath()}&host=evil.example`,
      `${authorizePath()}&origin=https%3A%2F%2Fevil.example`,
      `${authorizePath()}&path=%2Fother`,
    ];
    for (const path of cases) {
      const response = await apiRequest(path, { headers: { Cookie: cookie } });
      expect(response.status, path).toBe(400);
      expect(await response.json()).toEqual({ error: "invalid_pairing_request" });
    }
  });

  it("constructs only the fixed callback and resolves one approval once", async () => {
    const { cookie } = await register("approval@example.com");
    const result = await preview(cookie);
    const first = await approve(cookie, result.approvalToken);
    expect(first.status).toBe(303);
    expect(first.headers.get("Cache-Control")).toBe("no-store");
    const location = first.headers.get("Location");
    expect(location).toBeTruthy();
    const callback = new URL(location!);
    expect(`${callback.protocol}//${callback.hostname}:${callback.port}${callback.pathname}`).toBe(
      "http://127.0.0.1:49152/creator-sync/pairing-callback",
    );
    expect([...callback.searchParams.keys()].sort()).toEqual(["code", "state"]);
    expect(callback.searchParams.get("state")).toBe(STATE);

    const replay = await approve(cookie, result.approvalToken);
    expect(replay.status).toBe(409);
    expect(await replay.json()).toEqual({ error: "pairing_approval_resolved" });
    const refreshedPreview = await apiRequest(authorizePath(), {
      headers: { Cookie: cookie },
    });
    expect(refreshedPreview.status).toBe(409);
    expect(await refreshedPreview.json()).toEqual({
      error: "pairing_approval_resolved",
    });
  });

  it("binds approval to the current user session and enforces exact authorize quota", async () => {
    const first = await register("approval-owner@example.com");
    const other = await register("approval-other@example.com");
    const ownerPreview = await preview(first.cookie);
    const wrongSession = await approve(other.cookie, ownerPreview.approvalToken);
    expect(wrongSession.status).toBe(400);
    expect(await wrongSession.json()).toEqual({ error: "invalid_pairing_request" });

    const statuses: number[] = [];
    for (let index = 0; index < 5; index += 1) {
      const request = pairingRequest({
        state: base64Url(new Uint8Array(32).fill(index + 20)),
      });
      const authorization = await preview(first.cookie, request);
      statuses.push((await approve(first.cookie, authorization.approvalToken)).status);
    }
    expect(statuses).toEqual([303, 303, 303, 303, 303]);
    const limitedPreview = await apiRequest(authorizePath(pairingRequest({
      state: base64Url(new Uint8Array(32).fill(26)),
    })), { headers: { Cookie: first.cookie, Origin: bindings.WEB_ORIGIN } });
    expect(limitedPreview.status).toBe(429);
    expect(await limitedPreview.json()).toEqual({ error: "rate_limited" });
    expect(Number(limitedPreview.headers.get("Retry-After"))).toBeGreaterThan(0);
    const count = await bindings.DB.prepare(
      'select count(*) as "count" from "access_device" where "user_id" = ?',
    )
      .bind(first.userId)
      .first<{ count: number }>();
    expect(count?.count).toBe(5);
  });

  it("redirects an authenticated limited page and a late navigation race to safe UI", async () => {
    const owner = await register("pairing-limit-page@example.com");
    const authorization = await preview(owner.cookie);
    await bindings.DB.prepare(
      `insert into "desktop_pairing_rate_limit" ("bucket", "request_count", "reset_at")
       values (?, 5, ?)`,
    ).bind(`authorize:${owner.userId}`, Date.now() + 5 * 60 * 1000).run();

    const page = await apiRequest(authorizePagePath(), {
      headers: {
        Cookie: owner.cookie,
        "cf-connecting-ip": "203.0.113.70",
      },
      redirect: "manual",
    });
    expect(page.status).toBe(303);
    expect(page.headers.get("Location")).toBe("/pairing/rate-limited");
    expect(Number(page.headers.get("Retry-After"))).toBeGreaterThan(0);

    const navigation = await apiRequest("/api/pairing/authorize", {
      body: new URLSearchParams({ approvalToken: authorization.approvalToken }).toString(),
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Cookie: owner.cookie,
        Origin: bindings.WEB_ORIGIN,
        "Sec-Fetch-Mode": "navigate",
        "Sec-Fetch-Site": "same-origin",
        "cf-connecting-ip": "203.0.113.70",
      },
      method: "POST",
      redirect: "manual",
    });
    expect(navigation.status).toBe(303);
    expect(navigation.headers.get("Location")).toBe("/pairing/rate-limited");
    expect(await navigation.text()).toBe("");
  });

  it("rejects a second session for the same user and expires approval tokens", async () => {
    const owner = await register("session-bound@example.com");
    const authorization = await preview(owner.cookie);
    const secondCookie = await login("session-bound@example.com");
    const wrongSession = await approve(secondCookie, authorization.approvalToken);
    expect(wrongSession.status).toBe(400);
    expect(await wrongSession.json()).toEqual({ error: "invalid_pairing_request" });
    const tamperedToken = `${authorization.approvalToken.slice(0, -1)}${
      authorization.approvalToken.endsWith("A") ? "B" : "A"
    }`;
    const tampered = await approve(owner.cookie, tamperedToken);
    expect(tampered.status).toBe(400);
    expect(await tampered.json()).toEqual({ error: "invalid_pairing_request" });
    expect(
      await bindings.DB.prepare(
        'select count(*) as "count" from "access_device" where "user_id" = ?',
      )
        .bind(owner.userId)
        .first<{ count: number }>(),
    ).toEqual({ count: 0 });
    expect((await approve(owner.cookie, authorization.approvalToken)).status).toBe(303);

    const expiring = await register("approval-expired@example.com");
    const session = await bindings.DB.prepare(
      'select "id" from "session" where "userId" = ? order by "createdAt" desc limit 1',
    )
      .bind(expiring.userId)
      .first<{ id: string }>();
    if (!session) throw new Error("expected Better Auth session");
    const expiredToken = await createApprovalToken(
      bindings.PAIRING_HMAC_SECRET,
      expiring.userId,
      session.id,
      pairingRequest({ state: base64Url(new Uint8Array(32).fill(99)) }),
      1,
    );
    const expiredResponse = await approve(expiring.cookie, expiredToken.approvalToken);
    expect(expiredResponse.status).toBe(410);
    expect(await expiredResponse.json()).toEqual({ error: "pairing_approval_expired" });
  });

  it("rejects oversized approval before consuming the token", async () => {
    const owner = await register("body-limit@example.com");
    const authorization = await preview(owner.cookie);
    const body = JSON.stringify({
      approvalToken: authorization.approvalToken,
      padding: "x".repeat(3_000),
    });
    const oversized = await apiRequest("/api/pairing/authorize", {
      body,
      headers: jsonHeaders({
        "Content-Length": String(new TextEncoder().encode(body).length),
        Cookie: owner.cookie,
        "Sec-Fetch-Site": "same-origin",
      }),
      method: "POST",
    });
    expect(oversized.status).toBe(413);
    expect(oversized.headers.get("Cache-Control")).toBe("no-store");
    expect(await oversized.json()).toEqual({ error: "request_too_large" });
    expect((await approve(owner.cookie, authorization.approvalToken)).status).toBe(303);

    const streamedRequest = pairingRequest({
      state: base64Url(new Uint8Array(32).fill(88)),
    });
    const streamedToken = await preview(owner.cookie, streamedRequest);
    const streamed = await apiRequest("/api/pairing/authorize", {
      body: new Blob(["x".repeat(3_000)]),
      headers: jsonHeaders({
        Cookie: owner.cookie,
        "Sec-Fetch-Site": "same-origin",
      }),
      method: "POST",
    });
    expect(streamed.status).toBe(413);
    expect(await streamed.json()).toEqual({ error: "request_too_large" });
    expect((await approve(owner.cookie, streamedToken.approvalToken)).status).toBe(303);
  });

  it("resolves concurrent approval once with no orphan device", async () => {
    const owner = await register("approval-race@example.com");
    const [firstToken, secondToken] = await Promise.all([
      preview(owner.cookie),
      preview(owner.cookie),
    ]);
    const responses = await Promise.all([
      approve(owner.cookie, firstToken.approvalToken),
      approve(owner.cookie, secondToken.approvalToken),
    ]);
    expect(responses.map((response) => response.status).sort()).toEqual([303, 409]);
    const counts = await bindings.DB.prepare(
      `select
         (select count(*) from "access_device" where "user_id" = ?) as "devices",
         (select count(*) from "desktop_pairing_grant" p
          join "access_device" d on d."id" = p."device_id"
          where d."user_id" = ?) as "grants"`,
    )
      .bind(owner.userId, owner.userId)
      .first<{ devices: number; grants: number }>();
    expect(counts).toEqual({ devices: 1, grants: 1 });
  });

  it("enforces the 20-device cap under concurrent approval", async () => {
    const owner = await register("device-cap@example.com");
    await seedPairingDevices(owner.userId, 19);
    const requests = [
      pairingRequest({ state: base64Url(new Uint8Array(32).fill(110)) }),
      pairingRequest({ state: base64Url(new Uint8Array(32).fill(111)) }),
    ];
    const tokens = await Promise.all(requests.map((request) => preview(owner.cookie, request)));
    const responses = await Promise.all(
      tokens.map((token) => approve(owner.cookie, token.approvalToken)),
    );
    expect(responses.map((response) => response.status).sort()).toEqual([303, 409]);
    const count = await bindings.DB.prepare(
      'select count(*) as "count" from "access_device" where "user_id" = ?',
    )
      .bind(owner.userId)
      .first<{ count: number }>();
    expect(count?.count).toBe(20);
  });

  it("uses a five-minute per-U authorization window", async () => {
    const owner = await register("short-pairing-window@example.com");
    const authorization = await preview(owner.cookie);
    const before = Date.now();
    expect((await approve(owner.cookie, authorization.approvalToken)).status).toBe(303);
    const after = Date.now();
    const row = await bindings.DB.prepare(
      `select "request_count", "reset_at" from "desktop_pairing_rate_limit"
       where "bucket" = ?`,
    ).bind(`authorize:${owner.userId}`).first<{
      request_count: number;
      reset_at: number;
    }>();
    expect(row?.request_count).toBe(1);
    expect(row!.reset_at).toBeGreaterThanOrEqual(before + 5 * 60 * 1000);
    expect(row!.reset_at).toBeLessThanOrEqual(after + 5 * 60 * 1000);
  });
});

describe("DesktopPairingAdmission", () => {
  it("exports only the named RPC target and no HTTP exchange", async () => {
    expect(workerExports.DesktopPairingAdmission.consumePairingGrant).toBeTypeOf(
      "function",
    );
    for (const path of [
      "/api/devices",
      "/api/pairing/token",
      "/api/pairing/consume",
      "/api/internal/pairing",
    ]) {
      const response = await apiRequest(path, {
        body: "{}",
        headers: jsonHeaders(),
        method: "POST",
      });
      expect(response.status, path).toBe(path === "/api/devices" ? 405 : 404);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
    }
  });

  it("validates RFC 7636 S256 and atomically admits once", async () => {
    const pairing = await approvedPairing("consume-pairing@example.com");
    const input = { code: pairing.code, codeVerifier: RFC_VERIFIER };
    const results = await Promise.all([
      workerExports.DesktopPairingAdmission.consumePairingGrant(input),
      workerExports.DesktopPairingAdmission.consumePairingGrant(input),
    ]);
    const admitted = results.find(
      (result): result is Extract<ConsumePairingGrantResult, { admission: "admitted" }> =>
        result.admission === "admitted",
    );
    expect(admitted).toEqual({
      admission: "admitted",
      deviceId: pairing.device_id,
      pairingId: pairing.pairing_id,
      userId: pairing.userId,
    });
    expect(results.map((result) => result.admission).sort()).toEqual([
      "admitted",
      "consumed",
    ]);
    expect(await workerExports.DesktopPairingAdmission.consumePairingGrant(input)).toEqual({
      admission: "consumed",
    });
    const status = await apiRequest(
      `/api/devices/${pairing.device_id}/status`,
      { headers: { Cookie: pairing.cookie } },
    );
    expect(await status.json()).toMatchObject({
      device: { pairedAt: expect.any(String), status: "active" },
    });
  });

  it("keeps device list, status and revoke isolated to the Better Auth owner", async () => {
    const pairing = await approvedPairing("device-owner@example.com");
    const other = await register("device-other@example.com");
    const otherList = await apiRequest("/api/devices", {
      headers: { Cookie: other.cookie },
    });
    expect(await otherList.json()).toEqual({ devices: [], nextCursor: null });
    for (const method of ["DELETE", "GET", "POST"]) {
      const path =
        method === "GET"
          ? `/api/devices/${pairing.device_id}/status`
          : method === "POST"
            ? `/api/devices/${pairing.device_id}/revoke`
            : `/api/devices/${pairing.device_id}`;
      const response = await apiRequest(path, {
        ...(method === "POST" ? { body: "{}" } : {}),
        headers:
          method === "POST"
            ? jsonHeaders({ Cookie: other.cookie })
            : { Cookie: other.cookie },
        method,
      });
      expect(response.status).toBe(404);
    }
    const anonymous = await apiRequest(`/api/devices/${pairing.device_id}/status`);
    expect(anonymous.status).toBe(401);
    const owner = await apiRequest(`/api/devices/${pairing.device_id}/status`, {
      headers: { Cookie: pairing.cookie },
    });
    expect(owner.status).toBe(200);
    expect(await owner.json()).toMatchObject({ device: { status: "pending" } });
  });

  it("synchronizes an active revoke to B and deletes only closed device records", async () => {
    const pairing = await consumedPairing("device-delete@example.com");
    const activeDelete = await apiRequest(`/api/devices/${pairing.device_id}`, {
      headers: { Cookie: pairing.cookie, Origin: bindings.WEB_ORIGIN },
      method: "DELETE",
    });
    expect(activeDelete.status).toBe(409);
    expect(await activeDelete.json()).toEqual({ error: "device_must_be_revoked" });

    const revoke = await apiRequest(`/api/devices/${pairing.device_id}/revoke`, {
      body: "{}",
      headers: jsonHeaders({ Cookie: pairing.cookie }),
      method: "POST",
    });
    expect(revoke.status).toBe(200);
    expect(await revoke.json()).toMatchObject({ device: { status: "revoked" } });
    expect(await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.getCalls()).toEqual([
      {
        deviceId: pairing.device_id,
        pairingId: pairing.pairing_id,
        userId: pairing.userId,
      },
    ]);

    const deleted = await apiRequest(`/api/devices/${pairing.device_id}`, {
      headers: { Cookie: pairing.cookie, Origin: bindings.WEB_ORIGIN },
      method: "DELETE",
    });
    expect(deleted.status).toBe(200);
    expect(await deleted.json()).toEqual({ deletedDeviceId: pairing.device_id });
    expect(await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.getCalls()).toHaveLength(2);
    const readback = await apiRequest(`/api/devices/${pairing.device_id}/status`, {
      headers: { Cookie: pairing.cookie },
    });
    expect(readback.status).toBe(404);
    const rows = await bindings.DB.prepare(
      `select (select count(*) from "access_device" where "id" = ?) as "devices",
              (select count(*) from "desktop_pairing_grant" where "id" = ?) as "grants"`,
    ).bind(pairing.device_id, pairing.pairing_id).first<{ devices: number; grants: number }>();
    expect(rows).toEqual({ devices: 0, grants: 0 });
  });

  it("lets Creator Sync Admin delegate revocation to the Access device owner", async () => {
    const pairing = await consumedPairing("creator-sync-admin-revoke@example.com");
    expect(await workerExports.DesktopDeviceLifecycle.revokeAccessDevice({
      deviceId: pairing.device_id,
      pairingId: pairing.pairing_id,
      userId: pairing.userId,
    })).toEqual({ revocation: "revoked" });
    const status = await apiRequest(`/api/devices/${pairing.device_id}/status`, {
      headers: { Cookie: pairing.cookie },
    });
    expect(await status.json()).toMatchObject({ device: { status: "revoked" } });
    expect(await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.getCalls()).toEqual([
      {
        deviceId: pairing.device_id,
        pairingId: pairing.pairing_id,
        userId: pairing.userId,
      },
    ]);
  });

  it("keeps the Access device active when delegated Creator Sync revocation fails", async () => {
    const pairing = await consumedPairing("creator-sync-admin-failure@example.com");
    await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.setResult({
      revocation: "unavailable",
    });
    expect(await workerExports.DesktopDeviceLifecycle.revokeAccessDevice({
      deviceId: pairing.device_id,
      pairingId: pairing.pairing_id,
      userId: pairing.userId,
    })).toEqual({ revocation: "unavailable" });
    const status = await apiRequest(`/api/devices/${pairing.device_id}/status`, {
      headers: { Cookie: pairing.cookie },
    });
    expect(await status.json()).toMatchObject({ device: { status: "active" } });
  });

  it("does not mark A revoked when B revocation is unavailable or mismatched", async () => {
    const pairing = await consumedPairing("device-revoke-failure@example.com");
    const request = () => apiRequest(`/api/devices/${pairing.device_id}/revoke`, {
      body: "{}",
      headers: jsonHeaders({ Cookie: pairing.cookie }),
      method: "POST",
    });
    await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.setResult({
      revocation: "unavailable",
    });
    expect((await request()).status).toBe(503);

    await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.setResult({
      revocation: "not_found",
    });
    expect((await request()).status).toBe(409);

    await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.setResult({ leaked: true });
    expect((await request()).status).toBe(503);
    const status = await apiRequest(`/api/devices/${pairing.device_id}/status`, {
      headers: { Cookie: pairing.cookie },
    });
    expect(await status.json()).toMatchObject({ device: { status: "active" } });
  });

  it("deletes an expired unconsumed device without contacting B", async () => {
    const pairing = await approvedPairing("expired-device-delete@example.com");
    const expiredAt = Date.now() - 1;
    await bindings.DB.prepare(
      `update "desktop_pairing_grant" set "created_at" = ?, "expires_at" = ? where "id" = ?`,
    ).bind(expiredAt - 90_000, expiredAt, pairing.pairing_id).run();
    const deleted = await apiRequest(`/api/devices/${pairing.device_id}`, {
      headers: { Cookie: pairing.cookie, Origin: bindings.WEB_ORIGIN },
      method: "DELETE",
    });
    expect(deleted.status).toBe(200);
    expect(await bindings.CREATOR_SYNC_DEVICE_REVOCATION_CONTROL.getCalls()).toEqual([]);
  });

  it("rejects unknown fields, verifier guesses and then rate-limits the grant", async () => {
    const pairing = await approvedPairing("guess-pairing@example.com");
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: pairing.code,
        codeVerifier: RFC_VERIFIER,
        extra: true,
      } as never),
    ).toEqual({ admission: "invalid" });
    const wrongVerifier = `${"A".repeat(42)}B`;
    const attempts = await Promise.all(
      Array.from({ length: 9 }, () =>
        workerExports.DesktopPairingAdmission.consumePairingGrant({
          code: pairing.code,
          codeVerifier: wrongVerifier,
        }),
      ),
    );
    expect(attempts.filter((result) => result.admission === "invalid")).toHaveLength(8);
    expect(attempts.filter((result) => result.admission === "rate_limited")).toHaveLength(1);
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: pairing.code,
        codeVerifier: RFC_VERIFIER,
      }),
    ).toEqual({ admission: "rate_limited" });
  });

  it("allows the correct verifier after a bounded wrong proof", async () => {
    const pairing = await approvedPairing("recover-proof@example.com");
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: pairing.code,
        codeVerifier: `${"A".repeat(42)}B`,
      }),
    ).toEqual({ admission: "invalid" });
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: pairing.code,
        codeVerifier: RFC_VERIFIER,
      }),
    ).toMatchObject({ admission: "admitted", pairingId: pairing.pairing_id });
  });

  it("returns expired and revoked only after the PKCE proof matches", async () => {
    const expired = await approvedPairing("expired-pairing@example.com");
    const expiredAt = Date.now() - 1;
    await bindings.DB.prepare(
      `update "desktop_pairing_grant"
       set "created_at" = ?, "expires_at" = ? where "id" = ?`,
    )
      .bind(expiredAt - 90_000, expiredAt, expired.pairing_id)
      .run();
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: expired.code,
        codeVerifier: `${"A".repeat(42)}B`,
      }),
    ).toEqual({ admission: "invalid" });
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: expired.code,
        codeVerifier: RFC_VERIFIER,
      }),
    ).toEqual({ admission: "expired" });

    const revoked = await approvedPairing("revoked-pairing@example.com");
    const response = await apiRequest(`/api/devices/${revoked.device_id}/revoke`, {
      body: "{}",
      headers: jsonHeaders({ Cookie: revoked.cookie }),
      method: "POST",
    });
    expect(response.status).toBe(200);
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: revoked.code,
        codeVerifier: `${"A".repeat(42)}B`,
      }),
    ).toEqual({ admission: "invalid" });
    expect(
      await workerExports.DesktopPairingAdmission.consumePairingGrant({
        code: revoked.code,
        codeVerifier: RFC_VERIFIER,
      }),
    ).toEqual({ admission: "revoked" });
  });

  it("persists only HMAC/challenge material and normalizes internal failures", async () => {
    const pairing = await approvedPairing("redaction-pairing@example.com");
    const row = await bindings.DB.prepare(
      `select "code_hmac", "code_challenge", "request_hmac"
       from "desktop_pairing_grant" where "id" = ?`,
    )
      .bind(pairing.pairing_id)
      .first<Record<string, string>>();
    expect(row?.code_hmac).toHaveLength(43);
    expect(row?.code_challenge).toBe(RFC_CHALLENGE);
    expect(JSON.stringify(row)).not.toContain(pairing.code);
    expect(JSON.stringify(row)).not.toContain(RFC_VERIFIER);
    expect(row?.code_hmac).toBe(
      await pairingHmac(bindings.PAIRING_HMAC_SECRET, "code", pairing.code),
    );

    const unavailable = await safelyConsumePairingGrant(
      {
        db: { prepare: () => { throw new Error("private-db-error"); } } as unknown as D1Database,
        pairingHmacSecret: bindings.PAIRING_HMAC_SECRET,
      },
      { code: pairing.code, codeVerifier: RFC_VERIFIER },
    );
    expect(unavailable).toEqual({ admission: "unavailable" });
    expect(
      consumePairingGrantResultSchema.safeParse({ admission: "revoked", leaked: true })
        .success,
    ).toBe(false);
    expect(normalizeConsumePairingGrantResult({ admission: "unexpected" })).toEqual({
      admission: "unavailable",
    });
  });
});

describe("DesktopPairingRateLimitAdmin", () => {
  it("clears only the requested U authorization bucket and rejects malformed calls", async () => {
    await bindings.DB.batch([
      bindings.DB.prepare(
        `insert into "desktop_pairing_rate_limit" ("bucket", "request_count", "reset_at")
         values ('authorize:rate-limit-user-a', 5, ?)`,
      ).bind(Date.now() + 60_000),
      bindings.DB.prepare(
        `insert into "desktop_pairing_rate_limit" ("bucket", "request_count", "reset_at")
         values ('authorize:rate-limit-user-b', 5, ?)`,
      ).bind(Date.now() + 60_000),
    ]);
    const requestId = "11111111-1111-4111-8111-111111111111";
    expect(await workerExports.DesktopPairingRateLimitAdmin.resetAuthorizationRateLimit({
      requestId,
      userId: "rate-limit-user-a",
    })).toEqual({ reset: "completed", clearedAuthorizationBuckets: 1 });
    expect((await bindings.DB.prepare(
      `select "bucket" from "desktop_pairing_rate_limit" order by "bucket"`,
    ).all()).results).toEqual([{ bucket: "authorize:rate-limit-user-b" }]);
    expect(await workerExports.DesktopPairingRateLimitAdmin.resetAuthorizationRateLimit({
      requestId,
      userId: "rate-limit-user-a",
    })).toEqual({ reset: "completed", clearedAuthorizationBuckets: 0 });
    expect(await workerExports.DesktopPairingRateLimitAdmin.resetAuthorizationRateLimit({
      requestId,
      userId: "rate-limit-user-b",
      extra: true,
    } as never)).toEqual({ reset: "invalid" });
  });
});
