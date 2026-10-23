import { invoke } from "@/lib/local-runtime";

import type { Platform } from "@/types";

export type OperatorConnectionMethod = "browser" | "telegram_qr" | "telegram_phone" | "existing_account";
export type OperatorConnectionPhase =
  | "awaiting_secret_in_app"
  | "ready_to_begin"
  | "starting"
  | "awaiting_user_in_app"
  | "finalizing"
  | "completed"
  | "canceled"
  | "failed";
export type OperatorHandoffKind =
  | "configure_connection_in_desktop"
  | "complete_browser_login_in_desktop"
  | "scan_telegram_qr_in_desktop"
  | "enter_telegram_otp_in_desktop"
  | "enter_telegram_two_factor_in_desktop";

export type OperatorConnectionStatus = {
  flow_id: string;
  platform: Platform;
  intent: "connect" | "reconnect";
  method: OperatorConnectionMethod;
  phase: OperatorConnectionPhase;
  operator_account?: {
    operator_account_id: string;
    provider_account_id: string;
    username: string;
    label: string;
  };
  handoff?: { kind: OperatorHandoffKind; message: string };
  allowed_actions: Array<"open_desktop" | "begin" | "status" | "finalize" | "cancel">;
  error_code?: string;
  browser_capability?: OperatorBrowserStatus | null;
};

export type OperatorInAppHandoff = {
  flow_id: string;
  telegram_qr_challenge?: { image_data_url: string; expires_at: number };
  telegram_code_delivery?: {
    method: string;
    code_length: number | null;
    destination_hint: string | null;
    timeout_seconds: number | null;
    next_method: string | null;
    action_url: string | null;
    can_submit_code: boolean;
  };
};

export type OperatorBrowserStatus = {
  installed: boolean;
  version: string | null;
  path: string | null;
  os: "macos" | "windows" | "linux";
  state: "ready" | "browser_not_found" | "cookie_import_unavailable";
  channel?: "stable" | "beta" | "dev" | "canary" | "unknown" | null;
  install_scope?: "user" | "machine" | "custom" | "unknown" | null;
  cookie_provider:
    | "macos_safe_storage"
    | "windows_chrome_assisted"
    | "windows_legacy_dpapi"
    | "unsupported";
  limitations: string[];
  completion_notice?: {
    provider: "macos_safe_storage" | "windows_chrome_assisted" | "windows_legacy_dpapi" | "unsupported";
    message: string;
  } | null;
  error_code?: "browser_not_installed" | "browser_cookie_extraction_unsupported" | null;
};

export const operatorFlowKey = (flowId: string, action: "begin" | "finalize") =>
  `desktop:${action}:${flowId}`;

export async function prepareOperatorConnection(input: {
  platform: Platform;
  intent: "connect" | "reconnect";
  method: OperatorConnectionMethod;
  operator_account_id?: string;
  label?: string;
}) {
  return invoke<OperatorConnectionStatus>("onecrew_operator_account_connection_prepare", { input });
}

export async function operatorConnectionStatus(flowId: string) {
  return invoke<OperatorConnectionStatus>("onecrew_operator_account_connection_status", {
    input: { flow_id: flowId },
  });
}

export async function beginOperatorConnection(flowId: string) {
  return invoke<OperatorConnectionStatus>("onecrew_operator_account_connection_begin", {
    input: { flow_id: flowId, confirmed: true, idempotency_key: operatorFlowKey(flowId, "begin") },
  });
}

export async function finalizeOperatorConnection(flowId: string) {
  return invoke<OperatorConnectionStatus>("onecrew_operator_account_connection_finalize", {
    input: { flow_id: flowId, confirmed: true, idempotency_key: operatorFlowKey(flowId, "finalize") },
  });
}

export async function cancelOperatorConnection(flowId: string) {
  return invoke<OperatorConnectionStatus>("onecrew_operator_account_connection_cancel", {
    input: { flow_id: flowId, confirmed: true },
  });
}

type OperatorConnectionCompleted = boolean | (() => boolean);

const DEFAULT_CANCEL_RETRY_DELAYS_MS = [75, 250, 750] as const;

const isTerminalCancelError = (error: unknown) => {
  const message = String(error).toLowerCase();
  return message.includes("completed operator account connections cannot be canceled")
    || message.includes("operator account connection flow not found");
};

/** Best-effort, retrying cleanup for a GUI-owned connection operation.
 *
 * React StrictMode may run an effect cleanup before a user has started an
 * operation, while a successful flow closes its dialog immediately after the
 * runtime reports completion. Neither case may issue a cancel. A begin/finalize
 * may still be in flight when the dialog closes, so transient cancel failures
 * are retried and the latest completed state is checked before every attempt. */
export async function cleanupPendingOperatorConnection(
  flowId: string | null,
  completed: OperatorConnectionCompleted,
  cancel: (flowId: string) => Promise<unknown> = cancelOperatorConnection,
  retryDelaysMs: readonly number[] = DEFAULT_CANCEL_RETRY_DELAYS_MS,
): Promise<void> {
  const isCompleted = () => typeof completed === "function" ? completed() : completed;
  if (!flowId || isCompleted()) return;

  for (let attempt = 0; ; attempt += 1) {
    if (isCompleted()) return;
    try {
      await cancel(flowId);
      return;
    } catch (error) {
      if (isTerminalCancelError(error) || attempt >= retryDelaysMs.length) return;
      await new Promise((resolve) => setTimeout(resolve, retryDelaysMs[attempt]));
    }
  }
}

export async function operatorInAppHandoff(flowId: string) {
  return invoke<OperatorInAppHandoff>("onecrew_operator_account_connection_handoff_in_app", {
    flowId,
  });
}

export async function operatorBrowserStatus() {
  return invoke<OperatorBrowserStatus>("onecrew_operator_account_browser_status_in_app");
}
