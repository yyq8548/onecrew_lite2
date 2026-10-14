import { LoaderCircle } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { saveAccountProfile } from "@/lib/account-actions";
import type { SafeSessionUser } from "@/lib/access-api";
import { useAccessI18n } from "@/lib/access-i18n";

import { SettingsRow } from "./settings-row";

export function ProfileSettings({
  initialName,
  refreshSession,
  user,
}: {
  initialName: string;
  refreshSession: () => Promise<unknown>;
  user: SafeSessionUser;
}) {
  const { locale, t } = useAccessI18n();
  const [name, setName] = useState(initialName || user.name);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    setName(initialName || user.name);
  }, [initialName, user.name]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setMessage(null);
    setPending(true);
    const result = await saveAccountProfile(name.trim(), locale);
    if (!result.ok) {
      setMessage(result.message);
      setPending(false);
      return;
    }
    try {
      await refreshSession();
      setMessage(t("account.profileSaved"));
    } catch {
      setMessage(t("errors.profileUpdateFailed"));
    }
    setPending(false);
  }

  return (
    <CardContent className="p-0">
      <form onSubmit={handleSubmit}>
        <SettingsRow
          description={t("account.nameDescription")}
          htmlFor="account-name"
          label={t("account.name")}
        >
          <Input
            id="account-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            required
          />
        </SettingsRow>
        <div className="flex justify-end border-t border-border bg-card px-6 py-4">
          <Button type="submit" disabled={pending || name.trim() === ""}>
            {pending ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : null}
            {t("account.saveProfile")}
          </Button>
        </div>
      </form>
      {message ? <p className="px-6 py-3 text-sm text-muted-foreground" aria-live="polite">{message}</p> : null}
    </CardContent>
  );
}
