import { Activity, Loader2, LogOut } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { SERVICE_STATUS_QUERY_KEY, logoutFromService, refreshCodexPluginStatus, useServiceStatus } from "@/lib/connect";
import type { LocalServiceStatus } from "@onecrew/runtime-contracts/status";
import { formatDate } from "@/lib/creator";
import { pluginRepairDiagnostic } from "@/lib/plugin-status";
import { OpenCliSearch, OpenCliWizard } from "@/pages/connect/opencli-wizard";
import { clearOpenCliAuthObservations } from "@/lib/opencli-auth";
import { persistLocale } from "@/i18n";
import { ComponentCompatibility } from "@/pages/connect/component-compatibility";
import { TableBackendSync } from "@/pages/connect/table-backend-sync";

function StatusRow({ label, children }: { label: string; children: React.ReactNode }) {
  return <div className="flex items-start justify-between gap-6 py-3 text-sm">
    <dt className="shrink-0 whitespace-nowrap text-muted-foreground">{label}</dt><dd className="min-w-0 text-right wrap-anywhere">{children}</dd>
  </div>;
}

export function Connect({ freeEdition = false, freePage = "connections", onFreeNavigate }: {
  freeEdition?: boolean; freePage?: "connections" | "search"; onFreeNavigate?: () => void;
}) {
  const { t, i18n } = useTranslation("shell");
  const { t: tAccounts } = useTranslation("accounts");
  const queryClient = useQueryClient();
  const service = useServiceStatus();
  const status = service.data;
  const plugin = status?.plugin;
  const [checkedPlugin, setCheckedPlugin] = useState<NonNullable<LocalServiceStatus["plugin"]> | null>(null);
  const displayedPlugin = checkedPlugin ?? plugin;
  const pluginRepair = displayedPlugin ? pluginRepairDiagnostic(displayedPlugin) : null;
  const logout = useMutation({
    mutationFn: logoutFromService,
    onSuccess: () => { clearOpenCliAuthObservations(status?.login?.status.credential_id ?? null); void queryClient.invalidateQueries({ queryKey: SERVICE_STATUS_QUERY_KEY }); },
    onError: () => toast.error(t("service.logoutFailed")),
  });
  const pluginCheck = useMutation({
    mutationFn: refreshCodexPluginStatus,
    onMutate: () => setCheckedPlugin(null),
    onSuccess: result => setCheckedPlugin(result),
    onSettled: () => { void queryClient.invalidateQueries({ queryKey: SERVICE_STATUS_QUERY_KEY }); },
  });
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-3xl space-y-5 p-5">
        <div className="flex justify-end gap-2" aria-label={t("service.interfaceLanguage")}>
          {(["zh-CN", "en-US"] as const).map(locale => <Button key={locale} size="sm" variant={i18n.resolvedLanguage === locale ? "secondary" : "outline"}
            aria-pressed={i18n.resolvedLanguage === locale} onClick={() => { persistLocale(locale); void i18n.changeLanguage(locale); }}>{locale === "zh-CN" ? "中文" : "English"}</Button>)}
        </div>
        {service.isPending && <Loader2 className="size-4 animate-spin" aria-label={t("service.checking")} />}
        {service.isError && <Alert variant="destructive"><AlertDescription>{t("service.statusFailed")}</AlertDescription></Alert>}
        {status && <>
          {freePage === "connections" && <ComponentCompatibility version={status.version} hash={status.binary_sha256} />}
          {freeEdition && (freePage === "search" ? <OpenCliSearch credentialId={status.login?.status.credential_id ?? null} onConnections={onFreeNavigate} /> : <OpenCliWizard credentialId={status.login?.status.credential_id ?? null} plugin={displayedPlugin} pluginCheckPending={pluginCheck.isPending}
            pluginCheckFailed={pluginCheck.isError} onCheckPlugin={() => pluginCheck.mutate()} onSearch={onFreeNavigate} />)}
          {!freeEdition && <>
          <details className="rounded-lg border p-4">
            <summary className="cursor-pointer text-sm font-medium">{tAccounts("dataSyncPrivacy")}</summary>
            <div className="mt-4"><TableBackendSync key={status.login?.status.credential_id ?? ''} credentialId={status.login?.status.credential_id ?? null} /></div>
          </details>
          <Card>
            <CardHeader><CardTitle className="flex items-center gap-2"><Activity className="size-4" />{t("service.statusTitle")}</CardTitle></CardHeader>
            <CardContent>
              <dl className="divide-y">
                <StatusRow label={t("service.localService")}>{t(status.running ? "service.running" : "service.stopped")}</StatusRow>
                <StatusRow label={t("service.versionLabel")}>{status.version}</StatusRow>
                <StatusRow label={t("service.accountTitle")}>{t(`service.loginState.${status.login?.status.state ?? "unknown"}`)}</StatusRow>
                <StatusRow label={t("service.mcpSessions")}>{status.mcp_clients > 0 ? t("service.sessionsConnected", { count: status.mcp_clients }) : t("service.noSessions")}</StatusRow>
              </dl>
              <div className="mt-4 flex justify-end">
                <Button variant="outline" disabled={logout.isPending || status.login?.status.state !== "active"} onClick={() => logout.mutate()}>
                  {logout.isPending ? <Loader2 className="size-4 animate-spin" /> : <LogOut className="size-4" />}{t("service.logout")}
                </Button>
              </div>
            </CardContent>
          </Card>
          {displayedPlugin && <Card>
            <CardHeader><CardTitle>{t("service.pluginStatusTitle")}</CardTitle></CardHeader>
            <CardContent>
              <dl className="divide-y">
                <StatusRow label={t("service.installationState")}>{t(`service.pluginState.${displayedPlugin.state}`)}</StatusRow>
                <StatusRow label={t("service.enabledLabel")}>{t(displayedPlugin.enabled === true ? "service.enabled" : displayedPlugin.enabled === false ? "service.disabled" : "service.unknown")}</StatusRow>
                {displayedPlugin.installed_version && <StatusRow label={t("service.versionLabel")}>{displayedPlugin.installed_version}</StatusRow>}
                {displayedPlugin.last_verified_at != null
                  ? <StatusRow label={t("service.lastChecked")}>{formatDate(displayedPlugin.last_verified_at, i18n.language)}</StatusRow> : null}
              </dl>
              {pluginRepair && <Alert className="mt-3"><AlertDescription>{t(`service.pluginRepair.${pluginRepair}`)}</AlertDescription></Alert>}
              {/* The state above is whatever the last check recorded; this says whether it was asked again. */}
              {displayedPlugin.verification === "not_checked" ? null
                : <p className="mt-3 text-xs text-muted-foreground">{t(`service.verification.${displayedPlugin.verification}`)}</p>}
            </CardContent>
          </Card>}
          </>}
        </>}
      </div>
    </div>
  );
}
