import { scenarios } from './design-scenarios';
import { creatorBatchReview } from './creator-batch-fixture';
import type { AppLocale } from '../lib/i18n';
const qa = [
 ['What appears before the first search request?','Only the request field and Generate search scope button. No preset filters or confirmation action.'],
 ['What do I review before saving a creator?','Review who and which contacts you are saving. Show conflicts only when they affect this selection.'],
 ['Does clicking Save mean success?','No. Show processing until the server confirms the outcome.'],
 ['How much should a save receipt show?','Show the saved creator once. Display a few useful fields directly, without another expansion.'],
 ['Are failed records hidden?','No. Keep the failure and its reason visible.'],
 ['Does a timeout mean failure?','The outcome is unknown. Check the current state before repeating the operation.'],
 ['Does cancelling show a success check?','No. Show a neutral cancelled result.'],
 ['Can the old card submit after an edit request?','No. Wait for the revised confirmation.'],
 ['How do I spot issues during batch review?','Show identity and contact conflicts only when they affect selected creators and contacts.'],
 ['What if all five creators are saved?','Show the saved count and a check beside each creator.'],
 ['What if a batch has five different outcomes?','Keep saved, failed, unknown, not run and not selected distinct.'],
 ['Does partial completion mean everything failed?','No. Preserve successful records and show partial completion.'],
 ['What do I review before searching?','Review criteria, keywords and the bound search operations before confirming.'],
 ['Does confirming a plan mean the search is complete?','No. The plan is confirmed and waiting to start searching.'],
 ['What is missing from this cookware creator request?','You asked for cookware creators but have not specified the target market or collaboration goal. Answer these two questions to prepare a suitable search scope.'],
 ['Does a read-only result need confirmation?','No. Display the business information directly.'],
 ['Can an expired card still save?','No. Explain expiry and disable the old action.'],
 ['Will a connection error discard my input?','Keep the creator details, pause writes and offer a state check.'],
 ['What if saving succeeded but continuation paused?','Keep the saved result and explain the paused follow-up separately.'],
 ['Should an uncertain notification be sent again?','Keep the business result and check delivery before sending again.'],
];
const pairs: [string,string][] = [
 ['Complete your search request','补充搜索需求'],
 ['To find creators for your cookware brand, specify the target market and collaboration goal.','为厨具品牌寻找合作达人，还需要明确目标市场和合作目标。'],
 ['Target market','目标市场'],['Which market is your product intended for?','你的产品主要面向哪个市场？'],
 ['United States','美国'],['Japan','日本'],['United Kingdom','英国'],
 ['Find creators reaching US consumers','寻找面向美国消费者的达人'],['Find creators reaching Japanese consumers','寻找面向日本消费者的达人'],['Find creators reaching UK consumers','寻找面向英国消费者的达人'],
 ['Collaboration goal','合作目标'],['What is the main goal of this collaboration?','这次合作最希望达成什么？'],
 ['Product reviews','产品测评'],['Show the cookware in real use','展示厨具的实际使用体验'],['Drive sales','促进销售'],['Encourage purchases through recommendations and shopping links','通过产品推荐和购买链接带动下单'],['Brand awareness','品牌曝光'],['Introduce the brand to more target customers','让更多目标消费者认识品牌'],

 ['Find US cooking creators with ≥ 1,000 followers.','寻找美国烹饪类创作者，粉丝 ≥ 1,000。'],['Review Creator save','审核创作者保存'],['Select routes and confirm','选择联系方式并确认'],['Creator saved','创作者已保存'],['Creator saved to OneCrew.','创作者已保存到 OneCrew。'],['Review search plan','审核搜索计划'],['Edit acceptance criteria and keywords, then confirm before searching.','核对搜索条件与关键词后确认。'],
 ['Create creator manually','手动创建创作者'],['Review the name, platform and handle before creating.','确认名称、平台和账号后再创建。'],['Creator details','创作者信息'],['Platform','平台'],['Name','名称'],['Handle','账号'],['Notes','备注'],['Create this creator?','确认创建这位创作者？'],['Create creator','确认创建'],['The creator was created.','创作者已创建。'],
 ['Choose what to save','选择保存方式'],['Choose an option to continue','请选择后继续'],['Save scope','保存范围'],['Choose what to save','请选择保存范围'],['Save only the public email','仅保存公开邮箱'],['All trusted contact routes','全部可信路由'],['Notification','通知'],['Continue the task?','是否继续任务？'],['Yes','是'],['No','否'],
 ['Creator search results','创作者搜索结果'],['Creator','创作者'],['Public followers','公开粉丝数'],['Found one creator.','找到 1 位创作者。'],['The plan is confirmed.','计划已确认'],['Save failed. Please start a new save request later.','保存失败，请稍后重新发起保存。'],['Saving has not been confirmed. Check the current result first.','尚未确认是否保存成功，请先检查当前结果。'],['Cancelled','已取消'],['Edit requested','已请求修改'],['Your selection was submitted.','已提交选择'],
 ['Beauty and skincare creator. Public profile reviewed.','美妆与护肤创作者，已查看公开主页。'],['Partnership fit still needs review.','尚需核对后续合作需求。'],['Review creators','审核 Creator 列表'],['Select the creators to save.','选择需要保存的 Creator。'],['Some outcomes are unknown. Check each record.','存在未知结果，请逐项核对。'],['Some creators could not be saved.','部分 Creator 未能保存。'],['Selected creators have been saved.','所选 Creator 已保存。'],
 ['Email','邮箱'],['Search plan','搜索计划'],['Acceptance criteria','验收标准'],['Keywords','关键词'],
];
const assessments=['Public profile reviewed. Beauty and skincare content. Zero is an explicit example score, not an unscored result. Check posting frequency and contact sources.','Sensitive-skin and ingredient content appears relevant. Check recent posts before outreach.','Makeup tutorials and comparisons. The email comes from the public bio and still needs review.','Regular beauty and styling videos. Keep the platform profile; email and website are optional.'];
creatorBatchReview().rows.slice(0,4).forEach((row,i)=>pairs.push([assessments[i]!,row.preview.assessment!]));
export function localizedScenario(index:number,locale:AppLocale){
 const s=structuredClone(scenarios[index]!);
 const translate=(value:unknown):any=>{
  if(typeof value==='string')return pairs.find(p=>p[locale==='zh-CN'?0:1]===value)?.[locale==='zh-CN'?1:0]??value;
  if(Array.isArray(value))return value.map(translate);
  if(value && typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,translate(v)]));
  return value;
 };
 // Only fixtures are translated here. Production snapshots remain unchanged.
 if(s.snapshot){s.snapshot=translate(s.snapshot);s.snapshot!.locale=locale;}
 if(locale==='en-US'){s.question=qa[index]![0]!;s.answer=qa[index]![1]!;}

 return s;
}
