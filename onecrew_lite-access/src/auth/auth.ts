import { betterAuth } from "better-auth";
import { emailOTP } from "better-auth/plugins";
import type { Account } from "better-auth";
import type { RuntimeConfig } from "../config/env";
import { resolveEmailLocale } from "./email-locale";
import { sendVerificationOTP } from "./email-otp";
import { attributionContext, recordSuccessEvent } from "../analytics/service";

export function discardIdToken<T extends Partial<Account> & Record<string, unknown>>(
  account: T,
) {
  return {
    ...account,
    idToken: null,
  };
}

export function createAuth(config: RuntimeConfig) {
  const emailOtp = config.resend
    ? emailOTP({
        allowedAttempts: 3,
        // OTP sign-in is also the passwordless first-registration path. A
        // verified email may create the user; the account page can add a
        // password later through Better Auth's server-only setPassword API.
        disableSignUp: false,
        changeEmail: {
          enabled: true,
          verifyCurrentEmail: true,
        },
        expiresIn: 300,
        overrideDefaultEmailVerification: true,
        resendStrategy: "rotate",
        rateLimit: {
          // Better Auth applies this rule to every email-OTP endpoint. The
          // public send wrapper below tightens only the send operation to one
          // request per 60 seconds via rateLimit.customRules.
          window: 60,
          max: 3,
        },
        sendVerificationOnSignUp: true,
        storeOTP: "hashed",
        async sendVerificationOTP(data, context) {
          await sendVerificationOTP(
            config.resend!,
            data,
            resolveEmailLocale(context?.request),
          );
        },
      })
    : null;
  return betterAuth({
    account: {
      encryptOAuthTokens: true,
    },
    advanced: {
      cookiePrefix: config.secureCookies ? "__Host-onecrew" : "better-auth",
      defaultCookieAttributes: config.secureCookies
        ? {
            httpOnly: true,
            path: "/",
            sameSite: "lax",
            secure: true,
          }
        : undefined,
      ipAddress: {
        ipAddressHeaders: ["cf-connecting-ip"],
      },
      // The HTTPS prefix above is already the full RFC cookie name prefix.
      // Disable Better Auth's automatic __Secure- prefix to avoid producing a
      // double-prefixed cookie, while retaining Secure through the defaults.
      useSecureCookies: false,
    },
    appName: "Reacher",
    basePath: "/api/auth",
    baseURL: config.authOrigin,
    database: config.db,
    emailVerification: config.resend
      ? {
          autoSignInAfterVerification: true,
          sendOnSignIn: true,
          sendOnSignUp: true,
        }
      : undefined,
    emailAndPassword: {
      autoSignIn: config.resend === undefined,
      enabled: true,
      requireEmailVerification: config.resend !== undefined,
    },
    databaseHooks: {
      user: {
        create: {
          after: async (user, context) => { await recordSuccessEvent(config.db, user.id, "sign_up", user.id, attributionContext(context?.request?.headers)); },
        },
      },
      account: {
        create: {
          before: async (account) => ({ data: discardIdToken(account) }),
        },
        update: {
          before: async (account) => ({ data: discardIdToken(account) }),
        },
      },
    },
    onAPIError: {
      errorURL: `${config.webOrigin}/login`,
    },
    rateLimit: {
      enabled: true,
      storage: "database",
      customRules: config.resend
        ? {
            "/email-otp/send-verification-otp": { window: 60, max: 1 },
            "/email-otp/request-password-reset": { window: 60, max: 1 },
            "/email-otp/request-email-change": { window: 60, max: 1 },
          }
        : undefined,
    },
    secret: config.authSecret,
    plugins: emailOtp ? [emailOtp] : undefined,
    socialProviders: config.google
      ? {
          google: {
            clientId: config.google.clientId,
            clientSecret: config.google.clientSecret,
          },
        }
      : {},
    trustedOrigins: [config.webOrigin],
  });
}

export type ReacherAuth = ReturnType<typeof createAuth>;
