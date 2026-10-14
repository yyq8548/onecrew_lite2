import accessI18n from "./i18n";

export interface AuthErrorLike {
  code?: string;
  status?: number;
}

export type AuthLocale = "zh" | "en";

const AUTH_ERROR_KEYS: Readonly<Record<string, string>> = {
  INVALID_EMAIL_OR_PASSWORD: "errors.invalidCredentials",
  invalid_credentials: "errors.invalidCredentials",
  INVALID_PASSWORD: "errors.invalidPassword",
  USER_NOT_FOUND: "errors.invalidCredentials",
  USER_ALREADY_EXISTS: "errors.userAlreadyExists",
  user_already_exists: "errors.userAlreadyExists",
  USER_ALREADY_EXISTS_USE_ANOTHER_EMAIL: "errors.userAlreadyExists",
  PASSWORD_TOO_SHORT: "errors.passwordTooShort",
  INVALID_EMAIL: "errors.invalidEmail",
  EMAIL_NOT_VERIFIED: "errors.emailNotVerified",
  TOO_MANY_REQUESTS: "errors.tooManyRequests",
  email_otp_invalid: "errors.emailOtpInvalid",
  email_otp_unavailable: "errors.emailOtpUnavailable",
  email_change_failed: "errors.emailChangeFailed",
  password_reset_failed: "errors.passwordResetFailed",
  password_update_failed: "errors.passwordUpdateFailed",
  profile_update_failed: "errors.profileUpdateFailed",
  account_settings_failed: "errors.accountSettingsFailed",
};

const CALLBACK_ERROR_KEYS: Readonly<Record<string, string>> = {
  access_denied: "errors.oauth.accessDenied",
  account_already_linked_to_different_user: "errors.oauth.accountAlreadyLinked",
  email_not_found: "errors.oauth.emailNotFound",
  email_not_verified: "errors.oauth.emailNotVerified",
  invalid_code: "errors.oauth.invalidCode",
  no_code: "errors.oauth.noCode",
  oauth_callback_error: "errors.oauth.callback",
  oauth_provider_not_found: "errors.oauth.providerUnavailable",
  state_not_found: "errors.oauth.stateNotFound",
  state_mismatch: "errors.oauth.stateMismatch",
  unable_to_get_user_info: "errors.oauth.unableToReadUser",
  signup_disabled: "errors.oauth.signupDisabled",
};

function languageFor(locale: AuthLocale) {
  return locale === "en" ? "en-US" : "zh-CN";
}

export function getAuthErrorMessage(
  error: AuthErrorLike | null | undefined,
  locale: AuthLocale = "zh",
) {
  const language = languageFor(locale);
  if (error?.status === 429) {
    return accessI18n.t("errors.tooManyRequests", { lng: language });
  }
  const key = error?.code ? AUTH_ERROR_KEYS[error.code] : undefined;
  if (key) return accessI18n.t(key, { lng: language });
  if (error?.status === 401) {
    return accessI18n.t("errors.invalidCredentials", { lng: language });
  }
  if (error?.status === 409) {
    return accessI18n.t("errors.userAlreadyExists", { lng: language });
  }
  return accessI18n.t("errors.generic", { lng: language });
}

export function getCallbackErrorMessage(search: string, locale: AuthLocale = "zh") {
  const params = new URLSearchParams(search);
  const code = params.get("error") ?? params.get("code");
  if (!code) return null;
  const key = CALLBACK_ERROR_KEYS[code] ?? "errors.oauth.generic";
  return accessI18n.t(key, { lng: languageFor(locale) });
}
