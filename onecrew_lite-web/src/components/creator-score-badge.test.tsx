import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import zhCNLeads from "@/i18n/locales/zh-CN/leads";
import enUSLeads from "@/i18n/locales/en-US/leads";
import { CreatorScoreBadge } from "./creator-score-badge";

describe("Creator score badge", () => {
  for (const [locale, catalog] of [["zh-CN", zhCNLeads], ["en-US", enUSLeads]] as const) {
    it(`renders every score grade and an accessible grade name in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["leads"], defaultNS: "leads", resources: { [locale]: { leads: catalog } }, interpolation: { escapeValue: false } });
      const scores = [null, 0, 59, 60, 79, 80, 89, 90, 100, 87] as const;
      const markup = scores.map((score) => renderToStaticMarkup(
        <I18nextProvider i18n={i18n}><CreatorScoreBadge score={score} /></I18nextProvider>,
      )).join("\n");

      for (const label of Object.values(catalog.scoreGrades)) expect(markup).toContain(label);
      expect(markup).toContain(i18n.t("scoreGradeLabel", { grade: `87 · ${catalog.scoreGrades.good}` }));
      expect(markup).toContain(`87 · ${catalog.scoreGrades.good}`);
      expect(markup).toContain(`90 · ${catalog.scoreGrades.excellent}`);
      expect(markup).not.toContain(">87<");
      expect(markup).not.toContain(">90<");
    });
  }
});
