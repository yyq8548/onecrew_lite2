import type { OperatorBrowserStatus } from "@/lib/operator-account";
import { operatorBrowserStatus } from "@/lib/operator-account";
import { queryOptions } from "@tanstack/react-query";
import type { TFunction } from "i18next";

export const OPERATOR_BROWSER_STATUS_QUERY_KEY = ["operator-browser-environment"] as const;

export function operatorBrowserStatusQueryOptions(enabled: boolean) {
  return queryOptions({
    queryKey: OPERATOR_BROWSER_STATUS_QUERY_KEY,
    queryFn: operatorBrowserStatus,
    enabled,
    retry: false,
    staleTime: Infinity,
    gcTime: Infinity,
  });
}

export type BrowserEnvironmentView = {
  ready: boolean;
  loading: boolean;
  badgeLabel: string;
  description: string;
  diagnostic: string | null;
};

export type BrowserConnectionGateState = "loading" | "error" | "blocked" | "ready";

export type BrowserConnectionGate = {
  state: BrowserConnectionGateState;
  title: string;
  description: string;
  diagnostic: string | null;
};

type AccountsT = TFunction<"accounts">;

const BROWSER_ERROR_KEYS = {
  browser_not_installed: "browser.errors.browser_not_installed",
  browser_cookie_extraction_unsupported: "browser.errors.browser_cookie_extraction_unsupported",
  browser_cookie_store_not_found: "browser.errors.browser_cookie_store_not_found",
  browser_cookie_store_locked: "browser.errors.browser_cookie_store_locked",
  browser_cookie_local_state_not_found: "browser.errors.browser_cookie_local_state_not_found",
  browser_cookie_dpapi_failed: "browser.errors.browser_cookie_dpapi_failed",
  browser_cookie_app_bound_unsupported: "browser.errors.browser_cookie_app_bound_unsupported",
  browser_cookie_decryption_failed: "browser.errors.browser_cookie_decryption_failed",
  browser_cookie_authorization_failed: "browser.errors.browser_cookie_authorization_failed",
  browser_cookie_store_read_failed: "browser.errors.browser_cookie_store_read_failed",
  browser_close_failed: "browser.errors.browser_close_failed",
  browser_launch_failed: "browser.errors.browser_launch_failed",
  proxy_validation_failed: "browser.errors.proxy_validation_failed",
  browser_connection_failed: "browser.errors.browser_connection_failed",
} as const;

function errorCodeFrom(error: unknown): string | null {
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  const code = message.split(":", 1)[0]?.trim();
  return code && code in BROWSER_ERROR_KEYS ? code : null;
}

export function browserOperationErrorMessage(
  error: unknown,
  t: AccountsT,
  fallback: string = t("browser.errors.operationFailed"),
) {
  const code = errorCodeFrom(error);
  return code ? t(BROWSER_ERROR_KEYS[code as keyof typeof BROWSER_ERROR_KEYS]) : fallback;
}

export function browserConnectionGate(
  status: OperatorBrowserStatus | null,
  busy: boolean,
  t: AccountsT,
  error: unknown = null,
): BrowserConnectionGate {
  if (error) {
    return {
      state: "error",
      title: t("browser.gate.detectionFailedTitle"),
      description: browserOperationErrorMessage(error, t, t("browser.gate.detectionFailedDescription")),
      diagnostic: errorCodeFrom(error),
    };
  }
  if (status?.state === "ready") {
    return {
      state: "ready",
      title: t("browser.gate.readyTitle"),
      description: `${status.version ? `Chrome ${status.version} · ` : ""}${status.path ?? t("browser.gate.installed")}`,
      diagnostic: null,
    };
  }
  if (busy || status === null) {
    return {
      state: "loading",
      title: t("browser.gate.detectingTitle"),
      description: t("browser.gate.detectingDescription"),
      diagnostic: null,
    };
  }
  const code = status.error_code ?? null;
  return {
    state: "blocked",
    title: status.state === "browser_not_found"
      ? t("browser.gate.notFoundTitle")
      : t("browser.gate.unavailableTitle"),
    description: code
      ? t(BROWSER_ERROR_KEYS[code as keyof typeof BROWSER_ERROR_KEYS])
      : t("browser.gate.unsupportedDescription"),
    diagnostic: code,
  };
}

export function browserEnvironmentView(
  status: OperatorBrowserStatus | null,
  busy: boolean,
  t: AccountsT,
): BrowserEnvironmentView {
  const gate = browserConnectionGate(status, busy, t);
  const ready = gate.state === "ready";
  const loading = gate.state === "loading";
  return {
    ready,
    loading,
    badgeLabel: ready
      ? t("browser.badges.ready")
      : loading
        ? t("browser.badges.detecting")
        : t("browser.badges.notFound"),
    description: gate.description,
    diagnostic: gate.diagnostic,
  };
}

export function browserConnectionReady(status: OperatorBrowserStatus | null) {
  return status?.state === "ready";
}

export function browserCookieImportCopy(status: OperatorBrowserStatus | null, t: AccountsT) {
  if (status?.cookie_provider === "macos_safe_storage") {
    return {
      title: t("browser.cookieImport.macos.title"),
      description: t("browser.cookieImport.macos.description"),
    };
  }
  if (status?.cookie_provider === "windows_legacy_dpapi" || status?.cookie_provider === "windows_chrome_assisted") {
    return {
      title: t("browser.cookieImport.windows.title"),
      description: t("browser.cookieImport.windows.description"),
    };
  }
  return {
    title: t("browser.cookieImport.generic.title"),
    description: t("browser.cookieImport.generic.description"),
  };
}
