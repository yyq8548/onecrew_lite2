export type ScoreGrade = "excellent" | "good" | "medium" | "low" | "unrated";

/** Derive a read-only display grade without changing the numeric score. */
export function scoreGrade(score: number | null | undefined): ScoreGrade {
  if (score == null) return "unrated";
  if (!Number.isInteger(score) || score < 0 || score > 100) {
    throw new RangeError("score must be an integer between 0 and 100");
  }
  if (score >= 90) return "excellent";
  if (score >= 80) return "good";
  if (score >= 60) return "medium";
  return "low";
}
