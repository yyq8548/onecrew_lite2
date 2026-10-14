import type { Context } from "hono";
import { cors } from "hono/cors";
import { createFactory } from "hono/factory";
import { createAuth } from "../auth/auth";
import { authJsonResponse, authResponseHeaders } from "../auth/session";
import { parseRuntimeConfig, parseWebOrigin } from "../config/env";
import {
  edgeRateLimitGroup,
  passesEdgeRateLimit,
} from "../rate-limit/edge";
import type { AppEnv } from "../types/bindings";

const factory = createFactory<AppEnv>();

export const edgeRateLimitMiddleware = factory.createMiddleware(
  async (context, next) => {
    const group = edgeRateLimitGroup(context.req.method, context.req.path);
    if (
      group !== null &&
      !await passesEdgeRateLimit(
        context.env.EDGE_RATE_LIMITER,
        group,
        context.req.header("cf-connecting-ip"),
      )
    ) {
      if (
        group === "pairing_write"
        && context.req.header("Sec-Fetch-Mode") === "navigate"
      ) {
        const headers = authResponseHeaders();
        headers.set("Location", "/pairing/rate-limited");
        headers.set("Retry-After", "60");
        return new Response(null, { headers, status: 303 });
      }
      const response = authJsonResponse({ error: "rate_limited" as const }, 429);
      if (group === "cli_install") {
        response.headers.set("Retry-After", "60");
        response.headers.set("Referrer-Policy", "no-referrer");
      }
      return response;
    }
    await next();
  },
);

export const runtimeContextMiddleware = factory.createMiddleware(
  async (context, next) => {
    const config = parseRuntimeConfig(context.env);
    context.set("config", config);
    context.set("auth", createAuth(config));
    await next();
  },
);

export const webOriginContextMiddleware = factory.createMiddleware(
  async (context, next) => {
    context.set("webOrigin", parseWebOrigin(context.env));
    await next();
  },
);

export const noStoreMiddleware = factory.createMiddleware(
  async (context, next) => {
    await next();
    context.header("Cache-Control", "no-store");
    context.header("Pragma", "no-cache");
  },
);

export const originGuardMiddleware = factory.createMiddleware(
  async (context, next) => {
    const origin = context.req.header("Origin");
    if (origin !== undefined && origin !== context.get("webOrigin")) {
      return authJsonResponse({ error: "origin_not_allowed" as const }, 403);
    }
    await next();
  },
);

const configuredCorsMiddleware = cors({
  allowHeaders: ["Content-Type"],
  allowMethods: ["DELETE", "GET", "POST", "OPTIONS"],
  credentials: true,
  maxAge: 600,
  origin: (origin, context) =>
    origin === (context as Context<AppEnv>).get("webOrigin")
      ? origin
      : null,
});

export const apiCorsMiddleware = factory.createMiddleware((context, next) =>
  configuredCorsMiddleware(context, next),
);
