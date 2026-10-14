import { useState } from "react";

import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useAccessI18n } from "@/lib/access-i18n";
import {
  requestAccountEmailChange,
  sendCurrentEmailCode,
  verifyAccountEmailChange,
  verifyCurrentEmailCode,
} from "@/lib/account-actions";

import { useOtpCooldown } from "@/features/auth/use-otp-cooldown";

import { SettingsRow } from "./settings-row";

type EmailChangeStep = "current" | "new" | "verify";

export function EmailSettings({
  emailOtpEnabled,
  refreshSession,
  userEmail,
}: {
  emailOtpEnabled: boolean;
  refreshSession: () => Promise<unknown>;
  userEmail: string;
}) {
  const { locale, t } = useAccessI18n();
  const [step, setStep] = useState<EmailChangeStep>("current");
  const [newEmail, setNewEmail] = useState("");
  const [currentOtp, setCurrentOtp] = useState("");
  const [newOtp, setNewOtp] = useState("");
  const [currentOtpSent, setCurrentOtpSent] = useState(false);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const { cooldownSeconds, startCooldown } = useOtpCooldown();

  async function sendCurrentCode() {
    if (!emailOtpEnabled || cooldownSeconds > 0) return;
    setMessage(null);
    setPending(true);
    const result = await sendCurrentEmailCode(locale);
    if (!result.ok) {
      setMessage(result.message);
      setPending(false);
      return;
    }
    setCurrentOtpSent(true);
    startCooldown();
    setMessage(t("account.currentCodeSent"));
    setPending(false);
  }

  async function verifyCurrentCode() {
    if (!emailOtpEnabled || currentOtp.length !== 6) return;
    setMessage(null);
    setPending(true);
    const result = await verifyCurrentEmailCode(currentOtp.trim(), locale);
    if (!result.ok) {
      setMessage(result.message);
      setPending(false);
      return;
    }
    setStep("new");
    setMessage(t("account.currentEmailVerified"));
    setPending(false);
  }

  async function requestChange() {
    if (!emailOtpEnabled || newEmail.trim() === "" || currentOtp.length !== 6) return;
    setMessage(null);
    setPending(true);
    const result = await requestAccountEmailChange({
      currentOtp: currentOtp.trim(),
      newEmail: newEmail.trim(),
    }, locale);
    if (!result.ok) {
      setMessage(result.message);
      setPending(false);
      return;
    }
    setStep("verify");
    setMessage(t("account.emailChangeRequested"));
    setPending(false);
  }

  async function verifyChange() {
    if (newOtp.length !== 6) return;
    setMessage(null);
    setPending(true);
    const result = await verifyAccountEmailChange({
      newEmail: newEmail.trim(),
      otp: newOtp.trim(),
    }, locale);
    if (!result.ok) {
      setMessage(result.message);
      setPending(false);
      return;
    }
    setStep("current");
    setCurrentOtp("");
    setNewOtp("");
    setNewEmail("");
    setCurrentOtpSent(false);
    setMessage(t("account.emailUpdated"));
    setPending(false);
    await refreshSession().catch(() => undefined);
  }

  function restart() {
    setStep("current");
    setCurrentOtp("");
    setNewOtp("");
    setNewEmail("");
    setCurrentOtpSent(false);
    setMessage(null);
  }

  return (
    <CardContent className="p-0">
      <SettingsRow htmlFor="account-current-email" label={t("account.currentEmail")}>
        <Input id="account-current-email" value={userEmail} readOnly />
      </SettingsRow>

      {step === "current" ? (
        <>
          <SettingsRow
            description={t("account.currentEmailVerifyDescription")}
            htmlFor="account-current-email-otp"
            label={t("account.currentOtp")}
          >
            <div className="flex flex-col gap-2 sm:flex-row">
              <Input
                id="account-current-email-otp"
                inputMode="numeric"
                autoComplete="one-time-code"
                pattern="[0-9]{6}"
                maxLength={6}
                value={currentOtp}
                onChange={(event) => setCurrentOtp(event.target.value.replace(/\D/g, "").slice(0, 6))}
                disabled={!emailOtpEnabled || pending}
                required
              />
              <Button
                type="button"
                variant="outline"
                className="shrink-0"
                disabled={!emailOtpEnabled || pending || cooldownSeconds > 0}
                onClick={() => void sendCurrentCode()}
              >
                {pending ? t("common.sending") : cooldownSeconds > 0 ? `${cooldownSeconds}s` : t("account.sendCurrentOtp")}
              </Button>
            </div>
          </SettingsRow>
          <div className="flex justify-end border-t border-border bg-card px-6 py-4">
            <Button
              type="button"
              disabled={!emailOtpEnabled || pending || !currentOtpSent || currentOtp.length !== 6}
              onClick={() => void verifyCurrentCode()}
            >
              {t("account.verifyCurrentEmail")}
            </Button>
          </div>
        </>
      ) : null}

      {step === "new" ? (
        <>
          <SettingsRow htmlFor="account-new-email" label={t("account.newEmail")}>
            <Input
              id="account-new-email"
              type="email"
              autoComplete="email"
              placeholder={t("account.newEmailPlaceholder")}
              value={newEmail}
              onChange={(event) => setNewEmail(event.target.value)}
              disabled={!emailOtpEnabled || pending}
              required
              autoFocus
            />
          </SettingsRow>
          <div className="flex justify-end border-t border-border bg-card px-6 py-4">
            <Button
              type="button"
              disabled={!emailOtpEnabled || pending || newEmail.trim() === ""}
              onClick={() => void requestChange()}
            >
              {t("account.sendNewEmailOtp")}
            </Button>
          </div>
        </>
      ) : null}

      {step === "verify" ? (
        <>
          <SettingsRow htmlFor="account-pending-email" label={t("account.newEmail")}>
            <Input id="account-pending-email" value={newEmail} readOnly />
          </SettingsRow>
          <SettingsRow htmlFor="account-new-email-otp" label={t("account.newOtp")}>
            <Input
              id="account-new-email-otp"
              inputMode="numeric"
              autoComplete="one-time-code"
              pattern="[0-9]{6}"
              maxLength={6}
              value={newOtp}
              onChange={(event) => setNewOtp(event.target.value.replace(/\D/g, "").slice(0, 6))}
              disabled={pending}
              required
              autoFocus
            />
          </SettingsRow>
          <div className="flex flex-col justify-end gap-2 border-t border-border bg-card px-6 py-4 sm:flex-row">
            <Button type="button" variant="outline" disabled={pending} onClick={restart}>
              {t("account.restartEmailChange")}
            </Button>
            <Button type="button" disabled={pending || newOtp.length !== 6} onClick={() => void verifyChange()}>
              {t("account.verifyEmailChange")}
            </Button>
          </div>
        </>
      ) : null}

      {message ? <p className="px-6 py-3 text-sm text-muted-foreground" aria-live="polite">{message}</p> : null}
    </CardContent>
  );
}
