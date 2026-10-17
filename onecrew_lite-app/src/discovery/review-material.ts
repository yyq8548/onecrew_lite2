import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { discoveryJsonValue, discoveryMaterialDigest, discoverySearchHit } from "./library-material.ts";
const text = z.string().refine(value => value.isWellFormed());
const signed = z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]);
const unsigned = z.union([z.number().int().nonnegative().safe(), z.bigint().min(0n).max(18446744073709551615n)]);
const u32 = z.number().int().min(0).max(4294967295);
export const discoveryLanguageSample = z.strictObject({ author_id: text, source_url: text, text, comments: z.array(z.strictObject({ source_url: text, text })) });
export const discoveryFollowerEstimate = z.object({ display: text, nominal: unsigned, lower: unsigned, upper: unsigned });
export const discoveryNativeProfileObservation = z.object({ web_text: text.nullish(), profile_url: text, observed_at: signed, bio: text.nullish(), self_declared_location: text.nullish() });
export const discoveryTranscriptSegment = z.object({ timestamp: text, speaker: text, text });
export const discoveryTranscriptRead = z.object({ search_hit_id: text, candidate_id: text, source_url: text, profile_ref: text, platform_account_id: text, status: text, segments: z.array(discoveryTranscriptSegment), truncated: z.boolean(), stop_code: text.nullable(), call_number: u32, captured_at: signed });
export const discoveryContentClaim = z.object({ campaign_id: text, expected_revision: u32, search_hit_id: text, platform: text, profile_ref: text, author_id: text, source_url: text, call_number: u32 });
export const discoveryContentRead = z.object({ claim: discoveryContentClaim, status: text, native_content: z.unknown(), stop_code: text.nullable(), captured_at: signed, language_sample: discoveryLanguageSample.nullish() });
export const discoveryPublicPageRead = z.object({ candidate_id: text, requested_url: text, search_hit_id: text, source_url: text, source_excerpt: text, profile_ref: text, platform_account_id: text, call_number: u32, status: text, document_url: text.nullable(), title: text, markdown_excerpt: text, truncated: z.boolean(), stop_code: text.nullable(), captured_at: signed });
export const discoveryReviewBasis = z.object({ campaign_id: text, revision: u32, criteria_json: text, scopes_json: text, approved_at: signed, hit: discoverySearchHit, candidate_id: text, native_author_id: text, native_followers: unsigned.nullable(), native_follower_estimate: discoveryFollowerEstimate.nullable(), native_profile: discoveryNativeProfileObservation.nullable(), evidence_json: text, transcript: z.tuple([discoveryTranscriptRead, text]).nullable(), source_description: text.nullable(), native_content: discoveryContentRead.nullable(), language_samples: z.array(discoveryLanguageSample), pages: z.array(discoveryPublicPageRead) });
export type DiscoveryReviewBasis = z.infer<typeof discoveryReviewBasis>;
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
export function buildDiscoveryReviewMaterial(raw: unknown) {
  const basis = discoveryReviewBasis.parse(raw), parse = (value: string) => parseApplicationJson(Buffer.from(value));
  const scopes = z.array(z.object({ platform: text, profile_ref: text, keywords: z.array(text) })).parse(parse(basis.scopes_json));
  if (!scopes.some(scope => scope.platform === basis.hit.platform && scope.profile_ref === basis.hit.profile_ref && scope.keywords.includes(basis.hit.search_keyword))) throw new Error("review_source_scope_mismatch");
  const evidence = discoveryJsonValue(parse(basis.evidence_json)), evidenceObject = object(evidence), sources = evidenceObject.source_urls;
  const description = basis.source_description ?? (basis.hit.platform === "youtube" && Array.isArray(sources) && sources[0] === basis.hit.source_url && typeof evidenceObject.video_description === "string" ? evidenceObject.video_description : null);
  const transcript = basis.transcript === null ? null : { ...basis.transcript[0], segments: z.array(discoveryTranscriptSegment).parse(parse(basis.transcript[1])) };
  const platform = ["google", "yahoo"].includes(basis.hit.platform) ? object(evidenceObject.web_origin).target_platform : null;
  if (["google", "yahoo"].includes(basis.hit.platform) && !["youtube", "twitter", "douyin"].includes(String(platform))) throw new Error("review_native_platform_unavailable");
  const nativePlatform = typeof platform === "string" ? platform : null;
  const content = basis.native_content === null ? null : { claim: basis.native_content.claim, status: basis.native_content.status, native_content: discoveryJsonValue(basis.native_content.native_content), stop_code: basis.native_content.stop_code, captured_at: basis.native_content.captured_at, ...(basis.native_content.language_sample ? { language_sample: basis.native_content.language_sample } : {}) };
  if (basis.hit.platform === "facebook" && content !== null) {
    const native = object(content.native_content);
    if (typeof native.text === "string" && /…\s*See more\s*$/u.test(native.text)) native.text_truncated = true;
  }
  const profile = basis.native_profile === null ? null : { ...(basis.native_profile.web_text != null ? { web_text: basis.native_profile.web_text } : {}), profile_url: basis.native_profile.profile_url, observed_at: basis.native_profile.observed_at, ...(basis.native_profile.bio != null ? { bio: basis.native_profile.bio } : {}), ...(basis.native_profile.self_declared_location != null ? { self_declared_location: basis.native_profile.self_declared_location } : {}) };
  const material = { format_version: ["google", "yahoo", "twitter", "tiktok", "bilibili", "reddit", "facebook", "instagram", "weibo", "bluesky", "zhihu", "hackernews", "pinterest", "pixiv", "linkedin", "douyin"].includes(basis.hit.platform) ? 2 : 1, campaign_id: basis.campaign_id, criteria_revision: basis.revision, criteria: discoveryJsonValue(parse(basis.criteria_json)), approved_scopes: scopes, approved_at: basis.approved_at, hit: basis.hit, candidate_id: basis.candidate_id, native_author_id: basis.native_author_id, ...(nativePlatform !== null ? { native_platform: nativePlatform } : {}), ...(basis.native_followers !== null ? { native_followers: basis.native_followers } : {}), ...(basis.native_follower_estimate !== null ? { native_follower_estimate: basis.native_follower_estimate } : {}), ...(profile !== null ? { native_profile: profile } : {}), candidate_evidence_hash: discoveryMaterialDigest(evidence), video_description: description, transcript, ...(content !== null ? { native_content: content } : {}), ...(basis.language_samples.length ? { language_samples: basis.language_samples } : {}), public_pages: basis.pages, material_hash: "" };
  const matched = matchedDiscoveryLanguageSource(material);
  const samples = basis.language_samples.filter(sample => sample.author_id === basis.native_author_id && sample.source_url === matched).slice(0, 1).map(sample => ({ ...sample, comments: sample.comments.slice(0, 5) }));
  if (samples.length) material.language_samples = samples; else delete material.language_samples;
  const bytes = stringifyApplicationJson(material); if (Buffer.byteLength(bytes) + 64 > 1048576) throw new Error("review_material_too_large");
  material.material_hash = discoveryMaterialDigest(material); return material;
}
export type DiscoveryReviewMaterial = ReturnType<typeof buildDiscoveryReviewMaterial>;
export function matchedDiscoveryLanguageSource(material: DiscoveryReviewMaterial): string {
  if (["google", "yahoo"].includes(material.hit.platform) && material.native_platform === "youtube") {
    const url = URL.parse(material.hit.source_url); let id: string | null = null;
    if (url && url.protocol === "https:" && !url.username && !url.password && !url.port && !url.hash && ["youtube.com", "www.youtube.com"].includes(url.hostname)) {
      if (url.pathname === "/watch" && url.searchParams.getAll("v").length === 1) id = url.searchParams.get("v");
      const parts = url.pathname.split("/");
      if (parts.length === 3 && (parts[1] === "watch" && !url.search || ["shorts", "live"].includes(parts[1]!))) id = parts[2]!;
    }
    if (id !== null && /^[A-Za-z\d_-]{11}$/u.test(id)) return `https://www.youtube.com/watch?v=${id}`;
  }
  const read = material.native_content;
  if (material.hit.platform === "twitter" && read?.status === "completed" && read.stop_code === null) { const source = object(object(read.native_content).identity).source_url; if (typeof source === "string") return source; }
  if (read?.status === "completed" && read.stop_code === null) {
    if (["instagram", "pinterest", "linkedin"].includes(read.claim.platform)) { const source = object(read.native_content).source_url; if (typeof source === "string") return source; }
    else return read.claim.source_url;
  }
  return material.hit.source_url;
}
export type DiscoveryReviewCitation = { kind: "description" | "search_caption" | "native_content"; quote: string } | { kind: "transcript"; segment_index: number; quote: string } | { kind: "native_profile"; profile_url: string; quote: string } | { kind: "public_page"; requested_url: string; quote: string };
export function containsDiscoveryCitation(material: DiscoveryReviewMaterial, citation: DiscoveryReviewCitation): boolean {
  const quote = citation.quote; if (!quote.trim() || Buffer.byteLength(quote) > 4000) return false;
  if (citation.kind === "native_content" && material.language_samples?.some(sample => sample.author_id === material.native_author_id && (sample.text.includes(quote) || sample.comments.some(comment => comment.text.includes(quote))))) return true;
  let value: string | null | undefined;
  if (citation.kind === "description") value = material.video_description;
  if (citation.kind === "search_caption" && material.hit.platform === "tiktok") value = material.hit.title;
  if (citation.kind === "transcript" && material.transcript?.status === "completed" && material.transcript.stop_code === null) value = material.transcript.segments[citation.segment_index]?.text;
  if (citation.kind === "native_profile" && material.native_profile?.profile_url === citation.profile_url && material.hit.author_id === material.native_author_id && material.hit.author_url === citation.profile_url) value = material.native_profile.bio?.includes(quote) ? material.native_profile.bio : material.native_profile.web_text;
  if (citation.kind === "native_content" && material.native_content?.status === "completed" && material.native_content.stop_code === null) { const text = object(material.native_content.native_content)[material.hit.platform === "bilibili" ? "description" : "text"]; value = typeof text === "string" ? text : null; }
  if (citation.kind === "public_page") value = material.public_pages.find(page => page.requested_url === citation.requested_url && page.status === "completed" && page.stop_code === null)?.markdown_excerpt;
  return typeof value === "string" && value.includes(quote);
}
