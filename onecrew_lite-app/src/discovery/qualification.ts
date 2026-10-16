import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
export interface SubscriberEstimate { nominal: bigint; lower: bigint; upper: bigint }
export interface DiscoveryAssessment { verdict: "qualified" | "pending" | "rejected"; reasons: string[]; subscribers: bigint | null }
const MAX = 18446744073709551615n;
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const u64 = (value: unknown): bigint | null => typeof value === "bigint" && value >= 0n && value <= MAX ? value : typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? BigInt(value) : null;
const string = (value: unknown): string | null => typeof value === "string" ? value : null;
const nonempty = (value: unknown): boolean => typeof value === "string" && value.length > 0;
const publicHttps = (value: unknown): boolean => typeof value === "string" && URL.parse(value)?.protocol === "https:" && !!URL.parse(value)?.hostname;
const youtubeVideo = (value: unknown): boolean => { if (typeof value !== "string") return false; const url = URL.parse(value); return url !== null && url.protocol === "https:" && ["youtube.com", "www.youtube.com"].includes(url.hostname) && (url.pathname === "/watch" || url.pathname.startsWith("/shorts/")); };
export function requiredDiscoveryLanguage(criteria: unknown): string | null {
  const value = string(object(criteria).language)?.trim(); return value === undefined || ["", "不限", "语言不限", "any", "all", "unrestricted"].includes(value.toLowerCase()) ? null : value;
}
export function parseDiscoverySubscribers(raw: string): SubscriberEstimate | null {
  raw = raw.trim(); if (!raw || /[~<>+约]/u.test(raw) || raw.toLowerCase().includes("about")) return null;
  const start = raw.search(/[0-9]/u); if (start < 0) return null;
  const suffixAt = raw.slice(start).search(/[^0-9,.]/u), end = suffixAt < 0 ? raw.length : start + suffixAt;
  const number = raw.slice(start, end), suffix = raw.slice(end).trimStart();
  const multiplier = suffix.startsWith("万") ? 10000n : suffix.startsWith("亿") ? 100000000n : /^[Kk千]/u.test(suffix) ? 1000n : /^[Mm]/u.test(suffix) ? 1000000n : /^[Bb]/u.test(suffix) ? 1000000000n : 1n;
  const normalized = number.replaceAll(",", ""), dot = normalized.indexOf("."), whole = dot < 0 ? normalized : normalized.slice(0, dot), fraction = dot < 0 ? "" : normalized.slice(dot + 1);
  if (!/^[0-9]+$/u.test(whole) || !/^[0-9]*$/u.test(fraction) || fraction.length > 6 || multiplier === 1n && fraction.length) return null;
  const integer = BigInt(whole), fractional = fraction ? BigInt(fraction) : 0n, factor = 10n ** BigInt(fraction.length);
  if (integer > MAX || fractional > MAX || integer * multiplier > MAX || fractional * multiplier > MAX) return null;
  const nominal = integer * multiplier + fractional * multiplier / factor; if (nominal > MAX) return null;
  const resolution = multiplier === 1n ? 0n : multiplier / factor, margin = (resolution + 1n) / 2n;
  return { nominal, lower: nominal < margin ? 0n : nominal - margin, upper: nominal + margin > MAX ? MAX : nominal + margin };
}
export function discoverySubscriberDisplay(raw: string): string | null { const value = parseDiscoverySubscribers(raw); return value === null ? null : `${value.lower === value.upper ? "" : "~"}${value.nominal}`; }
export function parseDiscoveryDouyinFollowers(raw: string): (SubscriberEstimate & { display: string }) | null {
  const multiplier = raw.endsWith("万") ? 10000n : raw.endsWith("亿") ? 100000000n : raw.endsWith("千") ? 1000n : null; if (multiplier === null) return null;
  const number = raw.slice(0, -1), dot = number.indexOf("."), whole = dot < 0 ? number : number.slice(0, dot), fraction = dot < 0 ? "" : number.slice(dot + 1);
  if (!/^[0-9]+$/u.test(whole) || whole.length > 1 && whole.startsWith("0") || !/^[0-9]*$/u.test(fraction) || fraction.length > 6 || dot >= 0 && !fraction.length) return null;
  const factor = 10n ** BigInt(fraction.length); if (factor > multiplier) return null;
  const integer = BigInt(whole), fractional = fraction ? BigInt(fraction) : 0n;
  if (integer > MAX || integer * multiplier > MAX || fractional * multiplier > MAX) return null;
  const nominal = integer * multiplier + fractional * multiplier / factor, resolution = multiplier / factor, upper = nominal + resolution;
  if (nominal > MAX || upper > BigInt(Number.MAX_SAFE_INTEGER)) return null;
  return { display: raw, nominal, lower: nominal < resolution ? 0n : nominal - resolution, upper };
}
export function assessDiscoveryWithCount(rawCriteria: unknown, rawEvidence: unknown, author: string, estimate: SubscriberEstimate | null, youtubeContentSupported: boolean): DiscoveryAssessment {
  const criteria = object(rawCriteria), evidence = object(rawEvidence), sources = Array.isArray(evidence.source_urls) ? evidence.source_urls : [];
  const observed = (value: unknown) => typeof value === "string" && sources.some(source => source === value);
  const proof = object(evidence.reviewed_match), count = estimate?.nominal ?? null;
  if (criteria.excluded_accounts !== undefined) {
    const excluded = z.array(z.strictObject({ native_author_id: z.string().min(1).max(256), profile_url: z.url().startsWith("https://").max(2048), excerpt: z.string().min(1) })).max(1000).safeParse(criteria.excluded_accounts);
    if (!excluded.success) return { verdict: "pending", reasons: ["excluded_accounts_invalid"], subscribers: count };
    const matching = excluded.data.filter(item => item.native_author_id === author);
    if (matching.length) {
      const bound = matching.some(item => observed(item.profile_url));
      return { verdict: bound ? "rejected" : "pending", reasons: [bound ? "excluded_by_customer" : "excluded_account_identity_unverified"], subscribers: count };
    }
  }
  let initial: DiscoveryAssessment | null = null;
  if (proof.verified === true && proof.policy_version === "initial-match-evaluation-v1" && proof.native_author_id === author && nonempty(proof.review_id) && typeof proof.reason === "string" && proof.reason.trim() && Buffer.byteLength(proof.reason) <= 4000 && observed(proof.source_url)) {
    if (proof.outcome === "supported" && ["excellent", "good", "medium"].includes(String(proof.level))) initial = { verdict: "qualified", reasons: [], subscribers: count };
    if (proof.outcome === "supported" && proof.level === "low") initial = { verdict: "rejected", reasons: ["initial_match_low"], subscribers: count };
    if (proof.outcome === "insufficient" && proof.level == null) initial = { verdict: "pending", reasons: ["initial_match_unrated"], subscribers: count };
  }
  const reasons: string[] = [], rawMin = u64(Object.hasOwn(criteria, "min_subscribers") ? criteria.min_subscribers : criteria.min_followers), minimum = rawMin !== null && rawMin > 0n ? rawMin : null;
  let below = false;
  if (minimum !== null) {
    if (estimate === null) reasons.push("subscriber_count_unverified");
    else if (estimate.upper < minimum) below = true;
    else if (estimate.lower < minimum) reasons.push("subscriber_count_near_threshold");
  }
  if (below) reasons.push("below_min_subscribers");
  if (criteria.requires_ai_tool_demo !== false) {
    const demo = object(evidence.own_ai_tool_demo);
    if (!(youtubeContentSupported && demo.verified === true && demo.channel_id === author && youtubeVideo(demo.source_url) && observed(demo.source_url))) reasons.push("own_ai_tool_demo_unverified");
  }
  if (criteria.requires_public_tool_url !== false) {
    const tool = object(evidence.public_tool_url);
    if (!(tool.verified === true && typeof tool.tool_name === "string" && tool.tool_name.trim() && publicHttps(tool.url) && observed(tool.source_url))) reasons.push("public_tool_url_unverified");
  }
  if (typeof criteria.region === "string" && criteria.region.length) {
    const residence = object(evidence.reviewed_residence);
    if (!(criteria.region === "中国大陆以外" && residence.verified === true && residence.native_author_id === author && typeof residence.country_code === "string" && /^[A-Z]{2}$/u.test(residence.country_code) && residence.country_code !== "CN" && observed(residence.source_url) && publicHttps(residence.corroboration_url) && nonempty(residence.review_id) && ["apparel-residence-review-v1", "apparel-native-review-v2"].includes(String(residence.policy_version)))) reasons.push("region_unverified");
  }
  let languageConflict = false;
  if (requiredDiscoveryLanguage(criteria) !== null) {
    const language = object(evidence.reviewed_language), bound = language.verified === true && language.native_author_id === author && language.language === criteria.language && language.policy_version === "language-match-review-v1" && nonempty(language.review_id) && observed(language.source_url);
    languageConflict = bound && language.outcome === "contradicted";
    if (languageConflict) reasons.push("language_mismatch"); else if (!(bound && language.outcome === "supported")) reasons.push("language_unverified");
  }
  if (typeof criteria.category === "string" && criteria.category.length) {
    const apparel = criteria.category === "服装", content = object(evidence[apparel ? "reviewed_apparel_content" : "reviewed_category_content"]);
    if (!(content.verified === true && content.native_author_id === author && (apparel || content.category === criteria.category) && observed(content.source_url) && nonempty(content.review_id) && (apparel ? ["apparel-residence-review-v1", "apparel-native-review-v2"].includes(String(content.policy_version)) : content.policy_version === "category-content-review-v1"))) reasons.push("category_unverified");
  }
  const requirementReasons: string[] = []; let requirementConflict = false;
  if (criteria.requirements !== undefined) {
    if (!Array.isArray(criteria.requirements) || criteria.requirements.length > 32) requirementReasons.push("requirements_invalid");
    else for (const [index, requirement] of criteria.requirements.entries()) {
      const reviewed = Array.isArray(evidence.reviewed_requirements) ? object(evidence.reviewed_requirements[index]) : {};
      const bound = isDeepStrictEqual(reviewed.requirement, requirement) && u64(reviewed.index) === BigInt(index) && reviewed.native_author_id === author && nonempty(reviewed.review_id) && observed(reviewed.source_url);
      if (bound && reviewed.outcome === "contradicted") { requirementConflict = true; requirementReasons.push("requirement_contradicted"); }
      else if (!bound || reviewed.outcome !== "supported") requirementReasons.push("requirement_unverified");
    }
  }
  if (Object.keys(criteria).some(key => !["min_subscribers", "min_followers", "requires_ai_tool_demo", "requires_public_tool_url", "region", "language", "category", "table_subject", "contact_search_terms", "automatic_query_expansion", "requirements", "contact_preference", "refinement", "excluded_accounts"].includes(key))) reasons.push("unsupported_criterion");
  if (initial !== null) return { ...initial, verdict: requirementConflict ? "rejected" : requirementReasons.length && initial.verdict === "qualified" ? "pending" : initial.verdict, reasons: [...initial.reasons, ...requirementReasons] };
  reasons.push(...requirementReasons);
  return { verdict: below || languageConflict || requirementConflict ? "rejected" : reasons.length ? "pending" : "qualified", reasons, subscribers: count };
}
export function assessDiscoveryProfile(criteria: unknown, evidence: unknown, author: string, followers: bigint | number | null): DiscoveryAssessment {
  const value = followers === null ? null : u64(followers); if (followers !== null && value === null) throw new Error("discovery_native_follower_count_invalid");
  return assessDiscoveryWithCount(criteria, evidence, author, value === null ? null : { nominal: value, lower: value, upper: value }, false);
}
export function assessDiscoveryYoutube(criteria: unknown, raw: unknown, author: string): DiscoveryAssessment {
  const evidence = object(raw), sources = Array.isArray(evidence.source_urls) ? evidence.source_urls : [];
  const estimate = typeof evidence.subscribers_raw === "string" && typeof evidence.subscribers_source_url === "string" && sources.includes(evidence.subscribers_source_url) && (u64(evidence.captured_at) ?? 0n) > 0n && youtubeVideo(evidence.subscribers_source_url) ? parseDiscoverySubscribers(evidence.subscribers_raw) : null;
  return assessDiscoveryWithCount(criteria, evidence, author, estimate, true);
}
export function assessDiscoveryDouyin(criteria: unknown, evidence: unknown, author: string, rawCount: string | null): DiscoveryAssessment {
  const estimate = rawCount === null ? null : parseDiscoveryDouyinFollowers(rawCount), result = assessDiscoveryWithCount(criteria, evidence, author, estimate, false), values = object(criteria);
  const minimum = u64(values.min_subscribers) ?? u64(values.min_followers);
  if (estimate !== null && minimum !== null && estimate.lower < minimum && estimate.upper >= minimum && !result.reasons.includes("subscriber_count_near_threshold")) result.reasons.push("subscriber_count_near_threshold");
  return result;
}
export function assessedDiscoveryEvidence(raw: unknown, assessment: DiscoveryAssessment): unknown {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return raw;
  const evidence = object(raw), fit = object(evidence.reviewed_match), sources = Array.isArray(evidence.source_urls) ? evidence.source_urls : [];
  const rated = (assessment.verdict === "qualified" || assessment.verdict === "rejected" && fit.policy_version === "initial-match-evaluation-v1" && fit.level === "low") && fit.verified === true && ["language-match-review-v1", "initial-match-evaluation-v1"].includes(String(fit.policy_version)) && fit.outcome === "supported" && typeof fit.reason === "string" && !!fit.reason.trim() && Buffer.byteLength(fit.reason) <= 4000 && typeof fit.source_url === "string" && sources.includes(fit.source_url) && ["excellent", "good", "medium", "low"].includes(String(fit.level));
  return { ...evidence, assessment: { reasons: assessment.reasons, subscribers_normalized: assessment.subscribers, match_rating: rated ? { level: fit.level, reason: fit.reason, source_url: fit.source_url } : null } };
}

export function mergePendingDiscoveryEvidence(current: unknown, previous: unknown): unknown {
  if (current === null || typeof current !== "object" || Array.isArray(current) || previous === null || typeof previous !== "object" || Array.isArray(previous)) return current;
  const result = structuredClone(current) as Record<string, unknown>, prior = object(previous);
  for (const key of ["source_urls", "public_links"]) {
    const values = result[key], older = prior[key];
    if (!Array.isArray(values) || !Array.isArray(older)) continue;
    for (const value of older) {
      if (values.length === 20) break;
      if (typeof value === "string" && !values.includes(value)) values.push(value);
    }
  }
  for (const key of ["own_ai_tool_demo", "public_tool_url", "verified_region", "verified_language"]) {
    if (!Object.hasOwn(result, key) && Object.hasOwn(prior, key)) result[key] = structuredClone(prior[key]);
  }
  if (typeof result.subscribers_raw !== "string" || result.subscribers_raw.length === 0) {
    for (const key of ["subscribers_raw", "subscribers_source_url", "captured_at"]) if (Object.hasOwn(prior, key)) result[key] = structuredClone(prior[key]);
  }
  const values = result.source_urls;
  if (Array.isArray(values)) {
    const sources: unknown[] = values.length ? [values[0]] : [];
    for (const source of [result.subscribers_source_url, object(result.own_ai_tool_demo).source_url, object(result.public_tool_url).source_url]) {
      const observed = values.includes(source) || Array.isArray(prior.source_urls) && prior.source_urls.includes(source);
      if (typeof source === "string" && observed && !sources.includes(source)) sources.push(source);
    }
    for (const source of values) {
      if (sources.length >= 20) break;
      if (!sources.includes(source)) sources.push(source);
    }
    result.source_urls = sources;
  }
  return result;
}
