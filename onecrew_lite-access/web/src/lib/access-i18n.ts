import { useTranslation } from "react-i18next";

import accessI18n from "./i18n";
import type { AuthLocale } from "./auth-errors";

export function useAccessI18n() {
  const result = useTranslation("translation", { i18n: accessI18n });
  const locale: AuthLocale = result.i18n.language === "en-US" ? "en" : "zh";
  return { ...result, locale };
}

export function accessText(
  key: string,
  locale: AuthLocale,
  variables?: Record<string, unknown>,
) {
  return accessI18n.t(key, {
    lng: locale === "en" ? "en-US" : "zh-CN",
    ...variables,
  });
}
