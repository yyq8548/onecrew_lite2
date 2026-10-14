import { Link2Off, LoaderCircle, Smartphone, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  AccessApiError,
  deleteAccessDevice,
  revokeAccessDevice,
  type DeviceSummary,
  type SafeSession,
} from "@/lib/access-api";
import {
  type DeviceFailureCode,
  useDeviceAccessState,
} from "@/lib/use-device-access-state";
import { accessText, useAccessI18n } from "@/lib/access-i18n";
import type { AuthLocale } from "@/lib/auth-errors";

type SelectedDeviceAction = {
  device: DeviceSummary;
  kind: "delete" | "revoke";
};

export function DeviceAccessPanel({
  embedded = false,
  ownerUserId,
  refreshSession,
}: {
  embedded?: boolean;
  ownerUserId: string;
  refreshSession: () => Promise<SafeSession>;
}) {
  const { locale, t } = useAccessI18n();
  const mounted = useRef(false);
  const [selectedAction, setSelectedAction] = useState<SelectedDeviceAction | null>(null);
  const [activeDeviceId, setActiveDeviceId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const handleAuthenticationRequired = useCallback(() => {
    void refreshSession().catch(() => undefined);
  }, [refreshSession]);
  const devices = useDeviceAccessState({
    onAuthenticationRequired: handleAuthenticationRequired,
    ownerUserId,
  });

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  async function handleRevoke(device: DeviceSummary) {
    if (activeDeviceId) return;
    setActionError(null);
    setActiveDeviceId(device.deviceId);
    try {
      const revoked = await revokeAccessDevice(device.deviceId);
      if (mounted.current) {
        devices.replaceDevice(revoked);
        const refreshed = await devices.refresh();
        if (refreshed === null && mounted.current) {
          setActionError(t("account.devices.revokedRefresh"));
        }
      }
    } catch (error) {
      if (mounted.current) {
        setActionError(deviceFailureMessage(error, "revoke", locale));
        if (isAuthenticationFailure(error)) handleAuthenticationRequired();
        void devices.refresh();
      }
    } finally {
      if (mounted.current) setActiveDeviceId(null);
    }
  }

  async function handleDelete(device: DeviceSummary) {
    if (activeDeviceId) return;
    setActionError(null);
    setActiveDeviceId(device.deviceId);
    try {
      const deletedDeviceId = await deleteAccessDevice(device.deviceId);
      if (mounted.current) {
        devices.removeDevice(deletedDeviceId);
        const refreshed = await devices.refresh();
        if (refreshed === null && mounted.current) {
          setActionError(t("account.devices.deletedRefresh"));
        }
      }
    } catch (error) {
      if (mounted.current) {
        setActionError(deviceFailureMessage(error, "delete", locale));
        if (isAuthenticationFailure(error)) handleAuthenticationRequired();
        void devices.refresh();
      }
    } finally {
      if (mounted.current) setActiveDeviceId(null);
    }
  }

  const content = (
    <CardContent className={embedded ? "p-6" : undefined}>
      {devices.state.error ? (
        <Alert className="mb-4" variant="destructive">
          <AlertTitle>{t("account.devices.listUnavailable")}</AlertTitle>
          <AlertDescription>{deviceFailureMessage(devices.state.error, "list", locale)}</AlertDescription>
        </Alert>
      ) : null}
      {actionError ? (
        <Alert className="mb-4" variant="destructive">
          <AlertTitle>{t("account.devices.operationConfirm")}</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {devices.state.phase === "loading" ? (
        <div className="flex items-center gap-2 py-6 text-sm text-muted-foreground" aria-busy="true">
          <LoaderCircle aria-hidden="true" className="animate-spin" />
          {t("account.devices.loading")}
        </div>
      ) : devices.state.devices.length === 0 ? (
        <div className="account-empty-state rounded-lg border border-dashed border-border px-4 py-8 text-center">
          <Smartphone aria-hidden="true" className="mx-auto size-7 text-muted-foreground" />
          <p className="mt-3 text-sm font-medium">{t("account.devices.empty")}</p>
        </div>
      ) : (
        <ul className="account-device-list divide-y divide-border">
          {devices.state.devices.map((device) => (
            <DeviceRow
              busy={devices.state.phase !== "idle" || activeDeviceId !== null}
              device={device}
              key={device.deviceId}
              onAction={(kind) => setSelectedAction({ device, kind })}
              working={activeDeviceId === device.deviceId}
            />
          ))}
        </ul>
      )}

      {devices.state.nextCursor ? (
        <Button
          className="mt-5 w-full sm:w-auto"
          disabled={devices.state.phase !== "idle"}
          onClick={() => void devices.loadMore()}
          type="button"
          variant="outline"
        >
          {devices.state.phase === "loading_more" ? (
            <LoaderCircle aria-hidden="true" className="animate-spin" />
          ) : null}
          {t("account.devices.loadMore")}
        </Button>
      ) : null}
    </CardContent>
  );

  const actionDialog = (
    <AlertDialog
      onOpenChange={(open) => {
        if (!open) setSelectedAction(null);
      }}
      open={selectedAction !== null}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="min-w-0 [overflow-wrap:anywhere]">
            {selectedAction?.kind === "delete" ? t("account.devices.deleteTitle", { label: selectedAction ? deviceDisplayLabel(selectedAction.device.label) : "" }) : t("account.devices.revokeTitle", { label: selectedAction ? deviceDisplayLabel(selectedAction.device.label) : "" })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {selectedAction?.kind === "delete"
              ? t("account.devices.deleteDescription")
              : t("account.devices.revokeDescription")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={activeDeviceId !== null}>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            disabled={activeDeviceId !== null}
            onClick={() => {
              if (!selectedAction) return;
              if (selectedAction.kind === "delete") {
                void handleDelete(selectedAction.device);
              } else {
                void handleRevoke(selectedAction.device);
              }
            }}
          >
            {activeDeviceId ? (
              <LoaderCircle aria-hidden="true" className="animate-spin" />
            ) : selectedAction?.kind === "delete" ? (
              <Trash2 aria-hidden="true" />
            ) : (
              <Link2Off aria-hidden="true" />
            )}
            {selectedAction?.kind === "delete" ? t("account.devices.confirmDelete") : t("account.devices.confirmRevoke")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );

  if (embedded) return <>{content}{actionDialog}</>;
  return (
    <section aria-labelledby="device-access-heading" className="account-device-section">
      <Card className="account-device-card">
        <CardHeader>
          <CardTitle asChild><h2 id="device-access-heading">{t("account.devicesTitle")}</h2></CardTitle>
        </CardHeader>
        {content}
      </Card>
      {actionDialog}
    </section>
  );
}

function DeviceRow({
  busy,
  device,
  onAction,
  working,
}: {
  busy: boolean;
  device: DeviceSummary;
  onAction: (kind: "delete" | "revoke") => void;
  working: boolean;
}) {
  const { t } = useAccessI18n();
  const action = device.status === "revoked" || device.status === "expired"
    ? "delete"
    : "revoke";
  const displayLabel = deviceDisplayLabel(device.label);
  return (
    <li className="account-device-row min-w-0 first:pt-0 last:pb-0">
      <div className="flex min-w-0 flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <h3 className="truncate font-medium">{displayLabel}</h3>
            <DeviceStatusBadge status={device.status} />
          </div>
          <dl className="account-device-details mt-3 grid gap-1 text-xs leading-5 text-muted-foreground">
            <Detail label={t("account.devices.system")} value={`${platformLabel(device.platform)} · ${device.appVersion}`} />
            <Detail label={t("account.devices.bindingIp")} value={device.pairingIp ?? "—"} />
            <Detail label={t("account.devices.bindingLocation")} value={locationText(device.pairingLocation)} />
            <DateDetail
              label={device.pairedAt ? t("account.devices.connectedAt") : t("account.devices.addedAt")}
              value={device.pairedAt ?? device.createdAt}
            />
            {device.revokedAt ? <DateDetail label={t("account.devices.revokedAt")} value={device.revokedAt} /> : null}
          </dl>
        </div>
        <div className="flex w-full shrink-0 flex-col gap-2 sm:w-auto sm:flex-row">
          <Button
            aria-label={`${action === "delete" ? t("account.devices.delete") : t("account.devices.revoke")}“${displayLabel}”`}
            className="w-full sm:w-auto"
            disabled={busy || working}
            onClick={() => onAction(action)}
            size="sm"
            type="button"
            variant="destructive"
          >
            {working ? (
              <LoaderCircle aria-hidden="true" className="animate-spin" />
            ) : action === "delete" ? (
              <Trash2 aria-hidden="true" />
            ) : (
              <Link2Off aria-hidden="true" />
            )}
            {action === "delete" ? t("account.devices.deleteButton") : t("account.devices.revokeButton")}
          </Button>
        </div>
      </div>
    </li>
  );
}

function deviceDisplayLabel(label: string) {
  if (label === "Agent Platform Local") return "OneCrew Local";
  if (label === "Agent Platform") return "OneCrew";
  return label;
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="inline font-medium text-foreground">{label}：</dt>{" "}
      <dd className="inline">{value}</dd>
    </div>
  );
}

function DateDetail({ label, value }: { label: string; value: string }) {
  const { locale } = useAccessI18n();
  const formatter = new Intl.DateTimeFormat(locale === "en" ? "en-US" : "zh-CN", {
    dateStyle: "medium",
    timeStyle: "short",
  });
  return (
    <div>
      <dt className="inline font-medium text-foreground">{label}：</dt>{" "}
      <dd className="inline"><time dateTime={value}>{formatter.format(new Date(value))}</time></dd>
    </div>
  );
}

function locationText(location: DeviceSummary["pairingLocation"]) {
  const parts = [location.city, location.region, location.country].filter(Boolean);
  return parts.join(" · ") || "—";
}

function DeviceStatusBadge({ status }: { status: DeviceSummary["status"] }) {
  const { t } = useAccessI18n();
  if (status === "active") return <Badge variant="success">{t("account.devices.active")}</Badge>;
  if (status === "pending") return <Badge variant="secondary">{t("account.devices.pending")}</Badge>;
  if (status === "expired") return <Badge variant="outline">{t("account.devices.expired")}</Badge>;
  return <Badge variant="destructive">{t("account.devices.revoked")}</Badge>;
}

function platformLabel(platform: DeviceSummary["platform"]) {
  if (platform === "macos") return "macOS";
  if (platform === "windows") return "Windows";
  return "Linux";
}

function isAuthenticationFailure(error: unknown) {
  return error instanceof AccessApiError && error.code === "authentication_required";
}

function deviceFailureMessage(
  failure: DeviceFailureCode | unknown,
  action: "delete" | "list" | "revoke",
  locale: AuthLocale,
) {
  const code =
    typeof failure === "string"
      ? failure
      : failure instanceof AccessApiError
        ? (failure.code ?? "unknown")
        : "unknown";
  if (code === "authentication_required") return accessText("account.devices.authFailure", locale);
  if (code === "device_not_found") return accessText("account.devices.notFound", locale);
  if (code === "device_must_be_revoked") return accessText("account.devices.mustRevoke", locale);
  if (code === "device_revocation_conflict") return accessText("account.devices.revokeConflict", locale);
  if (code === "device_revocation_unavailable") return accessText("account.devices.revokeUnavailable", locale);
  if (code === "rate_limited") return accessText("account.devices.rateLimited", locale);
  if (action === "revoke") return accessText("account.devices.revokeUnknown", locale);
  if (action === "delete") return accessText("account.devices.deleteUnknown", locale);
  return accessText("account.devices.listUnknown", locale);
}
