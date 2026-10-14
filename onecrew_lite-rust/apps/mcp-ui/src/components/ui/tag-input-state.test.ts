import { describe, expect, it } from "vitest";
import { appendTags, tagCommitKey } from "@mcp-ui/components/ui/tag-input-state";

describe("local tag drafts", () => {
  it("preserves multiword phrases, trims edges, ignores empties and stable-deduplicates", () => {
    expect(appendTags(["crypto México"], " crypto México, RWA México\n中文  创作者,,", 50, 80)).toEqual({ values: ["crypto México", "RWA México", "中文  创作者"], error: null });
  });
  it("reports an already added keyword without changing tags, including at the limit", () => {
    expect(appendTags(["cooking"], " cooking ", 1, 80)).toEqual({ values: ["cooking"], error: "duplicate" });
  });
  it("rejects oversized batches atomically without losing existing values", () => {
    expect(appendTags(["kept"], "new,other", 2, 80)).toEqual({ values: ["kept"], error: "count" });
    expect(appendTags(["kept"], "new," + "🚀".repeat(81), 50, 80)).toEqual({ values: ["kept"], error: "length" });
    expect(appendTags([], "🚀".repeat(80), 50, 80).error).toBeNull();
  });
  it("never commits IME confirmation keys or splits on whitespace", () => {
    expect(tagCommitKey("Enter", true, 13)).toBe(false);
    expect(tagCommitKey("Enter", false, 229)).toBe(false);
    expect(tagCommitKey(",", true, 188)).toBe(false);
    expect(tagCommitKey(" ", false, 32)).toBe(false);
    expect(tagCommitKey("Enter", false, 13)).toBe(true);
    expect(tagCommitKey(",", false, 188)).toBe(true);
  });

  it("deduplicates against the full collection and preserves values outside the visible page", () => {
    const all = Array.from({ length: 200 }, (_, index) => `multi word ${index}`);
    const result = appendTags(all, "multi word 199, new phrase", 201, 80);
    expect(result.error).toBeNull();
    expect(result.values).toHaveLength(201);
    expect(result.values.slice(0, 200)).toEqual(all);
    expect(result.values.at(-1)).toBe("new phrase");
  });
});
