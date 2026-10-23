import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import en from "@mcp-ui/locales/en-US";
import zh from "@mcp-ui/locales/zh-CN";
import { translate } from "@mcp-ui/lib/i18n";
import { ResultCardView } from "@mcp-ui/features/result-card";
import { choiceSnapshot } from "@mcp-ui/test-support/fixtures";

describe("OneCrew-owned MCP App locale", () => {
  it("keeps both separate catalogs complete", () => {
    expect(Object.keys(zh).sort()).toEqual(Object.keys(en).sort());
    expect(translate("en-US", "questionProgress", { current: 2, total: 4 })).toBe("Question 2 of 4");
    expect(translate("zh-CN", "questionProgress", { current: 2, total: 4 })).toBe("第 2 / 4 题");
  });
  it("changes only fixed UI copy and preserves the server's question text", () => {
    const handlers = { onConfirm: vi.fn(), onSelect: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn() };
    const snapshot = choiceSnapshot();
    const chinese = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
    const english = renderToStaticMarkup(createElement(ResultCardView, { snapshot: { ...snapshot, locale: "en-US" }, busy: false, ...handlers }));
    expect(chinese).toContain("下一题");
    expect(english).toContain("Next");
    expect(english).toContain("Type my own answer");
    expect(english).toContain("请选择保存范围");
    expect(english).not.toContain("预览答案");
    expect(handlers.onSelect).not.toHaveBeenCalled();
  });
});
