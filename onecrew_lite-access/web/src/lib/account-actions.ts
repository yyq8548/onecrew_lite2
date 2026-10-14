import {
  AccessApiError,
  getAccountSettings,
  requestEmailChange,
  sendCurrentEmailOTP,
  verifyCurrentEmailOTP,
  updateAccountPassword,
  updateAccountProfile,
  verifyEmailChange,
  type AccountSettings,
} from "@/lib/access-api";
import { getAuthErrorMessage, type AuthLocale } from "@/lib/auth-errors";

export type AccountActionResult =
  | { ok: true }
  | { ok: false; message: string };

export async function loadAccountSettings(): Promise<AccountSettings> {
  return getAccountSettings();
}

export async function saveAccountProfile(name: string, locale: AuthLocale = "zh"): Promise<AccountActionResult> {
  try {
    await updateAccountProfile({ name });
    return { ok: true };
  } catch (error) {
    return failure(error, locale);
  }
}

export async function saveAccountPassword(input: {
  currentPassword?: string;
  newPassword: string;
}, locale: AuthLocale = "zh"): Promise<AccountActionResult> {
  try {
    await updateAccountPassword(input);
    return { ok: true };
  } catch (error) {
    return failure(error, locale);
  }
}

export async function sendCurrentEmailCode(locale: AuthLocale = "zh"): Promise<AccountActionResult> {
  try {
    await sendCurrentEmailOTP();
    return { ok: true };
  } catch (error) {
    return failure(error, locale);
  }
}

export async function verifyCurrentEmailCode(otp: string, locale: AuthLocale = "zh"): Promise<AccountActionResult> {
  try {
    await verifyCurrentEmailOTP(otp);
    return { ok: true };
  } catch (error) {
    return failure(error, locale);
  }
}

export async function requestAccountEmailChange(input: {
  newEmail: string;
  currentOtp: string;
}, locale: AuthLocale = "zh"): Promise<AccountActionResult> {
  try {
    await requestEmailChange(input);
    return { ok: true };
  } catch (error) {
    return failure(error, locale);
  }
}

export async function verifyAccountEmailChange(input: {
  newEmail: string;
  otp: string;
}, locale: AuthLocale = "zh"): Promise<AccountActionResult> {
  try {
    await verifyEmailChange(input);
    return { ok: true };
  } catch (error) {
    return failure(error, locale);
  }
}

function failure(error: unknown, locale: AuthLocale): { ok: false; message: string } {
  if (error instanceof AccessApiError) {
    return {
      ok: false,
      message: getAuthErrorMessage({ code: error.code, status: error.status }, locale),
    };
  }
  return { ok: false, message: getAuthErrorMessage(null, locale) };
}
