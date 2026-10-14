import { LoaderCircle } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useAccessI18n } from "@/lib/access-i18n";

export function EmailOtpFields({
  email,
  otp,
  sent,
  cooldownSeconds,
  pending,
  onEmailChange,
  onOtpChange,
  onSend,
  onSubmit,
}: {
  email: string;
  otp: string;
  sent: boolean;
  cooldownSeconds: number;
  pending: "send" | "login" | null;
  onEmailChange: (value: string) => void;
  onOtpChange: (value: string) => void;
  onSend: () => void;
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void;
}) {
  const { t } = useAccessI18n();
  return (
    <form className="flex flex-col gap-6" onSubmit={onSubmit}>
      <fieldset disabled={pending !== null}>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="login-otp-email">{t("auth.login.email")}</FieldLabel>
          <Input
            id="login-otp-email"
            type="email"
            autoComplete="email"
            inputMode="email"
            placeholder={t("auth.login.emailPlaceholder")}
            value={email}
            onChange={(event) => onEmailChange(event.target.value)}
            required
            autoFocus
          />
          </Field>
          {sent ? (
            <Field>
              <FieldLabel htmlFor="login-otp-code">{t("auth.login.otpCode")}</FieldLabel>
            <Input
              id="login-otp-code"
              type="text"
              name="otp"
              inputMode="numeric"
              autoComplete="one-time-code"
              pattern="[0-9]{6}"
              maxLength={6}
              placeholder={t("auth.login.otpPlaceholder")}
              value={otp}
              onChange={(event) => onOtpChange(event.target.value.replace(/\D/g, "").slice(0, 6))}
              required
              autoFocus
            />
            </Field>
          ) : null}
          <Field>
            <p className="text-sm leading-normal text-muted-foreground">{t("auth.otp.firstLoginHint")}</p>
          </Field>
          <Field>
            <div className="flex flex-col gap-2 sm:flex-row">
              <Button
                type={sent ? "button" : "submit"}
                className="w-full"
                variant={sent ? "outline" : "default"}
                disabled={pending !== null || cooldownSeconds > 0}
                onClick={sent ? onSend : undefined}
              >
                {pending === "send" ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : null}
                {sent
                  ? cooldownSeconds > 0
                  ? t("auth.login.cooldown", { seconds: cooldownSeconds })
                  : t("auth.login.resendOtp")
                : t("auth.login.sendOtp")}
              </Button>
              {sent ? (
                <Button type="submit" className="w-full" disabled={pending !== null}>
                  {pending === "login" ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : null}
                  {t("auth.login.verifyOtp")}
                </Button>
              ) : null}
            </div>
          </Field>
        </FieldGroup>
      </fieldset>
    </form>
  );
}
