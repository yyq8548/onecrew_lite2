import { describe, expect, it } from "vitest";

import {
  LOCALE_STORAGE_KEY,
  isSupportedLocale,
  normalizeLocale,
  persistLocale,
  resolveInitialLocale,
} from "./locale";

describe("browser locale selection", () => {
  it("accepts only catalogs that ship as persisted preferences", () => {
    expect(isSupportedLocale("zh-CN")).toBe(true);
    expect(isSupportedLocale("en-US")).toBe(true);
    expect(isSupportedLocale("en-GB")).toBe(false);
    expect(isSupportedLocale("EN-us")).toBe(false);
  });

  it("normalizes supported language families from browser locales", () => {
    expect(normalizeLocale("zh_HK")).toBe("zh-CN");
    expect(normalizeLocale("ZH-Hant-TW")).toBe("zh-CN");
    expect(normalizeLocale("en-GB")).toBe("en-US");
    expect(normalizeLocale("de-DE")).toBeNull();
  });

  it("uses stored preference, then navigator, then Chinese", () => {
    expect(resolveInitialLocale({
      storedLocale: "en-US",
      navigatorLanguages: ["zh-CN"],
    })).toBe("en-US");
    expect(resolveInitialLocale({
      storedLocale: "invalid",
      navigatorLanguages: ["en-GB", "zh-CN"],
    })).toBe("en-US");
    expect(resolveInitialLocale({
      storedLocale: "en-GB",
      navigatorLanguages: ["zh-HK", "en-US"],
    })).toBe("zh-CN");
    expect(resolveInitialLocale({
      navigatorLanguages: ["de-DE", "zh-HK", "en-US"],
    })).toBe("zh-CN");
    expect(resolveInitialLocale({
      navigatorLanguages: ["de-DE"],
    })).toBe("zh-CN");
  });

  it("persists only the explicit supported selection", () => {
    const values = new Map<string, string>();
    const storage = {
      setItem(key: string, value: string) {
        values.set(key, value);
      },
    } as Storage;

    persistLocale("en-US", storage);

    expect(values.get(LOCALE_STORAGE_KEY)).toBe("en-US");
  });
});
