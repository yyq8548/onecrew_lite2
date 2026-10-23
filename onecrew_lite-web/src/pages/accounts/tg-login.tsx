import { useEffect, useRef, useState } from "react";
import { invoke } from "@/lib/local-runtime";
import {
  Check,
  ExternalLink,
  KeyRound,
  Loader2,
  LogIn,
  QrCode,
  ShieldCheck,
  Smartphone,
} from "lucide-react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { openExternalUrl } from "@/lib/open-external";
import {
  beginOperatorConnection,
  cancelOperatorConnection,
  cleanupPendingOperatorConnection,
  operatorConnectionStatus,
  operatorInAppHandoff,
  prepareOperatorConnection,
  type OperatorConnectionStatus,
} from "@/lib/operator-account";

import { Stepper } from "./shared";
import { ProxyFields, SOCKS5_ONLY, toProxyConfig, type ProxyProbe } from "./proxy";
import { telegramLoginView, type TelegramLoginStage } from "./telegram-flow-state";

type LoginCodeMethod =
  | "telegram_app"
  | "sms"
  | "voice_call"
  | "flash_call"
  | "missed_call"
  | "email"
  | "email_setup_required"
  | "fragment"
  | "firebase_sms"
  | "sms_word"
  | "sms_phrase";

type LoginCodeDelivery = {
  method: LoginCodeMethod;
  code_length: number | null;
  destination_hint: string | null;
  timeout_seconds: number | null;
  next_method: LoginCodeMethod | null;
  action_url: string | null;
  can_submit_code: boolean;
};

type QrLoginChallenge = {
  image_data_url: string;
  expires_at: number;
};

const DELIVERY_LABEL_KEYS = {
  telegram_app: "telegram.delivery.labels.telegram_app",
  sms: "telegram.delivery.labels.sms",
  voice_call: "telegram.delivery.labels.voice_call",
  flash_call: "telegram.delivery.labels.flash_call",
  missed_call: "telegram.delivery.labels.missed_call",
  email: "telegram.delivery.labels.email",
  email_setup_required: "telegram.delivery.labels.email_setup_required",
  fragment: "telegram.delivery.labels.fragment",
  firebase_sms: "telegram.delivery.labels.firebase_sms",
  sms_word: "telegram.delivery.labels.sms_word",
  sms_phrase: "telegram.delivery.labels.sms_phrase",
} as const satisfies Record<LoginCodeMethod, string>;

function deliveryLabel(method: LoginCodeMethod, t: TFunction<"accounts">) {
  return t(DELIVERY_LABEL_KEYS[method]);
}

function deliveryDescription(delivery: LoginCodeDelivery, t: TFunction<"accounts">) {
  switch (delivery.method) {
    case "telegram_app":
      return t("telegram.delivery.descriptions.telegramApp");
    case "sms":
    case "firebase_sms":
    case "sms_word":
    case "sms_phrase":
      return t("telegram.delivery.descriptions.sms");
    case "voice_call":
    case "flash_call":
    case "missed_call":
      return t("telegram.delivery.descriptions.call");
    case "email":
      return t("telegram.delivery.descriptions.email", {
        destination: delivery.destination_hint ? ` (${delivery.destination_hint})` : "",
      });
    case "fragment":
      return t("telegram.delivery.descriptions.fragment");
    case "email_setup_required":
      return t("telegram.delivery.descriptions.emailSetup");
  }
}

/** Telegram's FAQ on the international phone format it expects. */
const INTL_FORMAT_FAQ = "https://telegram.org/faq#login-and-sms";

/** Loose E.164 check: leading `+` and 7–15 digits (ignoring spaces/symbols). */
const isIntlPhone = (phone: string) => {
  const trimmed = phone.trim();
  const digits = trimmed.replace(/\D/g, "");
  return trimmed.startsWith("+") && digits.length >= 7 && digits.length <= 15;
};

/** Telegram connect — QR or phone login (NOT a browser flow), but the 家宽 is
 *  configured first like every platform: the MTProto session logs in AND operates
 *  through the residential proxy (login IP == operating IP). grammers only speaks
 *  SOCKS5, so the 家宽 is locked to SOCKS5. All state transitions use the
 *  canonical account workflow; secrets enter only through desktop commands. */
export function TgLoginForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation("accounts");
  const telegramSteps = [t("telegram.steps.proxy"), t("telegram.steps.login")] as const;
  const [step, setStep] = useState(0);
  // 家宽 — SOCKS5 only (grammers has no HTTP-proxy path).
  const [type, setType] = useState("SOCKS5");
  const [ip, setIp] = useState("");
  const [port, setPort] = useState("");
  const [user, setUser] = useState("");
  const [pass, setPass] = useState("");
  const [detecting, setDetecting] = useState(false);
  const [detected, setDetected] = useState(false);
  const [geo, setGeo] = useState<ProxyProbe | null>(null);
  // 登录 — explicit QR or phone choice. Both use the same API credentials and
  // short-lived Rust-only pending state.
  const [loginMethod, setLoginMethod] = useState<"qr" | "phone">("qr");
  const [apiId, setApiId] = useState("");
  const [apiHash, setApiHash] = useState("");
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [flowId, setFlowId] = useState("");
  const [delivery, setDelivery] = useState<LoginCodeDelivery | null>(null);
  const [qrChallenge, setQrChallenge] = useState<QrLoginChallenge | null>(null);
  const [stage, setStage] = useState<TelegramLoginStage>("creds");
  const [busy, setBusy] = useState(false);
  const flowIdRef = useRef("");
  const completedRef = useRef(false);
  const onDoneRef = useRef(onDone);

  useEffect(() => {
    flowIdRef.current = flowId;
  }, [flowId]);

  useEffect(() => {
    onDoneRef.current = onDone;
  }, [onDone]);

  useEffect(
    () => () => {
      void cleanupPendingOperatorConnection(flowIdRef.current, () => completedRef.current);
    },
    [],
  );

  async function applyStatus(status: OperatorConnectionStatus) {
    if (status.phase === "completed") {
      completedRef.current = true;
      toast.success(t("telegram.toasts.connected"), {
        description: t("telegram.toasts.connectedDescription"),
      });
      onDoneRef.current();
      return;
    }
    if (status.phase === "failed" || status.phase === "canceled") {
      throw new Error(
        status.error_code
        ?? (status.phase === "failed"
          ? t("telegram.toasts.connectionFailed")
          : t("telegram.toasts.connectionCanceled")),
      );
    }
    const handoff = await operatorInAppHandoff(status.flow_id);
    setQrChallenge(handoff.telegram_qr_challenge ?? null);
    setDelivery((handoff.telegram_code_delivery as LoginCodeDelivery | undefined) ?? null);
    const view = telegramLoginView(status);
    setStep(view.step);
    setStage(view.stage);
  }

  useEffect(() => {
    if (stage !== "qr" || !flowId) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const poll = async () => {
      try {
        const result = await operatorConnectionStatus(flowId);
        if (stopped) return;
        await applyStatus(result);
        if (result.phase === "completed" || result.handoff?.kind === "enter_telegram_two_factor_in_desktop") return;
        timer = setTimeout(poll, 150);
      } catch (error) {
        if (stopped) return;
        setQrChallenge(null);
        setStage("creds");
        toast.error(t("telegram.toasts.qrFailed"), { description: String(error) });
      }
    };

    timer = setTimeout(poll, 150);
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flowId, stage]);

  async function openCredentials() {
    try {
      let currentFlowId = flowId;
      if (!currentFlowId) {
        const prepared = await prepareOperatorConnection({
          platform: "telegram",
          intent: "connect",
          method: loginMethod === "phone" ? "telegram_phone" : "telegram_qr",
          label: "Telegram",
        });
        currentFlowId = prepared.flow_id;
        flowIdRef.current = currentFlowId;
        setFlowId(currentFlowId);
      }
      await invoke("onecrew_operator_account_connection_open_telegram_credentials_in_app", {
        flowId: currentFlowId,
        proxy: toProxyConfig(type, ip, port, user, pass),
      });
      toast(t("telegram.toasts.credentialsOpened"), {
        description: t("telegram.toasts.credentialsOpenedDescription"),
      });
    } catch (error) {
      toast.error(t("telegram.toasts.browserOpenFailed"), { description: String(error) });
    }
  }

  function invalidateFromProxy() {
    setDetected(false);
    setGeo(null);
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

  async function sendCode() {
    await startTelegram("telegram_phone");
  }

  async function startQrLogin() {
    await startTelegram("telegram_qr");
  }

  async function startTelegram(method: "telegram_qr" | "telegram_phone") {
    setBusy(true);
    try {
      let currentFlowId = flowId;
      if (!currentFlowId) {
        const prepared = await prepareOperatorConnection({ platform: "telegram", intent: "connect", method, label: "Telegram" });
        currentFlowId = prepared.flow_id;
        flowIdRef.current = currentFlowId;
        setFlowId(currentFlowId);
      }
      await invoke("onecrew_operator_account_connection_configure_telegram_in_app", {
        flowId: currentFlowId,
        apiId: Number(apiId),
        apiHash: apiHash.trim(),
        phone: method === "telegram_phone" ? phone.trim() : null,
        proxy: toProxyConfig(type, ip, port, user, pass),
      });
      const status = await beginOperatorConnection(currentFlowId);
      await applyStatus(status);
    } catch (error) {
      toast.error(
        method === "telegram_qr"
          ? t("telegram.toasts.generateQrFailed")
          : t("telegram.toasts.sendCodeFailed"),
        { description: String(error) },
      );
    } finally {
      setBusy(false);
    }
  }

  async function cancelQrLogin() {
    const current = flowId;
    setQrChallenge(null);
    setStage("creds");
    if (current) await cancelOperatorConnection(current);
    flowIdRef.current = "";
    setFlowId("");
  }

  function chooseLoginMethod(value: string) {
    const method = value === "phone" ? "phone" : "qr";
    if (method !== loginMethod && flowId) {
      void cancelQrLogin();
    }
    setLoginMethod(method);
  }

  async function submitCode() {
    setBusy(true);
    try {
      const result = await invoke<OperatorConnectionStatus>("onecrew_operator_account_connection_submit_telegram_code_in_app", {
        flowId,
        code: code.trim(),
      });
      await applyStatus(result);
    } catch (error) {
      toast.error(t("telegram.toasts.verifyFailed"), { description: String(error) });
    } finally {
      setBusy(false);
    }
  }

  async function submitPassword() {
    setBusy(true);
    try {
      const result = await invoke<OperatorConnectionStatus>("onecrew_operator_account_connection_submit_telegram_password_in_app", { flowId, password });
      await applyStatus(result);
    } catch (error) {
      toast.error(t("telegram.toasts.twoFactorFailed"), { description: String(error) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-5">
      <Stepper step={step} steps={telegramSteps} />
      <div className="min-h-[212px] space-y-4">
        {step === 0 && (
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
              types={SOCKS5_ONLY}
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
              {t("telegram.proxyNotice")}
            </p>
          </>
        )}

        {step === 1 && (
          <div className="space-y-4">
            {stage === "creds" && (
              <>
                <div className="space-y-2 rounded-lg border p-3">
                  <Button type="button" variant="outline" className="w-full" onClick={openCredentials}>
                    <ExternalLink className="size-4" /> {t("telegram.openCredentials")}
                  </Button>
                  <p className="text-xs text-muted-foreground">
                    {t("telegram.openCredentialsDescription")}
                  </p>
                </div>
                <div className="flex items-center gap-3 text-xs text-muted-foreground">
                  <span className="h-px flex-1 bg-border" />
                  {t("telegram.fillDirectly")}
                  <span className="h-px flex-1 bg-border" />
                </div>
              </>
            )}
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-2">
                <Label htmlFor="tg-api-id">{t("telegram.credentials.apiId")}</Label>
                <Input
                  id="tg-api-id"
                  value={apiId}
                  onChange={(e) => setApiId(e.target.value)}
                  placeholder={t("telegram.credentials.apiIdPlaceholder")}
                  disabled={stage !== "creds"}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="tg-api-hash">{t("telegram.credentials.apiHash")}</Label>
                <Input
                  id="tg-api-hash"
                  value={apiHash}
                  onChange={(e) => setApiHash(e.target.value)}
                  placeholder={t("telegram.credentials.apiHashPlaceholder")}
                  disabled={stage !== "creds"}
                />
              </div>
            </div>
            {stage === "creds" && (
              <RadioGroup value={loginMethod} onValueChange={chooseLoginMethod} className="grid grid-cols-2">
                <Label
                  htmlFor="tg-login-qr"
                  className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 has-[[data-state=checked]]:border-primary has-[[data-state=checked]]:bg-primary/5"
                >
                  <RadioGroupItem id="tg-login-qr" value="qr" className="mt-0.5" />
                  <QrCode className="size-5 shrink-0" />
                  <span>
                    <span className="block font-medium">{t("telegram.qr.title")}</span>
                    <span className="mt-1 block text-xs font-normal text-muted-foreground">
                      {t("telegram.qr.description")}
                    </span>
                  </span>
                </Label>
                <Label
                  htmlFor="tg-login-phone"
                  className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 has-[[data-state=checked]]:border-primary has-[[data-state=checked]]:bg-primary/5"
                >
                  <RadioGroupItem id="tg-login-phone" value="phone" className="mt-0.5" />
                  <Smartphone className="size-5 shrink-0" />
                  <span>
                    <span className="block font-medium">{t("telegram.phone.title")}</span>
                    <span className="mt-1 block text-xs font-normal text-muted-foreground">
                      {t("telegram.phone.description")}
                    </span>
                  </span>
                </Label>
              </RadioGroup>
            )}
            {loginMethod === "phone" && (
              <div className="space-y-2">
                <Label htmlFor="tg-phone">{t("telegram.phone.label")}</Label>
                <Input
                  id="tg-phone"
                  value={phone}
                  onChange={(e) => setPhone(e.target.value)}
                  placeholder={t("telegram.phone.placeholder")}
                  disabled={stage !== "creds"}
                />
                {phone && !isIntlPhone(phone) ? (
                  <p className="text-xs text-[var(--tone-danger-fg)]">
                    {t("telegram.phone.invalid")}
                  </p>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    {t("telegram.phone.promptPrefix")}{" "}
                    <button
                      type="button"
                      onClick={() => void openExternalUrl(INTL_FORMAT_FAQ)}
                      className="text-primary underline underline-offset-2"
                    >
                      {t("telegram.phone.internationalFormat")}
                    </button>
                    {t("telegram.phone.promptSuffix")}
                  </p>
                )}
              </div>
            )}
            {stage === "qr" && qrChallenge && (
              <div className="flex flex-col items-center gap-3 rounded-xl border bg-muted/20 p-4 text-center">
                <div className="rounded-xl border bg-white p-3 shadow-sm">
                  <img
                    src={qrChallenge.image_data_url}
                    alt={t("telegram.qr.imageAlt")}
                    className="size-56"
                  />
                </div>
                <div className="space-y-1">
                  <p className="font-medium">{t("telegram.qr.scanTitle")}</p>
                  <p className="text-xs leading-5 text-muted-foreground">
                    {t("telegram.qr.scanDescription")}
                  </p>
                </div>
                <Button type="button" variant="outline" size="sm" onClick={() => void cancelQrLogin()}>
                  {t("telegram.qr.cancel")}
                </Button>
              </div>
            )}
            {stage === "code" && (
              <>
                {delivery && (
                  <div className="space-y-2 rounded-lg border bg-muted/35 p-3 text-sm">
                    <div className="flex flex-wrap items-center gap-2 font-medium">
                      <span>{deliveryLabel(delivery.method, t)}</span>
                      {delivery.code_length != null && (
                        <span className="rounded-md bg-background px-2 py-0.5 text-xs text-muted-foreground">
                          {t("telegram.delivery.codeLength", { count: delivery.code_length })}
                        </span>
                      )}
                    </div>
                    <p className="text-xs leading-5 text-muted-foreground">
                      {deliveryDescription(delivery, t)}
                    </p>
                    {delivery.destination_hint && delivery.method !== "email" && (
                      <p className="text-xs text-muted-foreground">
                        {t("telegram.delivery.hint", { hint: delivery.destination_hint })}
                      </p>
                    )}
                    {delivery.next_method && (
                      <p className="text-xs text-muted-foreground">
                        {delivery.timeout_seconds != null
                          ? t("telegram.delivery.nextMethodAfter", {
                              method: deliveryLabel(delivery.next_method, t),
                              seconds: delivery.timeout_seconds,
                            })
                          : t("telegram.delivery.nextMethod", {
                              method: deliveryLabel(delivery.next_method, t),
                            })}
                      </p>
                    )}
                    {delivery.action_url && (
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => void openExternalUrl(delivery.action_url!)}
                      >
                        <ExternalLink className="size-4" /> {t("telegram.delivery.openCodePage")}
                      </Button>
                    )}
                  </div>
                )}
                <div className="space-y-2">
                  <Label htmlFor="tg-code">{t("telegram.code.label")}</Label>
                  <Input
                    id="tg-code"
                    value={code}
                    onChange={(e) => setCode(e.target.value)}
                    placeholder={
                      delivery?.code_length
                        ? t("telegram.code.placeholderWithLength", { count: delivery.code_length })
                        : t("telegram.code.placeholder")
                    }
                    maxLength={delivery?.code_length ?? undefined}
                    disabled={stage !== "code"}
                  />
                </div>
              </>
            )}
            {stage === "password" && (
              <div className="space-y-2">
                <Label htmlFor="tg-2fa">{t("telegram.password.label")}</Label>
                <Input
                  id="tg-2fa"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </div>
            )}

            {stage === "creds" && loginMethod === "qr" ? (
              <Button
                type="button"
                className="w-full"
                disabled={busy || !apiId || !apiHash}
                onClick={startQrLogin}
              >
                {busy ? <Loader2 className="size-4 animate-spin" /> : <QrCode className="size-4" />}
                {t("telegram.qr.generate")}
              </Button>
            ) : stage === "creds" ? (
              <Button
                type="button"
                className="w-full"
                disabled={busy || !apiId || !apiHash || !isIntlPhone(phone)}
                onClick={sendCode}
              >
                {busy ? <Loader2 className="size-4 animate-spin" /> : <KeyRound className="size-4" />}
                {t("telegram.code.send")}
              </Button>
            ) : stage === "qr" ? null : stage === "code" ? (
              <Button type="button" className="w-full" disabled={busy || !code} onClick={submitCode}>
                {busy ? <Loader2 className="size-4 animate-spin" /> : <LogIn className="size-4" />}
                {t("telegram.code.verifyAndLogin")}
              </Button>
            ) : (
              <Button type="button" className="w-full" disabled={busy || !password} onClick={submitPassword}>
                {busy ? <Loader2 className="size-4 animate-spin" /> : <KeyRound className="size-4" />}
                {t("telegram.password.submit")}
              </Button>
            )}
            <p className="text-xs text-muted-foreground">
              {t("telegram.securityNote")}
            </p>
          </div>
        )}
      </div>

      <DialogFooter className="sm:justify-between">
        {step === 1 && stage === "creds" ? (
          <Button type="button" variant="ghost" onClick={() => setStep(0)}>
            {t("common.actions.previous")}
          </Button>
        ) : (
          <span />
        )}
        {step === 0 ? (
          <Button
            type="button"
            disabled={!ip || !port}
            onClick={() => {
              if (!detected) {
                toast.error(t("telegram.toasts.detectFirst"), {
                  description: t("telegram.toasts.detectFirstDescription"),
                });
                return;
              }
              setStep(1);
            }}
          >
            {t("common.actions.next")}
          </Button>
        ) : (
          <span />
        )}
      </DialogFooter>
    </div>
  );
}
