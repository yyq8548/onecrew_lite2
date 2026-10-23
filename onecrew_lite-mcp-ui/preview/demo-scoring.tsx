import { useState, type ReactNode } from 'react';
import type { ResultSnapshot } from '../src/lib/result-contract';
import type { AppLocale } from '../src/lib/i18n';
import { ScoreRulesProvider, validRequirements, type AppliedScore, type ScoreAssessment } from '../src/features/result-card/score-rules';
export function DemoScoring({snapshot,locale,children}:{snapshot:ResultSnapshot|null|undefined;locale:AppLocale;children:ReactNode}) {
 const zh=locale==='zh-CN';
 const [applied,setApplied]=useState<AppliedScore|null>(null);
 const [event,setEvent]=useState('');
 const pending=snapshot?.pending_action;
 const rows=pending?.kind==='creator_save_batch'?pending.review.rows:[];
 const assessments:Record<string,ScoreAssessment>=Object.fromEntries(rows.map(({row_id,preview})=>[row_id,{score:preview.score,assessment:preview.assessment}]));
 return <ScoreRulesProvider value={{requirements:null,assessments,names:Object.fromEntries(rows.map(({row_id,preview})=>[row_id,preview.display_name ?? preview.handle ?? (zh?'未命名达人':'Unnamed creator')])),applied,demo:true,
  recalculate:async(requirements)=>{
   if(!validRequirements(requirements))throw new Error('invalid requirements');
   // Simulate latency and a fixed response only. This is not an evaluation algorithm.
   await new Promise(resolve=>setTimeout(resolve,700));
   const mock=[82,67,48,31,0];
   const results=Object.fromEntries(rows.map(({row_id},index)=>[row_id,{score:mock[index%mock.length]!,assessment:zh?'模拟评分理由，仅用于测试展示，未评估达人内容。':'Mock assessment reason for display testing; creator content was not evaluated.'}]));
   setApplied({requirements,scores:Object.fromEntries(Object.entries(results).map(([id,item])=>[id,item.score])),assessments:results,calculatedAt:new Date().toISOString()});
   setEvent(zh?'演示：已接收评价要求，返回固定模拟分数；未调用 Agent。':'Demo: Requirements received; fixed mock scores returned. No agent was called.');
  }
 }}>{children}<details className="demo-machine"><summary>{zh?'评分交互测试数据':'Scoring interaction test data'}</summary><pre>{JSON.stringify({mode:'demo',applied,event},null,2)}</pre></details></ScoreRulesProvider>;
}
