import * as acceptLanguageParser from "accept-language-parser";

import type { EmailLocale } from "./email-types";

export const DEFAULT_EMAIL_LOCALE: EmailLocale = "zh-CN";
const SUPPORTED_EMAIL_LOCALES = ["zh-CN", "en-US"] as const;

export function resolveEmailLocale(request?: Request): EmailLocale {
  const header = request?.headers.get("accept-language");
  if (!header) return DEFAULT_EMAIL_LOCALE;
  return acceptLanguageParser.pick(SUPPORTED_EMAIL_LOCALES, header, { loose: true })
    ?? DEFAULT_EMAIL_LOCALE;
}
