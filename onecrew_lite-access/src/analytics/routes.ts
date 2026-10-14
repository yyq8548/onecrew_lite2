import { Hono } from "hono";
import { bodyLimit } from "hono/body-limit";
import { authJsonResponse, loadSafeSession } from "../auth/session";
import type { AppEnv } from "../types/bindings";
import { attributionContext, claimSuccessEvents } from "./service";

export const analyticsRoutes = new Hono<AppEnv>();
analyticsRoutes.post("/api/analytics/claim", bodyLimit({ maxSize: 1024 }), async c => {
  if (c.req.header("Origin") !== c.get("config").authOrigin) return authJsonResponse({ error: "origin_not_allowed" }, 403);
  if (c.req.header("Content-Type")?.split(";", 1)[0] !== "application/json") return authJsonResponse({ error: "invalid_request" }, 400);
  const body: unknown = await c.req.json().catch(() => null);
  if (!body || Array.isArray(body) || typeof body !== "object" || Object.keys(body).length) return authJsonResponse({ error: "invalid_request" }, 400);
  const { session, headers } = await loadSafeSession(c.get("auth"), c.req.raw.headers);
  if (!session) return authJsonResponse({ error: "authentication_required" }, 401, headers);
  try {
    const user = await c.env.DB.prepare('SELECT "emailVerified" AS verified FROM "user" WHERE id=?').bind(session.user.id).first<{ verified: number }>();
    const context = attributionContext(c.req.raw.headers);
    if (!user?.verified || !context) return authJsonResponse({ events: [] }, 200, headers);
    return authJsonResponse({ events: await claimSuccessEvents(c.env.DB, session.user.id, context) }, 200, headers);
  } catch {
    console.warn("analytics_success_claim_unavailable");
    return authJsonResponse({ events: [] }, 200, headers);
  }
});
