import {
  AccessApiError,
  loginWithOTP,
  loginAccessSession,
  logoutAccessSession,
  registerAccessSession,
  requestPasswordResetOTP,
  resetPasswordWithOTP,
  sendRegistrationOTP,
  sendLoginOTP,
  verifyRegistrationOTP,
} from "@/lib/access-api";
import { startGoogleOAuth } from "@/lib/auth-client";
import { getAuthErrorMessage, type AuthLocale } from "@/lib/auth-errors";
import { accessText } from "@/lib/access-i18n";

export type AuthActionResult =
  | { ok: true; email?: string; verificationRequired?: boolean }
  | {
      ok: false;
      challenge: "email_otp";
      email: string;
      reason: "new_ip" | "email_verification";
      message: string;
    }
  | { ok: false; message: string };

export async function signInWithEmail(input: {
  email: string;
  password: string;
}, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    const result = await loginAccessSession(input);
    if (!result.ok) {
      return {
        ok: false,
        challenge: "email_otp",
        email: result.email,
        reason: result.reason,
        message: accessText(
          result.reason === "new_ip"
            ? "auth.login.newIpChallenge"
            : "auth.login.emailVerificationChallenge",
          locale,
        ),
      };
    }
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function signUpWithEmail(input: {
  name: string;
  email: string;
  password: string;
}, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    const result = await registerAccessSession(input);
    return {
      ok: true,
      email: result.email,
      verificationRequired: result.verification_required,
    };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function sendEmailLoginOTP(email: string, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    await sendLoginOTP(email);
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function sendRegistrationEmailOTP(email: string, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    await sendRegistrationOTP(email);
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function signInWithOTP(input: {
  email: string;
  otp: string;
}, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    await loginWithOTP(input);
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function verifyRegistrationEmailOTP(input: {
  email: string;
  otp: string;
}, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    await verifyRegistrationOTP(input);
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function sendPasswordResetOTP(email: string, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    await requestPasswordResetOTP(email);
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function resetPasswordByOTP(input: {
  email: string;
  otp: string;
  password: string;
}, locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    await resetPasswordWithOTP(input);
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function startGoogleSignIn(locale: AuthLocale = "zh", returnTo?: string): Promise<AuthActionResult> {
  try {
    const result = await startGoogleOAuth(returnTo);
    return result.error
      ? { ok: false, message: getAuthErrorMessage(result.error, locale) }
      : { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

export async function signOutCurrentSession(locale: AuthLocale = "zh"): Promise<AuthActionResult> {
  try {
    await logoutAccessSession();
    return { ok: true };
  } catch (error) {
    return actionFailure(error, locale);
  }
}

function actionFailure(error: unknown, locale: AuthLocale): AuthActionResult {
  if (error instanceof AccessApiError) {
    return {
      ok: false,
      message: getAuthErrorMessage({ code: error.code, status: error.status }, locale),
    };
  }

  return { ok: false, message: getAuthErrorMessage(null, locale) };
}
