import { useEffect, useRef, useState } from "react";
import { invoke } from "@/lib/local-runtime";
import {
  BadgeCheck,
  Check,
  DownloadCloud,
  ExternalLink,
  Fingerprint,
  Loader2,
  ShieldCheck,
  Shuffle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { PLATFORM_LABEL } from "@/lib/creator";
import type { Platform } from "@/types";
import {
  beginOperatorConnection,
  cleanupPendingOperatorConnection,
  finalizeOperatorConnection,
  prepareOperatorConnection,
  type OperatorBrowserStatus,
} from "@/lib/operator-account";

import { Stepper } from "./shared";
import { BrowserCookieImportAlert } from "./browser-cookie-import-alert";
import { browserOperationErrorMessage } from "./browser-environment";
import { ProxyFields, toProxyConfig, type ProxyProbe } from "./proxy";
import {
  canContinueFromProfileStep,
  type XChatConnectionChoice,
  xChatConnectionPayload,
} from "./x-chat-passcode-state";

const randomProfileName = (p: Platform) =>
  `${p === "instagram" ? "ig" : "x"}-${Math.random().toString(36).slice(2, 8)}`;

/** Create-profile wizard for X / Instagram, wired to the canonical account
 * connection workflow plus in-app secret configuration. */
export function CreateProfileWizard({
  platform,
  onDone,
  browserStatus,
}: {
  platform: Platform;
  onDone: () => void;
  browserStatus: OperatorBrowserStatus | null;
}) {
  const { t } = useTranslation("accounts");
  const label = PLATFORM_LABEL[platform];
  const wizardSteps = [
    t("wizard.steps.naming"),
    t("wizard.steps.proxy"),
    t("wizard.steps.login"),
    t("wizard.steps.import"),
  ] as const;
  const [step, setStep] = useState(0);
  const [name, setName] = useState("");
  const [xChatPreference, setXChatPreference] = useState<XChatConnectionChoice>(
    platform === "x" ? null : "disabled",
  );
  const [passcode, setPasscode] = useState("");
  const [type, setType] = useState("HTTP");
  const [ip, setIp] = useState("");
  const [port, setPort] = useState("");
  const [user, setUser] = useState("");
  const [pass, setPass] = useState("");
  const [detecting, setDetecting] = useState(false);
  const [detected, setDetected] = useState(false);
  const [geo, setGeo] = useState<ProxyProbe | null>(null);
  const [flowId, setFlowId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [browserOpened, setBrowserOpened] = useState(false);
  const [completionNotice, setCompletionNotice] = useState<string | null>(null);
  const [extracting, setExtracting] = useState(false);
  const [extracted, setExtracted] = useState<string | null>(null);
  const flowRef = useRef<string | null>(null);
  const completedRef = useRef(false);

  useEffect(() => {
    flowRef.current = flowId;
  }, [flowId]);

  useEffect(
    () => () => {
      void cleanupPendingOperatorConnection(flowRef.current, () => completedRef.current);
    },
    [],
  );

  function invalidateFromProxy() {
    setDetected(false);
    setGeo(null);
    setBrowserOpened(false);
    setExtracted(null);
  }

  function onPaste(value: string) {
    const parts = value.split(":");
    if (parts.length >= 2) {
      setIp(parts[0] ?? "");
      setPort(parts[1] ?? "");
      setUser(parts[2] ?? "");
      setPass(parts[3] ?? "");
      invalidateFromProxy();
    }
  }

  async function detect() {
    setDetecting(true);
    try {
      const probe = await invoke<ProxyProbe>("probe_proxy", {
        proxy: toProxyConfig(type, ip, port, user, pass),
      });
      setDetected(true);
      setGeo(probe);
      toast.success(t("proxy.available"), {
        description: t("proxy.toastDescription", {
          ip: probe.ip,
          country: probe.country ? ` · ${probe.country}` : "",
          endpoint: probe.endpoint,
        }),
      });
    } catch (error) {
      setDetected(false);
      toast.error(t("proxy.detectionFailed"), { description: String(error) });
    } finally {
      setDetecting(false);
    }
  }

  async function openBrowser() {
    if (!flowId) return;
    setBusy(true);
    try {
      const status = await beginOperatorConnection(flowId);
      if (status.phase !== "awaiting_user_in_app" && status.phase !== "completed") {
        throw new Error(status.error_code ?? "browser_connection_failed");
      }
      setBrowserOpened(true);
      setCompletionNotice(status.browser_capability?.completion_notice?.message ?? null);
      toast(t("wizard.toasts.browserOpened"), {
        description: t("wizard.toasts.browserOpenedDescription", { platform: label }),
      });
    } catch (error) {
      toast.error(t("wizard.toasts.browserOpenFailed"), {
        description: browserOperationErrorMessage(
          error,
          t,
          t("wizard.toasts.browserOpenFailedDescription"),
        ),
      });
    } finally {
      setBusy(false);
    }
  }

  async function extract() {
    if (!flowId) return;
    setExtracting(true);
    try {
      const status = await finalizeOperatorConnection(flowId);
      if (status.phase !== "completed" || !status.operator_account) {
        throw new Error(status.error_code ?? "browser_connection_failed");
      }
      completedRef.current = true;
      setExtracted(status.operator_account.username);
      toast.success(t("wizard.toasts.connected", { username: status.operator_account.username }), {
        description: t("wizard.toasts.connectedDescription"),
      });
    } catch (error) {
      toast.error(t("wizard.toasts.extractFailed"), {
        description: browserOperationErrorMessage(
          error,
          t,
          t("wizard.toasts.extractFailedDescription"),
        ),
      });
    } finally {
      setExtracting(false);
    }
  }

  const canNext = [
    canContinueFromProfileStep({
      platform,
      profileName: name,
      preference: xChatPreference,
      passcode,
    }),
    ip.trim().length > 0 && port.trim().length > 0,
    browserOpened,
    extracted !== null,
  ][step];
  const nextLabel = [
    t("wizard.actions.next"),
    t("wizard.actions.next"),
    t("wizard.actions.loginCompleted"),
    t("wizard.actions.finishAndSave"),
  ][step];

  async function next() {
    if (step === 1) {
      if (!detected) {
        toast.error(t("wizard.toasts.detectFirst"), {
          description: t("wizard.toasts.detectFirstDescription"),
        });
        return;
      }
      if (!flowId) {
        setBusy(true);
        try {
          const xChat = xChatConnectionPayload({ platform, preference: xChatPreference, passcode }, t);
          const prepared = await prepareOperatorConnection({
            platform,
            intent: "connect",
            method: "browser",
            label: name.trim(),
          });
          flowRef.current = prepared.flow_id;
          setFlowId(prepared.flow_id);
          await invoke("onecrew_operator_account_connection_configure_browser_in_app", {
            flowId: prepared.flow_id,
            profileName: name.trim(),
            proxy: toProxyConfig(type, ip, port, user, pass),
            ...xChat,
          });
        } catch (error) {
          toast.error(t("wizard.toasts.prepareFailed"), { description: String(error) });
          setBusy(false);
          return;
        }
        setBusy(false);
      } else {
        try {
          const xChat = xChatConnectionPayload({ platform, preference: xChatPreference, passcode }, t);
          await invoke("onecrew_operator_account_connection_configure_browser_in_app", {
            flowId,
            profileName: name.trim(),
            proxy: toProxyConfig(type, ip, port, user, pass),
            ...xChat,
          });
        } catch (error) {
          toast.error(t("wizard.toasts.configureFailed"), { description: String(error) });
          return;
        }
      }
    }
    if (step < wizardSteps.length - 1) setStep((s) => s + 1);
    else onDone();
  }

  return (
    <div className="space-y-5">
      <Stepper step={step} steps={wizardSteps} />
      <div className="min-h-[172px] space-y-4">
        {step === 0 && (
          <>
            <div className="space-y-2">
              <Label htmlFor="pf-name">{t("wizard.profileName")}</Label>
              <div className="flex gap-2">
                <Input
                  id="pf-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder={t("wizard.profilePlaceholder")}
                />
                <Button type="button" variant="outline" onClick={() => setName(randomProfileName(platform))}>
                  <Shuffle className="size-4" /> {t("wizard.actions.random")}
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                {t("wizard.profileDescription")}
              </p>
            </div>
            {platform === "x" && (
              <div className="space-y-3">
                <Label>{t("wizard.xChatQuestion")}</Label>
                <RadioGroup
                  value={xChatPreference ?? undefined}
                  onValueChange={(value) => {
                    const preference = value as Exclude<XChatConnectionChoice, null>;
                    setXChatPreference(preference);
                    if (preference === "disabled") setPasscode("");
                  }}
                  className="gap-2"
                >
                  <Label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 font-normal">
                    <RadioGroupItem value="enabled" className="mt-0.5" />
                    <span>
                      <span className="block font-medium">{t("wizard.xChatEnabled")}</span>
                      <span className="mt-0.5 block text-xs text-muted-foreground">
                        {t("wizard.xChatEnabledDescription")}
                      </span>
                    </span>
                  </Label>
                  <Label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 font-normal">
                    <RadioGroupItem value="disabled" className="mt-0.5" />
                    <span>
                      <span className="block font-medium">{t("wizard.xChatDisabled")}</span>
                      <span className="mt-0.5 block text-xs text-muted-foreground">
                        {t("wizard.xChatDisabledDescription")}
                      </span>
                    </span>
                  </Label>
                </RadioGroup>
                {xChatPreference === "enabled" && (
                  <div className="space-y-2">
                    <Label htmlFor="passcode">{t("wizard.passcodeLabel")}</Label>
                    <Input
                      id="passcode"
                      type="password"
                      autoComplete="new-password"
                      value={passcode}
                      onChange={(e) => setPasscode(e.target.value)}
                      placeholder={t("wizard.passcodePlaceholder")}
                    />
                    <p className="text-xs text-muted-foreground">
                      {t("wizard.passcodeHint")}
                    </p>
                  </div>
                )}
              </div>
            )}
          </>
        )}

        {step === 1 && (
          <>
            <ProxyFields
              type={type}
              setType={setType}
              ip={ip}
              setIp={setIp}
              port={port}
              setPort={setPort}
              user={user}
              setUser={setUser}
              pass={pass}
              setPass={setPass}
              onPaste={onPaste}
              onAnyChange={invalidateFromProxy}
            />
            <div className="flex items-center gap-3">
              <Button type="button" variant="outline" disabled={!ip || !port || detecting} onClick={detect}>
                {detecting ? <Loader2 className="size-4 animate-spin" /> : <ShieldCheck className="size-4" />}
                {t("proxy.detect")}
              </Button>
              {detected && geo && (
                <span className="flex items-center gap-1.5 text-sm text-[var(--tone-success-fg)]">
                  <Check className="size-4" /> {t("proxy.egress", { ip: geo.ip })}
                  {geo.country ? ` · ${geo.country}` : ""}
                </span>
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              {t("proxy.forceNotice")}
            </p>
          </>
        )}

        {step === 2 && (
          <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
              {t("wizard.loginIntro", { platform: label })}
            </p>
            {/* Fingerprint confirm — shown BEFORE opening Chrome. Timezone comes
                from the egress IP's geolocation, language from CLDR; both are
                launched into Chrome. Hardware fingerprint stays the real machine. */}
            <div className="rounded-lg border p-3">
              <div className="mb-2.5 flex items-center gap-1.5 text-sm font-medium">
                <Fingerprint className="size-4 text-muted-foreground" /> {t("wizard.fingerprintTitle")}
              </div>
              <dl className="grid grid-cols-[auto_1fr] items-center gap-x-4 gap-y-2 text-sm">
                <dt className="text-muted-foreground">{t("wizard.fingerprint.browser")}</dt>
                <dd className="text-right font-medium">
                  {t("wizard.fingerprint.browserValue")}
                </dd>
                <dt className="text-muted-foreground">{t("wizard.fingerprint.egress")}</dt>
                <dd className="text-right font-medium tabular-nums">
                  {geo?.ip ?? "—"}
                  {geo?.country ? ` · ${geo.country}` : ""}
                </dd>
                <dt className="text-muted-foreground">{t("wizard.fingerprint.timezone")}</dt>
                <dd className="text-right font-medium">{geo?.timezone ?? t("wizard.fingerprint.followSystem")}</dd>
                <dt className="text-muted-foreground">{t("wizard.fingerprint.language")}</dt>
                <dd className="text-right font-medium">{geo?.accept_language?.split(",")[0] ?? t("wizard.fingerprint.followSystem")}</dd>
                <dt className="text-muted-foreground">{t("wizard.fingerprint.webrtc")}</dt>
                <dd className="text-right font-medium">{t("wizard.fingerprint.proxyOnly")}</dd>
              </dl>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("wizard.fingerprintNote")}
            </p>
            <Button type="button" variant={browserOpened ? "outline" : "default"} onClick={openBrowser} disabled={busy}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : <ExternalLink className="size-4" />}
              {browserOpened ? t("wizard.actions.reopenChrome") : t("wizard.actions.confirmAndOpenChrome")}
            </Button>
            {browserOpened && (
              <div className="rounded-lg border bg-muted/40 p-3 text-xs text-muted-foreground">
                {completionNotice ?? t("wizard.browserOpenedFallback")}
              </div>
            )}
          </div>
        )}

        {step === 3 && (
          <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
              {t("wizard.importDescription")}
            </p>
            <BrowserCookieImportAlert status={browserStatus} />
            <Button type="button" variant="outline" disabled={extracting || extracted !== null} onClick={extract}>
              {extracting ? <Loader2 className="size-4 animate-spin" /> : <DownloadCloud className="size-4" />}
              {extracted !== null ? t("wizard.actions.extracted") : t("wizard.actions.extractCookie")}
            </Button>
            {extracted !== null && (
              <div className="rounded-lg border p-3">
                <div className="flex items-center gap-1.5 text-sm text-[var(--tone-success-fg)]">
                  <BadgeCheck className="size-4" /> {t("wizard.connected", { username: extracted })}
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("wizard.profileSummary", { profile: name })}
                </p>
              </div>
            )}
          </div>
        )}
      </div>

      <DialogFooter className="sm:justify-between">
        {step > 0 ? (
          <Button type="button" variant="ghost" onClick={() => setStep((s) => s - 1)}>
            {t("common.actions.previous")}
          </Button>
        ) : (
          <span />
        )}
        <Button type="button" disabled={!canNext || busy} onClick={next}>
          {nextLabel}
        </Button>
      </DialogFooter>
    </div>
  );
}
