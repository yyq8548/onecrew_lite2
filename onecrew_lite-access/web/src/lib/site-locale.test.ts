import { describe, expect, it } from "vitest";

import { localeFromPathname, localizedPath, stripLocalePrefix } from "./site-locale";

describe("public Site URL locale", () => {
  it("defaults to English and selects Chinese only for the /zh segment", () => {
    expect(localeFromPathname("/")).toBe("en");
    expect(localeFromPathname("/product")).toBe("en");
    expect(localeFromPathname("/zh/product")).toBe("zh");
    expect(localeFromPathname("/en/product")).toBe("en");
    expect(localeFromPathname("/english")).toBe("en");
  });

  it("switches locale without losing slug, query, or hash", () => {
    expect(localizedPath("/zh/product/ai-agent?source=nav#details", "en")).toBe(
      "/product/ai-agent?source=nav#details",
    );
    expect(localizedPath("/en/product/ai-agent?source=nav#details", "zh")).toBe(
      "/zh/product/ai-agent?source=nav#details",
    );
    expect(localizedPath("/zh?source=nav#details", "en")).toBe("/?source=nav#details");
    expect(localizedPath("/en/about", "en")).toBe("/about");
  });

  it("strips only the locale segment used by the outer Framework route", () => {
    expect(stripLocalePrefix("/zh")).toBe("/");
    expect(stripLocalePrefix("/zh/about")).toBe("/about");
    expect(stripLocalePrefix("/en")).toBe("/");
    expect(stripLocalePrefix("/en/about")).toBe("/about");
    expect(stripLocalePrefix("/english")).toBe("/english");
  });
});
