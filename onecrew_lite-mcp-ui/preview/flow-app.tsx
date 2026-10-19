import {useEffect,useReducer,useState,type ReactNode} from 'react';
import {FilterSelect} from '../src/components/ui/filter-select';
import {CountryPicker} from './country-picker';
import {Button} from '../src/components/ui/button';
import {Card,CardContent,CardHeader,CardTitle,CardDescription} from '../src/components/ui/card';
import {OneCrewHeader} from '../src/components/ui/onecrew-brand';
import {ResultStatus} from '../src/components/ui/result-status';
import {CreatorIdentity} from '../src/features/result-card/components/creator-review-list';
import {ChannelMark} from '../src/components/ui/platform-mark';
import {flowCases,initialFlow,flowReducer,validFields,type CaseId,type Fields,type FlowEvent,type FlowState} from '../src/test-support/flow-simulator';
import '../src/styles/theme.css';
import './preview.css';
import './flow.css';
const titles:Partial<Record<FlowState['phase'],string>>={running:'正在搜索达人',results:'搜索结果',empty:'没有找到匹配的达人',error:'搜索暂未完成',limited:'搜索请求暂时受限',cancelled:'已取消',disconnected:'连接搜索账号',expired:'账号授权已过期',unverified:'检查账号搜索权限',authorizing:'正在连接账号',checking:'正在检查搜索权限',denied:'缺少搜索权限','check-error':'暂时无法确认账号可用性',ready:'账号可用于搜索',continued:'可以继续搜索',preview:'搜索范围',editing:'修改搜索范围',saving:'正在更新搜索范围','edit-error':'修改未保存',unknown:'修改结果尚未确认',readback:'正在检查修改结果',conflict:'搜索范围已被更新',confirmed:'计划已确认',idle:'搜索达人'};
const platformNames:Record<string,string>={tiktok:'TikTok',instagram:'Instagram',youtube:'YouTube',x:'X',telegram:'Telegram'};
const platformName=(platform:string)=>platformNames[platform]??platform;
const people=[['Nora Kitchen','nora_kitchen',18200],['Lin Home Cooking','lin_home',75000],['Mika Everyday Recipes and Cookware Reviews','mika_recipes',125000],['Emma Baking','emma_baking',5600],['Sora Family Meals','sora_meals',42000],['Olive Small Kitchen','olive_kitchen',9100],['Theo One Pot','theo_onepot',32000],['Ava Fresh Meals','ava_fresh',15000],['Ben Cook at Home','ben_home',88000],['Chloe Baking','chloe_baking',6500],['Diego Recipes','diego_recipes',52000],['Ella Kitchen','ella_kitchen',11000],['Finn Quick Meals','finn_meals',3900],['Grace Family Cooking','grace_family',68000],['Hana Food Diary','hana_food',24000],['Ivan Pantry','ivan_pantry',47000],['Jade Everyday Meals','jade_meals',8000],['Kai Kitchen','kai_kitchen',115000],['Luna Cooking','luna_cooking',35000],['Max Home Chef','max_chef',59000],['Nina Simple Recipes','nina_simple',1200],['Owen Baking','owen_baking',96000],['Pia Cookware','pia_cookware',21000]] as const;
// Mock metadata stays within the requested regions; filter options come from these results.
export function resultPage(minimum:string,band:string,page:number,region='all',tag='all',market='美国',keyword='cooking'){
 const regions=market.split(/[,，、]/).map(value=>value.trim()).filter(Boolean);
 const topics=['家常菜','烘焙','厨具测评'];
 const eligible=people.filter(p=>p[2]>=Number(minimum)).map((p,i)=>[...p,regions[i%regions.length]??market,[...new Set([keyword,topics[i%topics.length]])]] as const);
 const matches=eligible.filter(p=>(region==='all'||p[3]===region)&&(tag==='all'||p[4].includes(tag))&&(band==='under10k'?p[2]<10000:band==='10k50k'?p[2]>=10000&&p[2]<50000:band==='50kplus'?p[2]>=50000:true));
 const pages=Math.max(1,Math.ceil(matches.length/10));
 return {eligible,matches,pages,visible:matches.slice(page*10,page*10+10),regions:[...new Set(eligible.map(p=>p[3]))],tags:[...new Set(eligible.flatMap(p=>p[4]))]};
}
function Summary({fields}:{fields:Fields}){return <dl className="flow-summary"><dt>平台</dt><dd><ChannelMark platform={fields.platform}/></dd><dt>地区</dt><dd>{fields.market}</dd><dt>关键词</dt><dd>{fields.keyword}</dd><dt>粉丝</dt><dd>≥ {Number(fields.minimum).toLocaleString('zh-CN')}</dd></dl>;}
function Form({fields,onChange}:{fields:Fields;onChange:(key:keyof Fields,value:string)=>void}){return <div className="flow-form"><FilterSelect label="渠道" value={fields.platform} options={Object.entries(platformNames).map(([value,label])=>({value,label}))} onChange={value=>onChange('platform',value)}/><CountryPicker value={fields.market} onChange={value=>onChange('market',value)}/><label>关键词<input value={fields.keyword} onChange={e=>onChange('keyword',e.target.value)}/></label><label>最低粉丝数<input inputMode="numeric" value={fields.minimum} onChange={e=>onChange('minimum',e.target.value)} aria-invalid={!/^\d+$/.test(fields.minimum)}/></label>{!validFields(fields)&&<p role="alert" className="text-sm text-destructive">请填写地区、关键词和有效的非负整数粉丝数。</p>}</div>;}
function Notice({children,action,tone='unknown'}:{children:ReactNode;action?:ReactNode;tone?:'unknown'|'failure'}){return <div role="alert" className="grid gap-12 rounded-lg border border-destructive/30 p-3"><div className="flex items-start gap-2"><span aria-hidden="true" className="mt-0.5"><ResultStatus tone={tone} iconOnly label=""/></span><p className="text-sm leading-6">{children}</p></div>{action}</div>;}
export function FlowSession({id,initialState}:{id:CaseId;initialState?:FlowState}){
 const [s,dispatch]=useReducer(flowReducer,initialState ?? initialFlow(id));const [log,setLog]=useState<string[]>([]);const [band,setBand]=useState('all');const [region,setRegion]=useState('all');const [tag,setTag]=useState('all');const [page,setPage]=useState(0);
 const send=(event:FlowEvent)=>{if(event.type==='start'||event.type==='retry'){setPage(0);setBand('all');setRegion('all');setTag('all');}setLog(rows=>[...rows,JSON.stringify(event)]);dispatch(event);};
 useEffect(()=>{if(!['running','authorizing','checking','saving','readback'].includes(s.phase))return;const timer=setTimeout(()=>send({type:'finish',run:s.run}),900);return()=>clearTimeout(timer);},[s.run,s.phase]);
 useEffect(()=>{if(s.phase!=='limited'||!s.cooldown)return;const timer=setTimeout(()=>send({type:'cooled',run:s.run}),3000);return()=>clearTimeout(timer);},[s.phase,s.cooldown,s.run]);
 const systemMessage=s.phase==='limited'?{title:'请求过于频繁',description:'请稍后再试，你的搜索条件已保留。'}:id==='search-unavailable'&&s.phase==='error'?{title:'搜索服务暂不可用',description:'请稍后再试，你的搜索条件已保留。'}:null;
 if(systemMessage)return <Card className="flow-system-message shadow-none" role="status"><h2>{systemMessage.title}</h2><p>{systemMessage.description}</p><div className="mt-5 flex justify-end gap-3"><Button variant="ghost" onClick={()=>send({type:'cancel'})}>暂不搜索</Button><Button disabled={s.cooldown} onClick={()=>send({type:'retry'})}>{s.cooldown?'稍后可重试':'重新搜索'}</Button></div></Card>;
 const action=(label:string,type:FlowEvent['type'],disabled=false,secondary=false)=><Button type="button" variant={secondary?'outline':'default'} disabled={disabled} onClick={()=>send({type} as FlowEvent)}>{label}</Button>;
 const form=<Form fields={s.fields} onChange={(key,value)=>send({type:'change',key,value})}/>;
 const subtitles:Partial<Record<FlowState['phase'],string>>={"running": "正在查找符合这些条件的达人。", "empty": "没有找到符合本次条件的达人，可以调整关键词或放宽粉丝要求。", "disconnected": "尚未连接可用于 TikTok 搜索的账号。", "expired": "原授权已过期，需要重新授权后再检查搜索权限。", "authorizing": "正在等待账号授权结果。", "unverified": "账号已连接，本次搜索权限尚未检查。", "checking": "正在检查账号是否具备本次搜索所需的权限。", "continued": "已完成账号准备，搜索尚未开始。", "saving": "正在保存修改，完成后可核对新的搜索范围。", "readback": "正在核对上一次修改是否已保存。", "confirmed": "已确认以下搜索范围，搜索尚未开始。"};
 const subtitle=s.phase==='cancelled'?(id.startsWith('search')?'本次搜索已停止。':'账号连接未完成，尚不能用于搜索。'):subtitles[s.phase];
 let body:ReactNode;let footer:ReactNode;
 if(id.startsWith('search')){
  if(s.phase==='idle'){body=form;footer=action('开始搜索','start',!validFields(s.fields));}
  else if(s.phase==='running'){body=<><Summary fields={s.fields}/></>;footer=action('停止搜索','cancel',false,true);}
  else if(s.phase==='results'){
   const {matches,pages,visible,regions,tags}=resultPage(s.fields.minimum,band,page,region,tag,s.fields.market,s.fields.keyword);
   body=<><div className="flow-result-filters"><FilterSelect label="地区" value={region} onChange={value=>{setRegion(value);setPage(0);}} options={[{value:'all',label:'全部地区'},...regions.map(value=>({value,label:value}))]}/><FilterSelect label="标签" value={tag} onChange={value=>{setTag(value);setPage(0);}} options={[{value:'all',label:'全部标签'},...tags.map(value=>({value,label:value}))]}/><FilterSelect label="粉丝数" value={band} onChange={value=>{setBand(value);setPage(0);}} options={[{value:'all',label:'全部粉丝数'},{value:'under10k',label:'< 10,000'},{value:'10k50k',label:'≥ 10,000 且 < 50,000'},{value:'50kplus',label:'≥ 50,000'}]}/></div>{visible.length?<ul key={`${region}-${tag}-${band}-${page}`} className="flow-results" tabIndex={0} aria-label="达人搜索结果">{visible.map(([name,handle,count,location,labels])=><li key={handle}><div className="min-w-0 flex-1"><CreatorIdentity preview={{display_name:name,handle,platform:s.fields.platform}} expanded><span className="flow-result-metadata"><span className="flow-result-tag">{location}</span>{labels.map(label=><span className="flow-result-tag" key={label}>{label}</span>)}</span></CreatorIdentity></div><span className="text-sm text-muted-foreground">{count.toLocaleString('zh-CN')} 粉丝</span></li>)}</ul>:<div className="grid gap-3 py-4"><p>当前筛选没有匹配结果。</p><Button variant="outline" onClick={()=>{setBand('all');setRegion('all');setTag('all');setPage(0);}}>清除筛选</Button></div>}<div className="flex items-center justify-between gap-3"><Button variant="outline" disabled={page===0} onClick={()=>setPage(p=>p-1)}>上一页</Button><span className="text-xs text-muted-foreground">第 {page+1} / {pages} 页 · 共 {matches.length} 位 · 每页最多 10 位</span><Button variant="outline" disabled={page+1>=pages} onClick={()=>setPage(p=>p+1)}>下一页</Button></div></>;footer=action('修改搜索条件','edit');
  }else if(s.phase==='empty'){body=<><Summary fields={s.fields}/></>;footer=action('调整搜索条件','edit');}
  else if(s.phase==='cancelled'){body=null;footer=action('重新设置搜索','edit');}
  else{body=<><Summary fields={s.fields}/><Notice action={action(s.cooldown?'稍后可重试':'重新搜索','retry',s.cooldown)}>{s.phase==='limited'?'搜索服务暂时限制了请求频率，请稍后再试。':id==='search-unavailable'?'搜索服务暂时不可用，条件已保留。':'本次搜索失败，条件已保留。'}</Notice></>;}
 }else if(id.startsWith('account')){
  const identity=<div className="flex items-center gap-3"><ChannelMark platform="tiktok" compact/><div><p className="text-sm font-medium">TikTok · Nora Kitchen</p><p className="text-xs text-muted-foreground">@nora_brand</p></div></div>;
  if(s.phase==='disconnected'){body=null;footer=action('连接账号','connect');}
  else if(s.phase==='expired'){body=<>{identity}</>;footer=action('重新授权','connect');}
  else if(s.phase==='authorizing'){body=null;footer=action('取消连接','cancel',false,true);}
  else if(s.phase==='cancelled'){body=null;footer=action('重新连接','connect');}
  else if(s.phase==='unverified'){body=<>{identity}</>;footer=action('检查搜索权限','check-account');}
  else if(s.phase==='checking'){body=<>{identity}</>;}
  else if(s.phase==='denied'){body=<>{identity}<Notice action={action('重新授权','connect')}>账号已连接，但没有本次搜索所需的权限。</Notice></>;}
  else if(s.phase==='check-error'){body=<>{identity}<Notice action={action('再次检查','check-account')}>本次检查超时，暂时无法确认账号能否用于搜索。</Notice></>;}
  else if(s.phase==='ready'){body=<>{identity}<div className="flex justify-between gap-2"><p className="text-sm">本次搜索权限检查通过。</p><ResultStatus tone="success" iconOnly label="检查通过"/></div></>;footer=action('继续搜索','continue');}
  else{body=<>{identity}</>;}
 }else{
  if(s.phase==='preview'){body=<>{s.message&&<p role="status" className="text-sm">{s.message}</p>}<Summary fields={s.saved}/></>;footer=<>{action('修改搜索范围','edit',false,true)}{action('确认计划','confirm')}</>;}
  else if(s.phase==='editing'||s.phase==='edit-error'){body=<>{s.message&&<p role={s.phase==='edit-error'?'alert':'status'} className="text-sm">{s.message}</p>}{form}</>;footer=<>{action('放弃修改','discard',false,true)}{action(s.phase==='edit-error'?'重新提交':'保存修改','save',!validFields(s.fields)||JSON.stringify(s.fields)===JSON.stringify(s.saved))}</>;}
  else if(s.phase==='saving'){body=<><Summary fields={s.fields}/></>;}
  else if(s.phase==='unknown'){body=<><Summary fields={s.fields}/><Notice action={action('检查当前状态','check-result')}>{s.message}</Notice></>;}
  else if(s.phase==='readback'){body=<><Summary fields={s.fields}/></>;}
  else if(s.phase==='conflict'){body=<><Notice>其他窗口已更新搜索范围。你的输入已保留，请对照最新内容后决定是否继续。</Notice><section className="grid gap-4"><h2 className="text-sm font-medium">最新已保存的范围</h2><Summary fields={s.saved}/></section><section className="grid gap-4"><h2 className="text-sm font-medium">你的修改</h2><Summary fields={s.fields}/></section></>;footer=<>{action('使用最新范围','use-latest',false,true)}{action('基于最新范围继续修改','rebase')}</>;}
  else{body=<><Summary fields={s.saved}/></>;}
 }
 return <><Card className="result-shell gap-0 overflow-hidden py-0 shadow-none"><OneCrewHeader/><CardHeader className="result-shell-header"><CardTitle>{s.phase==='idle'?(s.attempt>0?'修改搜索条件':'搜索达人'):s.phase==='running'?`正在 ${platformName(s.fields.platform)} 搜索达人`:titles[s.phase]}</CardTitle>{subtitle&&<CardDescription>{subtitle}</CardDescription>}</CardHeader>{body&&<CardContent className="result-shell-content grid gap-6">{body}</CardContent>}{footer&&<div className="result-shell-footer flex flex-wrap items-center justify-end border-t">{footer}</div>}</Card><details className="demo-machine"><summary>模拟状态与操作记录</summary><pre>{JSON.stringify({simulation:true,state:s,events:log},null,2)}</pre></details></>;
}
export function FlowApp(){const [id,setId]=useState<CaseId>('search-results');const [seed,setSeed]=useState(0);const [dark,setDark]=useState(false);const current=flowCases.find(c=>c.id===id)!;
 return <main><header className="demo-toolbar"><span>第二轮 · 3 组连续流程 · 15 条分支</span><button onClick={()=>{setDark(!dark);document.documentElement.dataset.theme=dark?'light':'dark';}}>{dark?'浅色模式':'深色模式'}</button></header><p className="flow-simulation">全程本地模拟：不连接真实账号，不发送请求，不生成真实搜索结果。等待时间和响应均由测试脚本控制。</p><nav><FilterSelect label="选择测试分支" value={id} onChange={value=>setId(value as CaseId)} options={flowCases.map((c,i)=>({value:c.id,label:`${i+1}. ${c.label}`}))}/></nav><section className="demo-answer"><h1>{current.label}</h1><p>{current.expect}</p>{!['search-limited','search-unavailable'].includes(id)&&<p>异常分支的首次响应按题目模拟；主动恢复后的响应设为成功。</p>}<Button className="mt-3" variant="outline" onClick={()=>setSeed(n=>n+1)}>重新体验此流程</Button></section><FlowSession key={`${id}:${seed}`} id={id}/></main>;
}

