import type { LoginProjection } from "@/lib/generated/creator-sync-contracts";

export type PairingGatePhase =
  | "checking"
  | "idle"
  | "waiting"
  | "callback"
  | "enrolling"
  | "expired"
  | "canceled"
  | "invalidated"
  | "remote_unavailable"
  | "rate_limited"
  | "device_name_unavailable"
  | "device_name_invalid"
  | "error"
  | "active";

export type PairingGatePrimaryAction = "begin" | "reopen" | "restart" | null;

export type PairingGateCopy =
  | "checking"
  | "idle"
  | "waiting"
  | "callback"
  | "enrolling"
  | "expired"
  | "canceled"
  | "invalidated"
  | "remoteUnavailable"
  | "rateLimited"
  | "deviceNameUnavailable"
  | "deviceNameInvalid"
  | "error";

export type PairingGatePrimaryLabel =
  | "begin"
  | "reopen"
  | "restart"
  | "rebind"
  | "retryNetwork"
  | "retry";

export type PairingGateProgress = "checking" | "callback" | "enrolling";

export type PairingGateAlertCopy =
  | "expired"
  | "canceled"
  | "invalidated"
  | "remoteUnavailable"
  | "rateLimited"
  | "deviceNameUnavailable"
  | "deviceNameInvalid"
  | "error";

export type PairingGateView = {
  copy: PairingGateCopy | null;
  primaryAction: PairingGatePrimaryAction;
  primaryLabel: PairingGatePrimaryLabel | null;
  canCancel: boolean;
  progress: PairingGateProgress | null;
  alert: {
    variant: "warning" | "destructive";
    copy: PairingGateAlertCopy;
  } | null;
  renderChildren: boolean;
};

export function pairingGateView(phase: PairingGatePhase): PairingGateView {
  switch (phase) {
    case "checking":
      return view({
        copy: "checking",
        progress: "checking",
      });
    case "idle":
      return view({
        copy: "idle",
        primaryAction: "begin",
        primaryLabel: "begin",
      });
    case "waiting":
      return view({
        copy: "waiting",
        primaryAction: "reopen",
        primaryLabel: "reopen",
        canCancel: true,
      });
    case "callback":
      return view({
        copy: "callback",
        progress: "callback",
      });
    case "enrolling":
      return view({
        copy: "enrolling",
        progress: "enrolling",
      });
    case "expired":
      return restartView("expired", "expired", "warning");
    case "canceled":
      return restartView("canceled", "canceled", "warning");
    case "invalidated":
      return view({
        copy: "invalidated",
        primaryAction: "restart",
        primaryLabel: "rebind",
        alert: {
          variant: "warning",
          copy: "invalidated",
        },
      });
    case "remote_unavailable":
      return view({
        copy: "remoteUnavailable",
        primaryAction: "restart",
        primaryLabel: "retryNetwork",
        alert: {
          variant: "warning",
          copy: "remoteUnavailable",
        },
      });
    case "rate_limited":
      return view({
        copy: "rateLimited",
        primaryAction: "restart",
        primaryLabel: "retry",
        alert: {
          variant: "warning",
          copy: "rateLimited",
        },
      });
    case "device_name_unavailable":
      return restartView("deviceNameUnavailable", "deviceNameUnavailable", "destructive");
    case "device_name_invalid":
      return restartView("deviceNameInvalid", "deviceNameInvalid", "destructive");
    case "error":
      return restartView("error", "error", "destructive");
    case "active":
      return view({
        copy: null,
        renderChildren: true,
      });
  }
}

export function pairingGatePhase(projection: LoginProjection): PairingGatePhase {
  if (projection.issue === "canceled" || projection.status.issue === "pairing_cancelled") {
    return "canceled";
  }
  if (projection.issue === "callback_expired" || projection.status.issue === "pairing_expired") {
    return "expired";
  }
  if (
    (projection.status.state === "revoked" || projection.status.state === "unauthorized")
    && projection.status.issue !== "safe_cleanup_incomplete"
  ) {
    return "invalidated";
  }
  if (projection.status.issue === "remote_unavailable") return "remote_unavailable";
  if (projection.status.issue === "remote_rate_limited") return "rate_limited";
  if (projection.issue === "device_name_unavailable") return "device_name_unavailable";
  if (projection.issue === "device_name_invalid") return "device_name_invalid";
  if (projection.issue !== null) return "error";
  if (projection.status.issue !== null) return "error";
  if (projection.status.state === "validating") return "checking";
  if (projection.status.state === "pairing_completing") return "enrolling";
  if (projection.callback_received && projection.status.state === "pairing_pending") return "callback";
  switch (projection.status.state) {
    case "pairing_required":
      return "idle";
    case "pairing_pending":
      return "waiting";
    case "active":
      return projection.status.installation_id && projection.status.credential_id
        ? "active"
        : "error";
    case "unconfigured":
    case "validation_required":
    case "revoked":
    case "unauthorized":
    case "recovery_required":
    case "temporarily_unavailable":
      return "error";
  }
}

export function pairingOwnerKey(projection: LoginProjection): string | null {
  if (
    projection.status.state !== "active"
    || !projection.status.installation_id
    || !projection.status.credential_id
  ) return null;
  return `creator-sync:${projection.status.installation_id}:${projection.status.credential_id}`;
}

function restartView(
  copy: PairingGateCopy,
  alertCopy: PairingGateAlertCopy,
  variant: "warning" | "destructive",
): PairingGateView {
  return view({
    copy,
    primaryAction: "restart",
    primaryLabel: "restart",
    alert: {
      variant,
      copy: alertCopy,
    },
  });
}

function view(input: Partial<PairingGateView> & Pick<PairingGateView, "copy">): PairingGateView {
  return {
    copy: input.copy,
    primaryAction: input.primaryAction ?? null,
    primaryLabel: input.primaryLabel ?? null,
    canCancel: input.canCancel ?? false,
    progress: input.progress ?? null,
    alert: input.alert ?? null,
    renderChildren: input.renderChildren ?? false,
  };
}
