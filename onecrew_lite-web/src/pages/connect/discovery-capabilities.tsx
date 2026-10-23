import { useTranslation } from "react-i18next";

export const hasNativeScoring = (platform: string) => ["youtube", "twitter", "tiktok", "bilibili"].includes(platform);
export function DiscoveryCapabilities({ platforms }: { platforms: readonly string[] }) {
  const { t } = useTranslation("shell");
  const native = platforms.filter(hasNativeScoring), leads = platforms.filter(platform => !hasNativeScoring(platform));
  return <details className="text-sm text-muted-foreground">
    <summary className="cursor-pointer font-medium">{t("opencliPilot.capabilityTitle")}</summary>
    <div className="space-y-2 py-3">
      {!!native.length && <p><span className="font-medium text-foreground">{native.join(", ")}</span> · {t("opencliPilot.capabilityNative")}</p>}
      {!!leads.length && <p><span className="font-medium text-foreground">{leads.join(", ")}</span> · {t("opencliPilot.capabilityLeads")}</p>}
      {!!native.length && <p>{t("opencliPilot.capabilityLanguage")}</p>}
    </div>
  </details>;
}
