import {useEffect,useRef,useState} from 'react';
import {ResultCardView} from '../src/features/result-card/view';
import {FilterSelect} from '../src/components/ui/filter-select';
import {Card,CardTitle,CardDescription} from '../src/components/ui/card';
import {Button} from '../src/components/ui/button';
import {DemoScoring} from './demo-scoring';
import {round3Cases,round3Initial,round3Complete,round3Retry} from '../src/test-support/round3';
import '../src/styles/theme.css';
import './preview.css';
export function Round3Session({id}:{id:number}){
 const [snapshot,setSnapshot]=useState(()=>round3Initial(id));const initial=useRef(snapshot);
 const [duplicate,setDuplicate]=useState(false);const submitted=useRef(false);
 const [busy,setBusy]=useState(false),[unknown,setUnknown]=useState(false);
 const [events,setEvents]=useState<unknown[]>([]);const locked=useRef(false),timer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined),savedPayload=useRef<unknown>(null);
 useEffect(()=>()=>clearTimeout(timer.current),[]);
 function submit(payload:unknown){if(locked.current||snapshot.state.kind!=='available'){if(submitted.current){setDuplicate(true);setEvents(e=>[...e,{action:'duplicate_rejected',writeCount:1}]);}return;}submitted.current=true;locked.current=true;setBusy(true);savedPayload.current=payload;setEvents(e=>[...e,{action:'mock_submit',payload}]);timer.current=setTimeout(()=>{setBusy(false);if(id===10){setUnknown(true);setEvents(e=>[...e,{response:'unknown',writeCount:1}]);}else setSnapshot(round3Complete(initial.current,id,payload));},800);}
 const retryLock=useRef(false);
 function retry(rowIds:string[]){if(retryLock.current||busy)return;retryLock.current=true;setBusy(true);setEvents(e=>[...e,{action:'retry_failed_only',rowIds}]);timer.current=setTimeout(()=>{setSnapshot(s=>round3Retry(s,rowIds));setBusy(false);retryLock.current=false;},800);}
 function recover(){if(busy)return;setBusy(true);setEvents(e=>[...e,{action:'read_back',writeCount:1}]);timer.current=setTimeout(()=>{setSnapshot(round3Complete(initial.current,id,savedPayload.current));setUnknown(false);setBusy(false);},600);}
 function stop(instruction?:string){if(locked.current)return;locked.current=true;setEvents(e=>[...e,{action:instruction?'edit_requested':'cancelled',instruction}]);setSnapshot(s=>({...s,pending_action:null,state:{kind:'resolved',code:instruction?'edit_requested':'user_cancelled',summary:instruction?'修改要求已提交':'已取消'},presentation:{...s.presentation,sections:[]},batch:{outcomes:s.batch.outcomes.map(r=>({...r,disposition:{kind:'not_run',reason:instruction?'edit_requested':'user_cancelled'},summary:instruction?'修改要求已提交':'已取消',resource_refs:[]}))}}));}
 return <>{id===20 && submitted.current && <div className="demo-notice"><Button variant="outline" onClick={()=>submit(savedPayload.current)}>模拟再次提交同一请求</Button></div>}{duplicate && <Card role="status" className="mb-4 items-center gap-3 px-6 py-8 text-center shadow-none"><CardTitle>已经提交</CardTitle><CardDescription>无需再次提交，请查看下方首次提交的结果。</CardDescription></Card>}<DemoScoring snapshot={snapshot} locale="zh-CN"><ResultCardView snapshot={snapshot} busy={busy} revisionBlocked={unknown} recoveryFailure={unknown?{kind:'transport'}:null} onRecover={recover} onConfirm={()=>submit(null)} onSelect={submit} onCancel={()=>stop()} onRequestEdit={stop} onConfirmCreatorSaveBatch={submit} onRetryCreatorSaveBatch={id===9?retry:undefined}/></DemoScoring><details className="demo-machine"><summary>模拟操作记录</summary><pre>{JSON.stringify({simulation:true,events,snapshot},null,2)}</pre></details></>;
}
export function Round3App(){const [id,setId]=useState(9),[seed,setSeed]=useState(0),[dark,setDark]=useState(false);const current=round3Cases[id-1]!;return <main><header className="demo-toolbar"><span>第三轮 · 20 条分支 · 09-09 重试修订版</span><button onClick={()=>{setDark(!dark);document.documentElement.dataset.theme=dark?'light':'dark';}}>{dark?'浅色模式':'深色模式'}</button></header><p className="demo-notice">全程本地模拟，不写入 OneCrew、不发送消息。评分返回固定测试值。</p><FilterSelect label="选择测试分支" value={String(id)} options={round3Cases.map(c=>({value:String(c.id),label:`${c.id}. ${c.title}`}))} onChange={value=>setId(Number(value))}/><section className="demo-answer"><h1>{current.title}</h1><p>{current.steps}</p><Button variant="outline" className="mt-3" onClick={()=>setSeed(s=>s+1)}>重新体验此流程</Button></section><Round3Session key={`${id}-${seed}`} id={id}/></main>;}
