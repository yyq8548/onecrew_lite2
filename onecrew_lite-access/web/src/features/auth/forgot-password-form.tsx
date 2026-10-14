import { useState } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useAccessI18n } from "@/lib/access-i18n";
import { resetPasswordByOTP, sendPasswordResetOTP } from "@/lib/auth-actions";

import { AuthHeading, FormMessage, SubmitLabel } from "./form-parts";
import { useOtpCooldown } from "./use-otp-cooldown";

export function ForgotPasswordForm({
  emailOtpEnabled = false,
  loginTo = "/login",
}: {
  emailOtpEnabled?: boolean;
  loginTo?: string;
}) {
  const { locale, t } = useAccessI18n();
  const [email, setEmail] = useState("");
  const [otp, setOtp] = useState("");
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [sent, setSent] = useState(false);
  const [complete, setComplete] = useState(false);
  const [pending, setPending] = useState<"send" | "reset" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { cooldownSeconds, startCooldown } = useOtpCooldown();

  async function sendCode() {
    const normalizedEmail = email.trim();
    if (!emailOtpEnabled || normalizedEmail === "") {
      setError(t("auth.login.validEmailForOtp"));
      return;
    }
    setError(null);
    setPending("send");
    const result = await sendPasswordResetOTP(normalizedEmail, locale);
    if (!result.ok) {
      setError(result.message);
      setPending(null);
      return;
    }
    setSent(true);
    startCooldown();
    setPending(null);
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    if (password !== confirmation) {
      setError(t("auth.login.passwordMismatch"));
      return;
    }
    setPending("reset");
    const result = await resetPasswordByOTP({
      email: email.trim(),
      otp: otp.trim(),
      password,
    }, locale);
    if (!result.ok) {
      setError(result.message);
      setPending(null);
      return;
    }
    setComplete(true);
    setPending(null);
  }

  if (complete) {
    return (
      <div className="flex w-full flex-col gap-6">
        <AuthHeading title={t("auth.forgot.completeTitle")} description={t("auth.forgot.completeDescription")} />
        <Button asChild className="w-full">
          <Link to={loginTo}>{t("auth.forgot.backToLogin")}</Link>
        </Button>
      </div>
    );
  }

  return (
    <div className="flex w-full flex-col gap-6">
      <AuthHeading title={t("auth.forgot.title")} description={t("auth.forgot.description")} />
      {error ? <FormMessage>{error}</FormMessage> : null}
      <form className="flex flex-col gap-6" onSubmit={handleSubmit}>
        <fieldset disabled={pending !== null}>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="forgot-email">{t("auth.forgot.email")}</FieldLabel>
              <Input
                id="forgot-email"
                type="email"
                autoComplete="email"
                inputMode="email"
                placeholder={t("auth.forgot.emailPlaceholder")}
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                required
                autoFocus
              />
            </Field>
            {sent ? (
              <Field>
                <FieldLabel htmlFor="forgot-otp">{t("auth.forgot.otpCode")}</FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id="forgot-otp"
                    name="otp"
                    className="min-w-0 flex-1"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    pattern="[0-9]{6}"
                    maxLength={6}
                    placeholder={t("auth.forgot.otpPlaceholder")}
                    value={otp}
                    onChange={(event) => setOtp(event.target.value.replace(/\D/g, "").slice(0, 6))}
                    required
                    autoFocus
                  />
                  <Button
                    type="button"
                    variant="outline"
                    disabled={pending !== null || cooldownSeconds > 0}
                    onClick={() => void sendCode()}
                  >
                    {pending === "send" ? t("common.sending") : cooldownSeconds > 0 ? t("auth.forgot.cooldown", { seconds: cooldownSeconds }) : t("auth.forgot.resendOtp")}
                  </Button>
                </div>
                <FieldDescription>{t("auth.otp.validity")}</FieldDescription>
              </Field>
            ) : null}
            {sent ? (
              <>
                <Field>
                  <FieldLabel htmlFor="forgot-password">{t("auth.forgot.newPassword")}</FieldLabel>
                  <Input
                    id="forgot-password"
                    type="password"
                    autoComplete="new-password"
                    minLength={8}
                    value={password}
                    onChange={(event) => setPassword(event.target.value)}
                    required
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="forgot-password-confirmation">{t("auth.forgot.confirmPassword")}</FieldLabel>
                  <Input
                    id="forgot-password-confirmation"
                    type="password"
                    autoComplete="new-password"
                    minLength={8}
                    value={confirmation}
                    onChange={(event) => setConfirmation(event.target.value)}
                    required
                  />
                </Field>
                <Field>
                  <Button type="submit" className="w-full" disabled={pending !== null}>
                    <SubmitLabel pending={pending === "reset"}>{t("auth.forgot.reset")}</SubmitLabel>
                  </Button>
                </Field>
              </>
            ) : (
              <Field>
                <Button
                  type="button"
                  className="w-full"
                  disabled={pending !== null || !emailOtpEnabled || cooldownSeconds > 0}
                  onClick={() => void sendCode()}
                >
                  <SubmitLabel pending={pending === "send"}>{t("auth.forgot.sendOtp")}</SubmitLabel>
                </Button>
              </Field>
            )}
          </FieldGroup>
        </fieldset>
      </form>
      <FieldDescription className="text-center">
        {t("auth.forgot.remembered")}{" "}
        <Link className="font-medium text-foreground" to={loginTo}>{t("auth.forgot.backToLogin")}</Link>
      </FieldDescription>
    </div>
  );
}
