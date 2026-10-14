import { bodyLimit } from "hono/body-limit";
import type { ContentfulStatusCode } from "hono/utils/http-status";
import { z } from "zod";
import { authResponseHeaders } from "../../../auth/session";
import { IntegrationError } from "../../../devices/errors";
import { MAIL_PROTOCOL, MAIL_VERSION } from "./contracts";

export const requestIdSchema = z.string().uuid();
export function mailResponse(requestId: string, data: unknown, status: ContentfulStatusCode = 200) {
  const headers = authResponseHeaders();
  headers.set("Content-Type", "application/json; charset=UTF-8");
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("X-Robots-Tag", "noindex, nofollow");
  return new Response(JSON.stringify({ protocol: MAIL_PROTOCOL, version: MAIL_VERSION, requestId, data }), { status, headers });
}
export function mailFailure(requestId: string, error: IntegrationError) {
  const headers = authResponseHeaders();
  headers.set("Content-Type", "application/json; charset=UTF-8");
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("X-Robots-Tag", "noindex, nofollow");
  return new Response(JSON.stringify({ protocol: MAIL_PROTOCOL, version: MAIL_VERSION, requestId, error: { code: error.code, retryable: error.retryable } }), { status: error.status, headers });
}
export const mailBodyLimit = bodyLimit({ maxSize: 0, onError: (c) => mailFailure(c.get("mailRequestId") ?? crypto.randomUUID(), new IntegrationError("invalid_request", 400)) });
