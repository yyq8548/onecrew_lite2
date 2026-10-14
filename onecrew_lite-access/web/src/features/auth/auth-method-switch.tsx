import { Button } from "@/components/ui/button";
import { useAccessI18n } from "@/lib/access-i18n";

export type LoginMethod = "password" | "email-otp";

export function AuthMethodSwitch({
  value,
  emailOtpEnabled,
  onChange,
}: {
  value: LoginMethod;
  emailOtpEnabled: boolean;
  onChange: (value: LoginMethod) => void;
}) {
  const { t } = useAccessI18n();
  return (
    <div className="grid grid-cols-2 gap-1 rounded-lg bg-muted p-1" role="tablist" aria-label={t("auth.login.otpMethod")}>
      <Button
        type="button"
        role="tab"
        aria-selected={value === "password"}
        variant={value === "password" ? "default" : "ghost"}
        className="h-9 rounded-md"
        onClick={() => onChange("password")}
      >
        {t("auth.login.passwordMethod")}
      </Button>
      <Button
        type="button"
        role="tab"
        aria-selected={value === "email-otp"}
        variant={value === "email-otp" ? "default" : "ghost"}
        className="h-9 rounded-md"
        disabled={!emailOtpEnabled}
        onClick={() => onChange("email-otp")}
      >
        {t("auth.login.otpMethod")}
        {!emailOtpEnabled ? <span className="ml-1 text-xs opacity-70">{t("auth.login.otpUnavailable")}</span> : null}
      </Button>
    </div>
  );
}
