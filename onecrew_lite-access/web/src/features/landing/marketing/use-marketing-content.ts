import { useAccessI18n } from "@/lib/access-i18n";

import type { ComparisonLocaleContent } from "./competitors";
import type { MarketingContent } from "./content/types";

export type MarketingLocale = "en" | "zh";

export function useMarketingContent() {
  const { i18n, t } = useAccessI18n();
  const locale: MarketingLocale = i18n.language === "en-US" ? "en" : "zh";
  const content = t("marketing", { returnObjects: true }) as unknown as MarketingContent;
  const comparisonContent = t("competitors", {
    returnObjects: true,
  }) as unknown as ComparisonLocaleContent;

  return { comparisonContent, content, locale };
}
