import enUS from "./locales/en-US";
import zhCN from "./locales/zh-CN";

export const DEFAULT_LOCALE = "zh-CN" as const;
export const SUPPORTED_LOCALES = [DEFAULT_LOCALE, "en-US"] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

export const NAMESPACES = [
  "accounts",
  "common",
  "connect",
  "inbox",
  "leads",
  "mail",
  "pairing",
  "sidebar",
  "shell",
  "updates",
] as const;
export type I18nNamespace = (typeof NAMESPACES)[number];
export const DEFAULT_NAMESPACE: I18nNamespace = "common";

export const resources = {
  "zh-CN": zhCN,
  "en-US": enUS,
} as const satisfies Record<SupportedLocale, Record<I18nNamespace, object>>;
