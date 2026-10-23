import { describe, expect, it } from "vitest";
import { scoreGrade } from "./score-grade";

describe("Web score display grade", () => {
  it("uses the same boundaries as the MCP App", () => {
    const cases = [
      [null, "unrated"],
      [0, "low"], [59, "low"],
      [60, "medium"], [79, "medium"],
      [80, "good"], [89, "good"],
      [90, "excellent"], [100, "excellent"],
      [95, "excellent"], [85, "good"], [70, "medium"], [40, "low"], [87, "good"],
    ] as const;
    for (const [score, expected] of cases) expect(scoreGrade(score), String(score)).toBe(expected);
    expect(scoreGrade(undefined)).toBe("unrated");
  });

  it("rejects malformed scores instead of clamping them", () => {
    for (const score of [-1, 101, 0.5, NaN, Infinity] as number[]) {
      expect(() => scoreGrade(score)).toThrow(RangeError);
    }
  });
});
