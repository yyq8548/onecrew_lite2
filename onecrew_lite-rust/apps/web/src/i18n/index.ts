import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import { detectInitialLocale, normalizeLocale } from "./locale";
import { syncPresentationLocale } from "./presentation-locale";
import { isRuntimeAvailable, subscribeRuntimeAvailability } from "@/lib/local-runtime";
import {
  DEFAULT_LOCALE,
  DEFAULT_NAMESPACE,
  NAMESPACES,
  resources,
} from "./resources";

export const appI18n = i18next.createInstance();

let initialization: Promise<void> | null = null;

function applyDocumentLocale(locale: string): void {
  if (typeof document !== "undefined") document.documentElement.lang = locale;
}

async function applyPresentationLocale(locale: string, force = false): Promise<void> {
  const supported = normalizeLocale(locale);
  if (!supported) return;
  try { await syncPresentationLocale(supported, force); }
  catch { console.warn("onecrew_lite could not synchronize the MCP App presentation locale."); }
}

export function initializeI18n(): Promise<void> {
  if (appI18n.isInitialized) return Promise.resolve();
  if (initialization) return initialization;

  initialization = (async () => {
    const initialLocale = await detectInitialLocale();
    subscribeRuntimeAvailability(() => {
      if (isRuntimeAvailable()) void applyPresentationLocale(appI18n.resolvedLanguage ?? initialLocale, true);
    });
    appI18n.on("languageChanged", (locale) => {
      applyDocumentLocale(locale);
      void applyPresentationLocale(locale);
    });
    await appI18n.use(initReactI18next).init({
      defaultNS: DEFAULT_NAMESPACE,
      fallbackLng: DEFAULT_LOCALE,
      interpolation: { escapeValue: false },
      lng: initialLocale,
      load: "currentOnly",
      nonExplicitSupportedLngs: false,
      ns: NAMESPACES,
      resources,
      supportedLngs: Object.keys(resources),
    });
    applyDocumentLocale(appI18n.resolvedLanguage ?? initialLocale);
    await applyPresentationLocale(appI18n.resolvedLanguage ?? initialLocale);
  })();

  return initialization;
}

export { persistLocale } from "./locale";
export type { SupportedLocale } from "./resources";
