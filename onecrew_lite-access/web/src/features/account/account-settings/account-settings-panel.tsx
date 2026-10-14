import { useEffect, useState } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { loadAccountSettings } from "@/lib/account-actions";
import type { SafeSession, SafeSessionUser } from "@/lib/access-api";
import { useAccessI18n } from "@/lib/access-i18n";
import { DeviceAccessPanel } from "@/features/account/device-access-panel";

import { EmailSettings } from "./email-settings";
import { ProfileSettings } from "./profile-settings";
import { SecuritySettings } from "./security-settings";
import type { AccountSection } from "./types";

export function AccountSettingsPanel({
  emailOtpEnabled,
  refreshSession,
  user,
  section = "profile",
}: {
  emailOtpEnabled: boolean;
  refreshSession: () => Promise<SafeSession>;
  user: SafeSessionUser;
  section?: AccountSection;
}) {
  const { t } = useAccessI18n();
  const [hasPassword, setHasPassword] = useState<boolean | null>(null);
  const [settingsName, setSettingsName] = useState(user.name);
  const [settingsError, setSettingsError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void loadAccountSettings()
      .then((settings) => {
        if (!active) return;
        setHasPassword(settings.hasPassword);
        setSettingsName(settings.user.name);
      })
      .catch(() => {
        if (active) setSettingsError(t("errors.accountSettingsFailed"));
      });
    return () => {
      active = false;
    };
  }, [t]);

  const sectionTitle = section === "profile"
    ? t("account.profileTitle")
    : section === "security"
      ? t("account.securityTitle")
      : section === "email"
        ? t("account.emailTitle")
        : t("account.devicesTitle");
  const sectionDescription = section === "profile"
    ? t("account.profileDescription")
    : section === "security"
      ? t("account.securityDescription")
      : section === "email"
        ? t("account.emailDescription")
        : t("account.devicesDescription");

  return (
    <section className="account-settings-section" aria-labelledby="account-settings-heading">
      <h2 id="account-settings-heading" className="sr-only">{t("account.settingsTitle")}</h2>
      {settingsError ? (
        <Alert className="mb-6" variant="destructive">
          <AlertDescription>{settingsError}</AlertDescription>
        </Alert>
      ) : null}
      <div>
        <Card className="account-settings-card gap-0 overflow-hidden">
          <CardHeader className="border-b border-border">
            <CardTitle>{sectionTitle}</CardTitle>
            <CardDescription>{sectionDescription}</CardDescription>
          </CardHeader>
          {section === "profile" ? (
            <ProfileSettings
              initialName={settingsName}
              refreshSession={refreshSession}
              user={user}
            />
          ) : null}
          {section === "security" ? (
            <SecuritySettings
              hasPassword={hasPassword}
              onPasswordSet={() => setHasPassword(true)}
              refreshSession={refreshSession}
            />
          ) : null}
          {section === "email" ? (
            <EmailSettings
              emailOtpEnabled={emailOtpEnabled}
              refreshSession={refreshSession}
              userEmail={user.email}
            />
          ) : null}
          {section === "devices" ? (
            <DeviceAccessPanel
              embedded
              ownerUserId={user.id}
              refreshSession={refreshSession}
            />
          ) : null}
        </Card>
      </div>
    </section>
  );
}
