import { Hono, type Context } from "hono";
import { z } from "zod";

import { callPrivateAuthHandler, hasJsonContentType } from "../auth/request";
import { authJsonResponse } from "../auth/session";
import type { AppEnv } from "../types/bindings";
import {
  projectAuthenticatedUser,
  projectSimpleSuccess,
  readJson,
} from "./session-shared";

const emailSchema = z.string().max(254).email();
const sendSchema = z.object({ email: emailSchema }).strict();
const verifySchema = z.object({ email: emailSchema, otp: z.string().regex(/^\d{6}$/) }).strict();

type EmailOTPRequestType = "sign-in" | "email-verification";

async function sendEmailOTP(
  context: Context<AppEnv>,
  email: string,
  type: EmailOTPRequestType,
) {
  const response = await callPrivateAuthHandler(context, {
    body: JSON.stringify({ email, type }),
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
}

export function sendSignInOTP(context: Context<AppEnv>, email: string) {
  return sendEmailOTP(context, email, "sign-in");
}

export function sendRegistrationOTP(context: Context<AppEnv>, email: string) {
  return sendEmailOTP(context, email, "email-verification");
}

export const sessionOtpRoutes = new Hono<AppEnv>()
  .post("/login-otp/send", async (context) => {
    if (!context.get("config").resend || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" }, 503);
    }
    const parsed = sendSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" }, 400);
    return sendSignInOTP(context, parsed.data.email.toLowerCase());
  })
  .post("/register/verify/send", async (context) => {
    if (!context.get("config").resend || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" }, 503);
    }
    const parsed = sendSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" }, 400);
    return sendRegistrationOTP(context, parsed.data.email.toLowerCase());
  })
  .post("/forgot-password/send", async (context) => {
    if (!context.get("config").resend || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" }, 503);
    }
    const parsed = sendSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" }, 400);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({ email: parsed.data.email.toLowerCase() }),
      method: "POST",
      path: "/api/auth/email-otp/request-password-reset",
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
  .post("/forgot-password", async (context) => {
    if (!context.get("config").resend || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" }, 503);
    }
    const parsed = z
      .object({
        email: emailSchema,
        otp: z.string().regex(/^\d{6}$/),
        password: z.string().min(8).max(128),
      })
      .strict()
      .safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" }, 400);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({
        email: parsed.data.email.toLowerCase(),
        otp: parsed.data.otp,
        password: parsed.data.password,
      }),
      method: "POST",
      path: "/api/auth/email-otp/reset-password",
    });
    return projectSimpleSuccess(response, "password_reset_failed");
  })
  .post("/login-otp", async (context) => {
    if (!context.get("config").resend || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" }, 503);
    }
    const parsed = verifySchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" }, 400);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({
        email: parsed.data.email.toLowerCase(),
        otp: parsed.data.otp,
      }),
      method: "POST",
      path: "/api/auth/sign-in/email-otp",
    });
    return projectAuthenticatedUser(response, "email_otp_invalid");
  })
  .post("/register/verify", async (context) => {
    if (!context.get("config").resend || !hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "email_otp_unavailable" }, 503);
    }
    const parsed = verifySchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "email_otp_invalid" }, 400);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({
        email: parsed.data.email.toLowerCase(),
        otp: parsed.data.otp,
      }),
      method: "POST",
      path: "/api/auth/email-otp/verify-email",
    });
    return projectAuthenticatedUser(response, "email_otp_invalid");
  });
