import { Languages } from "lucide-react";
import { useTranslation } from "react-i18next";

import { persistLocale, type SupportedLocale } from "@/i18n";
import { normalizeLocale } from "@/i18n/locale";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";

export function nextLocale(locale: SupportedLocale): SupportedLocale {
  return locale === "en-US" ? "zh-CN" : "en-US";
}

export function LanguageSwitcher() {
  const { i18n, t } = useTranslation("sidebar");
  const locale = normalizeLocale(i18n.resolvedLanguage ?? i18n.language) ?? "zh-CN";
  const targetLocale = nextLocale(locale);
  const currentLabel = locale === "en-US"
    ? t("language.english")
    : t("language.chinese");
  const switchLabel = targetLocale === "en-US"
    ? t("language.switchToEnglish")
    : t("language.switchToChinese");

  const switchLanguage = () => {
    persistLocale(targetLocale);
    void i18n.changeLanguage(targetLocale);
  };

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <SidebarMenuButton
          type="button"
          variant="rail"
          size="rail"
          tooltip={switchLabel}
          aria-label={switchLabel}
          onClick={switchLanguage}
          className="cursor-pointer select-none"
        >
          <Languages aria-hidden="true" className="size-5 shrink-0" />
          <span className="group-data-[collapsible=icon]:hidden">{currentLabel}</span>
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
