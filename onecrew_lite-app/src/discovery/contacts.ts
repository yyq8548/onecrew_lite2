import type { z } from "zod";
import type { discoveryContact } from "./library-material.ts";
import { isIP } from "node:net";

export type DiscoveryContact = z.infer<typeof discoveryContact>;
const host = (value: string): string | null => { const name = URL.parse(value)?.hostname.replace(/^www\./u, ""); return name === "twitter.com" ? "x.com" : name === "space.bilibili.com" ? "bilibili.com" : name === "m.facebook.com" ? "facebook.com" : name ?? null; };
export function discoveryContactPlatform(value: string): string | null { return ({ "youtube.com": "youtube", "bilibili.com": "bilibili", "x.com": "twitter", "instagram.com": "instagram", "facebook.com": "facebook", "tiktok.com": "tiktok", "douyin.com": "douyin", "linkedin.com": "linkedin", "bsky.app": "bluesky", "xiaohongshu.com": "xiaohongshu" } as Record<string, string>)[host(value) ?? ""] ?? null; }
export function discoverySocialProfile(value: string): boolean {
  const url = URL.parse(value); if (!url) return false;
  const hostname = url.hostname.replace(/^www\./u, ""), path = url.pathname;
  if (url.protocol === "https:" && hostname === "facebook.com" && path === "/profile.php" && !url.username && !url.password && !url.port && !url.hash) { const pairs = [...url.searchParams]; return pairs.length === 1 && pairs[0]![0] === "id" && /^[1-9][0-9]{0,19}$/u.test(pairs[0]![1]); }
  if (url.protocol !== "https:" || url.username || url.password || url.port || url.search || url.hash || value.includes("...") || value.includes("…")) return false;
  const parts = path.replace(/^\/+|\/+$/gu, "").split("/"), handle = (part: string) => Buffer.byteLength(part) <= 128 && /^[\p{Alphabetic}\p{Number}_.%\-]+$/u.test(part);
  if (hostname === "xiaohongshu.com") return /^\/user\/profile\/[a-f0-9]{24}$/u.test(path);
  if (["instagram.com", "x.com", "twitter.com"].includes(hostname)) return parts.length === 1 && handle(parts[0]!) && !["home", "explore", "search", "i", "accounts", "reel", "reels", "p"].includes(parts[0]!);
  if (hostname === "tiktok.com") return parts.length === 1 && parts[0]!.startsWith("@") && handle(parts[0]!.slice(1));
  if (hostname === "douyin.com") return parts.length === 2 && parts[0] === "user" && /^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/u.test(parts[1]!);
  if (hostname === "youtube.com") return parts.length === 1 && parts[0]!.startsWith("@") && handle(parts[0]!.slice(1)) || parts.length === 2 && parts[0] === "channel" && parts[1]!.startsWith("UC") && Buffer.byteLength(parts[1]!) === 24;
  if (hostname === "space.bilibili.com") return parts.length === 1 && /^[0-9]+$/u.test(parts[0]!);
  if (hostname === "linkedin.com") return parts.length === 2 && parts[0] === "in" && handle(parts[1]!);
  if (hostname === "facebook.com") return parts.length === 1 && handle(parts[0]!) && !["groups", "watch", "login", "share", "reel", "marketplace"].includes(parts[0]!);
  return hostname === "bsky.app" && parts.length === 2 && parts[0] === "profile" && handle(parts[1]!);
}
export function discoveryNonContact(source: string): boolean {
  const url = URL.parse(source); if (!url) return true;
  const name = url.hostname.replace(/^www\./u, ""), path = url.pathname, parts = path.split("/");
  if (name === "go.shopmy.us") return true;
  if (["amzn.to", "geni.us", "a.co", "amzn.eu"].includes(url.hostname) || /(?:^|\.)amazon\.(?:com|co\.uk)$/u.test(url.hostname) || url.hostname.startsWith("shop.") && parts.includes("collections") || parts.some(part => ["dp", "products"].includes(part))) return true;
  return ["docs.google.com", "drive.google.com", "inex.osekai.net", "osekai.net", "youtu.be", "collabs.shop", "rover.ebay.com", "rocketreach.co", "contactout.com", "muckrack.com", "instainflu.com", "scrumball.com", "shop.anker.com", "ankerfast.club", "lddy.no", "rstyle.me", "go.magik.ly", "c.klarna.com", "aritzia.com", "kit.co", "store.cablemod.com", "go.hototools.com", "hototools.com"].includes(name)
    || name === "wooting.io" && !["/contact", "/contact/"].includes(path)
    || ["x.com", "twitter.com"].includes(name) && path.includes("/status/")
    || name === "instagram.com" && ["/p/", "/reel/", "/reels/"].some(prefix => path.startsWith(prefix))
    || name === "tiktok.com" && (path.includes("/video/") || path.startsWith("/discover/"))
    || name === "linkedin.com" && ["/posts/", "/feed/"].some(prefix => path.startsWith(prefix))
    || parts.some(part => ["form-submission-response", "checkout", "cart", "shop", "store", "stores", "product", "donate", "donation", "donations", "privacy-policy", "terms-of-service", "discord"].includes(part))
    || name === "bit.ly" && ["keychron", "k65plus", "hyper-next-drive"].some(slug => path.toLowerCase().includes(slug))
    || name === "youtube.com" && (["/watch", "/playlist"].includes(path) || path.startsWith("/shorts/") || path.endsWith("/join"))
    || name === "cherryxtrfy.com" && ["/mice/", "/xtrfy-"].some(prefix => path.startsWith(prefix))
    || [...url.searchParams.keys()].some(key => ["partner_id", "sca_ref", "aff_id"].includes(key));
}
function thirdParty(source: string, text: string): boolean {
  let credits = false, found = false;
  for (const line of text.split(/\r?\n/u)) {
    const label = line.trim().replace(/^#+/u, "").trim().toLowerCase(); if (!label) { credits = false; continue; }
    const explicit = ["follow the staff", "follow our staff", "follow the guests", "guest:", "guests:", "edited by:", "filmed by:", "关注工作人员", "關注工作人員", "嘉宾：", "嘉賓：", "制作人员：", "製作人員：", "出演者：", "ゲスト：", "출연진:", "게스트:", "invitados:", "invités :", "gäste:"].some(prefix => label.startsWith(prefix));
    if (!line.includes("https://") && /[:：]$/u.test(label)) credits = explicit; else if (explicit) credits = true;
    if (Buffer.byteLength(line) <= 16000 && line.split(/\s+/u).some(token => !token.includes("...") && !token.includes("…") && token.replace(/^[()[\]<>,.;!"']+|[()[\]<>,.;!"']+$/gu, "") === source)) { found = true; if (!credits) return false; }
  }
  return found;
}

export function extractDiscoveryContacts(text: string, source: string, author: string, capturedAt: number | bigint, attribution: DiscoveryContact["attribution"]): DiscoveryContact[] {
  const url = URL.parse(source);
  if (Buffer.byteLength(text) > 20000 || !author || Buffer.byteLength(author) > 128 || Buffer.byteLength(source) > 2048 || capturedAt <= 0 || !url || url.protocol !== "https:" || !url.hostname || url.username || url.password) return [];
  const contacts: DiscoveryContact[] = [], seen = new Set<string>(), telegramBoundaries = new Set<number>();
  const sourceLine = (start: number, length: number) => {
    const beginning = text.lastIndexOf("\n", start - 1) + 1, newline = text.indexOf("\n", start + length), end = newline < 0 ? text.length : newline;
    return text.slice(Math.max(beginning, start - 150), Math.min(end, start + length + 150)).trim();
  };
  const push = (kind: DiscoveryContact["kind"], original: string, normalized: string, excerpt: string) => { contacts.push({ kind, original_value: original, normalized_value: normalized, source_url: source, source_excerpt: excerpt, captured_at: capturedAt, native_author_id: author, attribution }); };
  for (const [kind, expression] of [["email", /[a-z0-9][a-z0-9.!#$%&'*+/=?^_`{|}~\-]*@[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+/giu], ["telegram", /https:\/\/(?:t\.me|telegram\.me)\/[a-z0-9_+/-]+(?:\?[a-z0-9_=&%\-]+)?/giu]] as const) {
    for (const match of text.matchAll(expression)) {
      let original = match[0]; const start = match.index;
      const gluedLabel = kind === "email" && /Telegram$/iu.test(original) && /\.[a-z0-9-]{2,63}$/iu.test(original.slice(0, -8))
        && /^[:：][ \t]*@?[a-z][a-z0-9_]{4,31}(?:$|[\s,，;；|])/iu.test(text.slice(start + original.length));
      if (gluedLabel) { original = original.slice(0, -8); telegramBoundaries.add(start + original.length); }
      const end = start + original.length, before = [...text.slice(0, start)].at(-1), after = gluedLabel ? undefined : [...text.slice(end)][0];
      if (before && /[A-Za-z0-9@/_]/u.test(before) || after && /[A-Za-z0-9@_…]/u.test(after) || text.slice(end).startsWith("...") || Buffer.byteLength(original) > 2048) continue;
      let normalized: string;
      if (kind === "email") { const at = original.lastIndexOf("@"), local = original.slice(0, at); if (local.length > 64 || original.length > 254 || local.endsWith(".") || local.includes("..")) continue; normalized = `${local}@${original.slice(at + 1).toLowerCase()}`; }
      else { const targetUrl = new URL(original), target = targetUrl.pathname.replace(/^\/+/u, ""); if (!target || ["share", "iv", "login", "proxy", "socks"].includes(target.toLowerCase()) || target.startsWith("share/") || target.startsWith("s/") || target.includes("/") && !target.startsWith("joinchat/")) continue; normalized = `https://t.me/${target}${targetUrl.search}`; }
      if (seen.has(normalized)) continue; seen.add(normalized);
      push(kind, original, normalized, sourceLine(start, original.length));
    }
  }
  for (const match of text.matchAll(/(?:telegram|tg|电报)[ \t]*[:：][ \t]*@?([a-z][a-z0-9_]{4,31})/gimu)) {
    if (match.index > 0 && !/[\s,，;；|]/u.test(text[match.index - 1]!) && !telegramBoundaries.has(match.index)) continue;
    const end = match.index + match[0].length, after = [...text.slice(end)][0]; if (after && /[\p{Alphabetic}\p{Number}_…/@.\-]/u.test(after) || text.slice(end).startsWith("...")) continue;
    const normalized = `https://t.me/${match[1]!}`; if (seen.has(normalized)) continue; seen.add(normalized); push("telegram", match[0].trim(), normalized, sourceLine(match.index, match[0].length));
  }
  for (const match of text.matchAll(/https:\/\/[^\s<>"']+/gu)) {
    const value = match[0].replace(/[.,;)\]]+$/u, ""); if (!discoverySocialProfile(value) || host(value) === host(source) || thirdParty(value, text) || seen.has(value)) continue; seen.add(value); push("social_profile", value, value, sourceLine(match.index, value.length));
  }
  if (attribution === "author_published") {
    for (const match of text.matchAll(/(?:https?:\/\/|www\.)[^\s<>"']+/gu)) {
      const original = match[0].replace(/[.,;)\]]+$/u, ""), target = URL.parse(original.startsWith("www.") ? `https://${original}` : original);
      const excerpt = sourceLine(match.index, original.length);
      if (!target || !["http:", "https:"].includes(target.protocol) || target.username || target.password || target.port || original.length > 2048 || seen.has(original)) continue;
      if (host(source) === "facebook.com" && target.href === `https://www.facebook.com/messages/t/${author}/`) {
        seen.add(original); push("messenger", original, original, excerpt); continue;
      }
      const hostname = target.hostname;
      if (!hostname.includes(".") || isIP(hostname.replace(/^\[|\]$/gu, "")) || /(?:^|\.)(?:localhost|local|internal|test)$/iu.test(hostname)
        || discoveryContactPlatform(target.href) !== null || ["t.me", "telegram.me", "wa.me", "vt.tiktok.com", "vm.tiktok.com"].includes(hostname)
        || discoveryNonContact(target.href) || thirdParty(original, text) || !/(?:website|contact|business|官网|官網|网站|網站|商务|商務|合作)/iu.test(excerpt)) continue;
      seen.add(original); push("website", original, original, excerpt);
    }
    for (const match of text.matchAll(/(?:phone|mobile|telephone|tel|电话|電話|手机|手機)[ \t]*[:：][ \t]*(\+?[0-9][0-9 ()-]{5,23}[0-9])/giu)) {
      const value = match[1]!, digits = value.replace(/[^0-9]/gu, ""), normalized = `${value.startsWith("+") ? "+" : ""}${digits}`;
      if (digits.length < 7 || digits.length > 15 || seen.has(normalized)) continue;
      seen.add(normalized); push("phone", value, normalized, sourceLine(match.index, match[0].length));
    }
    const messaging: DiscoveryContact[] = [], messagingSeen = new Map<string, DiscoveryContact>();
    for (const [kind, pattern] of [
      ["wechat", /(?:微信号?|wechat|\bwx|\bvx|商务(?:合作)?(?:[ \t]*(?:wx|vx|v|微信))?|[🌍🌎🌏][\uFE0F \t]*[:：])[ \t]*[:：]?[ \t]*([a-z][a-z0-9_-]{5,19})(?:[ \t]*[（(]([^\r\n）)]{1,100})[）)])?/giu],
      ["wechat", /[🌍🌎🌏][\uFE0F \t]*([Vv][0-9]{5,19})(?:[ \t]*[（(]([^\r\n）)]{1,100})[）)])?/gu],
      ["wechat_official", /(?:微信公众号|公众号)[ \t]*(?:(?:叫|是|名称)[ \t]*)?(?:[:：]?[ \t]*[「『“【"]([^」』”】"\r\n]{1,80})[」』”】"]|[:：][ \t]*([^\s，,；;（）()「『“【"]{1,80}))/gu],
      ["whatsapp", /https:\/\/wa\.me\/([1-9][0-9]{6,14})\b|whatsapp[ \t]*[:：][ \t]*(\+[1-9][0-9 ()-]{6,24}[0-9])/giu],
    ] as const) for (const match of text.matchAll(pattern)) {
      const original = match[0], remaining = text.slice(match.index + original.length), raw = match[1] ?? match[2]!;
      if (kind === "wechat" && original.startsWith("商务") && !["bilibili.com", "douyin.com", "xiaohongshu.com", "weibo.com", "kuaishou.com", "x.com", "twitter.com"].some(domain => url.hostname === domain || url.hostname.endsWith(`.${domain}`))) continue;
      if (/^[@_\-…A-Za-z0-9]/u.test(remaining) || remaining.startsWith("...") || /^[🌍🌎🌏]/u.test(original) && !/[0-9_-]/u.test(raw) || kind === "wechat" && ["wechat", "whatsapp", "telegram", "instagram"].includes(raw.toLowerCase())) continue;
      const digits = raw.replace(/[^0-9]/gu, ""); if (kind === "whatsapp" && (digits.length < 7 || digits.length > 15)) continue;
      const normalized = kind === "whatsapp" ? `https://wa.me/${digits}` : raw, key = `${kind}:${normalized}`, existing = messagingSeen.get(key);
      if (existing) { if ([...original].length > [...existing.source_excerpt].length) { existing.original_value = original; existing.source_excerpt = original; } continue; }
      const contact: DiscoveryContact = { kind, original_value: original, normalized_value: normalized, source_url: source, source_excerpt: sourceLine(match.index, original.length), captured_at: capturedAt, native_author_id: author, attribution }; messaging.push(contact); messagingSeen.set(key, contact);
    }
    contacts.push(...messaging);
  }
  return contacts;
}
