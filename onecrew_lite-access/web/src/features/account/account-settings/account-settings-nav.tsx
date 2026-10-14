import { KeyRound, MailCheck, Smartphone, UserRound } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useAccessI18n } from "@/lib/access-i18n";

import type { AccountSection } from "./types";

const sectionIcons = {
  profile: UserRound,
  security: KeyRound,
  email: MailCheck,
  devices: Smartphone,
} as const;

export function AccountSettingsNav({
  onChange,
  section,
}: {
  onChange: (section: AccountSection) => void;
  section: AccountSection;
}) {
  const { t } = useAccessI18n();
  const items: Array<{ id: AccountSection; label: string }> = [
    { id: "profile", label: t("account.profileNav") },
    { id: "security", label: t("account.securityNav") },
    { id: "email", label: t("account.emailNav") },
    { id: "devices", label: t("account.devicesNav") },
  ];

  return (
    <nav aria-label={t("account.settingsTitle")} className="flex gap-1 overflow-x-auto lg:flex-col">
      {items.map(({ id, label }) => {
        const Icon = sectionIcons[id];
        return (
          <Button
            key={id}
            type="button"
            variant="ghost"
            aria-pressed={section === id}
            data-active={section === id}
            className={`justify-start whitespace-nowrap ${section === id ? "bg-accent text-accent-foreground hover:bg-accent" : ""}`}
            onClick={() => onChange(id)}
          >
            <Icon aria-hidden="true" />
            {label}
          </Button>
        );
      })}
    </nav>
  );
}
