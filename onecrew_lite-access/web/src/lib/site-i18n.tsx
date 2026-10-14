import i18next from "i18next";
import { useMemo, type ReactNode } from "react";
import { I18nextProvider } from "react-i18next";
import { useLocation } from "react-router";

import enCompetitors from "@/locales/en-US/competitors";
import enMarketing from "@/locales/en-US/marketing";
import enShowcaseDemo from "@/locales/en-US/showcase-demo";
import zhCompetitors from "@/locales/zh-CN/competitors";
import zhMarketing from "@/locales/zh-CN/marketing";
import zhShowcaseDemo from "@/locales/zh-CN/showcase-demo";

import { localeFromPathname, type SiteLocale } from "./site-locale";

export function createSiteI18n(locale: SiteLocale) {
  const instance = i18next.createInstance();
  void instance.init({
    debug: false,
    fallbackLng: "en-US",
    initAsync: false,
    interpolation: { escapeValue: false },
    lng: locale === "en" ? "en-US" : "zh-CN",
    resources: {
      "en-US": {
        translation: {
          competitors: enCompetitors,
          layout: { showcaseDemo: enShowcaseDemo },
          marketing: enMarketing,
        },
      },
      "zh-CN": {
        translation: {
          competitors: zhCompetitors,
          layout: { showcaseDemo: zhShowcaseDemo },
          marketing: zhMarketing,
        },
      },
    },
    supportedLngs: ["zh-CN", "en-US"],
  });
  return instance;
}

export function SiteI18nProvider({ children }: { children: ReactNode }) {
  const location = useLocation();
  const locale = localeFromPathname(location.pathname);
  const instance = useMemo(() => createSiteI18n(locale), [locale]);
  return <I18nextProvider i18n={instance}>{children}</I18nextProvider>;
}
