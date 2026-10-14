import { Hono } from "hono";
import { z } from "zod";
import {
  callPrivateAuthHandler,
  hasJsonContentType,
} from "../auth/request";
import { authJsonResponse, authResponseHeaders } from "../auth/session";
import type { AppEnv } from "../types/bindings";
import { safeAuthReturnTo } from "../auth/return-to";

// Keep OAuth state on the private auth owner; only these public destinations
// may be carried through it. A download token never grants a session itself.
function allowedReturnTo(value: string) {
  return safeAuthReturnTo(value) !== null;
}

const socialStartSchema = z.object({
  provider: z.literal("google"),
  returnTo: z.string().refine(allowedReturnTo).optional(),
}).strict();
const CALLBACK_QUERY_FIELDS = ["code", "error", "state"] as const;
const PUBLIC_OAUTH_ERROR_CODES = new Set([
  "access_denied",
  "account_already_linked_to_different_user",
  "email_does_not_match",
  "email_not_found",
  "email_not_verified",
  "invalid_code",
  "issuer_mismatch",
  "no_callback_url",
  "no_code",
  "nonce_binding_missing",
  "oauth_provider_not_found",
  "state_not_found",
  "unable_to_get_user_info",
  "unable_to_link_account",
]);

function oauthRedirect(location: string, sourceHeaders?: HeadersInit) {
  const headers = authResponseHeaders(sourceHeaders);
  headers.set("Location", location);
  return new Response(null, { headers, status: 302 });
}

function callbackFailure(webOrigin: string, error = "oauth_callback_error", returnTo?: string) {
  const location = new URL("/login", webOrigin);
  location.searchParams.set(
    "error",
    PUBLIC_OAUTH_ERROR_CODES.has(error) ? error : "oauth_callback_error",
  );
  if (returnTo && allowedReturnTo(returnTo) && returnTo !== "/account") {
    location.searchParams.set("returnTo", returnTo);
  }
  return location.toString();
}

export function sanitizeOAuthCallbackResponse(
  response: Response,
  webOrigin: string,
) {
  const locationValue = response.headers.get("Location");
  if (!locationValue || response.status < 300 || response.status >= 400) {
    return oauthRedirect(callbackFailure(webOrigin), response.headers);
  }

  let location: URL;
  try {
    location = new URL(locationValue);
  } catch {
    return oauthRedirect(callbackFailure(webOrigin), response.headers);
  }

  const expectedOrigin = new URL(webOrigin).origin;
  if (
    location.origin === expectedOrigin &&
    location.username.length === 0 &&
    location.password.length === 0 &&
    allowedReturnTo(location.pathname + location.search) &&
    location.hash.length === 0
  ) {
    return oauthRedirect(new URL(location.pathname + location.search, webOrigin).toString(), response.headers);
  }

  if (location.origin === expectedOrigin && location.pathname === "/login"
    && !location.username && !location.password && !location.hash) {
    const errors = location.searchParams.getAll("error");
    const error = errors.length === 1 ? errors[0] : undefined;
    const destinations = location.searchParams.getAll("returnTo");
    return oauthRedirect(
      callbackFailure(webOrigin, error ?? "oauth_callback_error",
        destinations.length === 1 ? destinations[0] : undefined),
      response.headers,
    );
  }

  return oauthRedirect(callbackFailure(webOrigin), response.headers);
}

function sanitizedCallbackQuery(url: URL) {
  const query = new URLSearchParams();
  for (const field of CALLBACK_QUERY_FIELDS) {
    const values = url.searchParams.getAll(field);
    if (values.length > 1) return null;
    if (values[0] !== undefined) query.set(field, values[0]);
  }
  return query;
}

export const authRoutes = new Hono<AppEnv>()
  .get("/auth/capabilities", (context) =>
    context.json(
      {
        googleSignInEnabled: context.get("config").google !== undefined,
        emailOtpEnabled: context.get("config").resend !== undefined,
      },
      200,
      {
        "Cache-Control": "no-store",
        Pragma: "no-cache",
      },
    ),
  )
  .post("/auth/sign-in/social", async (context) => {
    if (context.get("config").google === undefined) {
      return authJsonResponse({ error: "oauth_provider_not_found" }, 404);
    }
    if (!hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "invalid_social_request" }, 400);
    }

    let body: unknown;
    try {
      body = await context.req.raw.json();
    } catch {
      return authJsonResponse({ error: "invalid_social_request" }, 400);
    }
    const parsed = socialStartSchema.safeParse(body);
    if (!parsed.success) {
      return authJsonResponse({ error: "invalid_social_request" }, 400);
    }

    const webOrigin = context.get("config").webOrigin;
    const returnTo = parsed.data.returnTo ?? "/account";
    const errorCallback = new URL("/login", webOrigin);
    if (returnTo !== "/account") errorCallback.searchParams.set("returnTo", returnTo);
    return callPrivateAuthHandler(context, {
      body: JSON.stringify({
        callbackURL: new URL(returnTo, webOrigin).toString(),
        errorCallbackURL: errorCallback.toString(),
        provider: parsed.data.provider,
      }),
      method: "POST",
      path: "/api/auth/sign-in/social",
    });
  })
  .get("/auth/callback/google", async (context) => {
    const webOrigin = context.get("config").webOrigin;
    if (context.get("config").google === undefined) {
      return oauthRedirect(
        callbackFailure(webOrigin, "oauth_provider_not_found"),
      );
    }
    const query = sanitizedCallbackQuery(new URL(context.req.url));
    if (!query) {
      return oauthRedirect(callbackFailure(webOrigin));
    }
    const response = await callPrivateAuthHandler(context, {
      method: "GET",
      path: "/api/auth/callback/google",
      query,
    });
    return sanitizeOAuthCallbackResponse(
      response,
      webOrigin,
    );
  });
