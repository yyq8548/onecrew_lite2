import { useTranslation } from "react-i18next";

import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { scoreGrade, type ScoreGrade } from "@/lib/score-grade";
import { cn } from "@/lib/utils";

const SCORE_GRADE_KEYS: Record<ScoreGrade, "scoreGrades.excellent" | "scoreGrades.good" | "scoreGrades.medium" | "scoreGrades.low" | "scoreGrades.unrated"> = {
  excellent: "scoreGrades.excellent",
  good: "scoreGrades.good",
  medium: "scoreGrades.medium",
  low: "scoreGrades.low",
  unrated: "scoreGrades.unrated",
};

const SCORE_GRADE_TONE: Record<ScoreGrade, string> = {
  excellent: "bg-[var(--tone-success-bg)] text-[var(--tone-success-fg)]",
  good: "bg-[var(--tone-info-bg)] text-[var(--tone-info-fg)]",
  medium: "bg-[var(--tone-warning-bg)] text-[var(--tone-warning-fg)]",
  low: "bg-[var(--tone-danger-bg)] text-[var(--tone-danger-fg)]",
  unrated: "bg-muted text-muted-foreground",
};

/** Show the score's grade while keeping the stored score and assessment untouched. */
export function CreatorScoreBadge({ score, assessment }: { score?: number | null; assessment?: string | null }) {
  const { t } = useTranslation("leads");
  const grade = scoreGrade(score);
  const label = t(SCORE_GRADE_KEYS[grade]);
  const accessibleLabel = t("scoreGradeLabel", { grade: label });
  const badge = (
    <Badge
      variant="secondary"
      aria-label={accessibleLabel}
      className={cn("border-transparent font-semibold", SCORE_GRADE_TONE[grade])}
    >
      {label}
    </Badge>
  );
  if (!assessment) return badge;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{badge}</TooltipTrigger>
      <TooltipContent side="top" className="max-w-80 whitespace-pre-wrap text-left">
        {assessment}
      </TooltipContent>
    </Tooltip>
  );
}
