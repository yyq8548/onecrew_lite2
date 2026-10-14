import pages from "../marketing-pages.json";
import { SITE_CSP_HASHES } from "./site-csp-manifest.generated";

export interface SiteEnv {
  ASSETS: Fetcher;
  SITE_ORIGIN: string;
  WWW_ORIGIN: string;
}

function localizedMarketingPath(path: string, locale: "en" | "zh") {
  if (locale === "en") return path;
  return path === "/" ? "/zh" : `/zh${path}`;
}

const ENGLISH_ALIASES = new Map(pages.map(({ path }) => [path === "/" ? "/en" : `/en${path}`, path]));
const MARKETING_PATHS = new Set(pages.flatMap(({ path }) => [
  localizedMarketingPath(path, "zh"),
  localizedMarketingPath(path, "en"),
]));
const STATIC_PATHS = new Set([
  "/ga4-init.js",
  "/claude-mark.png",
  "/codex-mark.png",
  "/llms.txt",
  "/onecrew-ai-mark.png",
  "/onecrew-favicon.svg",
  "/onecrew-logo-dark.svg",
  "/onecrew-watermark-mark.svg",
  "/robots.txt",
  "/sitemap.xml",
]);
const STATIC_PREFIXES = ["/assets/", "/images/"];
const SENSITIVE_PREFIXES = [
  "/api",
  "/account",
  "/forgot-password",
  "/login",
  "/pairing",
  "/register",
];

function plainResponse(status: number, message: string, extraHeaders?: HeadersInit) {
  return withSecurityHeaders(new Response(message, {
    status,
    headers: {
      "Content-Type": "text/plain; charset=UTF-8",
      ...extraHeaders,
    },
  }));
}

function withSecurityHeaders(
  response: Response,
  html = false,
  contentType?: string,
  scriptHashes: readonly string[] = [],
) {
  const headers = new Headers(response.headers);
  headers.delete("Set-Cookie");
  if (contentType) headers.set("Content-Type", contentType);
  const hashSources = scriptHashes.length > 0 ? ` ${scriptHashes.join(" ")}` : "";
  headers.set("Content-Security-Policy", `default-src 'none'; script-src 'self'${hashSources} https://static.cloudflareinsights.com https://www.googletagmanager.com; script-src-attr 'none'; style-src 'self'; style-src-attr 'none'; img-src 'self' data: https://*.google-analytics.com https://www.googletagmanager.com; font-src 'self'; connect-src 'self' https://*.google-analytics.com https://*.analytics.google.com https://www.googletagmanager.com; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`);
  headers.set("Cross-Origin-Resource-Policy", "same-origin");
  headers.set("Permissions-Policy", "camera=(), geolocation=(), microphone=()");
  headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("X-Frame-Options", "DENY");
  if (html) headers.set("Cache-Control", "public, max-age=0, must-revalidate");
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

function isSensitivePath(pathname: string) {
  return SENSITIVE_PREFIXES.some((prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`));
}

function isStaticPath(pathname: string) {
  return STATIC_PATHS.has(pathname) || STATIC_PREFIXES.some((prefix) => pathname.startsWith(prefix));
}

function canonicalMarketingPath(pathname: string) {
  const path = pathname.length > 1 && pathname.endsWith("/") ? pathname.slice(0, -1) : pathname;
  return ENGLISH_ALIASES.get(path) ?? (MARKETING_PATHS.has(path) ? path : null);
}

function redirectPath(pathname: string) {
  const canonical = canonicalMarketingPath(pathname);
  if (canonical) return canonical;
  if (isStaticPath(pathname)) return pathname;
  return null;
}

function staticContentType(pathname: string) {
  if (pathname.endsWith(".js")) return "text/javascript; charset=UTF-8";
  if (pathname.endsWith(".txt")) return "text/plain; charset=UTF-8";
  if (pathname.endsWith(".xml")) return "application/xml; charset=UTF-8";
  return undefined;
}

function assetRequest(request: Request, pathname: string) {
  const url = new URL(request.url);
  url.pathname = pathname;
  url.search = "";
  const headers = new Headers(request.headers);
  headers.delete("Authorization");
  headers.delete("Cookie");
  return new Request(url, { headers, method: request.method });
}

function configuredOrigin(value: string) {
  let origin: URL;
  try {
    origin = new URL(value);
  } catch {
    return null;
  }
  if (
    origin.protocol !== "https:" ||
    origin.origin !== value ||
    origin.pathname !== "/" ||
    origin.port !== ""
  ) {
    return null;
  }
  return origin.origin;
}

export const siteWorker = {
  async fetch(request: Request, env: SiteEnv) {
    const expectedOrigin = configuredOrigin(env.SITE_ORIGIN);
    const wwwOrigin = configuredOrigin(env.WWW_ORIGIN);
    const url = new URL(request.url);
    if (
      !expectedOrigin
      || !wwwOrigin
      || new URL(wwwOrigin).hostname !== `www.${new URL(expectedOrigin).hostname}`
    ) {
      return plainResponse(503, "Service unavailable");
    }
    const host = request.headers.get("Host");
    const urlHost = url.host.toLowerCase();
    const requestHost = (host ?? url.host).toLowerCase();
    const apexHost = new URL(expectedOrigin).host.toLowerCase();
    const wwwHost = new URL(wwwOrigin).host.toLowerCase();
    if (requestHost !== urlHost) return plainResponse(421, "Misdirected request");
    const knownRedirectPath = redirectPath(url.pathname);
    if ((urlHost === apexHost || urlHost === wwwHost) && (url.protocol === "http:" || urlHost === wwwHost)) {
      if (!knownRedirectPath) return plainResponse(404, "Not found");
      return withSecurityHeaders(Response.redirect(
        `${expectedOrigin}${knownRedirectPath}${url.search}`,
        308,
      ));
    }
    if (
      url.origin !== expectedOrigin ||
      urlHost !== apexHost
    ) {
      return plainResponse(421, "Misdirected request");
    }
    if (isSensitivePath(url.pathname)) return plainResponse(404, "Not found");
    const canonical = canonicalMarketingPath(url.pathname);
    if (canonical && canonical !== url.pathname) {
      return withSecurityHeaders(Response.redirect(
        `${expectedOrigin}${canonical}${url.search}`,
        308,
      ));
    }
    if (request.method !== "GET" && request.method !== "HEAD") {
      return plainResponse(405, "Method not allowed", { Allow: "GET, HEAD" });
    }

    if (MARKETING_PATHS.has(url.pathname)) {
      const scriptHashes = SITE_CSP_HASHES[url.pathname as keyof typeof SITE_CSP_HASHES];
      if (!scriptHashes) {
        return plainResponse(503, "Service unavailable");
      }
      const assetResponse = await env.ASSETS.fetch(assetRequest(
        request,
        url.pathname === "/" ? "/index.html" : `${url.pathname}/index.html`,
      ));
      const isHtml = assetResponse.ok
        && assetResponse.headers.get("Content-Type")?.split(";", 1)[0]?.trim().toLowerCase() === "text/html";
      return withSecurityHeaders(
        assetResponse,
        isHtml,
        undefined,
        isHtml ? scriptHashes : [],
      );
    }
    if (isStaticPath(url.pathname)) {
      return withSecurityHeaders(
        await env.ASSETS.fetch(assetRequest(request, url.pathname)),
        false,
        staticContentType(url.pathname),
      );
    }
    return plainResponse(404, "Not found");
  },
} satisfies ExportedHandler<SiteEnv>;

export { MARKETING_PATHS };
export default siteWorker;
