// Local simulation only. No provider API, MCP request or credential is used.
export type Flow = 'search' | 'account' | 'edit';
export const flowCases = [
 {id:'search-results',flow:'search',label:'搜索成功与分页',expect:'开始搜索后显示执行中，再展示 23 位模拟达人；每页最多 10 位，可按粉丝数筛选。'},
 {id:'search-empty',flow:'search',label:'搜索无结果',expect:'无匹配时保留搜索条件；修改条件后再次搜索得到模拟结果。'},
 {id:'search-failed',flow:'search',label:'搜索失败后重试',expect:'显示失败原因，保留条件；主动重试后成功。'},
 {id:'search-limited',flow:'search',label:'请求过于频繁',expect:'直接展示居中的系统提示：标题与副标题，冷却后可重试，也可暂不搜索。'},
 {id:'search-unavailable',flow:'search',label:'搜索服务暂不可用',expect:'直接展示居中的系统提示：说明服务暂不可用，保留条件并提供重试与暂不搜索。'},
 {id:'account-missing',flow:'account',label:'尚未连接账号',expect:'连接成功后仍需检查搜索权限；检查通过才允许继续。'},
 {id:'account-expired',flow:'account',label:'账号授权过期',expect:'重新授权后检查本次搜索权限，原连接记录不能当作已验证。'},
 {id:'account-denied',flow:'account',label:'已连接但权限不足',expect:'账号存在但检查发现缺少搜索权限；重新授权并检查后恢复。'},
 {id:'account-cancel',flow:'account',label:'用户取消授权',expect:'显示未完成连接，不能继续搜索；允许重新连接。'},
 {id:'account-timeout',flow:'account',label:'账号检查超时',expect:'明确无法确认可用性；再次检查，不重新创建账号记录。'},
 {id:'edit-success',flow:'edit',label:'修改成功后再确认',expect:'修改后生成新的范围预览；确认执行时使用更新后的条件。'},
 {id:'edit-failed',flow:'edit',label:'修改失败保留输入',expect:'模拟请求明确未应用，保留修改内容；主动重试后更新。'},
 {id:'edit-unknown-applied',flow:'edit',label:'提交超时但实际已更新',expect:'先检查结果；查到已更新后回到新预览，禁止重复提交。'},
 {id:'edit-unknown-missing',flow:'edit',label:'提交超时且尚未更新',expect:'先检查结果；确认未应用后才允许重试，输入仍在。'},
 {id:'edit-conflict',flow:'edit',label:'另一窗口已更新范围',expect:'展示最新条件与当前输入，不静默覆盖；用户选择继续修改后再提交。'},
] as const;
export type CaseId = typeof flowCases[number]['id'];
export type Phase = 'idle'|'running'|'results'|'empty'|'error'|'limited'|'cancelled'|'disconnected'|'expired'|'unverified'|'authorizing'|'checking'|'denied'|'check-error'|'ready'|'continued'|'preview'|'editing'|'saving'|'edit-error'|'unknown'|'readback'|'conflict'|'confirmed';
export type Fields = {platform:'tiktok'|'instagram'|'youtube'|'x'|'telegram';market:string;keyword:string;minimum:string};
export type FlowState = {id:CaseId;phase:Phase;run:number;attempt:number;checks:number;revision:number;fields:Fields;saved:Fields;cooldown:boolean;message:string};
export function initialFlow(id:CaseId):FlowState {
 const fields:Fields={platform:'tiktok',market:'美国',keyword:'cooking',minimum:'1000'};
 const phase:Phase=id==='search-limited'?'limited':id==='search-unavailable'?'error':id.startsWith('search')?'idle':id.startsWith('edit')?'preview':id==='account-expired'?'expired':id==='account-denied'||id==='account-timeout'?'unverified':'disconnected';
 return {id,phase,run:0,attempt:(['search-limited','search-unavailable'].includes(id)?1:0),checks:0,revision:1,fields:{...fields},saved:{...fields},cooldown:id==='search-limited',message:''};
}
export function validFields(f:Fields){return !!f.market.trim()&&!!f.keyword.trim()&&/^\d+$/.test(f.minimum)&&Number.isSafeInteger(Number(f.minimum));}
export type FlowEvent = {type:'change';key:keyof Fields;value:string}|{type:'start'|'retry'|'cancel'|'edit'|'discard'|'save'|'check-result'|'rebase'|'use-latest'|'confirm'|'connect'|'check-account'|'continue'}|{type:'finish'|'cooled';run:number};
export function flowReducer(s:FlowState,e:FlowEvent):FlowState {
 if(e.type==='change')return ['idle','editing','edit-error'].includes(s.phase)?{...s,fields:{...s.fields,[e.key]:e.value}}:s;
 if(e.type==='cooled')return s.run===e.run&&s.phase==='limited'?{...s,cooldown:false}:s;
 if(e.type==='finish'){
  if(e.run!==s.run)return s;
  if(s.phase==='running'){
   const first=s.attempt===1;
   const phase:Phase=(first&&s.id==='search-empty')||Number(s.fields.minimum)>125000?'empty':first&&s.id==='search-limited'?'limited':first&&['search-failed','search-unavailable'].includes(s.id)?'error':'results';
   return {...s,phase,cooldown:phase==='limited'};
  }
  if(s.phase==='authorizing')return {...s,phase:s.id==='account-cancel'&&s.attempt===1?'cancelled':'unverified'};
  if(s.phase==='checking')return {...s,phase:s.id==='account-denied'&&s.attempt===0?'denied':s.id==='account-timeout'&&s.checks===1?'check-error':'ready'};
  if(s.phase==='saving'){
   if(s.attempt===1&&s.id==='edit-failed')return {...s,phase:'edit-error',message:'本次修改未保存，输入已保留。'};
   if(s.attempt===1&&s.id.startsWith('edit-unknown'))return {...s,phase:'unknown',message:'暂时无法确认修改是否保存。已保留输入，请先检查当前状态。'};
   if(s.attempt===1&&s.id==='edit-conflict')return {...s,phase:'conflict',revision:2,saved:{...s.saved,market:'日本',keyword:'home cooking',minimum:'5000'}};
   return {...s,phase:'preview',saved:{...s.fields},revision:s.revision+1,message:'搜索范围已更新。'};
  }
  if(s.phase==='readback')return s.id==='edit-unknown-applied'?{...s,phase:'preview',saved:{...s.fields},revision:s.revision+1,message:'已确认修改保存成功。'}:{...s,phase:'edit-error',message:'已确认本次修改未保存，可以重新提交。'};
  return s;
 }
 if(e.type==='start'&&s.phase==='idle'&&validFields(s.fields))return {...s,phase:'running',attempt:s.attempt+1,run:s.run+1};
 if(e.type==='retry'&&['error','check-error','cancelled','limited'].includes(s.phase)&&!s.cooldown){
  if(s.id.startsWith('search'))return {...s,phase:'running',attempt:s.attempt+1,run:s.run+1};
 }
 if(e.type==='cancel'&&['running','authorizing','error','limited'].includes(s.phase))return {...s,phase:'cancelled',cooldown:false,run:s.run+1};
 if(e.type==='edit'&&['empty','results','cancelled'].includes(s.phase)&&s.id.startsWith('search'))return {...s,phase:'idle'};
 if(e.type==='edit'&&s.phase==='preview')return {...s,phase:'editing',fields:{...s.saved},message:''};
 if(e.type==='discard'&&['editing','edit-error'].includes(s.phase))return {...s,phase:'preview',fields:{...s.saved},message:''};
 if(e.type==='save'&&['editing','edit-error'].includes(s.phase)&&validFields(s.fields)&&JSON.stringify(s.fields)!==JSON.stringify(s.saved))return {...s,phase:'saving',run:s.run+1,attempt:s.attempt+1,message:''};
 if(e.type==='check-result'&&s.phase==='unknown')return {...s,phase:'readback',run:s.run+1};
 if(e.type==='use-latest'&&s.phase==='conflict')return {...s,phase:'preview',fields:{...s.saved},message:'已采用最新搜索范围。'};
 if(e.type==='rebase'&&s.phase==='conflict')return {...s,phase:'editing',message:'已保留你的修改，请核对后提交。'};
 if(e.type==='confirm'&&s.phase==='preview')return {...s,phase:'confirmed'};
 if(e.type==='connect'&&['disconnected','expired','denied','cancelled'].includes(s.phase))return {...s,phase:'authorizing',run:s.run+1,attempt:s.attempt+1};
 if(e.type==='check-account'&&['unverified','check-error'].includes(s.phase))return {...s,phase:'checking',run:s.run+1,checks:s.checks+1};
 if(e.type==='continue'&&s.phase==='ready')return {...s,phase:'continued'};
 return s;
}
