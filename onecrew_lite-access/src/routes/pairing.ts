import { Hono, type Context } from "hono";
import { bodyLimit } from "hono/body-limit";
import { z } from "zod";
import {
  authJsonResponse,
  authResponseHeaders,
  loadSafeSession,
} from "../auth/session";
import { createApprovalToken, approvalTokenSchema } from "../pairing/approval-token";
import { desktopPairingAuthorizeRequestSchema } from "../pairing/contracts";
import {
  approveDesktopPairing,
  isPairingRequestResolved,
  pairingAuthorizationReadiness,
  pairingPlatformLabel,
} from "../pairing/service";
import type { AppEnv } from "../types/bindings";
import { passesEdgeRateLimit } from "../rate-limit/edge";
import { normalizeIpAddress } from "../security/ip-address";
import { edgeLocation } from "../security/edge-location";
import { accessShellResponse } from "../security/access-boundary";

const approvalSchema = z.object({ approvalToken: approvalTokenSchema }).strict();
const QUERY_FIELDS = [
  "app_version",
  "callback_port",
  "code_challenge",
  "code_challenge_method",
  "label",
  "platform",
  "state",
] as const;

function parsePairingQuery(url: URL) {
  const keys = [...url.searchParams.keys()];
  if (
    keys.length !== QUERY_FIELDS.length ||
    keys.some((key) => !QUERY_FIELDS.includes(key as (typeof QUERY_FIELDS)[number])) ||
    QUERY_FIELDS.some((key) => url.searchParams.getAll(key).length !== 1)
  ) {
    return null;
  }
  const parsed = desktopPairingAuthorizeRequestSchema.safeParse({
    appVersion: url.searchParams.get("app_version"),
    callbackPort: url.searchParams.get("callback_port"),
    codeChallenge: url.searchParams.get("code_challenge"),
    codeChallengeMethod: url.searchParams.get("code_challenge_method"),
    label: url.searchParams.get("label"),
    platform: url.searchParams.get("platform"),
    state: url.searchParams.get("state"),
  });
  return parsed.success ? parsed.data : null;
}

async function readJson(request: Request) {
  try {
    return await request.json();
  } catch {
    return undefined;
  }
}

async function pairingAssetResponse(context: Context<AppEnv>) {
  return accessShellResponse(context.env.ASSETS, context.req.raw);
}

function rateLimitedResponse(retryAfterSeconds: number, sourceHeaders?: HeadersInit) {
  const response = authJsonResponse({ error: "rate_limited" }, 429, sourceHeaders);
  response.headers.set("Retry-After", String(retryAfterSeconds));
  return response;
}

function rateLimitedNavigation(retryAfterSeconds: number, sourceHeaders?: HeadersInit) {
  const headers = authResponseHeaders(sourceHeaders);
  headers.set("Location", "/pairing/rate-limited");
  headers.set("Retry-After", String(retryAfterSeconds));
  return new Response(null, { headers, status: 303 });
}

export const pairingRoutes = new Hono<AppEnv>()
  .get("/pairing/authorize", async (context) => {
    if (!await passesEdgeRateLimit(
      context.env.EDGE_RATE_LIMITER,
      "pairing_read",
      context.req.header("cf-connecting-ip"),
    )) {
      return rateLimitedNavigation(60);
    }
    if (parsePairingQuery(new URL(context.req.url))) {
      try {
        const session = await loadSafeSession(
          context.get("auth"),
          context.req.raw.headers,
        );
        if (session.session) {
          const readiness = await pairingAuthorizationReadiness(
            context.get("config"),
            session.session.user.id,
          );
          if (readiness.kind === "rate_limited") {
            return rateLimitedNavigation(readiness.retryAfterSeconds, session.headers);
          }
        }
      } catch {
        // The SPA owns session error presentation when this optional precheck is unavailable.
      }
    }
    return pairingAssetResponse(context);
  })
  .get("/pairing/rate-limited", async (context) => {
    const session = await loadSafeSession(
      context.get("auth"),
      context.req.raw.headers,
    );
    if (!session.session) {
      const headers = authResponseHeaders(session.headers);
      headers.set("Location", "/login");
      return new Response(null, { headers, status: 303 });
    }
    return pairingAssetResponse(context);
  })
  .get("/api/pairing/authorize", async (context) => {
    const request = parsePairingQuery(new URL(context.req.url));
    if (!request) {
      return authJsonResponse({ error: "invalid_pairing_request" }, 400);
    }
    const session = await loadSafeSession(
      context.get("auth"),
      context.req.raw.headers,
    );
    if (!session.session) {
      return authJsonResponse(
        { error: "authentication_required" },
        401,
        session.headers,
      );
    }
    const readiness = await pairingAuthorizationReadiness(
      context.get("config"),
      session.session.user.id,
    );
    if (readiness.kind === "rate_limited") {
      return rateLimitedResponse(readiness.retryAfterSeconds, session.headers);
    }
    if (
      await isPairingRequestResolved(
        context.get("config"),
        session.session.user.id,
        request,
      )
    ) {
      return authJsonResponse(
        { error: "pairing_approval_resolved" },
        409,
        session.headers,
      );
    }
    const token = await createApprovalToken(
      context.get("config").pairingHmacSecret,
      session.session.user.id,
      session.session.sessionId,
      request,
    );
    return authJsonResponse(
      {
        ...token,
        request: {
          appName: "Agent Platform" as const,
          appVersion: request.appVersion,
          callbackPort: request.callbackPort,
          label: request.label,
          platform: request.platform,
          platformLabel: pairingPlatformLabel(request.platform),
          bindingIp: normalizeIpAddress(context.req.header("cf-connecting-ip")),
          bindingLocation: edgeLocation(context.req.raw),
        },
      },
      200,
      session.headers,
    );
  })
  .post(
    "/api/pairing/authorize",
    bodyLimit({
      maxSize: 2 * 1024,
      onError: () => authJsonResponse({ error: "request_too_large" }, 413),
    }),
    async (context) => {
    const origin = context.req.header("Origin");
    const fetchSite = context.req.header("Sec-Fetch-Site");
    if (
      origin !== context.get("config").webOrigin ||
      fetchSite !== "same-origin"
    ) {
      return authJsonResponse({ error: "origin_not_allowed" }, 403);
    }
    const contentType = context.req.header("Content-Type")?.split(";", 1)[0]?.toLowerCase();
    if (contentType !== "application/x-www-form-urlencoded") {
      return authJsonResponse({ error: "invalid_content_type" }, 415);
    }
    let form: FormData;
    try {
      form = await context.req.raw.formData();
    } catch {
      return authJsonResponse({ error: "invalid_pairing_request" }, 400);
    }
    const formKeys = [...form.keys()];
    const body = approvalSchema.safeParse(
      formKeys.length === 1 &&
        formKeys[0] === "approvalToken" &&
        form.getAll("approvalToken").length === 1
        ? { approvalToken: form.get("approvalToken") }
        : undefined,
    );
    if (!body.success) {
      return authJsonResponse({ error: "invalid_pairing_request" }, 400);
    }
    const session = await loadSafeSession(
      context.get("auth"),
      context.req.raw.headers,
    );
    if (!session.session) {
      return authJsonResponse(
        { error: "authentication_required" },
        401,
        session.headers,
      );
    }
    const result = await approveDesktopPairing(
      context.get("config"),
      body.data.approvalToken,
      {
        sessionId: session.session.sessionId,
        userId: session.session.user.id,
        ipAddress: context.req.header("cf-connecting-ip") ?? null,
        bindingLocation: edgeLocation(context.req.raw),
      },
    );
    if (result.kind === "invalid") {
      return authJsonResponse({ error: "invalid_pairing_request" }, 400);
    }
    if (result.kind === "expired") {
      return authJsonResponse({ error: "pairing_approval_expired" }, 410);
    }
    if (result.kind === "resolved") {
      return authJsonResponse({ error: "pairing_approval_resolved" }, 409);
    }
    if (result.kind === "rate_limited") {
      return context.req.header("Sec-Fetch-Mode") === "navigate"
        ? rateLimitedNavigation(result.retryAfterSeconds, session.headers)
        : rateLimitedResponse(result.retryAfterSeconds, session.headers);
    }
    if (result.kind === "device_limit_reached") {
      return authJsonResponse({ error: "device_limit_reached" }, 409);
    }
    const headers = authResponseHeaders(session.headers);
    headers.set("Location", result.callbackLocation);
    return new Response(null, { headers, status: 303 });
    },
  );
