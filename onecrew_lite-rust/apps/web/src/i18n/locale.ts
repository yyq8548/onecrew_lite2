
import {
  DEFAULT_LOCALE,
  SUPPORTED_LOCALES,
  type SupportedLocale,
} from "./resources";

export const LOCALE_STORAGE_KEY = "onecrew-locale";

export function isSupportedLocale(value: unknown): value is SupportedLocale {
  return typeof value === "string"
    && (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

/** Maps a platform BCP-47 locale to the UI catalogs that ship with OneCrew. */
export function normalizeLocale(value: unknown): SupportedLocale | null {
  if (typeof value !== "string") return null;
  const language = value.trim().replaceAll("_", "-").split("-", 1)[0]?.toLowerCase();
  if (language === "zh") return "zh-CN";
  if (language === "en") return "en-US";
  return null;
}

export type InitialLocaleCandidates = {
  storedLocale?: unknown;
  navigatorLanguages?: readonly unknown[];
};

/**
 * An explicit stored preference is intentionally stricter than environment
 * detection: only a locale that this build can render is allowed to override
 * the browser choices.
 */
export function resolveInitialLocale({
  storedLocale,
  navigatorLanguages = [],
}: InitialLocaleCandidates): SupportedLocale {
  if (isSupportedLocale(storedLocale)) return storedLocale;

  for (const language of navigatorLanguages) {
    const normalized = normalizeLocale(language);
    if (normalized) return normalized;
  }

  return DEFAULT_LOCALE;
}

function localStorageOrNull(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function readStoredLocale(): unknown {
  try {
    return localStorageOrNull()?.getItem(LOCALE_STORAGE_KEY) ?? null;
  } catch {
    return null;
  }
}

function readNavigatorLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  try {
    return navigator.languages.length > 0
      ? navigator.languages
      : navigator.language
        ? [navigator.language]
        : [];
  } catch {
    return [];
  }
}

export async function detectInitialLocale(): Promise<SupportedLocale> {
  const storedLocale = readStoredLocale();
  if (isSupportedLocale(storedLocale)) return storedLocale;

  return resolveInitialLocale({
    storedLocale,
    navigatorLanguages: readNavigatorLanguages(),
  });
}

export function persistLocale(locale: SupportedLocale, storage = localStorageOrNull()): void {
  try {
    storage?.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // A privacy-restricted localStorage should not make language switching fail.
  }
}
