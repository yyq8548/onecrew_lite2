import { distributionApiMethods } from "../distribution/routes";
import { cliMethods } from "../cli-distribution/contracts";
import { createFactory } from "hono/factory";

import { authJsonResponse, authResponseHeaders } from "../auth/session";
import { parseAccessOrigin } from "../config/env";
import { DEVICE_ID_PATTERN } from "../devices/ids";
import type { AppEnv } from "../types/bindings";
import { mailApiMethods } from "../integrations/providers/mail/boundary";

const factory = createFactory<AppEnv>();

const ACCESS_SHELL_PATHS = new Set([
  "/index.html",
  "/login",
  "/register",
  "/forgot-password",
  "/account",
]);

const WORKER_PAGE_PATHS = new Set([
  "/integrations/mail/connect",
  "/integrations/gmail/callback",
  "/pairing/authorize",
  "/pairing/rate-limited",
]);

const ROOT_STATIC_ASSET_PATHS = new Set([
  "/ga4-init.js",
  "/favicon.ico",
  "/onecrew-favicon.svg",
  "/onecrew-logo-dark.svg",
  "/onecrew-logo-color.svg",
  "/onecrew-logo-pairing.png",
  "/onecrew-watermark-mark.svg",
  "/onecrew-ai-mark.png",
  "/codex-mark.png",
  "/claude-mark.png",
]);

const API_METHODS = new Map<string, readonly string[]>([
  ["/api/analytics/claim", ["POST"]],
  ["/api/health", ["GET"]],
  ["/api/auth/capabilities", ["GET"]],
  ["/api/auth/sign-in/social", ["POST"]],
  ["/api/auth/callback/google", ["GET"]],
  ["/api/session/register", ["POST"]],
  ["/api/session/login", ["POST"]],
  ["/api/session/login-otp/send", ["POST"]],
  ["/api/session/login-otp", ["POST"]],
  ["/api/session/register/verify/send", ["POST"]],
  ["/api/session/register/verify", ["POST"]],
  ["/api/session/forgot-password/send", ["POST"]],
  ["/api/session/forgot-password", ["POST"]],
  ["/api/session/logout", ["POST"]],
  ["/api/session", ["GET"]],
  ["/api/account/settings", ["GET"]],
  ["/api/account/profile", ["POST"]],
  ["/api/account/password", ["POST"]],
  ["/api/account/email/current-otp/send", ["POST"]],
  ["/api/account/email/current-otp/verify", ["POST"]],
  ["/api/account/email/change/request", ["POST"]],
  ["/api/account/email/change/verify", ["POST"]],
  ["/api/pairing/authorize", ["GET", "POST"]],
  ["/api/devices", ["GET"]],
]);

type AccessRequestDecision =
  | { kind: "api" | "robots" | "root" | "shell" | "static" | "worker-page" }
  | { kind: "method-not-allowed"; allow: readonly string[] }
  | { kind: "not-found" };

function deviceApiMethods(pathname: string) {
  const match = /^\/api\/devices\/([^/]+)\/(status|revoke)$/.exec(pathname);
  if (match?.[1] && DEVICE_ID_PATTERN.test(match[1])) {
    return match[2] === "status" ? (["GET"] as const) : (["POST"] as const);
  }
  const deleteMatch = /^\/api\/devices\/([^/]+)$/.exec(pathname);
  if (deleteMatch?.[1] && DEVICE_ID_PATTERN.test(deleteMatch[1])) {
    return ["DELETE"] as const;
  }
  return null;
}

export function accessApiMethods(pathname: string): readonly string[] | null {
  return API_METHODS.get(pathname)
    ?? cliMethods(pathname)
    ?? distributionApiMethods(pathname)
    ?? mailApiMethods(pathname)
    ?? deviceApiMethods(pathname);
}

function isStaticAssetPath(pathname: string) {
  if (ROOT_STATIC_ASSET_PATHS.has(pathname)) return true;
  if (!pathname.startsWith("/assets/") && !pathname.startsWith("/images/")) {
    return false;
  }
  const segments = pathname.slice(1).split("/");
  const filename = segments.at(-1) ?? "";
  return (
    filename.includes(".") &&
    segments.every((segment) => /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(segment))
  );
}

function methodDecision(
  method: string,
  allow: readonly string[],
  kind: "robots" | "root" | "shell" | "static" | "worker-page",
): AccessRequestDecision {
  return allow.includes(method)
    ? { kind }
    : { kind: "method-not-allowed", allow };
}

export function classifyAccessRequest(
  method: string,
  pathname: string,
  requestedMethod?: string,
): AccessRequestDecision {
  const normalizedMethod = method.toUpperCase();
  if (pathname === "/") {
    return methodDecision(normalizedMethod, ["GET", "HEAD"], "root");
  }
  if (pathname === "/robots.txt") {
    return methodDecision(normalizedMethod, ["GET", "HEAD"], "robots");
  }
  if (ACCESS_SHELL_PATHS.has(pathname) || /^\/d\/[A-Za-z0-9_-]{1,128}$/.test(pathname)) {
    return methodDecision(normalizedMethod, ["GET", "HEAD"], "shell");
  }
  if (WORKER_PAGE_PATHS.has(pathname)) {
    return methodDecision(normalizedMethod, pathname === "/integrations/mail/connect" ? ["GET", "POST"] : ["GET"], "worker-page");
  }
  if (isStaticAssetPath(pathname)) {
    return methodDecision(normalizedMethod, ["GET", "HEAD"], "static");
  }

  const apiMethods = accessApiMethods(pathname);
  if (!apiMethods) return { kind: "not-found" };
  if (normalizedMethod === "OPTIONS") {
    const normalizedRequestedMethod = requestedMethod?.toUpperCase();
    return normalizedRequestedMethod && apiMethods.includes(normalizedRequestedMethod)
      ? { kind: "api" }
      : { kind: "method-not-allowed", allow: [...apiMethods, "OPTIONS"] };
  }
  return apiMethods.includes(normalizedMethod)
    ? { kind: "api" }
    : { kind: "method-not-allowed", allow: [...apiMethods, "OPTIONS"] };
}

export function hasExactAccessHost(request: Request, configuredOrigin: string) {
  const expected = new URL(configuredOrigin);
  const actual = new URL(request.url);
  if (
    actual.origin !== expected.origin ||
    actual.username.length > 0 ||
    actual.password.length > 0
  ) return false;
  const host = request.headers.get("Host");
  return host === null || host.toLowerCase() === expected.host.toLowerCase();
}

function securedAssetResponse(response: Response, requestMethod: string, html: boolean) {
  const headers = new Headers(response.headers);
  headers.delete("Set-Cookie");
  headers.set("Cross-Origin-Resource-Policy", "same-origin");
  headers.set("X-Content-Type-Options", "nosniff");
  if (html) {
    headers.set("Cache-Control", "no-store");
    headers.set("Pragma", "no-cache");
    headers.set("Referrer-Policy", "origin");
    headers.set("X-Frame-Options", "DENY");
    headers.set(
      "Content-Security-Policy",
      "default-src 'self'; script-src 'self' https://www.googletagmanager.com; style-src 'self'; style-src-attr 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' https://*.google-analytics.com https://*.analytics.google.com https://www.googletagmanager.com; object-src 'none'; base-uri 'none'; form-action 'self' http://127.0.0.1:*; frame-ancestors 'none'",
    );
  }
  return new Response(requestMethod === "HEAD" ? null : response.body, {
    headers,
    status: response.status,
  });
}

function assetRequest(request: Request, url = request.url) {
  const headers = new Headers(request.headers);
  headers.delete("Authorization");
  headers.delete("Cookie");
  headers.delete("Host");
  return new Request(url, { headers, method: request.method });
}

export async function accessShellResponse(assets: Fetcher, request: Request) {
  const assetUrl = new URL(request.url);
  assetUrl.pathname = "/index.html";
  assetUrl.search = "";
  let response: Response;
  try {
    response = await assets.fetch(
      assetRequest(request, assetUrl.toString()),
    );
  } catch {
    return authJsonResponse({ error: "access_assets_unavailable" as const }, 503);
  }
  if (
    !response.ok ||
    !response.headers.get("Content-Type")?.toLowerCase().startsWith("text/html")
  ) {
    return authJsonResponse({ error: "access_assets_unavailable" as const }, 503);
  }
  const secured = securedAssetResponse(response, request.method, true);
  const incoming = new URL(request.url);
  if (incoming.pathname.startsWith("/d/") || incoming.searchParams.has("returnTo")) secured.headers.set("Referrer-Policy", "no-referrer");
  return secured;
}

async function staticAssetResponse(assets: Fetcher, request: Request) {
  let response: Response;
  try {
    response = await assets.fetch(assetRequest(request));
  } catch {
    return authJsonResponse({ error: "access_assets_unavailable" as const }, 503);
  }
  if (response.status === 404) {
    return authJsonResponse({ error: "not_found" as const }, 404);
  }
  return securedAssetResponse(response, request.method, false);
}

function redirectToLogin() {
  const headers = authResponseHeaders();
  headers.set("Location", "/login");
  return new Response(null, { headers, status: 302 });
}

function robotsResponse(method: string) {
  return new Response(method === "HEAD" ? null : "User-agent: *\nDisallow: /\n", {
    headers: {
      "Cache-Control": "no-store",
      "Content-Type": "text/plain; charset=UTF-8",
      "X-Content-Type-Options": "nosniff",
    },
  });
}

function methodNotAllowed(allow: readonly string[]) {
  const response = authJsonResponse({ error: "method_not_allowed" as const }, 405);
  response.headers.set("Allow", allow.join(", "));
  return response;
}

export const accessBoundaryMiddleware = factory.createMiddleware(
  async (context, next) => {
    const accessOrigin = parseAccessOrigin(context.env);
    if (!hasExactAccessHost(context.req.raw, accessOrigin)) {
      return authJsonResponse({ error: "not_found" as const }, 404);
    }

    const rawPathname = new URL(context.req.raw.url).pathname;
    if (rawPathname.includes("%") || rawPathname !== context.req.path) {
      return authJsonResponse({ error: "not_found" as const }, 404);
    }

    const decision = classifyAccessRequest(
      context.req.method,
      context.req.path,
      context.req.header("Access-Control-Request-Method"),
    );
    if (decision.kind === "root") return redirectToLogin();
    if (decision.kind === "robots") return robotsResponse(context.req.method);
    if (decision.kind === "shell") {
      return accessShellResponse(context.env.ASSETS, context.req.raw);
    }
    if (decision.kind === "static") {
      return staticAssetResponse(context.env.ASSETS, context.req.raw);
    }
    if (decision.kind === "method-not-allowed") {
      return methodNotAllowed(decision.allow);
    }
    if (decision.kind === "not-found") {
      return authJsonResponse({ error: "not_found" as const }, 404);
    }
    await next();
  },
);
