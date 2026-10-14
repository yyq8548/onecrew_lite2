import type { AppLocale } from './i18n';
// Only known presentation labels are localized. IDs and user data are never rewritten.
const labels: Record<string,string> = {
 'public profile':'公开主页', 'profile bio':'主页简介', Creator:'创作者', 'Creator ID':'创作者编号', Name:'名称', Status:'状态', Platform:'平台', Platforms:'平台', Account:'账号', Accounts:'账号', Keywords:'关键词', Regions:'地区', Description:'描述', Notes:'备注', Assessment:'评价', Score:'评分', Followers:'粉丝', 'Display Name':'显示名称', Handle:'账号名', 'Profile URL':'主页链接', 'Search plan':'搜索计划', 'Acceptance criteria':'验收标准', Email:'邮箱', website:'网站', email:'邮箱', followers:'粉丝', Sources:'来源', 'Contact routes':'联系方式',
 // A confirmation card's own chrome. The adapter projects these at mint time,
 // before the card has a locale, so the card that asks for approval is the one
 // surface the server cannot translate — and it is the first one a user reads.
 'Review every submitted value':'逐项核对提交的内容', Confirm:'确认', Operations:'操作', 'Execution result':'执行结果',
 // Argument labels the exact projection derives from tool schema keys.
 Confirmed:'已确认', 'Idempotency Key':'幂等键', 'Mailbox Ref Id':'邮箱标识', Subject:'主题', Text:'正文', To:'收件人', From:'发件人', Cc:'抄送', Bcc:'密送', Html:'HTML 正文',
};
export function displayLabel(value: string, locale: AppLocale): string {
 if(locale==='zh-CN')return labels[value] ?? value;
 return Object.entries(labels).find(([,zh])=>zh===value)?.[0] ?? value;
}
