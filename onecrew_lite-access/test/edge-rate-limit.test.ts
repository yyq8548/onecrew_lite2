import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import {
  edgeRateLimitGroup,
  edgeRateLimitKey,
  passesEdgeRateLimit,
} from "../src/rate-limit/edge";
import app from "../src/index";

const bindings = env as unknown as CloudflareBindings;

function limiter(
  success: boolean,
  observedKeys: string[],
): RateLimit {
  return {
    async limit({ key }) {
      observedKeys.push(key);
      return { success };
    },
  };
}

function apiRequest(
  path: string,
  edgeLimiter: RateLimit,
  init: RequestInit = {},
  overrides: Partial<CloudflareBindings> = {},
) {
  return app.request(
    new URL(path, bindings.BETTER_AUTH_URL),
    init,
    { ...bindings, ...overrides, EDGE_RATE_LIMITER: edgeLimiter },
  );
}

describe("Cloudflare in-Worker rate limit gate", () => {
  it("groups auth, session, device and pairing-preview reads separately", () => {
    expect(edgeRateLimitGroup("POST", "/api/session/register")).toBe("auth");
    expect(edgeRateLimitGroup("POST", "/api/session/login")).toBe("auth");
    expect(edgeRateLimitGroup("POST", "/api/session/forgot-password/send")).toBe("auth");
    expect(edgeRateLimitGroup("POST", "/api/session/forgot-password")).toBe("auth");
    expect(edgeRateLimitGroup("POST", "/api/auth/sign-in/social")).toBe("auth");
    expect(edgeRateLimitGroup("GET", "/api/auth/sign-in/social")).toBeNull();
    expect(edgeRateLimitGroup("GET", "/api/auth/callback/google")).toBe("auth");
    expect(edgeRateLimitGroup("GET", "/api/session")).toBe("session");
    expect(edgeRateLimitGroup("POST", "/api/session/logout")).toBe("session");
    expect(edgeRateLimitGroup("GET", "/api/account/settings")).toBe("session");
    expect(edgeRateLimitGroup("POST", "/api/account/password")).toBe("session");
    expect(edgeRateLimitGroup("GET", "/api/devices")).toBe("device");
    expect(edgeRateLimitGroup("GET", "/api/devices/dev_example/status")).toBe("device");
    expect(edgeRateLimitGroup("GET", "/api/pairing/authorize")).toBe("pairing_read");
    expect(edgeRateLimitGroup("POST", "/api/pairing/authorize")).toBe("pairing_write");
    expect(edgeRateLimitGroup("GET", "/api/health")).toBeNull();
    expect(edgeRateLimitGroup("GET", "/api/auth/capabilities")).toBeNull();
  });

  it("uses a short-lived edge key and never an Access D1 IP bucket", () => {
    expect(edgeRateLimitKey("auth", "203.0.113.90")).toBe(
      "auth:203.0.113.90",
    );
    expect(edgeRateLimitKey("device", "invalid/client/value")).toBe(
      "device:unknown-client",
    );
  });

  it("rate limits install issuance and reads without putting credentials in limiter keys", async () => {
    const observedKeys: string[] = [];
    for (const path of ["/api/cli/install-links", `/cli/install/v1.${"a".repeat(100)}`, `/cli/download/v1.${"b".repeat(100)}/install.sh`]) {
      expect(edgeRateLimitGroup("GET", path)).toBe("cli_install");
      const result = await apiRequest(path, limiter(false, observedKeys), { headers: { "cf-connecting-ip": "203.0.113.92" } });
      expect(result.status).toBe(429);
      expect(result.headers.get("Retry-After")).toBe("60");
    }
    expect(observedKeys).toEqual(Array(3).fill("cli_install:203.0.113.92"));
  });

  it("returns readable CORS errors before auth or D1 work", async () => {
    const observedKeys: string[] = [];
    const deny = limiter(false, observedKeys);
    const requests = [
      apiRequest(
        "/api/session/login",
        deny,
        {
          headers: {
            Origin: bindings.WEB_ORIGIN,
            "cf-connecting-ip": "203.0.113.91",
          },
          method: "POST",
        },
        { BETTER_AUTH_SECRET: "invalid-if-runtime-config-runs" },
      ),
      apiRequest("/api/session", deny, {
        headers: {
          Origin: bindings.WEB_ORIGIN,
          "cf-connecting-ip": "203.0.113.91",
        },
      }),
      apiRequest("/api/devices", deny, {
        headers: {
          Origin: bindings.WEB_ORIGIN,
          "cf-connecting-ip": "203.0.113.91",
        },
      }),
      apiRequest("/api/pairing/authorize", deny, {
        headers: {
          Origin: bindings.WEB_ORIGIN,
          "cf-connecting-ip": "203.0.113.91",
        },
      }),
    ];
    for (const response of await Promise.all(requests)) {
      expect(response.status).toBe(429);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
      expect(response.headers.get("Access-Control-Allow-Origin")).toBe(
        bindings.WEB_ORIGIN,
      );
      expect(response.headers.get("Access-Control-Allow-Credentials")).toBe(
        "true",
      );
      expect(await response.json()).toEqual({ error: "rate_limited" });
    }
    expect(observedKeys.sort()).toEqual([
      "auth:203.0.113.91",
      "device:203.0.113.91",
      "pairing_read:203.0.113.91",
      "session:203.0.113.91",
    ]);
  });

  it("keeps pairing confirmation writes in a separate edge bucket", async () => {
    const observedKeys: string[] = [];
    const response = await apiRequest("/api/pairing/authorize", limiter(false, observedKeys), {
      headers: {
        Origin: bindings.WEB_ORIGIN,
        "cf-connecting-ip": "203.0.113.93",
      },
      method: "POST",
    });
    expect(response.status).toBe(429);
    expect(await response.json()).toEqual({ error: "rate_limited" });
    expect(observedKeys).toEqual(["pairing_write:203.0.113.93"]);
  });

  it("redirects a limited pairing form navigation instead of exposing JSON", async () => {
    const observedKeys: string[] = [];
    const response = await apiRequest("/api/pairing/authorize", limiter(false, observedKeys), {
      headers: {
        Origin: bindings.WEB_ORIGIN,
        "Sec-Fetch-Mode": "navigate",
        "cf-connecting-ip": "203.0.113.95",
      },
      method: "POST",
      redirect: "manual",
    });
    expect(response.status).toBe(303);
    expect(response.headers.get("Location")).toBe("/pairing/rate-limited");
    expect(response.headers.get("Retry-After")).toBe("60");
    expect(await response.text()).toBe("");
    expect(observedKeys).toEqual(["pairing_write:203.0.113.95"]);
  });

  it("redirects a limited top-level pairing read instead of exposing JSON", async () => {
    const observedKeys: string[] = [];
    const response = await apiRequest("/pairing/authorize", limiter(false, observedKeys), {
      headers: { "cf-connecting-ip": "203.0.113.94" },
      redirect: "manual",
    });
    expect(response.status).toBe(303);
    expect(response.headers.get("Location")).toBe("/pairing/rate-limited");
    expect(response.headers.get("Retry-After")).toBe("60");
    expect(await response.text()).toBe("");
    expect(observedKeys).toEqual(["pairing_read:203.0.113.94"]);
  });

  it("rejects untrusted origins and completes preflight before counting", async () => {
    const observedKeys: string[] = [];
    const allow = limiter(true, observedKeys);
    const rejected = await apiRequest("/api/session", allow, {
      headers: {
        Origin: "https://attacker.example",
        "cf-connecting-ip": "203.0.113.92",
      },
    });
    expect(rejected.status).toBe(403);
    expect(rejected.headers.get("Access-Control-Allow-Origin")).toBeNull();
    expect(observedKeys).toEqual([]);

    const preflight = await apiRequest("/api/session/login", allow, {
      headers: {
        "Access-Control-Request-Headers": "content-type",
        "Access-Control-Request-Method": "POST",
        Origin: bindings.WEB_ORIGIN,
        "cf-connecting-ip": "203.0.113.92",
      },
      method: "OPTIONS",
    });
    expect(preflight.status).toBe(204);
    expect(preflight.headers.get("Access-Control-Allow-Origin")).toBe(
      bindings.WEB_ORIGIN,
    );
    expect(observedKeys).toEqual([]);
  });

  it("bypasses public health and explicitly allows when the binding errors", async () => {
    const observedKeys: string[] = [];
    const deny = limiter(false, observedKeys);
    const health = await apiRequest("/api/health", deny);
    expect(health.status).toBe(200);
    expect(observedKeys).toEqual([]);

    const unavailable = {
      async limit() {
        throw new Error("edge limiter unavailable");
      },
    } satisfies RateLimit;
    expect(
      await passesEdgeRateLimit(
        unavailable,
        "auth",
        "203.0.113.92",
      ),
    ).toBe(true);
  });
});
