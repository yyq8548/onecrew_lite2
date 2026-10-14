import { LoaderCircle } from "lucide-react";
import { useState, type FormEvent } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { FieldGroup } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { saveAccountPassword } from "@/lib/account-actions";
import { useAccessI18n } from "@/lib/access-i18n";

import { SettingsRow } from "./settings-row";

export function SecuritySettings({
  hasPassword,
  onPasswordSet,
  refreshSession,
}: {
  hasPassword: boolean | null;
  onPasswordSet: () => void;
  refreshSession: () => Promise<unknown>;
}) {
  const { locale, t } = useAccessI18n();
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setMessage(null);
    if (newPassword !== confirmation) {
      setMessage(t("auth.login.passwordMismatch"));
      return;
    }
    setPending(true);
    const result = await saveAccountPassword({
      ...(hasPassword ? { currentPassword } : {}),
      newPassword,
    }, locale);
    if (!result.ok) {
      setMessage(result.message);
      setPending(false);
      return;
    }
    onPasswordSet();
    setCurrentPassword("");
    setNewPassword("");
    setConfirmation("");
    setMessage(t("account.passwordSaved"));
    setPending(false);
    await refreshSession().catch(() => undefined);
  }

  return (
    <CardContent className="p-0">
      {hasPassword === false ? (
        <Alert className="m-6">
          <AlertDescription>{t("account.passwordSetHint")}</AlertDescription>
        </Alert>
      ) : null}
      <form onSubmit={handleSubmit}>
        <FieldGroup className="gap-0">
          {hasPassword ? (
            <SettingsRow
              htmlFor="account-current-password"
              label={t("account.currentPassword")}
            >
              <Input
                id="account-current-password"
                type="password"
                autoComplete="current-password"
                placeholder={t("account.currentPasswordPlaceholder")}
                value={currentPassword}
                onChange={(event) => setCurrentPassword(event.target.value)}
                required
              />
            </SettingsRow>
          ) : null}
          <SettingsRow htmlFor="account-new-password" label={t("account.newPassword")}>
            <Input
              id="account-new-password"
              type="password"
              autoComplete="new-password"
              minLength={8}
              placeholder={t("account.newPasswordPlaceholder")}
              value={newPassword}
              onChange={(event) => setNewPassword(event.target.value)}
              required
            />
          </SettingsRow>
          <SettingsRow htmlFor="account-password-confirmation" label={t("account.confirmPassword")}>
            <Input
              id="account-password-confirmation"
              type="password"
              autoComplete="new-password"
              minLength={8}
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
              required
            />
          </SettingsRow>
        </FieldGroup>
        <div className="flex justify-end border-t border-border bg-card px-6 py-4">
          <Button type="submit" disabled={pending || hasPassword === null}>
            {pending ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : null}
            {hasPassword ? t("account.changePassword") : t("account.setPassword")}
          </Button>
        </div>
      </form>
      {message ? <p className="px-6 py-3 text-sm text-muted-foreground" aria-live="polite">{message}</p> : null}
    </CardContent>
  );
}
