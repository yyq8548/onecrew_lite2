import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { scoreGrade, type ScoreGrade, useScoreRules } from "../score-rules";
import { ScoreRulePanel } from "./score-rule-panel";
import { useAppLocale, type CopyKey } from "@mcp-ui/lib/i18n";

const SCORE_GRADE_KEYS: Record<ScoreGrade, CopyKey> = {
  excellent: "scoreGradeExcellent",
  good: "scoreGradeGood",
  medium: "scoreGradeMedium",
  low: "scoreGradeLow",
  unrated: "scoreGradeUnrated",
};

export function creatorAssessmentText(assessment: string | null | undefined, fallback: string): string {
  return assessment?.trim() ? assessment : fallback;
}

/** A row's score explains itself with that Creator's assessment; the header label explains the scale. */
export function creatorScoreTooltipText(valueOnly: boolean, assessment: string | null | undefined, t: (key: "creatorNoAssessment" | "scoreScaleHelp") => string): string {
  return valueOnly ? creatorAssessmentText(assessment, t("creatorNoAssessment")) : t("scoreScaleHelp");
}

export function CreatorScore({ score, labelOnly = false, valueOnly = false, subjectId, assessment }: { score: number | null; labelOnly?: boolean; valueOnly?: boolean; subjectId?: string; assessment?: string | null }) {
  const { t } = useAppLocale();
  const api = useScoreRules();
  const [panel,setPanel] = useState(false);
  const effectiveScore = subjectId && api?.applied && Object.hasOwn(api.applied.scores, subjectId) ? api.applied.scores[subjectId] : score;
  const grade = scoreGrade(effectiveScore);
  const gradeLabel = t(SCORE_GRADE_KEYS[grade]);
  const accessibleGradeLabel = t("creatorScoreGrade", { grade: gradeLabel });
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const tooltip = useRef<HTMLDivElement>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ left: number; top: number; width: number; maxHeight: number } | null>(null);
  const text = creatorScoreTooltipText(valueOnly, assessment, t);
  const cancelClose = () => { if (closeTimer.current) clearTimeout(closeTimer.current); closeTimer.current = null; };
  const show = () => { cancelClose(); if(!panel)setOpen(true); };
  const hideLater = () => { cancelClose(); closeTimer.current = setTimeout(() => { if (!tooltip.current?.contains(document.activeElement) && document.activeElement !== trigger.current) setOpen(false); }, 120); };
  useEffect(() => () => cancelClose(), []);
  useLayoutEffect(() => {
    if (!open) { setPosition(null); return; }
    const place = () => {
      const rect = trigger.current?.getBoundingClientRect();
      if (!rect) return;
      const viewWidth = document.documentElement.clientWidth;
      const viewHeight = document.documentElement.clientHeight;
      const width = Math.max(80, Math.min(280, viewWidth - 16));
      const maxHeight = Math.max(48, Math.min(180, viewHeight - 16));
      const height = Math.min(tooltip.current?.scrollHeight ?? maxHeight, maxHeight);
      const left = Math.max(8, Math.min(rect.right - width, viewWidth - width - 8));
      const top = Math.max(8, Math.min(rect.bottom + height + 12 <= viewHeight ? rect.bottom + 6 : rect.top - height - 6, viewHeight - height - 8));
      setPosition({ left, top, width, maxHeight });
    };
    place();
    window.addEventListener("resize", place);
    document.addEventListener("scroll", place, true);
    return () => { window.removeEventListener("resize", place); document.removeEventListener("scroll", place, true); };
  }, [open, text]);
  const dismiss = () => { cancelClose(); setOpen(false); };
  return <>
    <button style={labelOnly ? { font: "inherit" } : undefined} className={valueOnly ? "cursor-help bg-transparent text-sm no-underline" : "cursor-help bg-transparent text-sm underline decoration-dotted underline-offset-4 "} ref={trigger} type="button" aria-label={valueOnly ? accessibleGradeLabel : t("creatorScore")} aria-describedby={open ? id : undefined} aria-haspopup="dialog" onClick={()=>{dismiss();setPanel(true);}} onMouseEnter={show} onMouseLeave={hideLater} onFocus={show} onBlur={hideLater} onKeyDown={event => { if (event.key === "Escape") dismiss(); else if (event.key === "ArrowDown" && open) { event.preventDefault(); tooltip.current?.focus(); } }}>{valueOnly ? gradeLabel : t("creatorScore")}</button>{!labelOnly && !valueOnly && <span aria-label={accessibleGradeLabel} className="ml-1.5">{gradeLabel}</span>}
    {panel && <ScoreRulePanel subjectId={subjectId} assessment={assessment} onClose={()=>{setPanel(false);dismiss();}} />}
    {open && !panel && createPortal(<div ref={tooltip} id={id} role="tooltip" tabIndex={0} className="fixed z-50 overflow-y-auto overscroll-contain rounded-md border bg-card px-3 py-2 text-xs leading-5 text-card-foreground shadow-md outline-none [overflow-wrap:anywhere]" style={position ? { ...position, visibility: "visible" } : { width: 280, maxHeight: 180, visibility: "hidden" }} onMouseEnter={cancelClose} onMouseLeave={hideLater} onFocus={cancelClose} onBlur={hideLater} onKeyDown={event => { if (event.key === "Escape") { trigger.current?.focus(); dismiss(); } }}><p className="whitespace-pre-wrap">{text}</p></div>, document.body)}
  </>;
}
