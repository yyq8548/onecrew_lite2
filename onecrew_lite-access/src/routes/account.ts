import { Hono, type Context } from "hono";
import { z } from "zod";

import {
  callPrivateAuthHandler,
  hasJsonContentType,
  privateAuthHeaders,
} from "../auth/request";
import { authJsonResponse, loadSafeSession } from "../auth/session";
import type { AppEnv } from "../types/bindings";
import { projectAuthenticatedUser, projectSimpleSuccess, readJson } from "./session-shared";

const emailSchema = z.string().max(254).email();
const passwordSchema = z.string().min(8).max(128);
const profileSchema = z.object({ name: z.string().trim().min(1).max(100) }).strict();
const passwordUpdateSchema = z.object({
  currentPassword: z.string().max(128).optional(),
  newPassword: passwordSchema,
}).strict();
const currentOtpSchema = z.object({ otp: z.string().regex(/^\d{6}$/) }).strict();
const emailChangeRequestSchema = z.object({
  currentOtp: z.string().regex(/^\d{6}$/),
  newEmail: emailSchema,
}).strict();
const emailChangeVerifySchema = z.object({
  newEmail: emailSchema,
  otp: z.string().regex(/^\d{6}$/),
}).strict();
const emptySchema = z.object({}).strict();

async function authenticated(context: Context<AppEnv>) {
  const result = await loadSafeSession(
    context.get("auth"),
    context.req.raw.headers,
  );
  if (!result.session) {
    return {
      response: authJsonResponse(
        { error: "authentication_required" as const },
        401,
        result.headers,
      ),
      session: null,
    } as const;
  }
  return {
    response: null,
    session: {
      headers: result.headers,
      user: result.session.user,
    },
  } as const;
}

async function hasPassword(db: D1Database, userId: string) {
  const row = await db.prepare(
    `SELECT 1 AS "present"
     FROM "account"
     WHERE "userId" = ?
       AND "providerId" = 'credential'
       AND "password" IS NOT NULL
     LIMIT 1`,
  ).bind(userId).first<{ present: number }>();
  return row?.present === 1;
}

function resendRequired(context: Context<AppEnv>) {
  return context.get("config").resend !== undefined;
}

export const accountRoutes = new Hono<AppEnv>()
  .get("/account/settings", async (context) => {
    const authorization = await authenticated(context);
    if (!authorization.session) return authorization.response;
    return authJsonResponse(
      {
        hasPassword: await hasPassword(context.get("config").db, authorization.session.user.id),
        user: authorization.session.user,
      },
      200,
      authorization.session.headers,
    );
  })
  .post("/account/profile", async (context) => {
    const authorization = await authenticated(context);
    if (!authorization.session) return authorization.response;
    if (!hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "profile_update_failed" as const }, 400, authorization.session.headers);
    }
    const parsed = profileSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) {
      return authJsonResponse({ error: "profile_update_failed" as const }, 400, authorization.session.headers);
    }
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify(parsed.data),
      method: "POST",
      path: "/api/auth/update-user",
    });
    return projectSimpleSuccess(response, "profile_update_failed");
  })
  .post("/account/password", async (context) => {
    const authorization = await authenticated(context);
    if (!authorization.session) return authorization.response;
    if (!hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "password_update_failed" as const }, 400, authorization.session.headers);
    }
    const parsed = passwordUpdateSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) {
      return authJsonResponse({ error: "password_update_failed" as const }, 400, authorization.session.headers);
    }
    const currentHasPassword = await hasPassword(
      context.get("config").db,
      authorization.session.user.id,
    );
    if (currentHasPassword) {
      if (!parsed.data.currentPassword) {
        return authJsonResponse({ error: "password_update_failed" as const }, 400, authorization.session.headers);
      }
      const response = await callPrivateAuthHandler(context, {
        body: JSON.stringify({
          currentPassword: parsed.data.currentPassword,
          newPassword: parsed.data.newPassword,
          revokeOtherSessions: true,
        }),
        method: "POST",
        path: "/api/auth/change-password",
      });
      return projectAuthenticatedUser(response, "password_update_failed");
    }

    try {
      const result = await context.get("auth").api.setPassword({
        body: { newPassword: parsed.data.newPassword },
        headers: privateAuthHeaders(context.req.raw.headers),
      });
      if (!result.status) {
        return authJsonResponse({ error: "password_update_failed" as const }, 400, authorization.session.headers);
      }
      return authJsonResponse({ ok: true }, 200, authorization.session.headers);
    } catch {
      return authJsonResponse({ error: "password_update_failed" as const }, 400, authorization.session.headers);
    }
  })
  .post("/account/email/current-otp/send", async (context) => {
    const authorization = await authenticated(context);
    if (!authorization.session) return authorization.response;
    if (!resendRequired(context) || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" as const }, 503, authorization.session.headers);
    }
    const parsed = emptySchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" as const }, 400, authorization.session.headers);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({ email: authorization.session.user.email, type: "email-verification" }),
      method: "POST",
      path: "/api/auth/email-otp/send-verification-otp",
    });
    if (!response.ok) {
      return authJsonResponse(
        { error: response.status === 429 ? "rate_limited" : "email_otp_unavailable" },
        response.status >= 500 ? 503 : response.status,
        response.headers,
      );
    }
    return authJsonResponse({ ok: true }, 200, response.headers);
  })
  .post("/account/email/current-otp/verify", async (context) => {
    const authorization = await authenticated(context);
    if (!authorization.session) return authorization.response;
    if (!resendRequired(context) || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" as const }, 503, authorization.session.headers);
    }
    const parsed = currentOtpSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" as const }, 400, authorization.session.headers);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({
        email: authorization.session.user.email,
        otp: parsed.data.otp,
        type: "email-verification",
      }),
      method: "POST",
      path: "/api/auth/email-otp/check-verification-otp",
    });
    return projectSimpleSuccess(response, "email_otp_invalid");
  })
  .post("/account/email/change/request", async (context) => {
    const authorization = await authenticated(context);
    if (!authorization.session) return authorization.response;
    if (!resendRequired(context) || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" as const }, 503, authorization.session.headers);
    }
    const parsed = emailChangeRequestSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_change_failed" as const }, 400, authorization.session.headers);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({ newEmail: parsed.data.newEmail.toLowerCase(), otp: parsed.data.currentOtp }),
      method: "POST",
      path: "/api/auth/email-otp/request-email-change",
    });
    return projectSimpleSuccess(response, "email_change_failed");
  })
  .post("/account/email/change/verify", async (context) => {
    const authorization = await authenticated(context);
    if (!authorization.session) return authorization.response;
    if (!resendRequired(context) || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" as const }, 503, authorization.session.headers);
    }
    const parsed = emailChangeVerifySchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_change_failed" as const }, 400, authorization.session.headers);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({ newEmail: parsed.data.newEmail.toLowerCase(), otp: parsed.data.otp }),
      method: "POST",
      path: "/api/auth/email-otp/change-email",
    });
    return projectSimpleSuccess(response, "email_change_failed");
  });
