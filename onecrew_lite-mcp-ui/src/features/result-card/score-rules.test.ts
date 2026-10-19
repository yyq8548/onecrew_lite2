import { describe, expect, it } from "vitest";
import { scoreGrade, validRequirements, validAssessment } from "./score-rules";
it('requires meaningful bounded evaluation requirements',()=>{
 expect(validRequirements('优先美国护肤达人')).toBe(true);
 expect(validRequirements(' \n ')).toBe(false);
 expect(validRequirements('a'.repeat(4001))).toBe(false);
});
it('keeps zero and unscored distinct and rejects invalid agent scores',()=>{
 expect(validAssessment({score:0,assessment:'Low fit'})).toBe(true);
 expect(validAssessment({score:null,assessment:null})).toBe(true);
 for(const score of [-1,101,0.5,NaN,Infinity]) expect(validAssessment({score,assessment:null})).toBe(false);
});

describe("score display grades", () => {
  it("uses the confirmed four-band boundaries and keeps null unrated", () => {
    const cases = [
      [null, "unrated"],
      [0, "low"],
      [59, "low"],
      [60, "medium"],
      [79, "medium"],
      [80, "good"],
      [89, "good"],
      [90, "excellent"],
      [100, "excellent"],
      [95, "excellent"],
      [85, "good"],
      [70, "medium"],
      [40, "low"],
      [87, "good"],
    ] as const;
    for (const [score, expected] of cases) expect(scoreGrade(score), String(score)).toBe(expected);
  });

  it("does not clamp malformed scores into a display grade", () => {
    for (const score of [-1, 101, 0.5, NaN, Infinity] as number[]) {
      expect(() => scoreGrade(score)).toThrow(RangeError);
    }
  });
});
