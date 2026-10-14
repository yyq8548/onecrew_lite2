import { Hono } from "hono";
import { isDatabaseReady } from "../readiness";
import type { AppEnv } from "../types/bindings";

export const healthRoutes = new Hono<AppEnv>().get("/health", async (context) => {
  try {
    if (!await isDatabaseReady(context.get("config").db)) {
      throw new Error("database_not_ready");
    }
    return context.json({ status: "ok" as const });
  } catch {
    return context.json({ status: "unavailable" as const }, 503);
  }
});
