import { platformDisplayName } from "@/lib/platform-display";
import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { CheckCircle2, ChevronDown, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  checkOpenCliProfile, OPENCLI_PROFILE_PREFERENCE_KEY,
  selectedProfileIsConnected, useOpenCliSetup, useOpenCliRecovery,
  canInstallOpenCliAddons, useInstallOpenCliAddons, openOpenCliSetupPage,
  openCliVersionIsCompatible,
  useRepairOpenCliInstallation,
  type OpenCliSetupReport,
} from "@/lib/opencli-setup";
import { DiscoveryPilot } from "./discovery-pilot";
import { displayOpenCliAuthObservations } from "@/lib/opencli-auth";
import { DefaultSyncAgreement } from "./default-sync-agreement";
import { platformWebsite } from "./platform-websites";
import { checkOpenCliSitesAuth, openOpenCliLoginPage, readOpenCliAuthObservations, saveOpenCliAuthObservation, isFreshOpenCliAuthObservation, type OpenCliAuthObservation } from "@/lib/opencli-auth";
import { formatDate } from "@/lib/creator";
import { openCliPlatformIsGreen, openCliPlatformNeedsSignIn, orderOpenCliPlatforms } from "@/lib/opencli-platform-presentation";
import type { LocalServiceStatus } from "@/lib/generated/creator-sync-contracts";
import { openCliSearchAccess, openCliSearchNeedsLoginCheck } from "@/lib/opencli-search-access";

const CODEX_USE_ACKNOWLEDGMENT_KEY = "onecrew:codex-use-acknowledgment:v1";
const ACTION_CLASS = "min-h-10 border-border bg-background text-foreground shadow-none hover:bg-muted hover:border-foreground/30 font-medium";
const SUCCESS_TEXT = "text-[var(--tone-success-fg)]";

type Issue = "not_installed" | "version_incompatible" | "daemon_unavailable" | "extension_unavailable" | "profile_unavailable" | "timeout" | "output_too_large" | "version_changed" | "read_command_unavailable" | "invalid_profile" | "check_failed";
const ISSUES: readonly string[] = ["not_installed", "version_incompatible", "daemon_unavailable", "extension_unavailable", "profile_unavailable", "timeout", "output_too_large", "version_changed", "read_command_unavailable", "invalid_profile", "check_failed"];
function issueKey(value: string | null): Issue | null {
  if (!value) return null;
  return (ISSUES.includes(value) ? value : "check_failed") as Issue;
}

function savedProfile(): string | null {
  if (typeof window === "undefined") return null;
  try { return window.localStorage.getItem(OPENCLI_PROFILE_PREFERENCE_KEY); }
  catch { return null; }
}

function savedCodexAcknowledgment(): boolean {
  if (typeof window === "undefined") return false;
  try { return window.localStorage.getItem(CODEX_USE_ACKNOWLEDGMENT_KEY) === "confirmed"; }
  catch { return false; }
}

type CodexConsentProps = {
  credentialId?: string | null;
  plugin?: LocalServiceStatus["plugin"];
  pluginCheckPending?: boolean;
  pluginCheckFailed?: boolean;
  onCheckPlugin?: () => void;
};

export function CodexConsentStep({ plugin, pluginCheckPending = false, pluginCheckFailed = false, onCheckPlugin }: CodexConsentProps) {
  const { t } = useTranslation("shell");
  const [checked, setChecked] = useState(false);
  const [acknowledged, setAcknowledged] = useState(savedCodexAcknowledgment);
  const pluginUpdateRequired = plugin?.diagnostic_code === "codex_plugin_update_required";

  function confirm() {
    if (!checked) return;
    try { window.localStorage.setItem(CODEX_USE_ACKNOWLEDGMENT_KEY, "confirmed"); }
    catch { /* Keep the acknowledgment for this page when storage is unavailable. */ }
    setAcknowledged(true);
    onCheckPlugin?.();
  }

  return <li className="grid gap-3 py-6 sm:grid-cols-[180px_minmax(0,1fr)] sm:gap-8">
    <h3 className="text-sm font-semibold">{t("opencli.codexConsentTitle")}</h3>
    <div className="min-w-0 space-y-3">
    <p className="text-sm text-muted-foreground">{t("opencli.codexConsentDescription")}</p>
    {acknowledged ? <p className={`text-sm ${SUCCESS_TEXT}`} role="status"><CheckCircle2 className="mr-1 inline size-4" aria-hidden />{t("opencli.codexConsentRecorded")}</p>
      : <>
        <div className="flex items-start gap-3 rounded-md border p-3 text-sm">
          <Checkbox id="codex-use-acknowledgment" checked={checked} onCheckedChange={value => setChecked(value === true)} className="mt-0.5" />
          <label htmlFor="codex-use-acknowledgment" className="cursor-pointer">{t("opencli.codexConsentCheckbox")}</label>
        </div>
        <Button size="sm" disabled={!checked} onClick={confirm}>{t("opencli.codexConsentConfirm")}</Button>
      </>}
    <p className={`text-sm ${!pluginCheckPending && !pluginCheckFailed && !pluginUpdateRequired && plugin?.state === "installed" && plugin.enabled === true && plugin.verification === "fresh" ? SUCCESS_TEXT : ""}`} role="status">{t("opencli.codexPluginCheckStatus")}: {pluginCheckPending ? t("opencli.codexPluginChecking") : pluginUpdateRequired ? t("opencli.codexPluginUpdateRequired") : t(`opencli.codexPluginState.${
      plugin?.state === "installed" && plugin.enabled === true ? (plugin.verification === "fresh" ? "enabledFresh" : "enabledCached")
        : plugin?.state === "installed" && plugin.enabled === false ? "disabled"
          : plugin?.state === "not_installed" ? "notInstalled" : "unknown"
    }`)}</p>
    <p className="text-xs text-muted-foreground">{t(pluginUpdateRequired ? "opencli.codexPluginUpgradeHelp" : "opencli.codexPluginOpenGuide")}</p>
    {onCheckPlugin && <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={pluginCheckPending} onClick={onCheckPlugin}>
      {pluginCheckPending && <Loader2 className="size-4 animate-spin" aria-hidden />}{t("opencli.codexPluginRecheck")}
    </Button>}
    {pluginCheckFailed && <p className="text-xs" role="status">{t("opencli.codexPluginCheckFailed")}</p>}
    <p className="text-xs text-muted-foreground">{t("opencli.codexConsentBoundary")}</p>

    </div>
  </li>;
}

export function OpenCliPlatformCapabilities({ capabilities, profileRef = null, credentialId = null }: {
  capabilities: OpenCliSetupReport["search_capabilities"];
  profileRef?: string | null;
  credentialId?: string | null;
}) {
  const { t, i18n } = useTranslation("shell");
  const scope = JSON.stringify([credentialId, profileRef]);
  const [, updateFreshness] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => updateFreshness(value => value + 1), 30_000);
    return () => window.clearInterval(timer);
  }, []);
  const [observationState, setObservationState] = useState<{ scope: string; items: Record<string, OpenCliAuthObservation> }>(
    () => ({ scope, items: displayOpenCliAuthObservations(credentialId ?? null, profileRef ?? null, {}) }),
  );
  const observations = observationState.scope === scope ? observationState.items : {};
  const sharedObservations = useQuery({ queryKey: ["opencli-auth-observations", credentialId, profileRef],
    queryFn: () => readOpenCliAuthObservations(profileRef!), enabled: !!profileRef,
    refetchInterval: 5000, retry: false });
  useEffect(() => {
    if (!sharedObservations.data) return;
    setObservationState({ scope, items: displayOpenCliAuthObservations(credentialId ?? null, profileRef ?? null, sharedObservations.data) });
    for (const observation of Object.values(sharedObservations.data)) saveOpenCliAuthObservation(credentialId, observation);
  }, [sharedObservations.data, credentialId, scope]);
  const [checking, setChecking] = useState<string | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [openResult, setOpenResult] = useState<{ scope: string; ok: boolean } | null>(null);
  async function openLogin(platform: string) {
    if (!profileRef || opening) return;
    setOpening(platform);
    setOpenResult(null);
    try {
      await openOpenCliLoginPage(profileRef, platform);
      setOpenResult({ scope, ok: true });
    } catch {
      setOpenResult({ scope, ok: false });
    } finally { setOpening(null); }
  }
  const [checkFailed, setCheckFailed] = useState<{ scope: string; platforms: string[] }>({ scope, platforms: [] });
  const [batchProgress, setBatchProgress] = useState<{ completed: number; total: number } | null>(null);
  const batchController = useRef<AbortController | null>(null);
  useEffect(() => {
    setCheckFailed({ scope, platforms: [] });
    setBatchProgress(null);
    return () => { batchController.current?.abort(); };
  }, [credentialId, profileRef, scope]);

  async function checkSites(platforms: string[], batch: boolean) {
    if (!profileRef || checking) return;
    setChecking(batch ? "batch" : platforms[0]);
    const controller = new AbortController();
    batchController.current = controller;
    setCheckFailed(previous => ({ scope, platforms: previous.platforms.filter(item => !platforms.includes(item)) }));
    setBatchProgress(batch ? { completed: 0, total: platforms.length } : null);
    try {
      await checkOpenCliSitesAuth(profileRef, platforms, controller.signal, (platform, observation, completed) => {
        if (observation) {
          setObservationState(previous => previous.scope === scope ? { scope, items: { ...previous.items, [platform]: observation } } : previous);
          saveOpenCliAuthObservation(credentialId, observation);
        } else setCheckFailed(previous => ({ scope, platforms: [...previous.platforms, platform] }));
        if (batch) setBatchProgress({ completed, total: platforms.length });
      });
    } finally { batchController.current = null; setChecking(null); }
  }

  return <section className="space-y-5" aria-label={t("opencli.platformsTitle")}>
    <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
      <div className="space-y-2">
        <h3 className="text-xl font-semibold">{t("opencli.platformsTitle")}</h3>
        <p className="max-w-prose text-sm text-muted-foreground">{t("opencli.platformsHelp")}</p>
      </div>
      <Button className="min-h-10 self-start" disabled={!profileRef || checking !== null || !capabilities.some(item => openCliSearchNeedsLoginCheck(item.platform, item.browser_required) && item.registered)} onClick={() => { void checkSites(capabilities.filter(item => openCliSearchNeedsLoginCheck(item.platform, item.browser_required) && item.registered).map(item => item.platform), true); }}>
        {checking === "batch" ? <Loader2 className="size-4 animate-spin" aria-hidden /> : <RefreshCw className="size-4" aria-hidden />}{t("opencli.authCheckAll")}
      </Button>
    </div>
    {sharedObservations.isError && <p role="alert" className="text-sm text-destructive">{t("opencli.authStatusReadFailed")}</p>}
    {checking === "batch" && <Button size="sm" variant="outline" className={ACTION_CLASS} onClick={() => batchController.current?.abort()}>{t("opencli.authBatchStop")}</Button>}
    {batchProgress && <p className="text-sm" role="status">{t("opencli.authBatchProgress", batchProgress)}</p>}
    {openResult?.scope === scope && <p role="status" className="text-sm">{t(openResult.ok ? "opencli.loginPageOpened" : "opencli.loginPageFailed")}</p>}
    {capabilities.length === 0 ? <p className="text-sm">{t("opencli.platformsUnavailable")}</p>
      : <ul className="divide-y divide-border border-y border-border">
        {orderOpenCliPlatforms(capabilities).map(item => {
          const observation = observations[item.platform];
          const access = openCliSearchAccess(item.platform, item.browser_required);
          const needsLoginCheck = openCliSearchNeedsLoginCheck(item.platform, item.browser_required);
          const failed = sharedObservations.isError || (checkFailed.scope === scope && checkFailed.platforms.includes(item.platform));
          const green = openCliPlatformIsGreen(item, profileRef, observation, failed);
          return <li key={item.platform} className="grid grid-cols-[90px_minmax(0,1fr)] items-start gap-x-4 gap-y-3 py-5 lg:grid-cols-[130px_minmax(0,1fr)_auto] lg:items-center lg:gap-x-6">
            <div className="break-words text-sm font-semibold">{platformDisplayName(item.platform)}</div>
            <div className="min-w-0 space-y-1">
              <p className={`text-sm ${green ? SUCCESS_TEXT : "text-muted-foreground"}`}>
                {green && <CheckCircle2 className="mr-1 inline size-4" aria-hidden />}
                {!item.registered ? t("opencli.commandMissing") : needsLoginCheck ? (failed ? t("opencli.authCheckFailed")
                  : observation?.status === "logged_in" && profileRef && !isFreshOpenCliAuthObservation(observation, profileRef) ? t("opencli.authExpired")
                  : observation ? t(`opencli.authState.${observation.status}`) : t("opencli.authNotChecked"))
                  : t(access === "public_browser" ? "opencli.authPublicBrowser" : "opencli.authPublicRead")}
              </p>
              {item.registered && (access === "extra_entitlement" || access === "conditional") && <p className="text-xs text-muted-foreground">{t(`opencli.searchAccess.${access}`)}</p>}
              {needsLoginCheck && observation && <p className="text-xs text-muted-foreground">{t("opencli.authCheckedAt", { date: formatDate(observation.checked_at, i18n.language) })}</p>}
              {observation?.issue && <p className="text-xs text-muted-foreground">{t(`opencli.authIssue.${observation.issue}`)}</p>}
            </div>
            <div className="col-start-2 flex flex-wrap items-center gap-2 lg:col-start-3 lg:justify-end">
              {platformWebsite(item.platform) && (item.browser_required
                ? <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={!profileRef || opening !== null} onClick={() => { void openLogin(item.platform); }} aria-label={t("opencli.platformOpenFor", { platform: platformDisplayName(item.platform) })}>{t(openCliPlatformNeedsSignIn(item, profileRef, observation, failed) ? "opencli.platformLogin" : "opencli.platformOpen")}{opening === item.platform ? <Loader2 className="size-4 animate-spin" aria-hidden /> : <ExternalLink className="size-4" aria-hidden />}</Button>
                : <Button asChild size="sm" variant="outline" className={ACTION_CLASS}><a href={platformWebsite(item.platform)} target="_blank" rel="noreferrer" aria-label={t("opencli.platformOpenFor", { platform: platformDisplayName(item.platform) })}>{t("opencli.platformOpen")}<ExternalLink className="size-4" aria-hidden /></a></Button>)}
              {needsLoginCheck && <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={!item.registered || !profileRef || checking !== null} onClick={() => { void checkSites([item.platform], false); }} aria-label={t("opencli.authCheckFor", { platform: platformDisplayName(item.platform) })}>
                {checking === item.platform && <Loader2 className="size-4 animate-spin" aria-hidden />}{t("opencli.authCheckAction")}
              </Button>}
            </div>
          </li>;
        })}
      </ul>}
    <details className="text-sm text-muted-foreground">
      <summary className="cursor-pointer font-medium">{t("opencli.loginDetails")}</summary>
      <div className="max-w-prose space-y-3 pt-3"><p>{t("opencli.platformsLoginHelp")}</p><p>{t("opencli.platformLoginBrowserHelp")}</p></div>
    </details>
  </section>;

}

export function OpenCliAddonSetup({ report, pending, failed, onInstall }: {
  report: Pick<OpenCliSetupReport, "version" | "addons">;
  pending: boolean;
  failed: boolean;
  onInstall: () => void;
}) {
  const { t } = useTranslation("shell");
  const allReady = report.addons.length > 0 && report.addons.every(addon => addon.state === "ready");
  const blockedAddon = report.addons.find(addon => addon.state === "conflict" || addon.state === "unavailable");
  return <section className="space-y-3" aria-label={t("opencli.addonsTitle")}>
    <details className="text-sm" open={!allReady}>
      <summary className="cursor-pointer font-medium">{t("opencli.addonsTitle")}</summary>
      <div className="space-y-3 pt-3">
        <p className="max-w-prose text-sm text-muted-foreground">{t("opencli.addonsHelp")}</p>
        <p className="max-w-prose text-xs text-muted-foreground">{t("opencli.addonsScope")}</p>
        {report.addons.length === 0 ? <p>{t("opencli.addonsUnavailable")}</p>
          : <Collapsible><CollapsibleTrigger asChild><Button size="sm" variant="ghost">{t("opencli.addonsDetails")}<ChevronDown className="size-4" aria-hidden /></Button></CollapsibleTrigger>
            <CollapsibleContent className="pt-3"><ul className="space-y-2" aria-live="polite">{report.addons.map(addon => <li key={addon.id} className={`break-words text-sm ${addon.state === "ready" ? SUCCESS_TEXT : ""}`}>
              {addon.id} · {t(`opencli.addonState.${addon.state}`)}
            </li>)}</ul></CollapsibleContent></Collapsible>}
      </div>
    </details>
    {failed && <Alert variant="destructive"><AlertDescription>{t("opencli.addonsFailed")}</AlertDescription></Alert>}
    {!allReady && !canInstallOpenCliAddons(report) && <p className="text-sm" role="status">{blockedAddon ? t(`opencli.addonState.${blockedAddon.state}`) : t("opencli.addonsUnavailable")}</p>}
    {allReady ? <p className={`flex items-center gap-2 text-sm ${SUCCESS_TEXT}`} role="status"><CheckCircle2 className="size-4" aria-hidden />{t("opencli.addonsInstalled")}</p> : <Button size="sm" disabled={pending || !canInstallOpenCliAddons(report)} onClick={onInstall}>
      {pending && <Loader2 className="size-4 animate-spin" aria-hidden />}{t("opencli.installAddons")}
    </Button>}
  </section>;
}

export function OpenCliSearch({ onConnections, credentialId = null }: { onConnections?: () => void; credentialId?: string | null }) {
  const { t } = useTranslation("shell");
  const setup = useOpenCliSetup();
  const recovery = useOpenCliRecovery(setup.data);
  const [profile] = useState(savedProfile);
  const report = setup.data;
  const ready = report && !setup.isError && !recovery.isPending && selectedProfileIsConnected(report, profile);
  if (setup.isPending || recovery.isPending || (!ready && setup.isFetching)) return <p role="status">{t("service.checking")}</p>;
  if (!ready || !profile || !report.version) return <Card><CardContent className="space-y-3 pt-5">
    <p>{t("opencli.searchNeedsConnections")}</p>
    <Button onClick={onConnections}>{t("opencli.backToConnections")}</Button>
  </CardContent></Card>;
  return <DiscoveryPilot credentialId={credentialId} profileRef={profile} version={report.version} capabilities={report.search_capabilities} />;
}

export function OpenCliWizard(props: CodexConsentProps & { onSearch?: () => void }) {
  const { t } = useTranslation("shell");
  const setup = useOpenCliSetup();
  const recovery = useOpenCliRecovery(setup.data);
  const addons = useInstallOpenCliAddons();
  const repair = useRepairOpenCliInstallation();
  const openPage = useMutation({ mutationFn: openOpenCliSetupPage, retry: false });
  const [selected, setSelected] = useState(savedProfile);
  const [candidate, setCandidate] = useState(savedProfile);
  const check = useMutation({
    mutationFn: checkOpenCliProfile,
    onSuccess: (_, profile) => {
      setSelected(profile);
      try { window.localStorage.setItem(OPENCLI_PROFILE_PREFERENCE_KEY, profile); }
      catch { /* The current-session selection still works without storage. */ }
    },
  });
  const report = setup.data;
  const issue = issueKey(report?.issue ?? null);
  const versionCompatible = report && openCliVersionIsCompatible(report);
  const versionBlocked = !!report?.installed && !versionCompatible;
  const ready = report && !setup.isError && !recovery.isPending && selectedProfileIsConnected(report, selected);
  const selectable = report?.profiles ?? [];
  const choice = candidate && selectable.some(profile => profile.context_id === candidate)
    ? candidate : selectable.length === 1 ? selectable[0].context_id : "";

  return <Card className="mx-auto w-full max-w-5xl rounded-xl shadow-none">
    <CardHeader className="px-6 pb-2 pt-6 sm:px-8"><CardTitle className="text-2xl font-semibold tracking-tight">{t("opencli.title")}</CardTitle></CardHeader>
    <CardContent className="space-y-8 px-6 pb-6 sm:px-8 sm:pb-8">
      <p className="text-sm text-muted-foreground">{t("opencli.intro")}</p>
      <ol className="divide-y divide-border border-y border-border">
        <li className="grid gap-3 py-6 sm:grid-cols-[180px_minmax(0,1fr)] sm:gap-8">
    <h3 className="text-sm font-semibold">{t("opencli.installTitle")}</h3>
    <div className="min-w-0 space-y-3">
          <p className="text-sm text-muted-foreground">{t("opencli.installDescription")}</p>
          <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={repair.isPending} onClick={() => repair.mutate()}>
            {repair.isPending && <Loader2 className="size-4 animate-spin" aria-hidden />}
            {t(repair.isPending ? "opencli.repairInstalling" : "opencli.repairAction")}
          </Button>
          {repair.isPending && <p className="text-sm text-muted-foreground" role="status">{t("opencli.repairProgress")}</p>}
          {repair.isError && <Alert variant="destructive"><AlertDescription>{t("opencli.repairFailed")}</AlertDescription></Alert>}
          {repair.isSuccess && <p className="text-sm" role="status">{t("opencli.repairInstalled")}</p>}
          <details className="text-sm text-muted-foreground">
            <summary className="cursor-pointer">{t("opencli.manualInstall")}</summary>
            <p className="my-2">{t("opencli.manualInstallHelp")}</p>
            <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={openPage.isPending} onClick={() => openPage.mutate("download")}>{t(versionBlocked ? "opencli.upgradeAction" : "opencli.openAppDownload")}<ExternalLink aria-hidden /></Button>
          </details>
          {!setup.isError && report?.installed && <p className={`text-sm ${versionCompatible ? SUCCESS_TEXT : ""}`} role="status">{versionCompatible && <CheckCircle2 className="mr-1 inline size-4" aria-hidden />}{t("opencli.installedVersion", { version: report.version ?? "—" })}</p>}
          {!setup.isError && versionBlocked && <Alert><AlertDescription>
            <p>{t("opencli.upgradeRequired", { version: report?.version ?? "—", required: report!.required_version })}</p>
            <p>{t("opencli.upgradeSteps")}</p>
            <p>{t("opencli.upgradeKeepsData")}</p>
          </AlertDescription></Alert>}
          {versionBlocked && report?.command_path && <details className="text-xs text-muted-foreground">
            <summary className="cursor-pointer">{t("opencli.installationDetails")}</summary>
            <p className="mt-2">{t("opencli.commandPath")}</p><code className="break-all">{report.command_path}</code>
            <p className="mt-2">{t("opencli.oldCommandHelp")}</p>
          </details>}

    </div>
  </li>
        <li className="grid gap-3 py-6 sm:grid-cols-[180px_minmax(0,1fr)] sm:gap-8">
    <h3 className="text-sm font-semibold">{t("opencli.extensionTitle")}</h3>
    <div className="min-w-0 space-y-3">
          <p className="text-sm text-muted-foreground">{t("opencli.extensionDescription")}</p>
          <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={openPage.isPending} onClick={() => openPage.mutate("extension")}>{t("opencli.openExtensionStore")}<ExternalLink aria-hidden /></Button>
          {!setup.isError && report?.extension_connected && <p className={`text-sm ${SUCCESS_TEXT}`} role="status"><CheckCircle2 className="mr-1 inline size-4" aria-hidden />{t("opencli.extensionConnected")}</p>}

    </div>
  </li>
        <li className="grid gap-3 py-6 sm:grid-cols-[180px_minmax(0,1fr)] sm:gap-8">
    <h3 className="text-sm font-semibold">{t("opencli.profileTitle")}</h3>
    <div className="min-w-0 space-y-3">
          <p className="text-sm text-muted-foreground">{t("opencli.profileDescription")}</p>
          {selectable.length === 0 && <p className="text-sm" role="status">{setup.isPending ? t("service.checking") : setup.isError ? t("opencli.statusFailed") : versionBlocked ? t("opencli.upgradeBeforeProfile") : issue ? t(`opencli.issue.${issue}`) : t("opencli.profilesEmpty")}</p>}
          {recovery.isPending && <p className="text-sm" role="status"><Loader2 className="mr-1 inline size-4 animate-spin" aria-hidden />{t("opencli.recovering")}</p>}
          {recovery.isError && <p className="text-sm" role="alert">{t("opencli.recoveryFailed")}</p>}
          {issue === "daemon_unavailable" && <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={recovery.isPending} onClick={() => recovery.mutate()}>{t("opencli.recover")}</Button>}
          {selectable.length > 0 && <RadioGroup value={choice} onValueChange={setCandidate} aria-label={t("opencli.profileTitle")}>
            {selectable.map(profile => <label key={profile.context_id} className="flex cursor-pointer items-center gap-3 rounded-md py-2 text-sm">
              <RadioGroupItem value={profile.context_id} />
              <span>{profile.alias ?? profile.context_id}</span>
            </label>)}
          </RadioGroup>}
          {ready && <p className={`text-sm ${SUCCESS_TEXT}`} role="status"><CheckCircle2 className="mr-1 inline size-4" aria-hidden />{t("opencli.profileSelected")}</p>}
          {check.isError && <Alert variant="destructive"><AlertDescription>{t("opencli.profileCheckFailed")}</AlertDescription></Alert>}
          <Button size="sm" variant="outline" className={ACTION_CLASS} disabled={!choice || !!issue || setup.isError || repair.isPending || recovery.isPending || check.isPending || (ready && choice === selected)} onClick={() => check.mutate(choice)}>
            {check.isPending && <Loader2 className="size-4 animate-spin" aria-hidden />}{t("opencli.chooseProfile")}
          </Button>

    </div>
  </li>
        <CodexConsentStep {...props} />
      </ol>
      {openPage.isSuccess && <p className="text-sm" role="status">{t("opencli.chromeLaunchRequested")}</p>}
      {openPage.isError && <Alert variant="destructive"><AlertDescription>{t("opencli.chromeLaunchFailed")}</AlertDescription></Alert>}
      {report && <OpenCliAddonSetup report={report} pending={addons.isPending} failed={addons.isError} onInstall={() => addons.mutate()} />}
      {report && <OpenCliPlatformCapabilities capabilities={report.search_capabilities} profileRef={ready ? selected : null} credentialId={props.credentialId ?? null} />}
      {setup.isError && <Alert variant="destructive"><AlertDescription>{t("opencli.statusFailed")}</AlertDescription></Alert>}
      {issue && !versionBlocked && <Alert><AlertDescription>{t(`opencli.issue.${issue}`)}</AlertDescription></Alert>}
      <Button variant="outline" disabled={setup.isFetching} onClick={() => { void setup.refetch(); }}>
        {setup.isFetching ? <Loader2 className="size-4 animate-spin" aria-hidden /> : <RefreshCw aria-hidden />}{t("opencli.recheck")}
      </Button>
      <div className="space-y-2 border-t pt-4">
        <p className="text-sm text-muted-foreground">{t("opencli.beforeSearchHelp")}</p>
        <DefaultSyncAgreement key={props.credentialId ?? 'signed-out'} credentialId={props.credentialId ?? null}
          disabled={!ready || !report?.version || repair.isPending || check.isPending || setup.isFetching} onContinue={props.onSearch} />
      </div>
    </CardContent>
  </Card>;
}
