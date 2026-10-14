import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { AppLocaleProvider, translate } from "@mcp-ui/lib/i18n";
import { ScoreRulePanel } from "./components/score-rule-panel";
import { ScoreRulesProvider, type ScoreRulesAdapter } from "./score-rules";

// This panel is a portal. Keep this focused presentation test DOM-free while
// still rendering the actual panel tree; interaction tests own the real Host.
vi.mock("react-dom", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-dom")>();
  return { ...actual, createPortal: (children: unknown) => children };
});
vi.stubGlobal("document", { body: {} });

const scoreRules: ScoreRulesAdapter = {
  requirements: "近期内容与美国护肤相关",
  assessments: {
    creator: { score: 87, assessment: "保留这段逐人评价；其中的 87 不应被改写。" },
  },
  applied: null,
};

describe("score rules presentation", () => {
  it("shows all four bands and an explicit unrated state in the real details panel", () => {
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, {
      locale: "en-US",
      children: createElement(ScoreRulesProvider, {
        value: scoreRules,
        children: createElement(ScoreRulePanel, { onClose() {} }),
      }),
    }));

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="score-rule-heading"');
    for (const key of [
      "scoreGradeExcellent", "scoreGradeGood", "scoreGradeMedium", "scoreGradeLow", "scoreGradeUnrated",
      "scoreBandExcellent", "scoreBandGood", "scoreBandMedium", "scoreBandLow",
    ] as const) expect(html).toContain(translate("en-US", key));
    expect(html).toContain(scoreRules.assessments.creator!.assessment!);
    expect(html).toContain(scoreRules.requirements!);
  });

  it("keeps the local reassessment input available without changing its draft contract", () => {
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, {
      locale: "zh-CN",
      children: createElement(ScoreRulesProvider, {
        value: { ...scoreRules, recalculate: async () => {} },
        children: createElement(ScoreRulePanel, { onClose() {} }),
      }),
    }));

    expect(html).toContain(translate("zh-CN", "scoreRequirementsLabel"));
    expect(html).toContain(translate("zh-CN", "scoreReassess"));
    expect(html).toContain('maxLength="4000"');
  });
});
