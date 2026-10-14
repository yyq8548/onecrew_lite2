import { Hono } from "hono";

import { authJsonResponse, loadSafeSession } from "../auth/session";
import type { AppEnv } from "../types/bindings";
import { sessionAuthRoutes } from "./session-auth";
import { sessionOtpRoutes } from "./session-otp";

export const sessionRoutes = new Hono<AppEnv>()
  .route("/session", sessionAuthRoutes)
  .route("/session", sessionOtpRoutes)
  .get("/session", async (context) => {
    const result = await loadSafeSession(
      context.get("auth"),
      context.req.raw.headers,
    );
    if (!result.session) {
      return authJsonResponse(
        {
          authenticated: false as const,
          expiresAt: null,
          user: null,
        },
        200,
        result.headers,
      );
    }

    return authJsonResponse(
      {
        authenticated: true as const,
        expiresAt: result.session.expiresAt,
        user: result.session.user,
      },
      200,
      result.headers,
    );
  });
