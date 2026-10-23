import { z } from "zod";
import { bilibiliProfilePlan, twitterIdentityPlan } from "../platforms/opencli/profiles.ts";
import { linkedinProfilePlan, normalizeLinkedinHandle } from "../platforms/opencli/linkedin.ts";
import { weiboProfilePlan } from "../platforms/opencli/weibo.ts";
import type { OpenCliReader, VerifiedOpenCliBrowserBinding } from "../platforms/opencli/reader.ts";

export function discoveryWebProfileTarget(source: string) {
  const url = URL.parse(source);
  if (!url || url.protocol !== "https:" || url.username || url.password || url.port || url.hash || url.search || source.includes("\\")) return null;
  const host = url.hostname;
  if (["x.com", "www.x.com", "twitter.com", "www.twitter.com"].includes(host)) {
    const handle = /^\/([A-Za-z\d_]{1,15})\/?$/u.exec(url.pathname)?.[1]?.toLowerCase();
    if (handle && !["home", "explore", "search", "settings", "messages", "notifications", "intent", "i"].includes(handle)) return { platform: "twitter" as const, id: `@${handle}`, cost: 1, profile_url: `https://x.com/${handle}` };
  }
  if (["linkedin.com", "www.linkedin.com"].includes(host)) {
    const slug = /^\/in\/([^/]+)\/?$/u.exec(url.pathname)?.[1];
    if (slug && !/%(?:2f|5c)/iu.test(slug)) { const handle = normalizeLinkedinHandle(slug); if (handle.length > 512) return null; return { platform: "linkedin" as const, id: handle, cost: 1, profile_url: `https://www.linkedin.com/in/${handle}/` }; }
  }
  if (host === "space.bilibili.com") {
    const id = /^\/([1-9]\d{0,19})\/?$/u.exec(url.pathname)?.[1];
    if (id) return { platform: "bilibili" as const, id, cost: 1, profile_url: `https://space.bilibili.com/${id}` };
  }
  if (["weibo.com", "www.weibo.com"].includes(host)) {
    const id = /^\/u\/([1-9]\d{0,19})\/?$/u.exec(url.pathname)?.[1];
    if (id) return { platform: "weibo" as const, id, cost: 1, profile_url: `https://weibo.com/u/${id}` };
  }
  if (["youtube.com", "www.youtube.com"].includes(host)) {
    const id = /^\/channel\/(UC[A-Za-z\d_-]{22})\/?$/u.exec(url.pathname)?.[1];
    if (id) return { platform: "youtube" as const, id, cost: 1, profile_url: `https://www.youtube.com/channel/${id}` };
  }
  return null;
}

export async function readDiscoveryWebProfile(reader: OpenCliReader, browser: VerifiedOpenCliBrowserBinding, source: string) {
  const target = discoveryWebProfileTarget(source);
  if (!target) throw new Error("web_source_profile_target_invalid");
  const profile = target.platform === "twitter" ? await reader.twitterIdentityProfile(browser, target.id.slice(1)) : target.platform === "linkedin" ? await reader.linkedinPublicProfile(browser, target.id) : target.platform === "bilibili" ? await reader.bilibiliPublicProfile(browser, target.id) : target.platform === "weibo" ? await reader.weiboPublicProfile(browser, target.id) : await reader.youtubeChannel(browser, target.id);
  return validateDiscoveryWebProfile(source, profile);
}

export function validateDiscoveryWebProfile(source: string, raw: unknown) {
  const target = discoveryWebProfileTarget(source), profile = z.record(z.string(), z.unknown()).parse(raw);
  if (!target) throw new Error("web_source_profile_target_invalid");
  if (target.platform === "linkedin") linkedinProfilePlan(target.id).decode([profile]);
  else if (target.platform === "bilibili") bilibiliProfilePlan(target.id).decode([profile]);
  else if (target.platform === "weibo") weiboProfilePlan(target.id).decode([profile]);
  else if (target.platform === "twitter") twitterIdentityPlan(target.id.slice(1)).decode([profile]);
  else {
    const text = z.string().refine(value => value.isWellFormed());
    z.strictObject({ channel_id: z.literal(target.id), name: text.refine(value => !!value.trim()), handle: text, description: text, subscribers_raw: text, url: z.literal(target.profile_url), recent_videos: z.array(z.strictObject({ title: text, summary: text })).max(100) }).parse(profile);
  }
  const account = z.string().min(1).parse(target.platform === "linkedin" || target.platform === "twitter" ? profile.account_id : target.platform === "bilibili" ? profile.mid : target.platform === "weibo" ? profile.id : profile.channel_id);
  return { identity: { account_id: account, profile_url: target.profile_url, evidence_source: "native_profile_link" as const }, profile };
}
