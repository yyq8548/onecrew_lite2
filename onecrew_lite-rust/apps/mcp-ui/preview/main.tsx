import {FilterSelect} from '../src/components/ui/filter-select';
import { AgentEditRequest } from '../src/features/result-card/components/editor-request';
import { OneCrewHeader } from '../src/components/ui/onecrew-brand';
import { Card, CardContent, CardHeader, CardTitle } from '../src/components/ui/card';
import { DemoScoring } from './demo-scoring';
import { localizedScenario } from '../src/test-support/demo-locale';
import type { AppLocale } from '../src/lib/i18n';
import { simulateChoiceResult } from "@mcp-ui/test-support/choice-result";
import type { ResultChoiceAnswer, ResultSnapshot } from '../src/lib/result-contract';
import { Button } from '../src/components/ui/button';
import { useState, useEffect } from 'react';
import { createRoot } from 'react-dom/client';
import { ResultCardView, LoadingShell } from '../src/features/result-card/view';
import { ResultFailureShell } from '../src/features/result-card/components/shell';
import { resultClientFailureCopy } from '../src/features/result-card/presentation';
import { AppLocaleProvider } from '../src/lib/i18n';
import { scenarios } from '../src/test-support/design-scenarios';
import '../src/styles/theme.css';
import './preview.css';
function Preview() {
 const [index,setIndex]=useState(0); const [dark,setDark]=useState(false); const [notice,setNotice]=useState('');
 const [cancelled,setCancelled]=useState<ResultSnapshot|null>(null);
 const [initialPlan,setInitialPlan]=useState(false); const [initialInstruction,setInitialInstruction]=useState('');
 const [request,setRequest]=useState<string|null>(null);
 const [locale,setLocale]=useState<AppLocale>('zh-CN');
 const english=locale==='en-US';
 useEffect(()=>{document.documentElement.lang=locale;document.title=english?'OneCrew · 20 state tests':'OneCrew · 20 个状态测试';},[locale,english]);
 const s=localizedScenario(index,locale); const current=cancelled ?? s.snapshot;
 const showingInitial = s.initialSearch || (index===12 && initialPlan);
 const change=(i:number)=>{setIndex(i);setNotice('');setCancelled(null);setRequest(null);setInitialPlan(false);setInitialInstruction('');};
 const cancel=()=>{
  if(!current || current.state.kind!=='available')return;
  setCancelled({...current,binding:{...current.binding,revision:current.binding.revision+1},pending_action:null,state:{kind:'resolved',code:'user_cancelled',summary:english?'This request was cancelled.':'本次请求已取消。'},presentation:{...current.presentation,sections:[]},batch:{outcomes:current.batch.outcomes.map(row=>({...row,disposition:{kind:'not_run',reason:'user_cancelled'},summary:english?'This request was cancelled.':'本次请求已取消。',resource_refs:[]}))},continuation:{state:'not_requested',continuation_revision:current.continuation.continuation_revision,dispatch_started_at_ms:null,dispatch_not_after_ms:null}});
  setNotice('');
 };
 const select=(answers:ResultChoiceAnswer[])=>{
  if(!current || current.state.kind!=='available' || current.pending_action?.kind!=='choice')return;
  setCancelled(simulateChoiceResult(current,answers));
  setNotice(english?'Local simulation: answers are shown below. No request was sent to OneCrew.':'本地模拟：下方保留已提交的全部问答；未向 OneCrew 发送请求。');
 };
 const simulate=()=>setNotice(english?'Simulation only: no request was sent to OneCrew. Select another scenario to inspect an outcome.':'这是本地展示测试：已接收到操作意图，未向 OneCrew 发送请求。请切换场景查看下一种结果。');
 const failure=s.failure ? resultClientFailureCopy(s.failure,locale) : null;
 return <main><header className="demo-toolbar"><span>{english?'20 state tests · Simulated data':'20 个状态测试 · 模拟数据'}</span><FilterSelect label="界面语言 / Interface language" value={locale} onChange={value=>{setLocale(value as AppLocale);setCancelled(null);setNotice('');}} options={[{value:'zh-CN',label:'中文'},{value:'en-US',label:'English'}]}/><button onClick={()=>{setDark(!dark);document.documentElement.dataset.theme=dark?'light':'dark';}}>{english?(dark?'Light mode':'Dark mode'):(dark?'浅色模式':'深色模式')}</button></header>
 <nav><FilterSelect label={english?'Choose a question':'选择问题'} value={String(index)} onChange={value=>change(Number(value))} options={scenarios.map((s,i)=>({value:String(i),label:`${i+1}. ${localizedScenario(i,locale).question}`}))}/><div className="demo-step"><button disabled={index===0} onClick={()=>change(index-1)}>{english?'Previous':'上一题'}</button><span>{index+1} / 20</span><button disabled={index===19} onClick={()=>change(index+1)}>{english?'Next':'下一题'}</button></div></nav>
 <section className="demo-answer"><h1>{s.question}</h1><p>{s.answer}</p></section>
 {index===12 && <div className="mb-4 flex gap-2" aria-label={english?'Search preview state':'搜索模拟状态'}><Button variant={initialPlan?'outline':'default'} onClick={()=>{setInitialPlan(false);setNotice('');}}>{english?'Existing plan':'已有搜索计划'}</Button><Button variant={initialPlan?'default':'outline'} onClick={()=>{setInitialPlan(true);setNotice('');}}>{english?'No plan yet':'尚未生成计划'}</Button></div>}
 {s.steps && <details className="demo-machine mb-4"><summary>{english?'Test steps (Chinese)':'本条测试步骤'}</summary><ol className="list-decimal pl-5">{s.steps.map(step=><li key={step}>{step}</li>)}</ol></details>}
 <DemoScoring key={`${index}-${locale}`} snapshot={showingInitial ? null : current} locale={locale}><section className="demo-card" aria-label={english?"Result card":"用户收到的卡片"} key={index}>
 {showingInitial ? <AppLocaleProvider locale={locale}><Card className="gap-0 overflow-hidden py-0 shadow-none"><OneCrewHeader/><CardHeader className="px-6 pt-6 pb-4"><CardTitle>{english?'Creator search':'搜索达人'}</CardTitle></CardHeader><CardContent className="px-6 pb-6"><AgentEditRequest searchPlan initialSearch value={initialInstruction} busy={false} error={null} onChange={setInitialInstruction} onSubmit={instruction=>{setRequest(instruction);setNotice(english?'Demo: requirement captured. No plan has been generated.':'演示已记录需求，尚未生成搜索范围。');}}/></CardContent></Card></AppLocaleProvider> : s.loading ? <LoadingShell label={english?'Loading…':'正在载入卡片…'}/> : failure ? <AppLocaleProvider locale={locale}><ResultFailureShell {...failure}/></AppLocaleProvider> : current && <ResultCardView snapshot={current} busy={false} syncing={s.syncing} revisionBlocked={s.recovery} recoveryFailure={s.recovery?{kind:'transport'}:null} onRecover={simulate} onConfirm={simulate} onCancel={cancel} onRequestEdit={(instruction)=>{setRequest(instruction);setNotice(english?"Demo: request captured. This preview does not generate a new plan.":"演示已记录需求；此预览不生成新计划。");}} onSelect={select} onConfirmCreatorSaveBatch={simulate} onReviseSearchPlan={async()=>{simulate();return false;}} onRefreshSearchPlan={simulate}/>}</section>
 </DemoScoring>
 {cancelled && <Button type="button" variant="outline" className="mt-4" onClick={()=>setCancelled(null)}>{english?'Try this scenario again':'重新体验此场景'}</Button>}
 {notice && <p role="status" className="demo-notice">{notice}</p>}
 <details className="demo-machine"><summary>{english?'Machine data':'机器收到什么'}</summary><pre>{JSON.stringify(showingInitial ? {ui:"search_request",plan:null,instruction:request,demo_only:true} : s.recovery ? {snapshot:current,client_state:{state:'awaiting_readback',write_actions_enabled:false}} : (request ? {snapshot:current,assistant_request:{instruction:request,status:"demo_only"}} : current) ?? s.failure ?? {ui:'loading',result:null},null,2)}</pre></details>
 </main>;
}
const root = document.getElementById('root')!;
createRoot(root).render(<Preview/>);
