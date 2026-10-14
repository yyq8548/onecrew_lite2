import { describe, expect, it, vi } from "vitest";

import pages from "../marketing-pages.json";
import siteWorker, { MARKETING_PATHS, type SiteEnv } from "./index";
import { SITE_CSP_HASHES } from "./site-csp-manifest.generated";

const SITE_ORIGIN = "https://onecrewai.io";
const WWW_ORIGIN = "https://www.onecrewai.io";

function fixture() {
  const fetch = vi.fn(async (request: Request) => {
    const url = new URL(request.url);
    return new Response(url.pathname.endsWith("/index.html") ? "<!doctype html><title>OneCrew</title>" : "asset", {
      headers: {
        "Content-Type": url.pathname.endsWith("/index.html") ? "text/html" : "application/octet-stream",
        "Set-Cookie": "must-not-cross=1",
      },
    });
  });
  return {
    env: { ASSETS: { fetch }, SITE_ORIGIN, WWW_ORIGIN } as unknown as SiteEnv,
    fetch,
  };
}

describe("OneCrew public site worker", () => {
  it("serves all 48 localized marketing paths from their prerendered HTML", async () => {
    expect(pages).toHaveLength(24);
    expect(MARKETING_PATHS.size).toBe(48);
    const paths = pages.flatMap(({ path }) => [
      path === "/" ? "/zh" : `/zh${path}`,
      path,
    ]);
    for (const path of paths) {
      const { env, fetch } = fixture();
      const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}${path}`), env);
      expect(response.status, path).toBe(200);
      const csp = response.headers.get("Content-Security-Policy")!;
      expect(csp).toBe(`default-src 'none'; script-src 'self' ${SITE_CSP_HASHES[path as keyof typeof SITE_CSP_HASHES].join(" ")} https://static.cloudflareinsights.com https://www.googletagmanager.com; script-src-attr 'none'; style-src 'self'; style-src-attr 'none'; img-src 'self' data: https://*.google-analytics.com https://www.googletagmanager.com; font-src 'self'; connect-src 'self' https://*.google-analytics.com https://*.analytics.google.com https://www.googletagmanager.com; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`);
      expect(csp).not.toMatch(/unsafe-inline|unsafe-eval|unsafe-hashes|strict-dynamic/);
      expect(response.headers.has("Set-Cookie")).toBe(false);
      expect(fetch).toHaveBeenCalledOnce();
      expect(new URL(fetch.mock.calls[0]![0].url).pathname).toBe(
        path === "/" ? "/index.html" : `${path}/index.html`,
      );
    }
  });

  it("redirects old English-prefixed paths to their exact canonical English path", async () => {
    for (const { path } of pages) {
      const { env, fetch } = fixture();
      const source = path === "/" ? "/en" : `/en${path}`;
      const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}${source}?legacy=1`), env);
      expect(response.status, path).toBe(308);
      expect(response.headers.get("Location")).toBe(`${SITE_ORIGIN}${path}?legacy=1`);
      expect(fetch).not.toHaveBeenCalled();
    }
  });

  it.each([
    ["https://onecrewai.io/en", `${SITE_ORIGIN}/`],
    ["https://onecrewai.io/en/product?from=legacy", `${SITE_ORIGIN}/product?from=legacy`],
    ["http://onecrewai.io/product?from=http", `${SITE_ORIGIN}/product?from=http`],
    ["http://onecrewai.io/en/product?from=http", `${SITE_ORIGIN}/product?from=http`],
    ["https://www.onecrewai.io/product?from=www", `${SITE_ORIGIN}/product?from=www`],
    ["https://www.onecrewai.io/en/about?from=www", `${SITE_ORIGIN}/about?from=www`],
    ["https://www.onecrewai.io/", `${SITE_ORIGIN}/`],
  ])("redirects a known URL directly to its final canonical URL for %s", async (requestUrl, location) => {
    const { env, fetch } = fixture();
    const response = await siteWorker.fetch(new Request(requestUrl), env);
    expect(response.status).toBe(308);
    expect(response.headers.get("Location")).toBe(location);
    expect(response.headers.get("Content-Security-Policy")).not.toContain("sha256-");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("serves the root directly regardless of language preference or crawler identity", async () => {
    for (const agent of ["Mozilla/5.0", "Googlebot"]) {
      const { env, fetch } = fixture();
      const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}/`, {headers:{"Accept-Language":"zh-CN", "User-Agent":agent, Cookie:"i18next=zh-CN"}}), env);
      expect(response.status).toBe(200);
      expect(response.headers.has("Location")).toBe(false);
      expect(new URL(fetch.mock.calls[0]![0].url).pathname).toBe("/index.html");
    }
  });

  it("canonicalizes every real marketing alias in one hop without changing query bytes", async () => {
    const query = "?utm_source=x&x=1&x=2&blank=&plus=a+b&space=a%20b&escaped=%2f%3F";
    for (const { path } of pages) {
      const english = path === "/" ? "/en" : `/en${path}`;
      const chinese = path === "/" ? "/zh" : `/zh${path}`;
      const aliases = [[english, path], [english + "/", path], [chinese + "/", chinese]];
      if (path !== "/") aliases.push([path + "/", path]);
      for (const [alias, target] of aliases) {
        for (const origin of [SITE_ORIGIN, WWW_ORIGIN, "http://onecrewai.io", "http://www.onecrewai.io"]) {
          const { env, fetch } = fixture();
          const response = await siteWorker.fetch(new Request(origin + alias + query), env);
          expect(response.status, origin + alias).toBe(308);
          expect(response.headers.get("Location")).toBe(SITE_ORIGIN + target + query);
          expect(fetch).not.toHaveBeenCalled();
          const destination = await siteWorker.fetch(new Request(response.headers.get("Location")!), env);
          expect(destination.status).toBe(200);
          expect(destination.headers.has("Location")).toBe(false);
        }
      }
    }
  });

  it("does not canonicalize missing pages, sensitive paths or static filenames", async () => {
    for (const path of ["/en/missing/", "/zh/missing/", "/product/missing/", "/en/zh/product/", "/product//", "/en/api/session/", "/api/session/", "/register/", "/login/", "/account/", "/robots.txt/", "/en/ga4-init.js/"]) {
      const { env, fetch } = fixture();
      const response = await siteWorker.fetch(new Request(SITE_ORIGIN + path), env);
      expect(response.status, path).toBe(404);
      expect(response.headers.has("Location")).toBe(false);
      expect(fetch).not.toHaveBeenCalled();
    }
    const { env, fetch } = fixture();
    expect((await siteWorker.fetch(new Request("https://access.onecrewai.io/zh/product/"), env)).status).toBe(421);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("does not redirect unknown URLs on apex, HTTP, or www", async () => {
    for (const origin of [SITE_ORIGIN, "http://onecrewai.io", WWW_ORIGIN]) {
      const { env, fetch } = fixture();
      const response = await siteWorker.fetch(new Request(`${origin}/not-a-page`), env);
      expect(response.status).toBe(404);
      expect(response.headers.has("Location")).toBe(false);
      expect(fetch).not.toHaveBeenCalled();
    }
  });

  it.each(["/api/session", "/login", "/register", "/account", "/pairing/authorize"])(
    "returns 404 for sensitive path %s without consulting assets",
    async (path) => {
      const { env, fetch } = fixture();
      const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}${path}`), env);
      expect(response.status).toBe(404);
      expect(fetch).not.toHaveBeenCalled();
    },
  );

  it("rejects alternate hosts before assets", async () => {
    const { env, fetch } = fixture();
    const response = await siteWorker.fetch(new Request("https://site.example.com/zh/product"), env);
    expect(response.status).toBe(421);
    const spoofedHost = await siteWorker.fetch(new Request(`${SITE_ORIGIN}/zh/product`, {
      headers: { Host: "site.example.com" },
    }), env);
    expect(spoofedHost.status).toBe(421);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("allows only GET and HEAD on public paths", async () => {
    const { env, fetch } = fixture();
    const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}/zh/product`, { method: "POST" }), env);
    expect(response.status).toBe(405);
    expect(response.headers.get("Allow")).toBe("GET, HEAD");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("serves the GA4 initializer with an executable MIME type", async () => {
    const { env, fetch } = fixture();
    const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}/ga4-init.js`), env);
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("text/javascript; charset=UTF-8");
    expect(new URL(fetch.mock.calls[0]![0].url).pathname).toBe("/ga4-init.js");
  });


  it.each([
    "/images/about/hero.png",
    "/images/about/world-dots.svg",
    "/images/footer-ai/grok.svg",
  ])("serves redesign assets at %s through the public static boundary", async (path) => {
    const { env, fetch } = fixture();
    const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}${path}`), env);
    expect(response.status).toBe(200);
    expect(new URL(fetch.mock.calls[0]![0].url).pathname).toBe(path);
  });

  it("serves only allowlisted static paths and strips credentials before ASSETS", async () => {
    const { env, fetch } = fixture();
    const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}/assets/site.js`, {
      headers: { Authorization: "Bearer secret", Cookie: "session=secret" },
    }), env);
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Security-Policy")).not.toContain("sha256-");
    const forwarded = fetch.mock.calls[0]![0];
    expect(forwarded.headers.has("Authorization")).toBe(false);
    expect(forwarded.headers.has("Cookie")).toBe(false);

    const unknown = await siteWorker.fetch(new Request(`${SITE_ORIGIN}/private.txt`), env);
    expect(unknown.status).toBe(404);
    expect(unknown.headers.get("Content-Security-Policy")).not.toContain("sha256-");
  });

  it("does not attach page hashes when the page asset is not an HTML success", async () => {
    const { env, fetch } = fixture();
    fetch.mockResolvedValueOnce(new Response("Not found", {
      status: 404,
      headers: { "Content-Type": "text/plain" },
    }));
    const response = await siteWorker.fetch(new Request(`${SITE_ORIGIN}/zh/about`), env);

    expect(response.status).toBe(404);
    expect(response.headers.get("Content-Security-Policy")).not.toContain("sha256-");
  });

  it("declares UTF-8 for text and XML metadata", async () => {
    const { env } = fixture();
    const llms = await siteWorker.fetch(
      new Request(`${SITE_ORIGIN}/llms.txt`),
      env,
    );
    const sitemap = await siteWorker.fetch(
      new Request(`${SITE_ORIGIN}/sitemap.xml`),
      env,
    );

    expect(llms.headers.get("Content-Type")).toBe("text/plain; charset=UTF-8");
    expect(sitemap.headers.get("Content-Type")).toBe("application/xml; charset=UTF-8");
  });

  it("fails closed when SITE_ORIGIN is missing or malformed", async () => {
    const { env, fetch } = fixture();
    await expect(siteWorker.fetch(new Request(`${SITE_ORIGIN}/zh`), { ...env, SITE_ORIGIN: "" })).resolves.toMatchObject({ status: 503 });
    await expect(siteWorker.fetch(new Request(`${SITE_ORIGIN}/zh`), { ...env, SITE_ORIGIN: "http://onecrewai.invalid" })).resolves.toMatchObject({ status: 503 });
    await expect(siteWorker.fetch(new Request(`${SITE_ORIGIN}/zh`), { ...env, WWW_ORIGIN: "" })).resolves.toMatchObject({ status: 503 });
    await expect(siteWorker.fetch(new Request(`${SITE_ORIGIN}/zh`), { ...env, WWW_ORIGIN: "https://other.onecrewai.io" })).resolves.toMatchObject({ status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });
});
