import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CreatorRowDetail } from "./components/creator-review-list";
import { CREATOR_SAVE_PREVIEW } from "@mcp-ui/test-support/creator-save-fixture";
import { AppLocaleProvider } from "@mcp-ui/lib/i18n";
import { ResultTextDetailsProvider } from "./components/text-details";

function detail(sourceUrl: string | null, locale: "en-US" | "zh-CN") {
  const preview = {
    ...CREATOR_SAVE_PREVIEW,
    profile_url: "https://x.com/researcher",
    source_url: sourceUrl,
    score: 87,
    assessment: "演示了任务拆解与工具调用。\n局限：仅观察到一个实例。原记录为 87 分。",
  };
  return renderToStaticMarkup(createElement(AppLocaleProvider, { locale,
    children: createElement(ResultTextDetailsProvider, { revision: 1, children: createElement(CreatorRowDetail, {
      preview, selected: true, optional: [], disabled: false, onRoute() {}, onClose() {},
    }) }),
  }));
}

describe("candidate evidence in the save review", () => {
  it("bounds the explanation through the card's existing paged text disclosure", () => {
    expect(detail(null, "en-US")).toContain("line-clamp-3");
  });
  it.each(["en-US", "zh-CN"] as const)("keeps the individual explanation and original public links in %s", locale => {
    const html = detail("https://x.com/researcher/status/123", locale);
    expect(html).toContain('href="https://x.com/researcher/status/123"');
    expect(html).toContain('href="https://x.com/researcher"');
    expect(html).toContain("演示了任务拆解与工具调用。\n局限：仅观察到一个实例。原记录为 87 分。");
    expect(html).toContain(locale === "zh-CN" ? "内容依据" : "Content evidence");
  });

  it.each([null, "javascript:alert(1)", "https://user:password@example.com/private"])("never turns an absent or unsafe source into a link: %s", sourceUrl => {
    const html = detail(sourceUrl, "en-US");
    expect(html).not.toContain("javascript:");
    expect(html).not.toContain("user:password");
    expect(html).toContain("原记录为 87 分。");
  });
});
