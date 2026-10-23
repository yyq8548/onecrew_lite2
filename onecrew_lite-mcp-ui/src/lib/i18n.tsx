import { createContext, useContext, type ReactNode } from "react";
import en from "@mcp-ui/locales/en-US";
import zh from "@mcp-ui/locales/zh-CN";

export type AppLocale = "en-US" | "zh-CN";
export type CopyKey = keyof typeof en;


export function translate(locale: AppLocale, key: CopyKey, values: Record<string, string | number> = {}): string {
  const text: string = (locale === "zh-CN" ? zh : en)[key];
  return text.replace(/\{(\w+)\}/g, (match, name: string) => name in values ? String(values[name]) : match);
}

const LocaleContext = createContext<AppLocale>("en-US");
export function AppLocaleProvider({ locale, children }: { locale: AppLocale; children: ReactNode }) {
  return <LocaleContext.Provider value={locale}><div lang={locale}>{children}</div></LocaleContext.Provider>;
}
export function useAppLocale() {
  const locale = useContext(LocaleContext);
  return { locale, t: (key: CopyKey, values?: Record<string, string | number>) => translate(locale, key, values) };
}
