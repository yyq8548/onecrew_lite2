import { discoveryNonContact } from "./contacts.ts";
import { equivalentSourceUrl } from "../../../onecrew_lite-opencli/onecrew-browser-search/source-url.mjs";

const contactPaths = new Set(["contact", "contact-us", "contacts", "business", "cooperation", "collaboration", "partnerships", "kontakt", "contacto", "contato", "contatti", "kontaktieren", "contacter"]);
const words = (value: string) => value.split(/[^\p{Alphabetic}\p{Number}]+/u).filter(Boolean).map(value => value.toLowerCase());
export const trimDiscoveryLink = (value: string) => value.replace(/^[()[\]<>,.;!"']+|[()[\]<>,.;!"']+$/gu, "");
export function discoveryObservesLink(description: string, link: string): boolean {
  return Buffer.byteLength(description) <= 16000 && description.split(/\p{White_Space}+/u).some(token => !token.includes("...") && !token.includes("…") && (trimDiscoveryLink(token) === link || equivalentSourceUrl(trimDiscoveryLink(token), link)));
}
export function discoveryCommunityInvite(link: string): boolean {
  const url = URL.parse(link);
  return url !== null && (["discord.gg", "www.discord.gg"].includes(url.hostname) || ["discord.com", "www.discord.com", "discordapp.com", "www.discordapp.com"].includes(url.hostname) && (url.pathname === "/invite" || url.pathname.startsWith("/invite/")));
}
export function discoveryExactUsername(source: string): string | null {
  const pinterest = /^https:\/\/www\.pinterest\.com\/([^/]+)\/$/u.exec(source)?.[1];
  if (pinterest !== undefined) return /^[A-Za-z0-9_]{1,30}$/u.test(pinterest) ? pinterest : null;
  const hackernews = /^https:\/\/news\.ycombinator\.com\/user\?id=([A-Za-z0-9_-]{1,64})$/u.exec(source)?.[1];
  if (hackernews !== undefined) return hackernews;
  const url = URL.parse(source); if (!url || url.protocol !== "https:" || url.username || url.password || url.port) return null;
  const parts = url.pathname.split("/").filter(Boolean), host = url.hostname;
  let name: string | undefined;
  if (["x.com", "twitter.com", "www.twitter.com"].includes(host) && parts.length === 1) name = parts[0];
  else if (["www.tiktok.com", "tiktok.com", "www.youtube.com", "youtube.com"].includes(host) && parts.length === 1 && parts[0]!.startsWith("@")) name = parts[0]!.slice(1);
  else if (["www.reddit.com", "reddit.com"].includes(host) && parts.length === 2 && ["user", "u"].includes(parts[0]!)) name = parts[1];
  else if (["www.instagram.com", "instagram.com"].includes(host) && parts.length === 1 && Buffer.byteLength(parts[0]!) <= 30) name = parts[0];
  else if (host === "www.zhihu.com" && parts.length === 2 && parts[0] === "people") name = parts[1];
  return name !== undefined && /^[A-Za-z0-9_.-]{2,64}$/u.test(name) ? name : null;
}
export function discoveryContactPriority(source: string, nativeProfile: string | null = null): number {
  if (nativeProfile === source) return 0;
  const url = URL.parse(source); if (!url) return 9;
  if (url.pathname.split("/").some(part => contactPaths.has(part.toLowerCase()))) return 1;
  const host = url.hostname.replace(/^www\./u, "");
  if (["linktr.ee", "beacons.ai", "bio.site", "carrd.co", "solo.to", "t.me"].includes(host) || discoveryExactUsername(source) !== null) return 2;
  return ["bit.ly", "tinyurl.com", "t.co", "shorturl.at", "goo.gl"].includes(host) ? 8 : 4;
}
export function discoveryThirdPartyLink(source: string, text: string): boolean {
  let credits = false, found = false;
  for (const line of text.split(/\r?\n/u)) {
    const label = line.trim().replace(/^#+/u, "").trim().toLowerCase();
    if (!label) { credits = false; continue; }
    const explicit = ["follow the staff", "follow our staff", "follow the guests", "guest:", "guests:", "edited by:", "filmed by:", "关注工作人员", "關注工作人員", "嘉宾：", "嘉賓：", "制作人员：", "製作人員：", "出演者：", "ゲスト：", "출연진:", "게스트:", "invitados:", "invités :", "gäste:"].some(prefix => label.startsWith(prefix));
    if (!line.includes("https://") && /[:：]$/u.test(label)) credits = explicit; else if (explicit) credits = true;
    if (discoveryObservesLink(line, source)) { found = true; if (!credits) return false; }
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
    if (productSection || /(?:affiliate|discount|coupon|promo code|sponsored|shopmy|折扣|优惠|優惠|购买|購買|产品链接|產品連結)/iu.test(label)) return true;
    if (["contact", "business", "website", "email", "联系", "商务", "官网"].some(word => label.includes(word))) return false;
    return ["use code ", "discount code ", "sizing chart", "typing demo", "my tutorials", "my unboxing", "ranking spreadsheet", "my edited version", "affiliate links", "keyboard supplies", "sponsored links", "video setup", "photo setup", "stream setup", "newsletter", "get your", "my gear", "my setup"].some(word => label.includes(word)) || label.startsWith("$") || ["buy ", "shop ", "my mouse", "my books:", "my affiliates:", "购买", "购买链接", "优惠链接"].some(word => label.startsWith(word));
  });
}
export function discoveryContactPageLinks(source: string, markdown: string): string[] {
  const base = URL.parse(source); if (!base || Buffer.byteLength(markdown) > 20000) return [];
  const result: string[] = [];
  for (const match of markdown.matchAll(/https:\/\/[^\s<>\[\]()+]+|\]\((\/[^\s()]+)\)/gu)) {
    const raw = match[1] ?? match[0]; if (raw.includes("...") || raw.includes("…")) continue;
    const token = raw.replace(/[,.;!"']+$/u, ""), url = URL.parse(token, base);
    if (!url || url.origin !== base.origin || url.protocol !== "https:" || url.username || url.password || url.port || Buffer.byteLength(url.href) > 2048 || !url.pathname.toLowerCase().split("/").some(part => contactPaths.has(part))) continue;
    if (!result.includes(url.href)) result.push(url.href);
  }
  return result;
}
export function discoveryContactReadKey(source: string): string {
  const url = URL.parse(source); if (!url) return source;
  if (["twitter.com", "www.twitter.com", "www.x.com"].includes(url.hostname)) url.hostname = "x.com";
  url.hash = "";
  for (const key of [...url.searchParams.keys()]) if (/^utm_/iu.test(key) || ["fbclid", "gclid", "igshid"].includes(key.toLowerCase())) url.searchParams.delete(key);
  if (["youtube.com", "www.youtube.com"].includes(url.hostname)) {
    const parts = url.pathname.replace(/^\/+|\/+$/gu, "").split("/");
    if (parts.length === 3 && parts[0] === "channel" && parts[2] === "about" || parts.length === 2 && parts[0]!.startsWith("@") && parts[1] === "about") { url.pathname = `/${parts.slice(0, -1).join("/")}`; url.search = ""; }
  }
  if (discoveryExactUsername(url.href) !== null) { url.search = ""; url.pathname = url.pathname.replace(/\/+$/u, ""); }
  return url.href;
}
export function discoveryContactLeadMatches(author: string, username: string | null, source: string, title: string): boolean {
  const url = URL.parse(source); if (!url || url.protocol !== "https:" || url.username || url.password || discoveryNonContact(source)) return false;
  const titleWords = words(title), priority = discoveryContactPriority(source);
  return [author, username].some(name => {
    if (name === null) return false; const parts = words(name), compact = parts.join(""); if ([...compact].length < 2) return false;
    return (priority <= 2 || ["商务", "联系", "合作", "contact", "email"].some(word => title.toLowerCase().includes(word))) && /[\u3400-\u9fff\u3040-\u30ff\uac00-\ud7af]/u.test(name) && title.toLowerCase().includes(name.toLowerCase())
      || (priority <= 2 || titleWords.some(word => ["contact", "email", "business", "partnership"].includes(word))) && titleWords.some((_, index) => parts.every((part, offset) => titleWords[index + offset] === part))
      || url.hostname.split(".").some(label => words(label).join("") === compact)
      || discoveryExactUsername(source) !== null && url.pathname.split("/").some(segment => words(segment.replace(/^@/u, "")).join("") === compact);
  });
}
export function discoveryContactLeadPriority(author: string, username: string | null, source: string, title: string): number {
  const base = discoveryContactPriority(source); if (base <= 2) return base;
  const url = URL.parse(source); if (!url) return 9;
  if ([author, username].some(name => name !== null && url.hostname.replace(/^www\./u, "").split(".").some(part => words(part).join("") === words(name).join("")))) return 3;
  return ["email", "contact", "business", "商务", "邮箱", "聯絡", "お問い合わせ", "문의", "kontakt", "contato", "contatti", "correo", "联系", "partnership", "сотрудничество", "تواصل"].some(word => title.toLowerCase().includes(word)) ? 4 : 7;
}
export function discoveryLinkedUsername(text: string): string | null {
  for (const match of text.matchAll(/https:\/\/[^\s<>\[\]()]+/gu)) {
    const source = match[0].replace(/[,.;]+$/u, "");
    if (!discoveryThirdPartyLink(source, text)) { const name = discoveryExactUsername(source); if (name !== null) return name; }
  }
  return null;
}
export function discoveryContactSearchQuery(name: string, language: string, translated: readonly string[]): string {
  const alternatives: readonly [readonly string[], string][] = [
    [["中文", "汉语", "繁体中文", "简体中文", "chinese", "zh"], "邮箱 OR 商务合作 OR 联系 OR 官网"],
    [["日语", "日本語", "japanese", "ja"], "メール OR お問い合わせ OR 仕事依頼"], [["韩语", "한국어", "korean", "ko"], "이메일 OR 문의 OR 비즈니스"],
    [["西班牙语", "español", "spanish", "es"], "correo OR contacto OR colaboración"], [["法语", "français", "french", "fr"], "courriel OR contact OR partenariat"],
    [["德语", "deutsch", "german", "de"], "E-Mail OR Kontakt OR Kooperation"], [["葡萄牙语", "português", "portuguese", "pt"], "email OR contato OR parceria"],
    [["意大利语", "italiano", "italian", "it"], "email OR contatti OR collaborazione"], [["俄语", "русский", "russian", "ru"], "почта OR контакты OR сотрудничество"],
    [["阿拉伯语", "العربية", "arabic", "ar"], "بريد OR تواصل OR تعاون"], [["印地语", "हिन्दी", "hindi", "hi"], "ईमेल OR संपर्क OR सहयोग"],
    [["泰语", "ไทย", "thai", "th"], "อีเมล OR ติดต่อ OR งานโฆษณา"], [["越南语", "vietnamese", "vi"], "email OR liên hệ OR hợp tác"],
    [["印尼语", "indonesian", "id"], "email OR kontak OR kerja sama"], [["土耳其语", "turkish", "tr"], "e-posta OR iletişim OR işbirliği"],
    [["荷兰语", "dutch", "nl"], "e-mail OR contact OR samenwerking"], [["波兰语", "polish", "pl"], "email OR kontakt OR współpraca"], [["乌克兰语", "ukrainian", "uk"], "пошта OR контакти OR співпраця"],
  ];
  const alias = language.trim().toLowerCase().split(/[-_]/u)[0]!, terms = alternatives.find(([names]) => names.includes(alias))?.[1] ?? "contact";
  const valid = translated.length > 0 && translated.length <= 6 && translated.every(term => !!term.trim() && [...term].length <= 50 && !/[\p{Cc}"():|<>\\/@]/u.test(term));
  const localized = valid ? translated.map(term => `"${term.trim()}"`).join(" OR ") : terms;
  return `"${name}" (${localized} OR email OR Telegram)`;
}
