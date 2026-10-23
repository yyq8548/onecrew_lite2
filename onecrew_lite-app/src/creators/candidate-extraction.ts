import type { z } from "zod";
import { channel, locatorKind, sourceKind, trim } from "./contracts.ts";
import type { CandidateBusinessResult, CandidateEvidence, CanonicalCandidate } from "./candidate-contracts.ts";

export interface CandidateContactObservation { channel: z.infer<typeof channel>; kind: z.infer<typeof locatorKind>; value: string; source: z.infer<typeof sourceKind>; source_url: string | null }
export const candidateHubHosts = new Set(["linktr.ee", "beacons.ai", "bio.link", "solo.to", "lnk.bio", "taplink.cc", "hoo.be", "stan.store", "carrd.co", "allmylinks.com", "campsite.bio"]);
const imageName = (value: string) => /\.(?:png|jpg|jpeg|gif|webp|svg|avif|ico|bmp|tiff)$/i.test(value);
const unicodeWord = "[\\p{Alphabetic}\\p{M}\\p{Nd}\\p{Pc}\\u200c\\u200d]";
const wordBoundary = `(?:(?<=${unicodeWord})(?!${unicodeWord})|(?<!${unicodeWord})(?=${unicodeWord}))`;
const emailPattern = new RegExp(`${wordBoundary}[A-Z0-9._%+\\-]+@[A-Z0-9.\\-]+\\.[A-Z]{2,}${wordBoundary}`, "giu");
const telegramPattern = new RegExp(`${wordBoundary}(?:telegram|tg|电报)${wordBoundary}(?:[\\s,，\\-–—]*@|\\s*[:：=]\\s*@?)([A-Za-z][A-Za-z0-9_]{4,31})`, "giu");
const urlPattern = /(?:https?:\/\/|www\.)[^\s<>\[\]{}"']+|(?:x\.com|twitter\.com|instagram\.com|t\.me|telegram\.me|youtube\.com)\/@?[A-Z0-9_+\-.]+/giu;
const punctuation = (value: string) => value.replace(/[.,;:!?)\]}]+$/u, "");
const shortLink = (value: string) => { const url = URL.parse(value); return url?.hostname === "t.co" || url?.hostname === "www.t.co"; };
export function classifyCandidateContact(value: string): Pick<CandidateContactObservation, "channel" | "kind" | "value"> | null {
  const url = URL.parse(value);
  if (!url || !["http:", "https:"].includes(url.protocol) || url.username || url.password || imageName(url.pathname) || ["pbs.twimg.com", "abs.twimg.com", "scontent.cdninstagram.com", "yt3.ggpht.com", "yt3.googleusercontent.com"].includes(url.hostname)) return null;
  const host = url.hostname.replace(/^(?:www\.)+/u, "").toLowerCase(), segments = url.pathname.split("/").filter(Boolean), first = (segments[0] ?? "").replace(/^@+/u, "");
  if (["x.com", "twitter.com"].includes(host) && first) return { channel: "x", kind: "handle", value: first };
  if (host === "instagram.com" && first) return { channel: "instagram", kind: "handle", value: first };
  if (["t.me", "telegram.me"].includes(host) && first) { const invite = first.startsWith("+") || first === "joinchat"; return { channel: "telegram", kind: segments.length === 1 && !invite ? "handle" : invite ? "invite_link" : "url", value: segments.length === 1 && !invite ? first : value }; }
  return { channel: ["youtube.com", "youtu.be"].includes(host) ? "youtube" : candidateHubHosts.has(host) ? "link_hub" : "website", kind: "url", value };
}
function visibleUrl(value: string): string | null {
  let candidate = punctuation(trim(value));
  if (!candidate.startsWith("https://") && !candidate.startsWith("http://") && candidate.includes(".") && !/\s/u.test(candidate)) candidate = `https://${candidate}`;
  const url = URL.parse(candidate); return url && !url.username && !url.password ? candidate : null;
}
type Field = Extract<CandidateEvidence, { kind: "profile_field" }>;
type Link = Extract<CandidateEvidence, { kind: "profile_link" }>;
export function discoverCandidateContacts(owner: CanonicalCandidate["account"]["platform"], fields: readonly Field[], links: readonly Link[]): CandidateContactObservation[] {
  const result: CandidateContactObservation[] = [];
  const append = (route: Pick<CandidateContactObservation, "channel" | "kind" | "value">, source: CandidateContactObservation["source"], source_url: string | null) => {
    if (route.channel === owner && ["x", "instagram", "telegram", "youtube"].includes(owner)) return;
    if (trim(route.value)) result.push({ ...route, source, source_url });
  };
  for (const field of fields) {
    const urls = [...field.text.matchAll(urlPattern)];
    for (const match of field.text.matchAll(emailPattern)) {
      if (imageName(match[0]) || urls.some(url => match.index >= url.index && match.index + match[0].length <= url.index + url[0].length)) continue;
      append({ channel: "email", kind: "email_address", value: match[0] }, field.source, field.source_url);
    }
    if (owner === "youtube" && field.source === "video_description") continue;
    if (owner !== "telegram") for (const match of field.text.matchAll(telegramPattern)) {
      if (/[.@/_A-Za-z0-9]/u.test(field.text[match.index + match[0].length] ?? "")) continue;
      append({ channel: "telegram", kind: "handle", value: match[1]! }, field.source, field.source_url);
    }
    for (const match of urls) { let value = punctuation(match[0]); if (!value.startsWith("http://") && !value.startsWith("https://")) value = `https://${value}`; const route = shortLink(value) ? null : classifyCandidateContact(value); if (route) append(route, field.source, field.source_url); }
  }
  for (const link of links) {
    if (owner === "youtube" && link.source === "video_description") continue;
    const mailto = URL.parse(trim(link.target_url));
    if (mailto?.protocol === "mailto:") {
      const email = mailto.pathname;
      const match = [...email.matchAll(emailPattern)][0];
      if (match?.index === 0 && match[0] === email && !imageName(email)) append({ channel: "email", kind: "email_address", value: email }, link.source, link.source_url);
      continue;
    }
    const visible = visibleUrl(link.display_text), target = visibleUrl(link.target_url), candidate = visible && !shortLink(visible) ? visible : target && !shortLink(target) ? target : null;
    if (candidate) { const route = classifyCandidateContact(candidate); if (route) append(route, link.source, link.source_url); }
  }
  return result;
}
export function candidateObservations(input: CanonicalCandidate): CandidateBusinessResult<CandidateContactObservation[]> {
  const fail = (message: string): CandidateBusinessResult<CandidateContactObservation[]> => ({ ok: false, kind: "contact_normalization", message });
  const account = input.account;
  if (account.profile_url === null) return fail("Creator profile URL is required");
  const value = account.platform === "youtube" ? account.profile_url : account.handle;
  if (value === null) return fail("initial platform contact route is required");
  const result: CandidateContactObservation[] = [{ channel: account.platform, kind: account.platform === "youtube" ? "url" : "handle", value, source: "initial_platform_account", source_url: account.profile_url }];
  const fields: Field[] = [], links: Link[] = [];
  for (const evidence of input.contact_evidence) {
    if (evidence.kind === "observation") {
      if (!["profile_bio", "profile_website", "profile_location", "video_description", "channel_description", "link_hub", "manual_business_email"].includes(evidence.source)) return fail("unsupported public contact observation source");
      if (account.platform === "youtube" && evidence.source === "video_description" && evidence.channel !== "email" || imageName(evidence.value) || ["url", "invite_link"].includes(evidence.locator_kind) && classifyCandidateContact(evidence.value) === null) continue;
      result.push({ channel: evidence.channel, kind: evidence.locator_kind, value: evidence.value, source: evidence.source, source_url: evidence.source_url });
    } else if (evidence.kind === "profile_field") {
      if (!["profile_bio", "profile_location", "channel_description", "video_description", "profile_website"].includes(evidence.source)) return fail("unsupported public profile evidence source");
      if (!fields.some(field => field.source === evidence.source && field.source_url === evidence.source_url && field.text === evidence.text)) fields.push(evidence);
    } else if (evidence.kind === "profile_link") {
      if (!["profile_bio", "profile_website", "channel_description"].includes(evidence.source)) return fail("unsupported public profile link source");
      if (!links.some(link => link.source === evidence.source && link.source_url === evidence.source_url && link.display_text === evidence.display_text && link.target_url === evidence.target_url)) links.push(evidence);
    } else result.push(...discoverCandidateContacts(account.platform, [{ kind: "profile_field", source: "link_hub", source_url: evidence.source_url, text: evidence.text }], evidence.links.map(link => ({ kind: "profile_link", source: "link_hub", source_url: evidence.source_url, ...link }))));
  }
  result.push(...discoverCandidateContacts(account.platform, fields, links));
  return { ok: true, value: result };
}
