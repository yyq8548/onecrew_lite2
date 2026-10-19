import { createContext, useContext, type ReactNode } from "react";
export type ScoreAssessment = { score: number | null; assessment: string | null; evidence?: { label: string; url: string }[] };
export type AppliedScore = { requirements: string; scores: Record<string, number | null>; assessments: Record<string, ScoreAssessment>; calculatedAt: string };
export type ScoreGrade = "excellent" | "good" | "medium" | "low" | "unrated";
export const SCORE_GRADE_ORDER = ["excellent", "good", "medium", "low", "unrated"] as const;

/** Derive a read-only display grade without changing the numeric score. */
export function scoreGrade(score: number | null): ScoreGrade {
  if (score === null) return "unrated";
  if (!Number.isInteger(score) || score < 0 || score > 100) {
    throw new RangeError("score must be an integer between 0 and 100");
  }
  if (score >= 90) return "excellent";
  if (score >= 80) return "good";
  if (score >= 60) return "medium";
  return "low";
}
export type ScoreRulesAdapter = {
  requirements: string | null;
  assessments: Record<string, ScoreAssessment>;
  names?: Record<string, string>;
  applied: AppliedScore | null;
  demo?: boolean;
  recalculate?: (requirements: string) => Promise<void>;
};
const Context = createContext<ScoreRulesAdapter | null>(null);
export const useScoreRules = () => useContext(Context);
export function ScoreRulesProvider({ value, children }: { value: ScoreRulesAdapter; children: ReactNode }) { return <Context.Provider value={value}>{children}</Context.Provider>; }
export function validRequirements(value: string): boolean { return !!value.trim() && value.length <= 4000; }
export function validAssessment(value: ScoreAssessment): boolean { return (value.score === null || (Number.isInteger(value.score) && value.score >= 0 && value.score <= 100)) && (value.assessment === null || typeof value.assessment === 'string'); }
