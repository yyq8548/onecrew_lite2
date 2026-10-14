import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button } from "@mcp-ui/components/ui/button";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { useScoreRules, validRequirements } from "../score-rules";
import { SCORE_GRADE_ORDER, type ScoreGrade } from "../score-rules";

const SCORE_GRADE_LABEL_KEYS: Record<ScoreGrade, "scoreGradeExcellent" | "scoreGradeGood" | "scoreGradeMedium" | "scoreGradeLow" | "scoreGradeUnrated"> = {
  excellent: "scoreGradeExcellent",
  good: "scoreGradeGood",
  medium: "scoreGradeMedium",
  low: "scoreGradeLow",
  unrated: "scoreGradeUnrated",
};

const SCORE_GRADE_BAND_KEYS: Record<ScoreGrade, "scoreBandExcellent" | "scoreBandGood" | "scoreBandMedium" | "scoreBandLow" | "scoreBandUnrated"> = {
  excellent: "scoreBandExcellent",
  good: "scoreBandGood",
  medium: "scoreBandMedium",
  low: "scoreBandLow",
  unrated: "scoreBandUnrated",
};

export function ScoreRulePanel({ onClose, subjectId, assessment }: { onClose: () => void; subjectId?: string; assessment?: string | null }) {
  const api = useScoreRules(); const {t} = useAppLocale();
  const dialog=useRef<HTMLElement>(null);
  const [draft,setDraft]=useState(api?.applied?.requirements ?? api?.requirements ?? '');
  const [busy,setBusy]=useState(false); const [error,setError]=useState(''); const [done,setDone]=useState(false);
  useEffect(()=>{
    const previous=document.activeElement as HTMLElement|null;
    const background=Array.from(document.body.children).filter((el):el is HTMLElement=>el instanceof HTMLElement && !el.contains(dialog.current));
    const old=background.map(el=>el.inert);background.forEach(el=>{el.inert=true;});
    dialog.current?.focus();return ()=>{background.forEach((el,i)=>{el.inert=old[i]!;});previous?.focus();};
  },[]);
  const entries = Object.entries(api?.applied?.assessments ?? api?.assessments ?? {}).filter(([id])=>!subjectId || id===subjectId);
  const reasons = entries.filter(([,item])=>item.assessment?.trim());
  const run=async()=>{
    if(!api?.recalculate || busy || !validRequirements(draft))return;
    setBusy(true);setError('');setDone(false);
    try { await api.recalculate(draft.trim()); setDone(true); }
    catch { setError(t("scoreReassessFailure")); }
    finally { setBusy(false); }
  };
  return createPortal(<div className="score-rule-overlay"><section ref={dialog} role="dialog" aria-modal="true" tabIndex={-1} className="score-rule-dialog" aria-labelledby="score-rule-heading" onKeyDown={event=>{
    if(event.key==='Escape'){event.preventDefault();if(!busy)onClose();}
    if(event.key==='Tab'){
      const nodes=Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href]')??[]);
      const first=nodes[0],last=nodes[nodes.length-1];
      if(!first){event.preventDefault();return;}
      if(event.shiftKey && (document.activeElement===first || document.activeElement===dialog.current)){event.preventDefault();last?.focus();}
      else if(!event.shiftKey && (document.activeElement===last || document.activeElement===dialog.current)){event.preventDefault();first.focus();}
    }
  }}>
    <header><h2 id="score-rule-heading">{t("scoreAbout")}</h2><Button variant="ghost" disabled={busy} onClick={onClose}>{t("scoreClose")}</Button></header>
    <p className="text-sm text-muted-foreground">{t("scoreScaleHelp")}</p>
    <section aria-labelledby="score-scale-heading"><h3 id="score-scale-heading">{t("scoreScaleTitle")}</h3><ul aria-label={t("scoreScaleTitle")}>
      {SCORE_GRADE_ORDER.map((grade) => <li key={grade}><span>{t(SCORE_GRADE_LABEL_KEYS[grade])}</span><span> · {t(SCORE_GRADE_BAND_KEYS[grade])}</span></li>)}
    </ul></section>
    {api?.demo && <p className="text-xs text-muted-foreground">{t("scoreDemoNotice")}</p>}
    <section><h3>{t("scoreCurrentRequirements")}</h3><p className="text-sm whitespace-pre-wrap">{api?.applied?.requirements || api?.requirements || (t("scoreNotProvided"))}</p></section>
    {(reasons.length>0 || assessment?.trim()) && <section><h3>{t("scoreReasons")}</h3>
      {reasons.length ? reasons.map(([id,item])=><div key={id} className="py-2">
        {!subjectId && <p className="text-sm font-medium">{api?.names?.[id] ?? (t("scoreCreator"))}</p>}
        <p className="text-sm whitespace-pre-wrap">{item.assessment}</p>
        {item.evidence?.filter(ref=>/^https?:\/\//i.test(ref.url)).map((ref,index)=><a key={index} className="block text-sm underline" href={ref.url} target="_blank" rel="noopener noreferrer">{ref.label}</a>)}
      </div>) : <p className="text-sm whitespace-pre-wrap">{assessment}</p>}
    </section>}
    <form onSubmit={event=>{event.preventDefault();void run();}}>
      <label>{t("scoreRequirementsLabel")}<textarea rows={3} maxLength={4000} value={draft} disabled={busy || !api?.recalculate} placeholder={t("scoreRequirementsPlaceholder")} onChange={event=>{setDraft(event.target.value);setDone(false);}} /></label>
      <p className="text-xs text-muted-foreground">{api?.recalculate ? (t("scoreReassessHelp")) : (t("scoreReassessUnavailable"))}</p>
      {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
      {done && <p role="status" className="text-sm">{api?.demo ? (t("scoreDemoComplete")) : (t("scoreUpdated"))}</p>}
      <footer><Button type="submit" disabled={busy || !api?.recalculate || !validRequirements(draft)}>{busy?(t("scoreAssessing")):(t("scoreReassess"))}</Button></footer>
    </form>
  </section></div>,document.body);
}
