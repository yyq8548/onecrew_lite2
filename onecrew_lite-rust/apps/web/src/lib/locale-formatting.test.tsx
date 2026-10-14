import { createInstance } from "i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it } from "vitest";

import DATE_FILTER_SOURCE from "@/components/data-table/date-filter.tsx?raw";
import { Calendar } from "@/components/ui/calendar";
import CALENDAR_SOURCE from "@/components/ui/calendar.tsx?raw";
import { appI18n } from "@/i18n";
import {
  DEFAULT_LOCALE,
  NAMESPACES,
  resources,
  SUPPORTED_LOCALES,
  type SupportedLocale,
} from "@/i18n/resources";
import { formatCount, formatDate } from "@/lib/creator";
import CREATOR_SOURCE from "@/lib/creator.ts?raw";

const ENGLISH_LOCALE = SUPPORTED_LOCALES[1];

function expectedDate(epoch: number, locale: SupportedLocale): string {
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(new Date(epoch * 1000));
}

function expectedCount(value: number, locale: SupportedLocale): string {
  return new Intl.NumberFormat(locale, {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(value);
}

async function renderCalendar(locale: SupportedLocale): Promise<string> {
  const instance = createInstance();
  await instance.init({
    defaultNS: "common",
    fallbackLng: DEFAULT_LOCALE,
    interpolation: { escapeValue: false },
    lng: locale,
    ns: NAMESPACES,
    resources,
  });

  return renderToStaticMarkup(
    <I18nextProvider i18n={instance}>
      <Calendar defaultMonth={new Date(2026, 8, 1)} />
    </I18nextProvider>,
  );
}

describe.sequential("locale-aware desktop formatting", () => {
  it("formats dates and compact counts differently for explicit Chinese and English locales", () => {
    const epoch = new Date(2026, 8, 5, 14, 7).getTime() / 1000;
    const count = 12_345;

    expect(formatDate(epoch, DEFAULT_LOCALE)).toBe(expectedDate(epoch, DEFAULT_LOCALE));
    expect(formatDate(epoch, ENGLISH_LOCALE)).toBe(expectedDate(epoch, ENGLISH_LOCALE));
    expect(formatDate(epoch, DEFAULT_LOCALE)).not.toBe(formatDate(epoch, ENGLISH_LOCALE));

    expect(formatCount(count, DEFAULT_LOCALE)).toBe(expectedCount(count, DEFAULT_LOCALE));
    expect(formatCount(count, ENGLISH_LOCALE)).toBe(expectedCount(count, ENGLISH_LOCALE));
    expect(formatCount(count, DEFAULT_LOCALE)).not.toBe(formatCount(count, ENGLISH_LOCALE));
    expect(formatCount(Number.NaN, ENGLISH_LOCALE)).toBe("0");
    expect(formatCount(Number.POSITIVE_INFINITY, DEFAULT_LOCALE)).toBe("0");
  });

  it("falls back to Chinese before initialization and follows actual language changes by default", async () => {
    const epoch = new Date(2026, 8, 5, 14, 7).getTime() / 1000;
    const count = 12_345;
    const previousLanguage = appI18n.resolvedLanguage;

    expect(formatDate(epoch)).toBe(formatDate(epoch, DEFAULT_LOCALE));
    expect(formatCount(count)).toBe(formatCount(count, DEFAULT_LOCALE));

    if (!appI18n.isInitialized) {
      await appI18n.init({
        defaultNS: "common",
        fallbackLng: DEFAULT_LOCALE,
        interpolation: { escapeValue: false },
        lng: DEFAULT_LOCALE,
        ns: NAMESPACES,
        resources,
      });
    }

    try {
      await appI18n.changeLanguage(DEFAULT_LOCALE);
      const chineseDate = formatDate(epoch);
      const chineseCount = formatCount(count);

      await appI18n.changeLanguage(ENGLISH_LOCALE);
      expect(formatDate(epoch)).toBe(formatDate(epoch, ENGLISH_LOCALE));
      expect(formatCount(count)).toBe(formatCount(count, ENGLISH_LOCALE));
      expect(formatDate(epoch)).not.toBe(chineseDate);
      expect(formatCount(count)).not.toBe(chineseCount);
    } finally {
      await appI18n.changeLanguage(previousLanguage ?? DEFAULT_LOCALE);
    }
  });

  it("renders calendar month, weekday, and day labels with the application locale", async () => {
    const chineseMarkup = await renderCalendar(DEFAULT_LOCALE);
    const englishMarkup = await renderCalendar(ENGLISH_LOCALE);

    expect(chineseMarkup).toContain("2026年9月");
    expect(chineseMarkup).toContain("星期日");
    expect(chineseMarkup).toContain("2026年9月1日 星期二");
    expect(chineseMarkup).toContain("前往上个月");
    expect(englishMarkup).toContain("September 2026");
    expect(englishMarkup).toContain("Sunday");
    expect(englishMarkup).toContain("Tuesday, September 1st, 2026");
    expect(chineseMarkup).not.toBe(englishMarkup);
  });

  it("has no browser-default or fixed-Chinese locale formatting path", () => {
    const formatterSource = CREATOR_SOURCE.slice(
      CREATOR_SOURCE.indexOf("function formattingLocale"),
      CREATOR_SOURCE.indexOf("// ===== Reply triage"),
    );

    expect(formatterSource).not.toContain('"zh-CN"');
    expect(DATE_FILTER_SOURCE).toContain("i18n.resolvedLanguage");
    expect(DATE_FILTER_SOURCE).not.toContain('"zh-CN"');
    expect(CALENDAR_SOURCE).toContain('from "react-day-picker/locale/en-US"');
    expect(CALENDAR_SOURCE).toContain('from "react-day-picker/locale/zh-CN"');
    expect(CALENDAR_SOURCE).not.toContain('from "react-day-picker/locale"');
    expect(CALENDAR_SOURCE).toContain("locale={localeOverride ?? DAY_PICKER_LOCALES[appLocale]}");
    expect(CALENDAR_SOURCE).not.toContain('"zh-CN"');
    expect(CALENDAR_SOURCE).not.toContain('toLocaleString("default"');
    expect(CALENDAR_SOURCE).not.toContain("toLocaleDateString()");
    expect(CALENDAR_SOURCE).toContain('data-day={format(day.date, "yyyy-MM-dd")}');
  });
});
