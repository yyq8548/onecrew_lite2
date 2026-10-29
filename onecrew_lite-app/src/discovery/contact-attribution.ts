import { equivalentSourceUrl } from "../../../onecrew_lite-opencli/onecrew-browser-search/source-url.mjs";

export const trimDiscoveryLink = (value: string) => value.replace(/^[()[\]<>,.;!"']+|[()[\]<>,.;!"']+$/gu, "");
export function discoveryObservesLink(description: string, link: string): boolean {
  return Buffer.byteLength(description) <= 20000 && [...description.matchAll(/(?:https:\/\/|www\.)[^\s<>"']+/gu)].some(match => {
    const value = trimDiscoveryLink(match[0]); return !value.includes("...") && !value.includes("…") && (value === link || equivalentSourceUrl(value, link));
  });
}
export function discoveryThirdPartyLink(source: string, text: string, owner = ""): boolean {
  let credits = false, found = false;
  const ownerWords = owner.toLowerCase().split(/[^\p{Alphabetic}\p{Number}]+/u).filter(Boolean);
  for (const line of text.split(/\r?\n/u)) {
    const label = line.trim().replace(/^#+/u, "").trim().toLowerCase();
    if (!label) { credits = false; continue; }
    const named = /^(?:find|follow|connect with)\s+(.+?)\s+on social(?:s| media)?\s*[:：]?$/iu.exec(label)?.[1];
    const self = named !== undefined && (["me", "us", "our team"].includes(named) || named.split(/[^\p{Alphabetic}\p{Number}]+/u).filter(Boolean).every(word => ownerWords.includes(word)));
    const explicit = ["follow the staff", "follow our staff", "follow the guests", "guest:", "guests:", "edited by:", "filmed by:", "关注工作人员", "關注工作人員", "嘉宾：", "嘉賓：", "制作人员：", "製作人員：", "出演者：", "ゲスト：", "출연진:", "게스트:", "invitados:", "invités :", "gäste:"].some(prefix => label.startsWith(prefix));
    if (named !== undefined) credits = !self;
    else if (!line.includes("https://") && /[:：]$/u.test(label)) credits = explicit;
    else if (explicit) credits = true;
    if (discoveryObservesLink(line, source) || !source.startsWith("https://") && line.includes(source)) { found = true; if (!credits) return false; }
  }
  return found;
}
export function discoveryPromotional(source: string, text: string): boolean {
  const lines = text.split(/\r?\n/u);
  let productSection = false;
  return lines.some((line, index) => {
    if (!line.trim()) productSection = false;
    if (!line.includes("https://") && /products? (?:used|mentioned)|shop (?:the|this) (?:look|video)|使用(?:的)?产品|使用產品|产品清单|產品清單|本期好物|优惠(?:码|碼)/iu.test(line)) productSection = true;
    if (!discoveryObservesLink(line, source)) return false;
    const label = (line.trim() === source ? `${lines[index - 1] ?? ""} ${line}` : line).trim().toLowerCase();
    if (["business inquiries", "business enquiries", "contact me", "my website", "商务联系", "商務聯絡", "合作洽谈", "我的官网", "我的官網"].some(word => label.includes(word))) return false;
    const url = URL.parse(source);
    if (productSection || /(?:affiliate|discount|coupon|promo code|sponsored|shopmy|折扣|优惠|優惠|购买|購買|产品链接|產品連結|masterclass|donat(?:e|ion)|一站式.*工具)/iu.test(label) || url?.hostname.endsWith(".pxf.io")) return true;
    if (["contact", "business", "website", "email", "联系", "商务", "官网"].some(word => label.includes(word))) return false;
    return ["use code ", "discount code ", "sizing chart", "typing demo", "my tutorials", "my unboxing", "ranking spreadsheet", "my edited version", "affiliate links", "keyboard supplies", "sponsored links", "video setup", "photo setup", "stream setup", "newsletter", "get your", "my gear", "my setup"].some(word => label.includes(word)) || label.startsWith("$") || ["buy ", "shop ", "my mouse", "my books:", "my affiliates:", "购买", "购买链接", "优惠链接"].some(word => label.startsWith(word));
  });
}
