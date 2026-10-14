import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";

import app from "../src/index";
import {
  accessApiMethods,
  classifyAccessRequest,
} from "../src/security/access-boundary";

const bindings = env as unknown as CloudflareBindings;

function assetBinding(
  handler: (request: Request) => Response | Promise<Response>,
) {
  return {
    fetch(input: RequestInfo | URL, init?: RequestInit) {
      return handler(
        input instanceof Request ? input : new Request(input, init),
      );
    },
  } as Fetcher;
}

function accessRequest(
  path: string,
  init: RequestInit = {},
  overrides: Partial<CloudflareBindings> = {},
) {
  return app.request(new URL(path, bindings.ACCESS_ORIGIN), init, {
    ...bindings,
    ...overrides,
  });
}

describe("Access HTTP boundary", () => {
  it("redirects only the root and serves known Access shells through index.html", async () => {
    const requestedPaths: string[] = [];
    const assets = assetBinding((request) => {
      requestedPaths.push(new URL(request.url).pathname);
      return new Response("<!doctype html><title>Access</title>", {
        headers: { "Content-Type": "text/html" },
      });
    });

    const root = await accessRequest(
      "/",
      { redirect: "manual" },
      { ASSETS: assets },
    );
    expect(root.status).toBe(302);
    expect(root.headers.get("Location")).toBe("/login");
    expect(requestedPaths).toEqual([]);

    for (const path of [
      "/login",
      "/register",
      "/forgot-password",
      "/account",
    ]) {
      const response = await accessRequest(path, {}, { ASSETS: assets });
      expect(response.status, path).toBe(200);
      expect(response.headers.get("Cache-Control"), path).toBe("no-store");
      expect(response.headers.get("X-Frame-Options"), path).toBe("DENY");
      expect(response.headers.get("Content-Security-Policy"), path).toContain(
        "frame-ancestors 'none'",
      );
      expect(response.headers.get("Content-Security-Policy"), path).toContain(
        "style-src-attr 'unsafe-inline'",
      );
    }
    expect(requestedPaths).toEqual(Array(4).fill("/index.html"));
  });

  it("serves bounded malformed download links as private shells while keeping API tokens strict", async () => {
    let calls = 0;
    const assets = assetBinding(() => {
      calls++;
      return new Response("<!doctype html><title>Access</title>", {
        headers: { "Content-Type": "text/html" },
      });
    });
    for (const path of ["/d/short", "/d/" + "a".repeat(128)]) {
      for (const method of ["GET", "HEAD"]) {
        const response = await accessRequest(
          path,
          { method },
          { ASSETS: assets },
        );
        expect(response.status).toBe(200);
        expect(response.headers.get("Content-Type")).toContain("text/html");
        expect(response.headers.get("Cache-Control")).toBe("no-store");
        expect(response.headers.get("Referrer-Policy")).toBe("no-referrer");
        if (method === "HEAD") expect(await response.text()).toBe("");
      }
    }
    expect(calls).toBe(4);
    for (const path of [
      "/d/" + "a".repeat(129),
      "/d/short/extra",
      "/d/short.html",
      "/d/short%2Fextra",
      "/d/",
      "/api/distribution/download-links/short",
    ]) {
      const response = await accessRequest(path, {}, { ASSETS: assets });
      expect(response.status, path).toBe(404);
    }
    expect(calls).toBe(4);
  });

  it("serves only safe, existing build assets", async () => {
    const requestedPaths: string[] = [];
    const assets = assetBinding((request) => {
      const pathname = new URL(request.url).pathname;
      requestedPaths.push(pathname);
      expect(request.headers.get("Cookie")).toBeNull();
      expect(request.headers.get("Authorization")).toBeNull();
      return pathname === "/assets/access-ABC_123.js"
        ? new Response("export {};", {
            headers: {
              "Content-Type": "text/javascript",
              "Set-Cookie": "must-not-cross=1",
            },
          })
        : new Response("missing", { status: 404 });
    });

    const existing = await accessRequest(
      "/assets/access-ABC_123.js",
      {
        headers: {
          Authorization: "Bearer must-not-cross",
          Cookie: "session=private",
        },
      },
      { ASSETS: assets },
    );
    expect(existing.status).toBe(200);
    expect(existing.headers.get("Set-Cookie")).toBeNull();
    expect(existing.headers.get("X-Content-Type-Options")).toBe("nosniff");

    const missing = await accessRequest(
      "/images/missing.png",
      {},
      { ASSETS: assets },
    );
    expect(missing.status).toBe(404);
    expect(await missing.json()).toEqual({ error: "not_found" });
    expect(requestedPaths).toEqual([
      "/assets/access-ABC_123.js",
      "/images/missing.png",
    ]);
  });

  it("serves the new pairing logo through the static allowlist without forwarding credentials", async () => {
    const assets = assetBinding((request) => {
      expect(new URL(request.url).pathname).toBe("/onecrew-logo-pairing.png");
      expect(request.headers.get("Cookie")).toBeNull();
      expect(request.headers.get("Authorization")).toBeNull();
      return new Response(new Uint8Array([137, 80, 78, 71]), { headers: { "Content-Type": "image/png" } });
    });
    const response = await accessRequest("/onecrew-logo-pairing.png", {
      headers: { Cookie: "session=private", Authorization: "Bearer must-not-cross" },
    }, { ASSETS: assets });
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("image/png");
    expect(response.headers.get("X-Content-Type-Options")).toBe("nosniff");
  });

  it("keeps crawlers out of every Access route", async () => {
    const response = await accessRequest("/robots.txt");
    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(await response.text()).toBe("User-agent: *\nDisallow: /\n");

    const head = await accessRequest("/robots.txt", { method: "HEAD" });
    expect(head.status).toBe(200);
    expect(await head.text()).toBe("");
  });

  it("rejects marketing, sync, admin and unknown sensitive paths without asset fallback", async () => {
    let assetCalls = 0;
    const assets = assetBinding(() => {
      assetCalls += 1;
      return new Response("unexpected");
    });
    for (const path of [
      "/product",
      "/use-cases",
      "/resources/guide",
      "/sync",
      "/admin",
      "/api/sync/installations",
      "/api/admin/users",
      "/.env",
      "/wrangler.toml",
      "/login/",
      "/api/session//login",
      "/api/session/%6cogin",
      "/assets/%2e%2e/admin.js",
      "/unknown",
    ]) {
      const response = await accessRequest(path, {}, { ASSETS: assets });
      expect(response.status, path).toBe(404);
      expect(await response.json(), path).toEqual({ error: "not_found" });
    }
    expect(assetCalls).toBe(0);
  });

  it("requires the configured request host and rejects an explicit conflicting Host", async () => {
    const wrongUrl = await app.request(
      "http://other.invalid/login",
      {},
      bindings,
    );
    expect(wrongUrl.status).toBe(404);
    expect(await wrongUrl.json()).toEqual({ error: "not_found" });

    const wrongHeader = await accessRequest("/login", {
      headers: { Host: "other.invalid" },
    });
    expect(wrongHeader.status).toBe(404);
  });

  it("fails closed before routing when configured origins diverge", async () => {
    const response = await accessRequest(
      "/api/health",
      {},
      { WEB_ORIGIN: "http://localhost:5173" },
    );
    expect(response.status).toBe(500);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(await response.json()).toEqual({
      error: "service_configuration_error",
    });
  });

  it("allows methods and preflight only for real API paths", async () => {
    expect(accessApiMethods("/api/auth/sign-in/social")).toEqual(["POST"]);
    expect(
      accessApiMethods("/api/devices/dev_0123456789abcdefghijklmn/status"),
    ).toEqual(["GET"]);
    expect(accessApiMethods("/api/devices/not-a-device/status")).toBeNull();
    expect(accessApiMethods("/api/admin/users")).toBeNull();
    expect(classifyAccessRequest("GET", "/api/auth/sign-in/social")).toEqual({
      kind: "method-not-allowed",
      allow: ["POST", "OPTIONS"],
    });

    const socialGet = await accessRequest("/api/auth/sign-in/social");
    expect(socialGet.status).toBe(405);
    expect(socialGet.headers.get("Allow")).toBe("POST, OPTIONS");

    const rootPost = await accessRequest("/", { method: "POST" });
    expect(rootPost.status).toBe(405);
    expect(rootPost.headers.get("Allow")).toBe("GET, HEAD");

    const validPreflight = await accessRequest("/api/session/login", {
      headers: {
        "Access-Control-Request-Headers": "content-type",
        "Access-Control-Request-Method": "POST",
        Origin: bindings.WEB_ORIGIN,
      },
      method: "OPTIONS",
    });
    expect(validPreflight.status).toBe(204);

    const invalidPreflight = await accessRequest("/api/session/login", {
      headers: { "Access-Control-Request-Method": "DELETE" },
      method: "OPTIONS",
    });
    expect(invalidPreflight.status).toBe(405);

    const unknownPreflight = await accessRequest("/api/admin/users", {
      headers: { "Access-Control-Request-Method": "GET" },
      method: "OPTIONS",
    });
    expect(unknownPreflight.status).toBe(404);
  });
});
