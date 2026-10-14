import { simulateChoiceResult } from "@mcp-ui/test-support/choice-result";
import type {ResultChoiceAnswer,ResultSnapshot,CreatorSaveBatchSelection} from '../lib/result-contract';
import {creatorSaveSnapshot} from './creator-save-fixture';
import {creatorBatchSnapshot,creatorBatchResultSnapshot} from './creator-batch-fixture';
import {choiceSnapshot,SNAPSHOT} from './fixtures';
export const round3Cases=[
 ['单人保存完整流程','核对主页与联系方式，点击保存；处理中不能重复提交，随后展示保存回执。'],
 ['可选联系方式冲突','公开邮箱存在可能冲突，默认不纳入保存；核对提交记录中的联系方式。'],
 ['必需主页冲突','主页归属冲突时提供“补充达人主页”和“暂不添加”；补充后提交核对请求，核实前不添加。'],
 ['选择时识别已有达人','第一位已在达人库，显示身份提示；仍需核对联系方式并确认当前请求。'],
 ['长姓名和无可选联系方式','核对长姓名换行；只保留必需主页也能完成保存。'],
 ['批量部分选择','取消几位达人后保存，回执保留未选择项。'],
 ['批量全部取消选择','取消全部选择，保存按钮不可用；重新选择后可继续。'],
 ['跨页选择 20 位达人','翻页并改变选择，再保存；跨页选择应保留，回执数量与提交相符。'],
 ['批量部分失败后重试','选择 5 位后模拟 3 位已保存、2 位临时写入失败；点击重试仅处理失败项，已成功的 3 位保持不变。'],
 ['批量超时后核对','首次提交结果未知，只能检查状态；核对后恢复实际回执，不重复写入。'],
 ['评分 0 与未评分','展开评分与评价理由，区分 0 分与未评分。'],
 ['修改评分要求','打开评价要求，输入新要求后重新评分，再检查模拟分数与理由。'],
 ['评分空输入校验','打开评价要求，清空内容，检查提交受限；填入有效要求后再提交。'],
 ['多选合作目标','第一题允许多选；完成最后一题先核对全部问答，逐题修改后再正式提交；提交后保留全部答案。'],
 ['自定义回答','改为人工输入并填写需求，回答完后先核对全部问答；正式提交后保留自定义文本。'],
 ['返回修改上题','回答全部问题进入核对页，逐题修改并返回核对，再正式提交；结果采用修改后的答案。'],
 ['手动创建创作者','核对创作者信息后创建，先处理中再显示创建回执。'],
 ['取消创建创作者','点击取消，显示取消结果，不出现创建成功。'],
 ['修改创建需求','填写修改要求并提交；旧卡关闭，不把请求已提交说成创作者已更新。'],
 ['重复点击提交','先提交一次，再点击“模拟再次提交同一请求”；应提示已经提交，无需再次提交，首次结果保留且只写入一次。'],
].map(([title,steps],index)=>({id:index+1,title,steps}));
export function round3Initial(id:number):ResultSnapshot{
 let s=id===4?creatorBatchSnapshot():id<=5?creatorSaveSnapshot():id<=13?creatorBatchSnapshot(id===8?20:5):id<=16?choiceSnapshot():structuredClone(SNAPSHOT);
 s=structuredClone(s);
 const p=s.pending_action;
 if(p?.kind==='creator_save_batch' && id<=5){
  const preview=p.review.rows[0]!.preview;
  preview.display_name=id===5?'Nora Beauty · Everyday Skincare and Detailed Product Reviews':'Nora Beauty';
  if(id===3){preview.routes[0]!.collision={status:'conflict',creator_ids:['existing-1']};preview.status='contact_conflict';preview.reason='required_contact_route_conflict';}
  if(id===4){preview.identity_status='exact_existing_creator';preview.identity_creator_ids=['existing-1'];preview.status='existing_skipped';preview.reason='existing_creator';}
  if(id===5)preview.routes=preview.routes.filter(r=>r.required);
 }
  if(p?.kind==='creator_save_batch' && id>5 || p?.kind==='creator_save_batch' && id===4)p.review.rows.forEach((r,i)=>{if(id===4&&i===0){r.preview.identity_status='exact_existing_creator';r.preview.identity_creator_ids=['existing-1'];r.preview.status='existing_skipped';r.preview.reason='existing_creator';}r.preview.display_name=`${['Nora','Lin','Mika','Emma','Sora'][i%5]} Beauty ${i+1}`;});
 if(p?.kind==='choice'){
  s.presentation={kind:'choice',title:'补充合作需求',summary:null,sections:[]};
  p.questions=[{id:'goals',selection_mode:id===14?'multiple':'single',header:'合作目标',prompt:'本次希望达成哪些合作目标？',options:[{id:'review',label:'产品测评',description:null},{id:'sales',label:'促进销售',description:null},{id:'awareness',label:'品牌曝光',description:null}]},{id:'market',selection_mode:'single',header:'目标市场',prompt:'主要面向哪个市场？',options:[{id:'us',label:'美国',description:null},{id:'jp',label:'日本',description:null}]}];
 }
 return s;
}
export function round3Complete(initial:ResultSnapshot,id:number,payload:unknown):ResultSnapshot{
 const pending=initial.pending_action;
 if(pending?.kind==='choice') return simulateChoiceResult(initial,payload as ResultChoiceAnswer[]);
 if(pending?.kind==='creator_save_batch'){
  const choices=payload as CreatorSaveBatchSelection[];
  const selected=new Set(choices.filter(c=>pending.review.rows.some(r=>r.row_id===c.row_id)).map(r=>r.row_id));
  const statuses=pending.review.rows.map((row,i)=>!selected.has(row.row_id)?'not_selected' as const:id===9&&i%2===1?'failed' as const:'saved' as const);
  const result=creatorBatchResultSnapshot(statuses);
  const section=result.presentation.sections.find(s=>s.kind==='creator_save_rows');
  if(section?.kind==='creator_save_rows')section.rows.forEach((row,i)=>{if(id===9&&row.status==='failed')row.failure_code='temporary_write_failure';row.display_name=pending.review.rows[i]!.preview.display_name;row.confirmed_route_ids=choices.find(c=>c.row_id===row.row_id)?.confirmed_route_ids??[];});
  result.batch.outcomes.forEach((row,i)=>{row.resource_refs.forEach(ref=>{ref.label=pending.review.rows[i]!.preview.display_name;});});
  return result;
 }
 const result=structuredClone(initial);result.pending_action=null;result.binding.revision++;
 const summary='创作者已创建';
 result.state={kind:'resolved',code:'completed',summary};
 result.presentation={...result.presentation,summary:null,sections:[{kind:'fields',heading:null,fields:[{label:'创作者',value:'Nora Beauty'}]}]};
 result.batch.outcomes=result.batch.outcomes.map(row=>({...row,summary,disposition:{kind:'succeeded'},resource_refs:[]}));
 return result;
}

export function round3Retry(snapshot:ResultSnapshot,rowIds:string[]):ResultSnapshot {
 const next=structuredClone(snapshot);
 const rows=next.presentation.sections.flatMap(s=>s.kind==='creator_save_rows'?s.rows:[]);
 const retried=rows.filter(r=>rowIds.includes(r.row_id)&&r.status==='failed'&&r.failure_code==='temporary_write_failure');
 if(!retried.length)return next;
 retried.forEach(row=>{row.status='saved';row.failure_code=null;row.creator_id=`mock-retry-${row.row_id}`;const outcome=next.batch.outcomes.find(o=>o.call_id===row.row_id);if(outcome){outcome.disposition={kind:'succeeded'};outcome.summary='saved';outcome.resource_refs=[{kind:'creator',id:row.creator_id,label:row.display_name}];}});
 next.binding.revision++;
 if(!rows.some(r=>r.status==='failed'||r.status==='unknown'||r.status==='not_run'))next.state={kind:'resolved',code:'creator_save_completed',summary:'所选达人已保存。'};
 return next;
}
