import { Hono } from "hono";
import { z } from "zod";

import { callPrivateAuthHandler, hasJsonContentType } from "../auth/request";
import { requiresEmailOTPForLogin } from "../auth/login-risk";
import { authJsonResponse } from "../auth/session";
import type { AppEnv } from "../types/bindings";
import { projectAuthenticatedUser, readJson } from "./session-shared";
import { sendSignInOTP } from "./session-otp";

const emailSchema = z.string().max(254).email();
const passwordSchema = z.string().min(8).max(128);
const registerSchema = z
  .object({
    email: emailSchema,
    name: z.string().trim().min(1).max(100),
    password: passwordSchema,
  })
  .strict();
const loginSchema = z
  .object({
    email: emailSchema,
    password: passwordSchema,
  })
  .strict();
const logoutSchema = z.object({}).strict();

export const sessionAuthRoutes = new Hono<AppEnv>()
  .post("/register", async (context) => {
    if (!hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "registration_failed" }, 400);
    }
    const parsed = registerSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) {
      return authJsonResponse({ error: "registration_failed" }, 400);
    }
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify(parsed.data),
      method: "POST",
      path: "/api/auth/sign-up/email",
    });
    const projected = await projectAuthenticatedUser(response, "registration_failed");
    if (projected.status !== 200) return projected;
    // Keep the existing response contract when email OTP is not configured.
    // Once Resend is enabled, the extra fields tell the client to render the
    // verification step without changing the Better Auth session handling.
    if (!context.get("config").resend) return projected;
    let body: { ok?: boolean; user?: { email?: string } } | null = null;
    try { body = await projected.clone().json(); } catch { body = null; }
    return authJsonResponse({
      ok: true,
      verification_required: context.get("config").resend !== undefined,
      email: body?.user?.email ?? parsed.data.email,
      ...(body?.user ? { user: body.user } : {}),
    }, 200, projected.headers);
  })
  .post("/login", async (context) => {
    if (!hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "login_failed" }, 400);
    }
    const parsed = loginSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "login_failed" }, 400);
    const email = parsed.data.email.toLowerCase();
    if (context.get("config").resend) {
      try {
        const risky = await requiresEmailOTPForLogin(
          context.get("config").db,
          email,
          context.req.header("cf-connecting-ip"),
        );
        if (risky) {
          const sent = await sendSignInOTP(context, email);
          if (!sent.ok) return sent;
          return authJsonResponse(
            { ok: false, challenge: "email_otp", email, reason: "new_ip" },
            200,
            sent.headers,
          );
        }
      } catch {
        return authJsonResponse({ error: "login_failed" }, 503);
      }
    }
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify({ ...parsed.data, email }),
      method: "POST",
      path: "/api/auth/sign-in/email",
    });
    if (!response.ok && context.get("config").resend && response.status === 403) {
      return authJsonResponse(
        { ok: false, challenge: "email_otp", email, reason: "email_verification" },
        200,
        response.headers,
      );
    }
    return projectAuthenticatedUser(response, "login_failed");
  })
  .post("/logout", async (context) => {
    if (!hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse({ error: "logout_failed" }, 400);
    }
    const parsed = logoutSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) return authJsonResponse({ error: "logout_failed" }, 400);
    const response = await callPrivateAuthHandler(context, {
      body: JSON.stringify(parsed.data),
      method: "POST",
      path: "/api/auth/sign-out",
    });
    if (!response.ok) {
      return authJsonResponse(
        { error: "logout_failed" },
        response.status,
        response.headers,
      );
    }
    return authJsonResponse({ ok: true }, 200, response.headers);
  });
