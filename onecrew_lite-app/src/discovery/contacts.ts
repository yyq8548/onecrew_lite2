import type { z } from "zod";
import type { discoveryContact } from "./library-material.ts";

export type DiscoveryContact = z.infer<typeof discoveryContact>;
const host = (value: string): string | null => { const name = URL.parse(value)?.hostname.replace(/^www\./u, ""); return name === "twitter.com" ? "x.com" : name === "space.bilibili.com" ? "bilibili.com" : name === "m.facebook.com" ? "facebook.com" : name ?? null; };
export function discoveryContactPlatform(value: string): string | null { return ({ "youtube.com": "youtube", "bilibili.com": "bilibili", "x.com": "twitter", "instagram.com": "instagram", "facebook.com": "facebook", "tiktok.com": "tiktok", "douyin.com": "douyin", "linkedin.com": "linkedin", "reddit.com": "reddit", "bsky.app": "bluesky" } as Record<string, string>)[host(value) ?? ""] ?? null; }
export function discoverySocialProfile(value: string): boolean {
  const url = URL.parse(value); if (!url) return false;
  const hostname = url.hostname.replace(/^www\./u, ""), path = url.pathname;
  if (url.protocol === "https:" && hostname === "facebook.com" && path === "/profile.php" && !url.username && !url.password && !url.port && !url.hash) { const pairs = [...url.searchParams]; return pairs.length === 1 && pairs[0]![0] === "id" && /^[1-9][0-9]{0,19}$/u.test(pairs[0]![1]); }
  if (url.protocol !== "https:" || url.username || url.password || url.port || url.search || url.hash || value.includes("...") || value.includes("…")) return false;
  const parts = path.replace(/^\/+|\/+$/gu, "").split("/"), handle = (part: string) => Buffer.byteLength(part) <= 128 && /^[\p{Alphabetic}\p{Number}_.%\-]+$/u.test(part);
  if (["instagram.com", "x.com", "twitter.com"].includes(hostname)) return parts.length === 1 && handle(parts[0]!) && !["home", "explore", "search", "i", "accounts", "reel", "reels", "p"].includes(parts[0]!);
  if (hostname === "tiktok.com") return parts.length === 1 && parts[0]!.startsWith("@") && handle(parts[0]!.slice(1));
  if (hostname === "douyin.com") return parts.length === 2 && parts[0] === "user" && /^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/u.test(parts[1]!);
  if (hostname === "youtube.com") return parts.length === 1 && parts[0]!.startsWith("@") && handle(parts[0]!.slice(1)) || parts.length === 2 && parts[0] === "channel" && parts[1]!.startsWith("UC") && Buffer.byteLength(parts[1]!) === 24;
  if (hostname === "space.bilibili.com") return parts.length === 1 && /^[0-9]+$/u.test(parts[0]!);
  if (hostname === "linkedin.com") return parts.length === 2 && parts[0] === "in" && handle(parts[1]!);
  if (hostname === "reddit.com") return parts.length === 2 && ["user", "u"].includes(parts[0]!) && handle(parts[1]!);
  if (hostname === "facebook.com") return parts.length === 1 && handle(parts[0]!) && !["groups", "watch", "login", "share", "reel", "marketplace"].includes(parts[0]!);
  return hostname === "bsky.app" && parts.length === 2 && parts[0] === "profile" && handle(parts[1]!);
}
export function discoveryNonContact(source: string): boolean {
  const url = URL.parse(source); if (!url) return true;
  const name = url.hostname.replace(/^www\./u, ""), path = url.pathname, parts = path.split("/");
  if (["amzn.to", "geni.us", "a.co", "amzn.eu"].includes(url.hostname) || /(?:^|\.)amazon\.(?:com|co\.uk)$/u.test(url.hostname) || url.hostname.startsWith("shop.") && parts.includes("collections") || parts.some(part => ["dp", "products"].includes(part))) return true;
  return ["docs.google.com", "drive.google.com", "inex.osekai.net", "osekai.net", "youtu.be", "collabs.shop", "rover.ebay.com", "rocketreach.co", "contactout.com", "muckrack.com", "instainflu.com", "scrumball.com", "shop.anker.com", "ankerfast.club", "lddy.no", "rstyle.me", "go.magik.ly", "c.klarna.com", "aritzia.com", "kit.co", "store.cablemod.com", "go.hototools.com", "hototools.com"].includes(name)
    || name === "wooting.io" && !["/contact", "/contact/"].includes(path)
    || ["x.com", "twitter.com"].includes(name) && path.includes("/status/")
    || name === "instagram.com" && ["/p/", "/reel/", "/reels/"].some(prefix => path.startsWith(prefix))
    || name === "tiktok.com" && (path.includes("/video/") || path.startsWith("/discover/"))
    || name === "linkedin.com" && ["/posts/", "/feed/"].some(prefix => path.startsWith(prefix))
    || name === "reddit.com" && (path.startsWith("/r/") || path.includes("/comments/"))
    || parts.some(part => ["form-submission-response", "checkout", "cart", "shop", "store", "stores", "product", "privacy-policy", "terms-of-service", "discord"].includes(part))
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
  const contacts: DiscoveryContact[] = [], seen = new Set<string>();
  const push = (kind: DiscoveryContact["kind"], original: string, normalized: string, excerpt: string) => { contacts.push({ kind, original_value: original, normalized_value: normalized, source_url: source, source_excerpt: excerpt, captured_at: capturedAt, native_author_id: author, attribution }); };
  for (const [kind, expression] of [["email", /[a-z0-9][a-z0-9.!#$%&'*+/=?^_`{|}~\-]*@[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+/giu], ["telegram", /https:\/\/(?:t\.me|telegram\.me)\/[a-z0-9_+/-]+(?:\?[a-z0-9_=&%\-]+)?/giu]] as const) {
    for (const match of text.matchAll(expression)) {
      const original = match[0], start = match.index, end = start + original.length, before = [...text.slice(0, start)].at(-1), after = [...text.slice(end)][0];
      if (before && /[A-Za-z0-9@/_]/u.test(before) || after && /[A-Za-z0-9@_…]/u.test(after) || text.slice(end).startsWith("...") || Buffer.byteLength(original) > 2048) continue;
      let normalized: string;
      if (kind === "email") { const at = original.lastIndexOf("@"), local = original.slice(0, at); if (local.length > 64 || original.length > 254 || local.endsWith(".") || local.includes("..")) continue; normalized = `${local}@${original.slice(at + 1).toLowerCase()}`; }
      else { const targetUrl = new URL(original), target = targetUrl.pathname.replace(/^\/+/u, ""); if (!target || ["share", "iv", "login", "proxy", "socks"].includes(target.toLowerCase()) || target.startsWith("share/") || target.startsWith("s/") || target.includes("/") && !target.startsWith("joinchat/")) continue; normalized = `https://t.me/${target}${targetUrl.search}`; }
      if (seen.has(normalized)) continue; seen.add(normalized);
      push(kind, original, normalized, [...text.slice(0, start)].slice(-100).join("") + original + [...text.slice(end)].slice(0, 100).join(""));
    }
  }
  for (const match of text.matchAll(/(?:^|[\s,，;；|])(?:telegram|tg|电报)[ \t]*[:：][ \t]*@?([a-z][a-z0-9_]{4,31})/gimu)) {
    const end = match.index + match[0].length, after = [...text.slice(end)][0]; if (after && /[\p{Alphabetic}\p{Number}_…/@.\-]/u.test(after) || text.slice(end).startsWith("...")) continue;
    const normalized = `https://t.me/${match[1]!}`; if (seen.has(normalized)) continue; seen.add(normalized); push("telegram", match[0].trim(), normalized, match[0].trim());
  }
  for (const match of text.matchAll(/https:\/\/[^\s<>"']+/gu)) {
    const value = match[0].replace(/[.,;)\]]+$/u, ""); if (!discoverySocialProfile(value) || host(value) === host(source) || thirdParty(value, text) || seen.has(value)) continue; seen.add(value); push("social_profile", value, value, value);
  }
  if (attribution === "author_published") {
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
      const contact: DiscoveryContact = { kind, original_value: original, normalized_value: normalized, source_url: source, source_excerpt: original, captured_at: capturedAt, native_author_id: author, attribution }; messaging.push(contact); messagingSeen.set(key, contact);
    }
    contacts.push(...messaging);
  }
  return contacts;
}
