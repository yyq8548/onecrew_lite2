import { distributionRoutes } from "./distribution/routes";
import { analyticsRoutes } from "./analytics/routes";
import { cliRoutes } from "./cli-distribution/routes";
import { Hono } from "hono";
import { authJsonResponse } from "./auth/session";
import { RuntimeConfigError } from "./config/env";
import {
  apiCorsMiddleware,
  edgeRateLimitMiddleware,
  noStoreMiddleware,
  originGuardMiddleware,
  runtimeContextMiddleware,
  webOriginContextMiddleware,
} from "./middleware/api";
import { authRoutes } from "./routes/auth";
import { accountRoutes } from "./routes/account";
import { deviceRoutes } from "./routes/devices";
import { healthRoutes } from "./routes/health";
import { pairingRoutes } from "./routes/pairing";
import { sessionRoutes } from "./routes/session";
import { accessBoundaryMiddleware } from "./security/access-boundary";
import type { AppEnv } from "./types/bindings";
import { integrationRoutes } from "./integrations";

export function createApp() {
  const app = new Hono<AppEnv>();

  app.use("*", accessBoundaryMiddleware);
  app.use("/api/*", webOriginContextMiddleware);
  app.use("/api/*", noStoreMiddleware);
  app.use("/api/*", originGuardMiddleware);
  app.use("/api/*", apiCorsMiddleware);
  app.use("/api/*", edgeRateLimitMiddleware);
  app.use("/api/*", runtimeContextMiddleware);
  app.use("/cli/*", edgeRateLimitMiddleware);
  app.use("/pairing/authorize", noStoreMiddleware);
  app.use("/pairing/authorize", runtimeContextMiddleware);
  app.use("/pairing/rate-limited", noStoreMiddleware);
  app.use("/pairing/rate-limited", runtimeContextMiddleware);
  for (const path of ["/integrations/mail/connect", "/integrations/gmail/callback"]) {
    app.use(path, noStoreMiddleware);
    app.use(path, edgeRateLimitMiddleware);
    app.use(path, runtimeContextMiddleware);
  }

  app.onError((error, context) => {
    if (error instanceof RuntimeConfigError) {
      return authJsonResponse(
        { error: "service_configuration_error" as const },
        500,
      );
    }
    return authJsonResponse({ error: "internal_server_error" as const }, 500);
  });

  return app
    .route("/", cliRoutes)
    .route("/", analyticsRoutes)
    .route("/", distributionRoutes)
    .route("/", integrationRoutes)
    .route("/", pairingRoutes)
    .route("/api", healthRoutes)
    .route("/api", authRoutes)
    .route("/api", accountRoutes)
    .route("/api", sessionRoutes)
    .route("/api", deviceRoutes);
}

export type AppType = ReturnType<typeof createApp>;
