import type { ResultSnapshot } from '../lib/result-contract';
import type { ResultClientFailure } from '../lib/result-client';
import { creatorSaveSnapshot, creatorSavedSnapshot } from './creator-save-fixture';
import { creatorBatchSnapshot, creatorBatchResultSnapshot } from './creator-batch-fixture';
import { searchPlanSnapshot } from './search-plan-fixture';
import { choiceSnapshot, SNAPSHOT } from './fixtures';

export type Scenario = { question: string; answer: string; snapshot?: ResultSnapshot; failure?: ResultClientFailure; loading?: boolean; recovery?: boolean; syncing?: boolean; initialSearch?: boolean; steps?: string[] };
function terminal(kind: 'failed' | 'unknown' | 'resolved', code: string, summary: string): ResultSnapshot {
  const s = creatorSaveSnapshot();
  return { ...s, pending_action: null, state: kind === 'failed' ? { kind, code, summary, retryable: false } : { kind, code, summary }, batch: { outcomes: s.batch.outcomes.map(r => ({ ...r, summary, disposition: kind === 'resolved' ? { kind: 'not_run', reason: code } : {kind, code} })) } };
}
const confirmedPlan = searchPlanSnapshot(); confirmedPlan.pending_action = null; confirmedPlan.state = {kind:'resolved',code:'search_plan_confirmed',summary:'计划已确认'};
confirmedPlan.presentation.sections = [{kind:'fields',heading:'搜索计划',fields:[
  {label:'验收标准',value:'在 X 找到最终 5 位活跃且内容有实质的个人美妆创作者。以近 30 天持续发布原创美妆内容为活跃参考，查看近期帖子，优先有具体产品体验、妆容教程、试色或有依据的测评；排除主要为灌水、重复营销、抽奖、搬运或纯转发的账号。输出主页链接、近期活跃证据和代表内容及入选理由。地区、语言、粉丝数不限。先获取候选再核验，最终只交付 5 位；不保存到 OneCrew、不联系。'},
  {label:'关键词',value:'makeup、beauty、美妆'},
]}];
const read = structuredClone(SNAPSHOT); read.binding.view.kind='read_only';read.pending_action=null;read.presentation={kind:'read_only',title:'创作者搜索结果',summary:null,sections:[{kind:'fields',heading:null,fields:[{label:'创作者',value:'BeautyBlogger'},{label:'平台',value:'X'},{label:'公开粉丝数',value:'12,500'}]}]};read.batch.outcomes=[{call_id:'read.creators',operation_id:'onecrew.creator.list',disposition:{kind:'succeeded'},summary:'找到 1 位创作者。',resource_refs:[]}];read.state={kind:'resolved',code:'completed',summary:'找到 1 位创作者。'};
const selected = choiceSnapshot();selected.pending_action=null;selected.state={kind:'resolved',code:'choice_selected',summary:'已提交选择'};
function continued(state: ResultSnapshot['continuation']['state']) { const s=creatorSavedSnapshot();s.continuation={...s.continuation,state,continuation_revision:2}; return s; }
function searchNeedsSnapshot(): ResultSnapshot {
  const snapshot = choiceSnapshot();
  snapshot.presentation = {kind:'choice',title:'补充搜索需求',summary:'为厨具品牌寻找合作达人，还需要明确目标市场和合作目标。',sections:[]};
  snapshot.pending_action = {kind:'choice',questions:[
    {id:'target_market',selection_mode:'single',header:'目标市场',prompt:'你的产品主要面向哪个市场？',options:[
      {id:'us',label:'美国',description:'寻找面向美国消费者的达人'},
      {id:'jp',label:'日本',description:'寻找面向日本消费者的达人'},
      {id:'uk',label:'英国',description:'寻找面向英国消费者的达人'}]},
    {id:'collaboration_goal',selection_mode:'single',header:'合作目标',prompt:'这次合作最希望达成什么？',options:[
      {id:'product_review',label:'产品测评',description:'展示厨具的实际使用体验'},
      {id:'sales',label:'促进销售',description:'通过产品推荐和购买链接带动下单'},
      {id:'awareness',label:'品牌曝光',description:'让更多目标消费者认识品牌'}]}
  ]};
  return snapshot;
}
export const scenarios: Scenario[] = [
 {question:'首次搜索，还没有输入需求时显示什么？',answer:'只有需求输入和生成搜索范围按钮；没有预填条件，也没有确认计划按钮。输入有效内容后可提交生成请求。',initialSearch:true},
 {question:'保存一位创作者前，我需要确认什么？',answer:'核对模拟达人 Nora Beauty 的主页和联系方式。正常联系方式默认保存，冲突联系方式不自动纳入；主页入口位于姓名后。',snapshot:creatorSaveSnapshot()},
 {question:'点击保存后，能马上显示绿勾吗？',answer:'不能。先显示处理中，确认保存成功后才显示绿勾。',snapshot:{...creatorSaveSnapshot(),pending_action:null,state:{kind:'submitted',action:{kind:'confirmed'}}}},
 {question:'已经保存成功，回执有多长？',answer:'只显示一次已保存对象，标题不附加图标；少量有效结果直接展示。',snapshot:creatorSavedSnapshot()},
 {question:'明确保存失败时会被隐藏吗？',answer:'保留红叉和具体原因，不把错误记录从列表中删掉。',snapshot:terminal('failed','save_failed','保存失败，请稍后重新发起保存。')},
 {question:'请求超时，算失败还是成功？',answer:'显示结果未知。先核对当前结果，不能把未知当失败自动重做。',snapshot:terminal('unknown','save_unknown','尚未确认是否保存成功，请先检查当前结果。')},
 {question:'我取消保存，会出现成功勾吗？',answer:'显示已取消，不出现成功勾；取消完成不代表保存成功。',snapshot:terminal('resolved','user_cancelled','已取消')},
 {question:'提交修改要求后，原卡片还能提交吗？',answer:'显示已请求修改，原卡片停止提交，等待新的可确认内容。',snapshot:terminal('resolved','edit_requested','已请求修改')},
 {question:'批量保存时，怎么判断哪些人需要复核？',answer:'首位达人有 6 个正常联系方式，最多显示 3 个图标和第 4 个省略号；头像平台徽标仍为小尺寸。分数包含 0 和未评分。',snapshot:creatorBatchSnapshot()},
 {question:'五位创作者全部保存成功，如何显示？',answer:'标题汇总数量，逐行用绿勾；每位创作者的完整结果仍在机器数据中。',snapshot:creatorBatchResultSnapshot(['saved','saved','saved','saved','saved'])},
 {question:'同一批里成功、失败、未知、未执行和未选中怎么办？',answer:'五种状态分别保留；成功绿勾、失败红叉，其余状态保留短标签。',snapshot:creatorBatchResultSnapshot()},
 {question:'只完成一部分，会说全部失败吗？',answer:'显示部分完成，逐项保留已保存、失败与未选择，不抹掉已完成成果。',snapshot:creatorBatchResultSnapshot(['saved','failed','not_selected'])},
 {question:'搜索前如何确认条件？',answer:'已有条件优先展示；补充或调整需求默认收起。编辑关键词时新增标签紧邻前一项，间距 12px；已绑定的搜索操作仅供查看，调整范围需提交修改请求。',snapshot:searchPlanSnapshot()},
 {question:'确认搜索计划，等于已经搜完了吗？',answer:'不等于。显示计划已确认，等待开始搜索。',snapshot:confirmedPlan},
 {question:'找厨具合作达人，还缺哪些信息？',answer:'你提出“为我的厨具品牌找合作达人”，但尚未说明目标市场和合作目标。补充这两项后，才能准备合适的搜索范围。',snapshot:searchNeedsSnapshot()},
 {question:'只读结果也要让我点击确认吗？',answer:'不需要。直接展示业务信息，不给只读数据附加确认按钮。',snapshot:read},
 {question:'旧卡片过期后还能保存吗？',answer:'显示卡片已过期与恢复建议，关闭旧操作入口。',failure:{kind:'server',failure:{code:'expired'}}},
 {question:'提交时断网，原来的输入会消失吗？',answer:'模拟手动创建创作者时断网：保留名称、平台、账号和备注，禁用提交并提供检查当前状态；不显示待确认或重复确认提问。',snapshot:structuredClone(SNAPSHOT),recovery:true},
 {question:'保存已完成，但后续任务没有继续怎么办？',answer:'已保存回执保留；另外提示后续流程暂停，不把保存改成失败。',snapshot:continued('paused')},
 {question:'后续通知送达状态未知，要自动重发吗？',answer:'保留业务成功，说明后续通知状态未知；不盲目重发导致重复执行。',snapshot:continued('delivery_unknown')},
];

// Refreshed review data. These names and domains are fictional; no live requests are made.
const replacementNames: Record<string,string> = {
  'Demo beauty creator':'Nora Beauty', 'Dena Beauty':'Nora Beauty',
  'River Skin':'Lin Sensitive Skin', 'Mia Makeup':'Mika Makeup & Everyday Product Reviews',
  'Emma Style':'Emma Daily', 'Sike Cosmetics':'Sora Cosmetics', 'BeautyBlogger':'Nora Beauty',
};
function refreshNames(value: unknown): any {
  if (typeof value === 'string') return replacementNames[value] ?? value;
  if (Array.isArray(value)) return value.map(refreshNames);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,refreshNames(item)]));
  return value;
}
scenarios.forEach(scenario => {
  if (scenario.snapshot) scenario.snapshot = refreshNames(structuredClone(scenario.snapshot));
});
const batch = scenarios[8]!.snapshot!.pending_action;
if (batch?.kind === 'creator_save_batch') batch.review.rows.forEach(row => {
  const count = Object.values(row.preview.follower_counts ?? {})[0] ?? 0;
  row.preview.follower_counts = { [row.preview.platform]: count };
});
const plan = scenarios[12]!.snapshot!.pending_action;
if (plan?.kind === 'search_plan') plan.draft.keywords = ['cooking', 'baking', 'quick family meals', 'small kitchen recipes', 'cookware review'];
const testSteps = [
 ['确认初始条件区和确认按钮不存在','输入“找日本美妆达人”后点击生成搜索范围','只出现模拟接收提示，不凭空显示生成成功'],
 ['点击姓名后的主页箭头核对链接','核对正常联系方式和冲突提示','确认未真实保存到 OneCrew'],
 ['观察处理中说明','确认没有成功勾或可重复提交按钮'],
 ['检查对象名称只出现一次','检查标题左对齐且没有状态图标'],
 ['确认显示失败和原因','确认失败没有被当成成功或隐藏'],
 ['确认结果显示未知','确认没有自动重试写入'],
 ['确认取消状态不显示保存成功','确认已取消的原操作不能再次提交'],
 ['确认修改请求提交后的原卡片不可再确认','区别修改请求已提交与内容已更新'],
 ['检查第一行 3 个渠道加省略号','检查 0 分与未评分不同','检查长名称、头像小徽标、表头对齐','选择/取消达人，核对选择数量'],
 ['核对标题已保存 5 位且无图标','核对 5 行绿色勾、颜色 #25D766'],
 ['核对五种结果各一行','状态文字在前、图标在后','失败红色 #E45C3A，状态图标与勾同尺寸'],
 ['核对已保存、失败、未选择均保留','标题不能误称全部失败或全部成功'],
 ['默认只有搜索范围、折叠的补充入口和操作按钮','展开后标题只出现一次','点击修改搜索范围，新增一个关键词并检查 12px 间距','输入多个长关键词检查换行','核对已绑定的搜索操作只读显示，调整范围只能通过修改请求'],
 ['检查计划已确认与等待开始搜索的区别','检查搜索计划标题和内容间距'],
 ['先回答目标市场再进入合作目标','返回上一题确认答案保留','选择人工输入并检查没有常驻字节/密码提示','确认没有弹窗、数字标签切换、待确认标记'],
 ['只读结果没有确认按钮','检查平台与粉丝信息'],
 ['检查过期提示与恢复说明','确认旧操作入口不可用'],
 ['核对创作者信息完整','检查没有重复确认提问','检查断网时内容仍在，写入按钮禁用'],
 ['核对保存成功仍保留','后续暂停不能把已保存变成失败'],
 ['核对保存成功与后续送达未知分别表达','不因通知未知自动重复执行业务'],
];
scenarios.forEach((scenario,index)=>{ scenario.steps = testSteps[index]; });
