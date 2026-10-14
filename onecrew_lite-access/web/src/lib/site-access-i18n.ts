import { useTranslation } from "react-i18next";

export function useAccessI18n() {
  const result = useTranslation("translation");
  const locale = result.i18n.language === "en-US" ? ("en" as const) : ("zh" as const);
  return { ...result, locale };
}
