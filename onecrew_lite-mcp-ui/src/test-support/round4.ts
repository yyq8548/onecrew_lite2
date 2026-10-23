import type {ResultSnapshot} from '../lib/result-contract';
import {creatorBatchSnapshot} from './creator-batch-fixture';
import {SNAPSHOT} from './fixtures';
export const round4Cases=[
 ['同名达人会被合并吗？','两位同名但账号不同，分别选择和保存，核对回执仍保留两个对象。','前三轮未单独验证同名身份'],
 ['缺少显示名称时如何辨认？','第一位没有姓名，使用账号作为身份；可继续保存。','姓名缺失兜底'],
 ['缺少账号时如何展示？','第一位没有账号，保留姓名与平台，不显示空 @。','账号缺失兜底'],
 ['候选人全部已在库怎么办？','全员不可选，全选与添加按钮不可用；取消可退出。','全员已存在而非部分已存在'],
 ['审核期间被其他人保存怎么办？','先调整选择，再点“模拟数据变化”；刷新审核后第一位应不可选且不提交。','已入库状态在审核中变化'],
 ['审核期间某位候选被撤回怎么办？','先勾选，再模拟数据变化；刷新后撤回对象不应出现在提交记录。','候选删除与旧选择清理'],
 ['审核期间联系方式变更怎么办？','模拟数据变化后重新审核；提交记录不能包含已删除的联系方式。','联系方式在审核中失效'],
 ['一个达人有多个平台粉丝数怎么办？','展开第一位，核对其他平台的粉丝数；主列仍保持单一平台数值。','多平台粉丝详情展开'],
 ['粉丝数未知会显示成 0 吗？','第一位未提供粉丝数据，应显示 —，不能表示为 0。','未知数量与实际零值区分'],
 ['粉丝数极大时是否挤占评分列？','核对亿级粉丝的紧凑显示，姓名和评分仍对齐；窄屏横向滚动。','极大数值边界'],
 ['100 分与未评分是否清楚？','第一位评分 100，第二位未评分；核对列及评分说明。','评分上边界'],
 ['头像文件损坏怎么办？','第一位头像无法解码，应回退到首字母；不影响选择和保存。','图片加载失败'],
 ['批量里只有一位主页冲突怎么办？','第一位冲突阻止整批提交；取消选择该位后可保存其他人。','单行冲突下的批量继续'],
 ['只读信息很多时怎么阅读？','浏览 25 个字段，翻页读取最后一项，不提供确认写入按钮。','只读长列表分页'],
 ['返回文本包含 HTML 时会执行吗？','页面应将模拟 HTML 显示为普通文本，不出现弹窗或执行内容。','不可信返回文本展示'],
 ['重复添加同一个关键词怎么办？','已有 cooking，再输入 cooking 并添加；显示“该关键词已添加，无需重复添加。”，保留输入，不生成重复标签。','重复关键词去重'],
 ['一次粘贴多行短语会拆错吗？','粘贴下方样例，空格保留在短语内，逗号和换行分隔标签。','批量粘贴与短语保留'],
 ['关键词已满后还能添加吗？','已有 50 个，尝试再添加；提示数量限制，删除一个后可添加。','标签容量及删除恢复'],
 ['中文修改内容过长时会怎样？','点击填入超长样例，提交应不可用且有易懂提示；精简后可提交。','中文编码长度边界'],
 ['修改内容只有空格和换行怎么办？','填入空白样例，不能提交；输入有效内容后可提交。','修改请求空白规范化'],
].map(([title,steps,added],index)=>({id:index+1,title,steps,added}));
export function round4Initial(id:number):ResultSnapshot{
 if(id>=14){const s=structuredClone(SNAPSHOT);s.pending_action=null;s.batch.outcomes=s.batch.outcomes.map(r=>({...r,operation_id:'onecrew.creator.get',disposition:{kind:'succeeded'},summary:'已返回模拟资料',resource_refs:[]}));s.binding.view.kind='read_only';s.state={kind:'resolved',code:'completed',summary:'信息已载入'};s.presentation={kind:'read_only',title:'创作者资料',summary:null,sections:[{kind:'fields',heading:null,fields:id===15?[{label:'备注',value:'<img src=x onerror="alert(1)"> 这只是返回的文本。'}]:Array.from({length:25},(_,i)=>({label:`资料 ${i+1}`,value:`模拟创作者资料内容 ${i+1}`}))}]};return s;}
 const s=structuredClone(creatorBatchSnapshot());const p=s.pending_action;if(p?.kind!=='creator_save_batch')throw Error('fixture');const rows=p.review.rows;rows.forEach((r,i)=>{r.preview.display_name=`${['Nora','Lin','Mika','Emma','Sora'][i]} Studio`;});
 const first=rows[0].preview;
 if(id===1)rows[1].preview.display_name=first.display_name;
 if(id===2)first.display_name=null;
 if(id===3)first.handle=null;
 if(id===4)rows.forEach((r,i)=>{r.preview.identity_status='exact_existing_creator';r.preview.identity_creator_ids=[`existing-${i}`];});
 if(id===8)first.follower_counts={x:12500,instagram:38000,youtube:220000};
 if(id===9)first.follower_counts={};
 if(id===10)first.follower_counts={x:987654321};
 if(id===11){first.score=100;rows[1].preview.score=null;}
 if(id===12)first.avatar_url='data:image/png;base64,invalid';
 if(id===13)first.routes[0].collision={status:'conflict',creator_ids:['existing-conflict']};
 return s;
}
export function round4Change(snapshot:ResultSnapshot,id:number):ResultSnapshot{
 const s=structuredClone(snapshot);if(s.pending_action?.kind!=='creator_save_batch')return s;
 const rows=s.pending_action.review.rows;s.binding.revision++;
 if(id===5){rows[0].preview.identity_status='exact_existing_creator';rows[0].preview.identity_creator_ids=['existing-new'];}
 if(id===6){const removed=rows.splice(0,1)[0];s.batch.outcomes=s.batch.outcomes.filter(o=>o.call_id!==removed.row_id);}
 if(id===7)rows[0].preview.routes=rows[0].preview.routes.filter(r=>r.required);
 return s;
}
