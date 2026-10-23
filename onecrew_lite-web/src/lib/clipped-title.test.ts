import { describe, expect, it } from "vitest";

import { isClipped } from "./clipped-title";

describe("isClipped", () => {
  it("reports clipping when content is wider than the box", () => {
    expect(isClipped({ scrollWidth: 200, clientWidth: 120 })).toBe(true);
  });

  it("reports nothing when the content fits", () => {
    expect(isClipped({ scrollWidth: 120, clientWidth: 120 })).toBe(false);
  });

  it("tolerates a 1px sub-pixel overflow", () => {
    // 两个值都是整数取整,差 1px 时文字其实完全看得见 —— 给它挂 tooltip
    // 就是噪音。
    expect(isClipped({ scrollWidth: 121, clientWidth: 120 })).toBe(false);
    expect(isClipped({ scrollWidth: 122, clientWidth: 120 })).toBe(true);
  });

  it("never reports clipping for an unrendered box", () => {
    expect(isClipped({ scrollWidth: 0, clientWidth: 0 })).toBe(false);
  });
});
