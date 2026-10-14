import { describe, expect, it } from "vitest";
import { TEXT_PAGE_CHARACTERS, TEXT_PAGE_LINES, textPages } from "@mcp-ui/features/result-card/text-pages";

describe("bounded text paging", () => {
  it("paginates full text losslessly across CJK, emoji, long words and newlines", () => {
    for (const text of ["", "汉字🚀".repeat(500), "a".repeat(8000), "\n".repeat(100), "x\r\ny".repeat(100)]) {
      const pages = textPages(text);
      expect(pages.join("")).toBe(text);
      for (const page of pages) {
        expect(Array.from(page).length).toBeLessThanOrEqual(TEXT_PAGE_CHARACTERS);
        expect(page.match(/[\r\n]/g)?.length ?? 0).toBeLessThanOrEqual(TEXT_PAGE_LINES);
        expect(page).not.toMatch(/^[\uDC00-\uDFFF]|[\uD800-\uDBFF]$/u);
      }
    }
  });

  it("returns exactly one empty page for empty text and never an empty trailing page", () => {
    expect(textPages("")).toEqual([""]);
    expect(textPages("a".repeat(TEXT_PAGE_CHARACTERS))).toEqual(["a".repeat(TEXT_PAGE_CHARACTERS)]);
    expect(textPages("a".repeat(TEXT_PAGE_CHARACTERS + 1))).toEqual(["a".repeat(TEXT_PAGE_CHARACTERS), "a"]);
  });
});
