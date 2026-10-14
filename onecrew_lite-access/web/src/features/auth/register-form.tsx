import { useState } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "./password-input";
import { useAccessI18n } from "@/lib/access-i18n";
import {
  sendRegistrationEmailOTP,
  signUpWithEmail,
  startGoogleSignIn,
  verifyRegistrationEmailOTP,
} from "@/lib/auth-actions";

import {
  AuthHeading,
  FormMessage,
  GoogleButton,
  SubmitLabel,
} from "./form-parts";
import { useOtpCooldown } from "./use-otp-cooldown";

export function RegisterForm({
  callbackError,
  emailOtpEnabled = false,
  googleSignInEnabled,
  loginTo = "/login",
  returnTo,
  onAuthenticated,
}: {
  callbackError?: string | null;
  returnTo?: string;
  emailOtpEnabled?: boolean;
  googleSignInEnabled: boolean;
  loginTo?: string;
  onAuthenticated: () => Promise<void> | void;
}) {
  const { locale, t } = useAccessI18n();
  const [pending, setPending] = useState<"email" | "otp-send" | "google" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [verificationEmail, setVerificationEmail] = useState<string | null>(null);
  const { cooldownSeconds, startCooldown } = useOtpCooldown();

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setError(null);

    if (verificationEmail) {
      setPending("email");
      const result = await verifyRegistrationEmailOTP({
        email: verificationEmail,
        otp: String(form.get("otp") ?? "").trim(),
      }, locale);
      if (!result.ok) {
        setError(result.message);
        setPending(null);
        return;
      }
      try {
        await onAuthenticated();
      } catch {
        setPending(null);
      }
      return;
    }

    const password = String(form.get("password") ?? "");
    const confirmation = String(form.get("password-confirmation") ?? "");
    if (password !== confirmation) {
      setError(t("auth.login.passwordMismatch"));
      return;
    }

    setPending("email");
    const result = await signUpWithEmail({
      name: String(form.get("name") ?? "").trim(),
      email: String(form.get("email") ?? "").trim(),
      password,
    }, locale);

    if (!result.ok) {
      setError(result.message);
      setPending(null);
      return;
    }

    if (result.verificationRequired) {
      setVerificationEmail(result.email ?? String(form.get("email") ?? "").trim());
      startCooldown();
      setPending(null);
      return;
    }

    try {
      await onAuthenticated();
    } catch {
      setPending(null);
    }
  }

  async function handleResendVerification() {
    if (!verificationEmail || cooldownSeconds > 0) return;
    setError(null);
    setPending("otp-send");
    const result = await sendRegistrationEmailOTP(verificationEmail, locale);
    if (!result.ok) {
      setError(result.message);
      setPending(null);
      return;
    }
    startCooldown();
    setPending(null);
  }

  async function handleGoogle() {
    setError(null);
    setPending("google");
    const result = await startGoogleSignIn(locale, returnTo);
    if (!result.ok) {
      setError(result.message);
      setPending(null);
    }
  }

  const visibleError = error ?? callbackError;

  return (
    <div className="flex w-full flex-col gap-6">
      {verificationEmail ? (
        <>
          <AuthHeading
            title={t("auth.register.verificationTitle")}
            description={t("auth.register.verificationDescription", { email: verificationEmail })}
          />
          {visibleError ? <FormMessage>{visibleError}</FormMessage> : null}
          <form className="flex flex-col gap-6" onSubmit={handleSubmit}>
            <fieldset disabled={pending !== null}>
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="register-otp">{t("auth.register.otpCode")}</FieldLabel>
                  <Input
                    id="register-otp"
                    name="otp"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    pattern="[0-9]{6}"
                    maxLength={6}
                    placeholder={t("auth.register.otpPlaceholder")}
                    required
                    autoFocus
                  />
                </Field>
                <Field>
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <Button
                      type="button"
                      className="w-full"
                      variant="outline"
                      disabled={pending !== null || cooldownSeconds > 0}
                      onClick={() => void handleResendVerification()}
                    >
                      <SubmitLabel pending={pending === "otp-send"}>
                        {cooldownSeconds > 0
                          ? t("auth.register.cooldown", { seconds: cooldownSeconds })
                          : t("auth.register.resendOtp")}
                      </SubmitLabel>
                    </Button>
                    <Button type="submit" className="w-full" disabled={pending !== null}>
                      <SubmitLabel pending={pending === "email"}>{t("auth.register.verify")}</SubmitLabel>
                    </Button>
                  </div>
                </Field>
              </FieldGroup>
            </fieldset>
          </form>
          <p className="text-center text-sm text-muted-foreground">
            {t("auth.register.wrongEmail")}{" "}
            <button
              className="font-medium text-foreground underline underline-offset-4 hover:text-primary"
              type="button"
              onClick={() => setVerificationEmail(null)}
            >{t("auth.register.backToRegister")}</button>
          </p>
        </>
      ) : null}

      {!verificationEmail ? <>
        <AuthHeading
          title={t("auth.register.title")}
          description={t("auth.register.description")}
        />

        {visibleError ? <FormMessage>{visibleError}</FormMessage> : null}

        <Field>
          <GoogleButton
            disabled={pending !== null || !googleSignInEnabled}
            enabled={googleSignInEnabled}
            loading={pending === "google"}
            onClick={handleGoogle}
          />
        </Field>

        <FieldSeparator>{t("common.or")}</FieldSeparator>

        <form className="flex flex-col gap-6" onSubmit={handleSubmit}>
          <fieldset disabled={pending !== null}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="register-name">{t("auth.register.name")}</FieldLabel>
                <Input
                  id="register-name"
                  name="name"
                  autoComplete="name"
                  placeholder={t("auth.register.namePlaceholder")}
                  required
                  autoFocus
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="register-email">{t("auth.register.email")}</FieldLabel>
                <Input
                  id="register-email"
                  name="email"
                  type="email"
                  autoComplete="email"
                  inputMode="email"
                  placeholder={t("auth.register.emailPlaceholder")}
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="register-password">{t("auth.register.password")}</FieldLabel>
                <PasswordInput
                  id="register-password"
                  name="password"
                  type="password"
                  autoComplete="new-password"
                  minLength={8}
                  aria-describedby="password-help"
                  placeholder={t("auth.register.passwordHelp")}
                  required
                />
                <FieldDescription id="password-help">{t("auth.register.passwordHelp")}</FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="register-password-confirmation">{t("auth.register.confirmation")}</FieldLabel>
                <PasswordInput
                  id="register-password-confirmation"
                  name="password-confirmation"
                  type="password"
                  autoComplete="new-password"
                  minLength={8}
                  required
                />
              </Field>
              <Field>
                <Button type="submit" className="w-full" disabled={pending !== null}>
                  <SubmitLabel pending={pending === "email"}>{t("auth.register.submit")}</SubmitLabel>
                </Button>
              </Field>
            </FieldGroup>
          </fieldset>
        </form>

        <FieldDescription className="text-center">
          {t("auth.register.hasAccount")}{" "}
          <Link className="font-medium text-foreground" to={loginTo}>
            {t("auth.register.backToLogin")}
          </Link>
        </FieldDescription>
      </> : null}
    </div>
  );
}
