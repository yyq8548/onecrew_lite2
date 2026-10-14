import i18next from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";

import enUS from "@/locales/en-US";
import zhCN from "@/locales/zh-CN";

const accessI18n = i18next.createInstance();
const isTest = import.meta.env.MODE === "test";
void accessI18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    debug: false,
    detection: {
      // Node 22 exposes an experimental localStorage getter that emits a
      // warning without --localstorage-file. Tests use the deterministic
      // default language and never need browser persistence.
      caches: isTest ? [] : ["localStorage"],
      lookupLocalStorage: "onecrew-locale",
      lookupQuerystring: "lng",
      // Explicit language from the marketing site takes precedence over a
      // previous Access preference, then persists for subsequent visits.
      order: isTest ? [] : ["querystring", "localStorage"],
    },
    fallbackLng: "zh-CN",
    interpolation: { escapeValue: false },
    resources: {
      "en-US": { translation: enUS },
      "zh-CN": { translation: zhCN },
    },
    supportedLngs: ["zh-CN", "en-US"],
  });

export default accessI18n;
