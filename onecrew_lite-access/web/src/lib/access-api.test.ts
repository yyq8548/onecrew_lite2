import { afterEach, describe, expect, it, vi } from "vitest";

import {
  AccessApiError,
  AccessContractError,
  deleteAccessDevice,
  getAccessDeviceStatus,
  getAuthCapabilities,
  getDesktopPairingPreview,
  getSafeSession,
  isSafeSessionAuthenticated,
  listAccessDevices,
  loginAccessSession,
  logoutAccessSession,
  registerAccessSession,
  revokeAccessDevice,
  sendLoginOTP,
  sendRegistrationOTP,
} from "./access-api";
import accessI18n from "./i18n";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("safe access API client", () => {
  it("restores only the safe session DTO with credentialed fetch and no browser storage", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({
        authenticated: true,
        expiresAt: "2026-09-01T00:00:00.000Z",
        user: {
          email: "alex@example.com",
          id: "user-1",
          image: null,
          name: "Alex Chen",
        },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const session = await getSafeSession();

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/session",
      expect.objectContaining({ cache: "no-store", credentials: "include", method: "GET" }),
    );
    expect(session).not.toHaveProperty("token");
    expect(session.user).not.toHaveProperty("token");
    expect(setItem).not.toHaveBeenCalled();
  });

  it("rejects a token-bearing response instead of tolerating contract drift", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({
          authenticated: false,
          expiresAt: null,
          token: "must-not-cross-public-boundary",
          user: null,
        }),
      ),
    );

    await expect(getSafeSession()).rejects.toThrow();
  });

  it("rejects a non-ISO session expiry", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({
          authenticated: true,
          expiresAt: "tomorrow afternoon",
          user: {
            email: "alex@example.com",
            id: "user-1",
            image: null,
            name: "Alex Chen",
          },
        }),
      ),
    );

    await expect(getSafeSession()).rejects.toThrow();
  });

  it("treats an authenticated projection as current only before its ISO expiry", () => {
    const session = {
      authenticated: true,
      expiresAt: "2026-08-26T12:00:01.000Z",
      user: {
        email: "alex@example.com",
        id: "user-1",
        image: null,
        name: "Alex Chen",
      },
    } as const;

    expect(isSafeSessionAuthenticated(session, Date.parse("2026-08-26T12:00:00.000Z"))).toBe(true);
    expect(isSafeSessionAuthenticated(session, Date.parse("2026-08-26T12:00:01.000Z"))).toBe(false);
  });

  it("uses custom email session routes with HttpOnly-cookie credentials", async () => {
    const fetchMock = vi.fn().mockImplementation((url: string) => {
      if (url === "/api/session/register") {
        return Promise.resolve(jsonResponse({
          ok: true,
          verification_required: false,
          email: "alex@example.com",
        }));
      }
      if (url === "/api/session/login") {
        return Promise.resolve(jsonResponse({
          ok: true,
          user: {
            email: "alex@example.com",
            id: "user-1",
            image: null,
            name: "Alex Chen",
          },
        }));
      }
      return Promise.resolve(jsonResponse({ ok: true }));
    });
    vi.stubGlobal("fetch", fetchMock);

    await registerAccessSession({
      email: "alex@example.com",
      name: "Alex Chen",
      password: "password123",
    });
    await loginAccessSession({ email: "alex@example.com", password: "password123" });
    await sendLoginOTP("alex@example.com");
    await sendRegistrationOTP("alex@example.com");
    await logoutAccessSession();

    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/api/session/register",
      "/api/session/login",
      "/api/session/login-otp/send",
      "/api/session/register/verify/send",
      "/api/session/logout",
    ]);
    for (const [, init] of fetchMock.mock.calls) {
      expect(init).toEqual(expect.objectContaining({ credentials: "include", method: "POST" }));
    }
  });

  it("sends the selected UI language to the Worker", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ ok: true }));
    vi.stubGlobal("fetch", fetchMock);
    await accessI18n.changeLanguage("en-US");

    await sendLoginOTP("alex@example.com");

    expect(fetchMock.mock.calls[0]?.[1]).toEqual(
      expect.objectContaining({
        headers: expect.objectContaining({ "Accept-Language": "en-US" }),
      }),
    );
    await accessI18n.changeLanguage("zh-CN");
  });

  it("strictly reads the Google capability", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(jsonResponse({ googleSignInEnabled: true })),
    );

    await expect(getAuthCapabilities()).resolves.toEqual({ googleSignInEnabled: true });
  });

  it("strictly previews loopback pairing without touching browser storage", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const preview = {
      approvalToken: "reacher_pairing_approval_v1." + "A".repeat(180),
      expiresAt: "2026-08-31T12:05:00.000Z",
      request: {
        appName: "Agent Platform",
        appVersion: "1.2.3",
        callbackPort: "49152",
        label: "Studio Mac",
        platform: "macos",
        platformLabel: "macOS",
        bindingIp: null,
        bindingLocation: { country: null, region: null, city: null, colo: null },
      },
    };
    const fetchMock = vi.fn().mockResolvedValueOnce(jsonResponse(preview));
    vi.stubGlobal("fetch", fetchMock);

    await expect(getDesktopPairingPreview("?state=request")).resolves.toEqual(preview);
    expect(setItem).not.toHaveBeenCalled();
  });

  it("uses strict safe contracts for list, status, revoke and delete", async () => {
    const first = deviceSummary();
    const active = deviceSummary({
      pairedAt: "2026-08-31T12:05:00.000Z",
      status: "active",
    });
    const revoked = deviceSummary({
      revokedAt: "2026-08-31T12:06:00.000Z",
      status: "revoked",
    });
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({ devices: [first], nextCursor: "dev_cursor/next" }),
      )
      .mockResolvedValueOnce(jsonResponse({ device: active }))
      .mockResolvedValueOnce(jsonResponse({ device: revoked }))
      .mockResolvedValueOnce(jsonResponse({ deletedDeviceId: first.deviceId }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(listAccessDevices("dev_cursor/next")).resolves.toEqual({
      devices: [first],
      nextCursor: "dev_cursor/next",
    });
    await expect(getAccessDeviceStatus(first.deviceId)).resolves.toEqual(active);
    await expect(revokeAccessDevice(first.deviceId)).resolves.toEqual(revoked);
    await expect(deleteAccessDevice(first.deviceId)).resolves.toBe(first.deviceId);

    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/api/devices?cursor=dev_cursor%2Fnext",
      `/api/devices/${first.deviceId}/status`,
      `/api/devices/${first.deviceId}/revoke`,
      `/api/devices/${first.deviceId}`,
    ]);
    expect(fetchMock.mock.calls[2]?.[1]).toEqual(
      expect.objectContaining({ body: "{}", method: "POST" }),
    );
    expect(fetchMock.mock.calls[3]?.[1]).toEqual(
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("rejects secret-bearing safe DTO drift and discards unsafe error bodies", async () => {
    const secret = "raw-pairing-code-must-not-cross-this-boundary";
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(
          jsonResponse({
            devices: [{ ...deviceSummary(), code: secret }],
            nextCursor: null,
          }),
        )
        .mockResolvedValueOnce(
          jsonResponse({ error: "rate_limited", detail: secret }, 429),
        ),
    );

    await expect(listAccessDevices()).rejects.toBeInstanceOf(AccessContractError);
    const failure = await listAccessDevices().catch(
      (error: unknown) => error,
    );
    expect(failure).toBeInstanceOf(AccessApiError);
    expect(failure).toMatchObject({ code: undefined, status: 429 });
    expect(String(failure)).not.toContain(secret);
    expect(JSON.stringify(failure)).not.toContain(secret);
  });
});

function deviceSummary(overrides: Record<string, unknown> = {}) {
  return {
    appVersion: "1.2.3",
    createdAt: "2026-08-31T12:00:00.000Z",
    deviceId: "dev_000000000000000000000000",
    label: "Studio Mac",
    pairedAt: null,
    pairingExpiresAt: "2026-08-31T12:01:30.000Z",
    pairingId: "pair_000000000000000000000000",
    pairingIp: null,
    pairingLocation: { country: null, region: null, city: null, colo: null },
    platform: "macos",
    revokedAt: null,
    status: "pending",
    ...overrides,
  };
}

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    headers: { "Content-Type": "application/json" },
    status,
  });
}
