import { invoke, isRuntimeAvailable } from "@/lib/local-runtime";
import type { SupportedLocale } from "./resources";

/** Serialize preference updates so a slow earlier invoke cannot win a rapid toggle. */
export function createLocaleSynchronizer(send: (locale: SupportedLocale) => Promise<void>) {
  let pending = Promise.resolve();
  let last: SupportedLocale | null = null;
  return (locale: SupportedLocale, force = false): Promise<void> => {
    const next = pending.catch(() => undefined).then(async () => {
      if (!force && last === locale) return;
      await send(locale);
      last = locale;
    });
    pending = next;
    return next;
  };
}

const synchronize = createLocaleSynchronizer((locale) => invoke("set_application_locale", { locale }));

export function syncPresentationLocale(locale: SupportedLocale, force = false): Promise<void> {
  return typeof window !== "undefined" && isRuntimeAvailable() ? synchronize(locale, force) : Promise.resolve();
}
