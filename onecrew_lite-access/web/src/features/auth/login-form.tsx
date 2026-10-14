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
  sendEmailLoginOTP,
  signInWithEmail,
  signInWithOTP,
  startGoogleSignIn,
} from "@/lib/auth-actions";

import { AuthMethodSwitch, type LoginMethod } from "./auth-method-switch";
import { EmailOtpFields } from "./email-otp-fields";
import { useOtpCooldown } from "./use-otp-cooldown";
import {
  AuthHeading,
  FormMessage,
  GoogleButton,
  SubmitLabel,
} from "./form-parts";

export function LoginForm({
  callbackError,
  emailOtpEnabled = false,
  googleSignInEnabled,
  onAuthenticated,
  registerTo = "/register",
  returnTo,
}: {
  callbackError?: string | null;
  returnTo?: string;
  emailOtpEnabled?: boolean;
  googleSignInEnabled: boolean;
  onAuthenticated: () => Promise<void> | void;
  registerTo?: string;
}) {
  const { locale, t } = useAccessI18n();
  const [method, setMethod] = useState<LoginMethod>("password");
  const [pending, setPending] = useState<"password" | "otp-send" | "otp-login" | "google" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [otp, setOtp] = useState("");
  const [otpSent, setOtpSent] = useState(false);
  const { cooldownSeconds, startCooldown } = useOtpCooldown();

  function switchMethod(next: LoginMethod) {
    setMethod(next);
    setError(null);
    setOtpSent(false);
    setOtp("");
  }

  async function finishAuthentication() {
    try {
      await onAuthenticated();
    } catch {
      setPending(null);
    }
  }

  async function handlePasswordSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setError(null);
    setPending("password");

    const result = await signInWithEmail({
      email: String(form.get("email") ?? "").trim(),
      password: String(form.get("password") ?? ""),
    }, locale);

    if (!result.ok) {
      setPending(null);
      if ("challenge" in result && result.challenge === "email_otp") {
        setEmail(result.email);
        setMethod("email-otp");
        setOtpSent(true);
        startCooldown();
        setError(result.message);
      } else {
        setError(result.message);
      }
      return;
    }

    await finishAuthentication();
  }

  async function sendOtp() {
    if (!emailOtpEnabled || email.trim() === "") {
      setError(t("auth.login.validEmailForOtp"));
      return;
    }
    setError(null);
    setPending("otp-send");
    const result = await sendEmailLoginOTP(email.trim(), locale);
    if (!result.ok) {
      setError(result.message);
      setPending(null);
      return;
    }
    setOtpSent(true);
    startCooldown();
    setPending(null);
  }

  async function handleOtpSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!otpSent) {
      await sendOtp();
      return;
    }
    setError(null);
    setPending("otp-login");
    const result = await signInWithOTP({ email: email.trim(), otp: otp.trim() }, locale);
    if (!result.ok) {
      setError(result.message);
      setPending(null);
      return;
    }
    await finishAuthentication();
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
      <AuthHeading title={t("auth.login.title")} description={t("auth.login.description")} />

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

      <AuthMethodSwitch
        value={method}
        emailOtpEnabled={emailOtpEnabled}
        onChange={switchMethod}
      />

      {method === "password" ? (
        <form className="flex flex-col gap-6" onSubmit={handlePasswordSubmit}>
          <fieldset disabled={pending !== null}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="login-email">{t("auth.login.email")}</FieldLabel>
                <Input
                  id="login-email"
                  name="email"
                  type="email"
                  autoComplete="email"
                  inputMode="email"
                  placeholder={t("auth.login.emailPlaceholder")}
                  required
                  autoFocus
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="login-password">{t("auth.login.password")}</FieldLabel>
                <PasswordInput
                  id="login-password"
                  name="password"
                  type="password"
                  autoComplete="current-password"
                  placeholder={t("auth.login.passwordPlaceholder")}
                  required
                />
                <Link className="auth-forgot-link underline-offset-4 hover:underline" to="/forgot-password">
                  {t("auth.login.forgotPassword")}
                </Link>
              </Field>
              <Field>
                <Button type="submit" className="w-full" disabled={pending !== null}>
                <SubmitLabel pending={pending === "password"}>{t("auth.login.submit")}</SubmitLabel>
                </Button>
              </Field>
            </FieldGroup>
          </fieldset>
        </form>
      ) : (
        <EmailOtpFields
          email={email}
          otp={otp}
          sent={otpSent}
          cooldownSeconds={cooldownSeconds}
          pending={pending === "otp-send" ? "send" : pending === "otp-login" ? "login" : null}
          onEmailChange={(value) => setEmail(value)}
          onOtpChange={setOtp}
          onSend={() => void sendOtp()}
          onSubmit={handleOtpSubmit}
        />
      )}

      <FieldDescription className="text-center">
        {t("auth.login.noAccount")}{" "}
        <Link className="font-medium text-foreground" to={registerTo}>
          {t("auth.login.createAccount")}
        </Link>
      </FieldDescription>
    </div>
  );
}
