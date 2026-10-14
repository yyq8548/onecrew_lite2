import { Globe, Languages } from "lucide-react";
import { useSearchParams } from "react-router";
import { Button } from "@/components/ui/button";
import { useAccessI18n } from "@/lib/access-i18n";
import { safeDistributionReturnTo } from "@/lib/distribution-return";

export function AccessLanguageToggle({ inverse = false, iconOnly = false }: { inverse?: boolean; iconOnly?: boolean }) {
  const { i18n, t } = useAccessI18n();
  const [searchParams, setSearchParams] = useSearchParams();
  const english = i18n.language === "en-US";

  return (
    <Button
      type="button"
      size="sm"
      variant="ghost"
      className={inverse
        ? "gap-1.5 text-white hover:bg-white/10 hover:text-white"
        : "gap-1.5 text-muted-foreground hover:text-foreground"}
      aria-label={english ? t("common.switchToChinese") : t("common.switchToEnglish")}
      onClick={() => {
        const language = english ? "zh-CN" : "en-US";
        const next = new URLSearchParams(searchParams);
        next.set("lng", language);
        const returnTo = safeDistributionReturnTo(next.get("returnTo"));
        if (returnTo?.startsWith("/account")) {
          const destination = new URL(returnTo, "https://access.invalid");
          destination.searchParams.set("lng", language);
          next.set("returnTo", `${destination.pathname}${destination.search}`);
        }
        setSearchParams(next, { replace: true });
        void i18n.changeLanguage(language);
      }}
    >
      {iconOnly ? <Globe aria-hidden="true" /> : <Languages aria-hidden="true" />}
      {iconOnly ? null : english ? "中文" : "EN"}
    </Button>
  );
}
