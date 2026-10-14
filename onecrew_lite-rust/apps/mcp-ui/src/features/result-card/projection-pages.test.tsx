import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { parseResultSnapshot } from "@mcp-ui/lib/result-contract";
import { projectionRows } from "@mcp-ui/features/result-card/projection-pages";
import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { detailPages } from "@mcp-ui/features/result-card/choice-state";
import legacy from "@mcp-ui/test-support/creator-save-legacy-wire.json";

describe("bounded fallback for arbitrary projections", () => {
  it("retains all 78 observed fields and renders every one of them", () => {
    const snapshot = parseResultSnapshot(legacy);
    if (!snapshot) throw new Error("Invalid anonymized legacy fixture");
    const rows = projectionRows(snapshot.presentation);
    expect(rows.filter((row) => row.value.startsWith("Example value "))).toHaveLength(78);
    expect(rows.some((row) => row.value === "Example value 78")).toBe(true);
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, onConfirm: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn(), onSelect: vi.fn() }));
    expect(html).toContain("Example value 1");
    expect(html).toContain("Example value 78");
    expect(html).not.toContain(snapshot.locale === "zh-CN" ? "下一页" : "Next page");
  });

  it("preserves newline-heavy and CJK details losslessly without unbounded whitespace pages", () => {
    const text = "\n".repeat(300) + "复杂文本🚀".repeat(150);
    const pages = detailPages(text);
    expect(pages.join("")).toBe(text);
    for (const page of pages) { expect(Array.from(page).length).toBeLessThanOrEqual(160); expect((page.match(/\n/g) ?? []).length).toBeLessThanOrEqual(6); }
  });
});
