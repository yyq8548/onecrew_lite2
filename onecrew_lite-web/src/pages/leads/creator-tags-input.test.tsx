import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import zhCNLeads from "@/i18n/locales/zh-CN/leads";
import enUSLeads from "@/i18n/locales/en-US/leads";
import { CreatorTagsBadges, CreatorTagsInput } from "./creator-tags-input";

describe("Creator tags presentation", () => {
  for (const [locale, catalog] of [["zh-CN", zhCNLeads], ["en-US", enUSLeads]] as const) {
    it(`renders localized accessible controls and Unicode count in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["leads"], defaultNS: "leads", resources: { [locale]: { leads: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
        <CreatorTagsInput value={{ tags: ["Art"], text: "😀".repeat(12) }} onChange={() => {}} error="tags.validation.duplicate" />
      </I18nextProvider>);
      expect(markup).toContain(catalog.tags.label);
      expect(markup).toContain(catalog.tags.hint);
      expect(markup).toContain(catalog.tags.add);
      expect(markup).toContain(catalog.tags.validation.duplicate);
      expect(markup).toContain("12/12");
      expect(markup).toContain('type="button"');
      expect(markup).toContain('aria-invalid="true"');
      expect(markup).not.toContain("maxLength");
      expect(markup).not.toContain("maxlength");
    });
  }

  it("renders every saved tag in order, including long Unicode tags, without a collapsed count chip", () => {
    const markup = renderToStaticMarkup(<CreatorTagsBadges tags={["设计", "AI", "😀".repeat(12)]} />);
    expect(markup.indexOf("设计")).toBeLessThan(markup.indexOf("AI"));
    expect(markup).toContain("😀".repeat(12));
    expect(markup).not.toContain("+2");
  });
});
