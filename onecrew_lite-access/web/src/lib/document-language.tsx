import { useEffect } from "react";

import { useAccessI18n } from "@/lib/access-i18n";

export function DocumentLanguage() {
  const { i18n } = useAccessI18n();

  useEffect(() => {
    document.documentElement.lang = i18n.language === "en-US" ? "en" : "zh-CN";
  }, [i18n.language]);

  return null;
}
