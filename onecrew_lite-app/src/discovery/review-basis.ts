import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { creatorSearchMaterial } from "../platforms/opencli/search-results.ts";
import { retainedLinkedinExperience } from "./linkedin-experience.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { discoveryReviewBasis, type DiscoveryReviewBasis } from "./review-material.ts";
import { stripUnauditedDiscoveryProofs } from "./review-proof-audit.ts";
import { loadDiscoveryContentReceipt, loadDiscoveryLanguageSamples, loadDiscoveryProfileReceipt, loadDiscoveryReviewReceipts } from "./review-receipts.ts";
import { validateSavedDiscoveryInstagramProfile } from "./contact-profile-contracts.ts";
import { loadDiscoveryWebSourceReceipt } from "./web-receipt.ts";
import { discoveryWebProfileTarget } from "./web-profile.ts";
import { validateDiscoveryRetainedContent, validateDiscoveryRetainedProfile } from "./native-material.ts";
import { validateDiscoveryContentResult } from "./content-result.ts";
import { linkedinConnectionObservation, linkedinExperiencePlan } from "../platforms/opencli/linkedin.ts";
import { sameYoutubeVideoSource } from "../platforms/opencli/youtube.ts";
import { loadRetainedYoutubeEvidence } from "./youtube-retained.ts";
import { assessDiscoveryYoutube, discoveryCriteriaForPlatform, parseDiscoverySubscribers, parseDiscoveryDouyinFollowers, parseDiscoveryFacebookFollowers, parseDiscoveryInstagramFollowers } from "./qualification.ts";

const scopesSchema = z.array(z.strictObject({ platform: z.string(), profile_ref: z.string(), keywords: z.array(z.string()) }));
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const parse = (value: string) => parseApplicationJson(Buffer.from(value));
function bounded(value: unknown, maximum: number): string | null { if (typeof value !== "string") return null; const trimmed = value.trim(); return trimmed.length && [...trimmed].length <= maximum ? trimmed : null; }

// 调用者持有原账号租约，并为一次或一对材料开启同一只读事务。
export class DiscoveryReviewBasisUnavailable extends Error {
  constructor() { super("review_material_author_binding_unavailable"); }
}
export function findDiscoveryReviewBasis(database: DatabaseSync, campaign: string, revision: number, hit: string, contactNavigation = false): DiscoveryReviewBasis | null {
  try { return loadDiscoveryReviewBasis(database, campaign, revision, hit, contactNavigation); }
  catch (error) { if (error instanceof DiscoveryReviewBasisUnavailable) return null; throw error; }
}
export function loadDiscoveryReviewBasis(database: DatabaseSync, campaign: string, revision: number, hit: string, contactNavigation = false): DiscoveryReviewBasis {
  if (!database.isTransaction) throw new Error("review_basis_requires_snapshot");
  if (!campaign || Buffer.byteLength(campaign) > 128 || !hit || Buffer.byteLength(hit) > 128 || !Number.isInteger(revision) || revision <= 0 || revision > 4294967295) throw new Error("review_material_input_invalid");
  const source = database.prepare("SELECT platform FROM discovery_search_hits WHERE discovery_campaign_id=?1 AND id=?2").get(campaign, hit);
  if (!source) throw new Error("review_source_unavailable");
  const platform = z.string().parse(source.platform);
  const webSource = ["google", "yahoo"].includes(platform) || platform === "instagram" && database.prepare("SELECT 1 FROM discovery_web_source_reads WHERE discovery_campaign_id=? AND search_hit_id=?").get(campaign, hit) !== undefined;
  let sourceJoin: string, candidateJoin: string;
  if (webSource) {
    sourceJoin = `JOIN discovery_web_source_reads web ON web.search_hit_id=h.id AND web.discovery_campaign_id=d.id
      AND web.expected_revision=d.current_revision AND web.origin_platform=h.platform AND web.profile_ref=h.profile_ref
      AND web.source_url=h.source_url AND web.target_platform IN ('youtube','twitter','douyin','instagram','tiktok','linkedin','bilibili','weibo') AND web.status='completed' AND web.stop_code IS NULL
      JOIN discovery_video_completions v ON v.search_hit_id=h.id
      JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=d.id
      AND b.criteria_revision=d.current_revision AND b.native_author_id=web.native_author_id`;
    candidateJoin = `JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=d.id
      AND c.criteria_revision=d.current_revision AND c.platform=web.target_platform AND c.source_platform=h.platform
      AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=web.native_author_id
      AND json_extract(c.evidence_json,'$.source_urls[0]')=h.source_url`;
  } else if (platform === "youtube") {
    sourceJoin = `JOIN discovery_video_completions v ON v.search_hit_id=h.id
      JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=d.id AND b.criteria_revision=d.current_revision`;
    candidateJoin = `JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=d.id
      AND c.criteria_revision=d.current_revision AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=b.native_author_id`;
  } else if (platform === "twitter") {
    sourceJoin = `JOIN discovery_content_reads v ON v.search_hit_id=h.id AND v.discovery_campaign_id=d.id
      AND v.expected_revision=d.current_revision AND v.platform=h.platform AND v.profile_ref=h.profile_ref
      AND v.source_url=h.source_url AND v.status='completed' AND v.stop_code IS NULL
      JOIN discovery_content_identity_bindings b ON b.search_hit_id=h.id AND b.identity_search_hit_id=h.id AND b.profile_search_hit_id=h.id`;
    candidateJoin = `JOIN discovery_candidates c ON c.discovery_campaign_id=d.id AND c.criteria_revision=d.current_revision
      AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=b.account_id`;
  } else if (["tiktok", "instagram", "xiaohongshu"].includes(platform)) {
    sourceJoin = `JOIN discovery_search_authors s ON s.search_hit_id=h.id
      JOIN discovery_profile_reads p ON p.search_hit_id=h.id AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
      AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id
      AND p.status='completed' AND p.stop_code IS NULL AND json_extract(p.native_profile_json,'$.${platform === "xiaohongshu" ? "author_id" : "id"}')=s.author_id`;
    candidateJoin = `JOIN discovery_candidates c ON c.discovery_campaign_id=d.id AND c.criteria_revision=d.current_revision
      AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=s.author_id
      AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')`;
  } else if (platform === "bilibili") {
    sourceJoin = `JOIN discovery_search_authors s ON s.search_hit_id=h.id
      JOIN discovery_profile_reads p ON p.search_hit_id=h.id AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
        AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id AND p.status='completed' AND p.stop_code IS NULL
        AND json_extract(p.native_profile_json,'$.mid')=s.author_id
      JOIN discovery_content_reads v ON v.search_hit_id=h.id AND v.discovery_campaign_id=d.id AND v.expected_revision=d.current_revision
        AND v.platform=h.platform AND v.profile_ref=h.profile_ref AND v.author_id=s.author_id
        AND v.source_url=h.source_url AND v.status='completed' AND v.stop_code IS NULL`;
    candidateJoin = `JOIN discovery_candidates c ON c.discovery_campaign_id=d.id AND c.criteria_revision=d.current_revision
      AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=s.author_id AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')`;
  } else if (platform === "linkedin") {
    sourceJoin = `JOIN discovery_search_authors s ON s.search_hit_id=h.id
      JOIN discovery_profile_reads p ON p.search_hit_id=h.id AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
        AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id AND p.status='completed' AND p.stop_code IS NULL
        AND json_extract(p.native_profile_json,'$.author_handle')=s.author_id
        AND json_extract(p.native_profile_json,'$.profile_url')=s.author_url AND s.author_url=h.source_url`;
    candidateJoin = `JOIN discovery_candidates c ON c.discovery_campaign_id=d.id AND c.criteria_revision=d.current_revision
      AND c.platform=h.platform AND c.source_platform=h.platform AND c.source_profile_ref=h.profile_ref
      AND c.platform_account_id=json_extract(p.native_profile_json,'$.account_id')
      AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')`;
  } else if (["facebook", "weibo", "bluesky", "douyin", "zhihu", "hackernews", "pixiv"].includes(platform)) {
    const authorKey = ["douyin", "zhihu", "hackernews", "pixiv"].includes(platform) ? "author_id" : platform === "bluesky" ? "did" : "id";
    const joint = platform === "douyin" ? `JOIN discovery_douyin_author_reads joint ON joint.search_hit_id=h.id AND joint.discovery_campaign_id=d.id
      AND joint.expected_revision=d.current_revision AND joint.platform=h.platform AND joint.profile_ref=h.profile_ref AND joint.source_url=h.source_url
      AND joint.status='completed' AND joint.stop_code IS NULL AND joint.duplicate_of IS NULL AND joint.author_id=s.author_id AND joint.profile_url=s.author_url` : "";
    sourceJoin = `JOIN discovery_search_authors s ON s.search_hit_id=h.id ${joint}
      JOIN discovery_profile_reads p ON p.search_hit_id=h.id AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
        AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id AND p.status='completed' AND p.stop_code IS NULL
        AND json_extract(p.native_profile_json,'$.${authorKey}')=s.author_id
        ${platform === "douyin" ? "AND p.call_number=joint.call_number AND json_extract(p.native_profile_json,'$.profile_url')=s.author_url" : ""}
      JOIN discovery_content_reads v ON v.search_hit_id=h.id AND v.discovery_campaign_id=d.id AND v.expected_revision=d.current_revision
        AND v.platform=h.platform AND v.profile_ref=h.profile_ref AND v.author_id=s.author_id
        AND (v.source_url=h.source_url OR (h.platform='facebook' AND EXISTS(
          SELECT 1 FROM discovery_facebook_content_targets t WHERE t.search_hit_id=h.id AND t.discovery_campaign_id=d.id
            AND t.expected_revision=d.current_revision AND t.profile_ref=h.profile_ref AND t.author_id=s.author_id AND t.profile_url=s.author_url
            AND t.status='completed' AND t.stop_code IS NULL AND t.call_number<v.call_number
            AND EXISTS(SELECT 1 FROM json_each(t.source_urls_json) l WHERE l.value=v.source_url))))
        AND v.status='completed' AND v.stop_code IS NULL
        ${platform === "douyin" ? "AND v.call_number=joint.call_number" : ""}`;
    candidateJoin = `JOIN discovery_candidates c ON c.discovery_campaign_id=d.id AND c.criteria_revision=d.current_revision
      AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref
      AND c.platform_account_id=s.author_id
      AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')`;
  } else throw new Error("review_platform_unsupported");
  const row = database.prepare(`SELECT a.criteria_json,a.scope_json,a.approved_at,h.profile_ref,h.search_keyword,h.source_url,
    h.title,h.author_label,h.captured_at,c.id AS candidate_id,c.platform_account_id,c.evidence_json
    FROM discovery_campaigns d JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
    JOIN discovery_search_hits h ON h.discovery_campaign_id=d.id AND h.id=?3 AND h.platform=?4
    ${sourceJoin} ${candidateJoin}
    WHERE d.id=?1 AND d.current_revision=?2 AND d.phase='active' AND a.approved_at IS NOT NULL`).get(campaign, revision, hit, platform);
  if (!row) throw new DiscoveryReviewBasisUnavailable();
  const basis = discoveryReviewBasis.parse({ campaign_id: campaign, revision, criteria_json: row.criteria_json, scopes_json: row.scope_json, approved_at: row.approved_at,
    hit: { id: hit, campaign_id: campaign, platform, profile_ref: row.profile_ref, search_keyword: row.search_keyword, source_url: row.source_url, title: row.title, author_label: row.author_label, captured_at: row.captured_at, author_id: null, author_url: null, author_observed_at: null },
    candidate_id: row.candidate_id, native_author_id: row.platform_account_id, native_followers: null, native_follower_estimate: null, native_profile: null, evidence_json: row.evidence_json, transcript: null, source_description: null, native_content: null, language_samples: [], pages: [] });
  const scopes = scopesSchema.parse(parse(basis.scopes_json)), scope = scopes.find(value => value.platform === platform && value.profile_ref === basis.hit.profile_ref);
  if (scope && !scope.keywords.includes(basis.hit.search_keyword)) {
    const admitted = database.prepare("SELECT scopes_json FROM discovery_query_rounds WHERE discovery_campaign_id=?1 AND revision=?2 ORDER BY round").all(campaign, revision).some(round => scopesSchema.parse(parse(z.string().parse(round.scopes_json))).some(value => value.platform === platform && value.profile_ref === scope.profile_ref && value.keywords.includes(basis.hit.search_keyword)));
    if (admitted) { scope.keywords.push(basis.hit.search_keyword); basis.scopes_json = stringifyApplicationJson(scopes); }
  }
  if (platform === "youtube") {
    const evidence = object(parse(basis.evidence_json)), sources = evidence.source_urls;
    const retained = loadRetainedYoutubeEvidence(database, evidence, basis.native_author_id, basis.hit.profile_ref);
    // 旧版回执没有视频标题字段，保留其材料结构及历史身份回执的摘要。
    if (typeof evidence.video_title === "string" && Array.isArray(sources) && (retained !== null || sameYoutubeVideoSource(sources[0], basis.hit.source_url)) && assessDiscoveryYoutube({}, evidence, basis.native_author_id).subscribers !== null && typeof evidence.subscribers_raw === "string") {
      const estimate = parseDiscoverySubscribers(evidence.subscribers_raw)!;
      const profileUrl = `https://www.youtube.com/channel/${basis.native_author_id}`;
      basis.native_followers = estimate.lower === estimate.upper ? estimate.nominal : null;
      basis.native_follower_estimate = { display: evidence.subscribers_raw, ...estimate };
      basis.native_profile = { profile_url: profileUrl, observed_at: z.number().int().positive().safe().parse(evidence.captured_at) };
      basis.hit.author_id = basis.native_author_id; basis.hit.author_url = profileUrl; basis.hit.author_observed_at = basis.native_profile.observed_at;
    }
    if (retained) basis.native_content = { claim: { campaign_id: campaign, expected_revision: revision, search_hit_id: hit, platform: "youtube", profile_ref: basis.hit.profile_ref, author_id: basis.native_author_id, source_url: retained.source_url, call_number: retained.call_number }, status: "completed", stop_code: null, captured_at: z.number().int().positive().parse(evidence.captured_at), native_content: { source_url: retained.source_url, author_id: basis.native_author_id, text: `${z.string().parse(evidence.video_title)}\n\n${z.string().parse(evidence.video_description)}`, retained_source_hit_id: retained.search_hit_id } };
  }
  if (webSource) {
    const read = loadDiscoveryWebSourceReceipt(database, campaign, hit), evidence = object(parse(basis.evidence_json)), native = read.native, profile = object(native.profile), content = object(native.content);
    if (read.is_primary_source !== 1 || read.status !== "completed" || read.expected_revision !== revision || read.origin_platform !== platform || read.profile_ref !== basis.hit.profile_ref || read.source_url !== basis.hit.source_url || read.candidate_id !== basis.candidate_id || read.native_author_id !== basis.native_author_id || object(evidence.web_origin).target_platform !== read.target_platform) throw new Error("review_native_web_source_binding_unavailable");
    const generic = discoveryWebProfileTarget(read.source_url), profileTarget = ["instagram", "tiktok"].includes(read.target_platform), social = generic !== null || ["twitter", "douyin", "instagram", "tiktok"].includes(read.target_platform), description = generic ? null : profileTarget ? null : social ? content.text : native.description;
    basis.source_description = typeof description === "string" ? description : null;
    basis.hit.author_label = z.string().parse(generic ? read.target_platform === "linkedin" ? profile.author_label : profile.name : read.target_platform === "douyin" ? profile.author_label : read.target_platform === "twitter" ? profile.name : profileTarget ? profile.display_name : native.author);
    basis.hit.author_id = basis.native_author_id; basis.hit.author_url = generic?.profile_url ?? (social ? z.string().parse(profile.profile_url) : `https://www.youtube.com/channel/${basis.native_author_id}`); basis.hit.author_observed_at = read.updated_at;
    if (social) {
      basis.native_followers = typeof profile.follower_count === "number" || typeof profile.follower_count === "bigint" ? profile.follower_count : null;
      const bio = generic ? read.target_platform === "bilibili" ? profile.sign : read.target_platform === "youtube" ? profile.description : profile.bio : profileTarget ? read.target_platform === "instagram" ? profile.biography : profile.signature : profile.bio;
      basis.native_profile = { web_text: typeof profile.web_text === "string" ? profile.web_text : null, profile_url: basis.hit.author_url, observed_at: read.updated_at, bio: typeof bio === "string" ? bio : null, bio_status: profile.bio_truncated === true ? "truncated" : profile.bio_truncated === false || read.target_platform === "tiktok" ? typeof bio === "string" && bio.trim() ? "complete" : "missing" : "unknown", self_declared_location: typeof profile.self_declared_location === "string" ? profile.self_declared_location : null };
      if (read.target_platform === "instagram") {
        if (basis.native_followers === null && typeof profile.follower_text === "string") basis.native_follower_estimate = parseDiscoveryInstagramFollowers(profile.follower_text);
        if (database.prepare("SELECT 1 FROM discovery_content_reads WHERE discovery_campaign_id=? AND search_hit_id=?").get(campaign, hit)) {
          const saved = loadDiscoveryContentReceipt(database, campaign, hit);
          if (saved.claim.expected_revision !== revision || saved.claim.platform !== "instagram" || saved.claim.profile_ref !== read.profile_ref || saved.claim.author_id !== read.native_author_id || saved.claim.source_url !== read.source_url || saved.claim.call_number <= read.call_number) throw new Error("review_native_content_binding_unavailable");
          if (saved.status === "completed" && saved.stop_code === null) { validateDiscoveryContentResult(saved.claim, saved.native_content, null); basis.native_content = saved; }
        }
      }
      if (generic?.platform === "linkedin") {
        basis.criteria_json = stringifyApplicationJson(discoveryCriteriaForPlatform(parse(basis.criteria_json), "linkedin"));
        basis.native_profile.web_text = [profile.headline, bio].filter(value => typeof value === "string" && value.trim()).join("\n");
      }
    }
  }
  if (platform === "instagram" && !webSource) {
    const profile = loadDiscoveryProfileReceipt(database, campaign, hit), native = object(profile.native_profile), sources = object(parse(basis.evidence_json)).source_urls;
    if (profile.status !== "completed" || profile.stop_code !== null || profile.claim.expected_revision !== revision || profile.claim.platform !== platform || profile.claim.profile_ref !== basis.hit.profile_ref || profile.claim.author_id !== basis.native_author_id || profile.claim.source_url !== basis.hit.source_url || profile.claim.author_url !== basis.hit.source_url || !Array.isArray(sources) || sources[0] !== basis.hit.source_url) throw new Error("review_native_profile_binding_unavailable");
    if (Buffer.byteLength(stringifyApplicationJson(native)) > 16000) throw new Error("review_native_profile_binding_unavailable");
    validateSavedDiscoveryInstagramProfile(basis.native_author_id, native);
    if (native.profile_url !== profile.claim.author_url) throw new Error("review_native_profile_binding_unavailable");
    if (native.private_account === true) throw new Error("review_source_unavailable");
    basis.native_followers = typeof native.follower_count === "number" ? native.follower_count : null;
    if (basis.native_followers === null && typeof native.follower_text === "string") basis.native_follower_estimate = parseDiscoveryInstagramFollowers(native.follower_text);
    basis.native_profile = { web_text: bounded(native.web_text, 4000), profile_url: profile.claim.author_url, observed_at: profile.captured_at, bio: bounded(native.biography, 8000), self_declared_location: null };
    basis.native_profile.bio_status = native.bio_status === "complete" || native.bio_status === "truncated" || native.bio_status === "missing" ? native.bio_status : "unknown";
    if (database.prepare("SELECT EXISTS(SELECT 1 FROM discovery_content_reads WHERE discovery_campaign_id=?1 AND search_hit_id=?2) AS present").get(campaign, hit)?.present === 1) {
      const content = loadDiscoveryContentReceipt(database, campaign, hit);
      if (content.status === "completed" && content.stop_code === null) {
        const value = object(content.native_content);
        if (content.claim.expected_revision !== revision || content.claim.platform !== platform || content.claim.profile_ref !== basis.hit.profile_ref || content.claim.author_id !== basis.native_author_id || content.claim.source_url !== profile.claim.source_url || content.claim.call_number <= profile.claim.call_number) throw new Error("review_native_content_binding_unavailable");
        validateDiscoveryContentResult(content.claim, value, null);
        if (Buffer.byteLength(stringifyApplicationJson(value)) > 16000) throw new Error("review_native_content_binding_unavailable"); basis.native_content = content;
      }
    }
    basis.hit.author_id = profile.claim.author_id; basis.hit.author_url = profile.claim.author_url; basis.hit.author_observed_at = profile.captured_at;
  }
  if (platform === "xiaohongshu") {
    const profile = loadDiscoveryProfileReceipt(database, campaign, hit), authorUrl = profile.claim.author_url;
    if (profile.status !== "completed" || profile.stop_code !== null || profile.claim.expected_revision !== revision || profile.claim.profile_ref !== basis.hit.profile_ref || profile.claim.author_id !== basis.native_author_id || profile.claim.source_url !== basis.hit.source_url) throw new Error("review_native_profile_binding_unavailable");
    const native = validateDiscoveryRetainedProfile(platform, profile.claim.author_id, authorUrl, profile.claim.source_url, profile.native_profile);
    basis.native_followers = typeof native.follower_count === "number" ? native.follower_count : null;
    const previews = z.array(z.object({ title: z.string() })).parse(native.visible_posts);
    basis.native_profile = { web_text: previews.map(post => post.title).join("\n"), profile_url: authorUrl, observed_at: profile.captured_at, bio: bounded(native.bio, 8000), bio_status: native.bio_status === "complete" ? "complete" : "missing", self_declared_location: null };
    const saved = database.prepare("SELECT 1 FROM discovery_content_reads WHERE discovery_campaign_id=? AND search_hit_id=?").get(campaign, hit);
    if (saved) {
      const content = loadDiscoveryContentReceipt(database, campaign, hit);
      if (content.status === "completed" && content.stop_code === null) {
        if (content.claim.expected_revision !== revision || content.claim.platform !== platform || content.claim.profile_ref !== basis.hit.profile_ref || content.claim.author_id !== basis.native_author_id || content.claim.source_url !== basis.hit.source_url || content.claim.call_number <= profile.claim.call_number) throw new Error("review_native_content_binding_unavailable");
        validateDiscoveryRetainedContent(platform, content.claim.author_id, content.claim.source_url, content.native_content);
        basis.native_content = content;
      }
    }
    basis.hit.author_id = profile.claim.author_id; basis.hit.author_url = authorUrl; basis.hit.author_observed_at = profile.captured_at;
  }
  if (platform === "tiktok") {
    const stored = database.prepare(`SELECT s.author_id,s.author_url,s.observed_at,p.native_profile_json,p.updated_at
      FROM discovery_search_authors s JOIN discovery_profile_reads p ON p.search_hit_id=s.search_hit_id
      WHERE s.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.expected_revision=?3 AND p.platform='tiktok'
        AND p.profile_ref=?4 AND p.author_id=s.author_id AND p.status='completed' AND p.stop_code IS NULL`).get(hit, campaign, revision, basis.hit.profile_ref);
    if (!stored) throw new Error("review_native_profile_binding_unavailable");
    const profile = object(parse(z.string().parse(stored.native_profile_json))), username = typeof profile.username === "string" ? profile.username : "";
    const profileUrl = `https://www.tiktok.com/@${username}`, prefix = `${profileUrl}/video/`, video = basis.hit.source_url.startsWith(prefix) ? basis.hit.source_url.slice(prefix.length) : "";
    const evidence = object(parse(basis.evidence_json)), sources = evidence.source_urls;
    if (stored.author_id !== basis.native_author_id || profile.id !== stored.author_id || profile.profile_url !== profileUrl || stored.author_url !== profileUrl || username !== basis.hit.author_label || !/^[1-9][0-9]{0,24}$/u.test(video) || !basis.hit.title.trim() || [...basis.hit.title].length > 500 || !Array.isArray(sources) || sources[0] !== basis.hit.source_url) throw new Error("review_native_profile_binding_unavailable");
    const followers = profile.follower_count;
    if (!(typeof followers === "number" && Number.isSafeInteger(followers) && followers >= 0 || typeof followers === "bigint" && followers >= 0n && followers <= BigInt(Number.MAX_SAFE_INTEGER))) throw new Error("review_native_followers_invalid");
    basis.native_followers = followers;
    basis.native_profile = { web_text: null, profile_url: profileUrl, observed_at: z.number().int().safe().parse(stored.updated_at), bio: bounded(profile.signature, 16000), self_declared_location: null, bio_status: profile.bio_truncated === true ? "truncated" : typeof profile.signature === "string" && profile.signature ? "complete" : "missing" };
    basis.hit.author_id = z.string().parse(stored.author_id); basis.hit.author_url = z.string().parse(stored.author_url); basis.hit.author_observed_at = z.number().int().safe().parse(stored.observed_at);
  }
  if (platform === "linkedin") {
    basis.criteria_json = stringifyApplicationJson(discoveryCriteriaForPlatform(parse(basis.criteria_json), platform));
    const profile = loadDiscoveryProfileReceipt(database, campaign, hit), sources = object(parse(basis.evidence_json)).source_urls;
    const authorUrl = profile.claim.author_url;
    if (profile.status !== "completed" || profile.stop_code !== null || profile.claim.expected_revision !== revision || profile.claim.platform !== platform || profile.claim.profile_ref !== basis.hit.profile_ref || profile.claim.source_url !== basis.hit.source_url || authorUrl !== basis.hit.source_url || !Array.isArray(sources) || sources[0] !== basis.hit.source_url || sources[1] !== authorUrl) throw new Error("review_native_profile_binding_unavailable");
    const native = validateDiscoveryRetainedProfile(platform, profile.claim.author_id, authorUrl, profile.claim.source_url, profile.native_profile);
    if (native.account_id !== basis.native_author_id) throw new Error("review_native_profile_binding_unavailable");
    basis.native_followers = typeof native.follower_count === "number" || typeof native.follower_count === "bigint" ? native.follower_count : null;
    const bioStatus = native.bio_status === "absent" ? "missing" : native.bio_status === "loading" || native.bio_status === "unknown" ? "unknown" : native.bio_status === "complete" || native.bio_status === "truncated" ? native.bio_status : native.bio_truncated === true ? "truncated" : native.bio_truncated === false ? native.bio ? "complete" : "missing" : "unknown";
    basis.native_profile = { web_text: bounded(native.headline, 2000), profile_url: authorUrl, observed_at: profile.captured_at, bio: bounded(native.bio, 8000), bio_status: bioStatus, self_declared_location: null };
    if (native.connections !== undefined) {
      basis.native_profile.connections = linkedinConnectionObservation.parse(native.connections);
      basis.native_profile.web_text = [basis.native_profile.web_text, bounded(native.follower_text, 100), basis.native_profile.connections?.raw].filter(Boolean).join("\n") || null;
    }
    const initialExperience = native.current_experience === undefined ? null : linkedinExperiencePlan(profile.claim.author_id,basis.native_author_id).decode([native.current_experience]);
    const experience = retainedLinkedinExperience(database,{...profile.claim,source_url:authorUrl},basis.native_author_id);
    const entries = [...new Set([...(initialExperience?.entries ?? []),...(experience?.native.entries ?? [])].map(entry=>entry.text))];
    if (entries.length) basis.native_profile.web_text = [basis.native_profile.web_text,"Experience",...entries].filter(Boolean).join("\n");
    if (experience) {
      basis.native_profile.observed_at = Math.max(Number(profile.captured_at),experience.captured_at);
      basis.native_profile.experience_checked = true;
    }
    basis.hit.author_id = basis.native_author_id; basis.hit.author_url = authorUrl; basis.hit.author_observed_at = profile.captured_at;
    if (database.prepare("SELECT 1 FROM discovery_content_reads WHERE discovery_campaign_id=? AND search_hit_id=?").get(campaign, hit)) {
      const content = loadDiscoveryContentReceipt(database, campaign, hit);
      if (content.status === "completed" && content.stop_code === null) {
        if (content.claim.expected_revision !== revision || content.claim.platform !== platform || content.claim.profile_ref !== basis.hit.profile_ref || content.claim.author_id !== profile.claim.author_id || content.claim.source_url !== basis.hit.source_url || content.claim.call_number <= profile.claim.call_number) throw new Error("review_native_content_binding_unavailable");
        const value = validateDiscoveryRetainedContent(platform, content.claim.author_id, content.claim.source_url, content.native_content);
        if (value.account_id !== basis.native_author_id) throw new Error("review_native_content_binding_unavailable");
        basis.native_content = content;
      }
    }
  }
  if (["facebook", "weibo", "bluesky", "douyin", "zhihu", "hackernews", "pixiv"].includes(platform)) {
    const profile = loadDiscoveryProfileReceipt(database, campaign, hit), content = loadDiscoveryContentReceipt(database, campaign, hit), sources = object(parse(basis.evidence_json)).source_urls;
    const authorUrl = profile.claim.author_url;
    if (profile.status !== "completed" || profile.stop_code !== null || content.status !== "completed" || content.stop_code !== null || profile.claim.expected_revision !== revision || content.claim.expected_revision !== revision || profile.claim.platform !== platform || content.claim.platform !== platform || profile.claim.profile_ref !== basis.hit.profile_ref || content.claim.profile_ref !== basis.hit.profile_ref || profile.claim.author_id !== basis.native_author_id || content.claim.author_id !== profile.claim.author_id || profile.claim.source_url !== basis.hit.source_url || platform !== "facebook" && content.claim.source_url !== basis.hit.source_url || (platform === "douyin" ? profile.claim.call_number !== content.claim.call_number : profile.claim.call_number >= content.claim.call_number) || !Array.isArray(sources) || sources[0] !== basis.hit.source_url || sources[1] !== authorUrl) throw new Error("review_native_content_binding_unavailable");
    const native = validateDiscoveryRetainedProfile(platform, profile.claim.author_id, authorUrl, profile.claim.source_url, profile.native_profile);
    if (platform === "facebook" && native.follower_text === "") throw new Error("review_source_unavailable");
    validateDiscoveryRetainedContent(platform, content.claim.author_id, content.claim.source_url, content.native_content);
    basis.native_followers = typeof native.follower_count === "number" || typeof native.follower_count === "bigint" ? native.follower_count : null;
    if (platform === "douyin" && typeof native.follower_text === "string") basis.native_follower_estimate = parseDiscoveryDouyinFollowers(native.follower_text);
    if (platform === "facebook" && basis.native_followers === null && typeof native.follower_text === "string") basis.native_follower_estimate = parseDiscoveryFacebookFollowers(native.follower_text);
    basis.native_profile = { web_text: null, profile_url: authorUrl, observed_at: profile.captured_at, bio: bounded(native[["weibo", "douyin", "zhihu", "pixiv"].includes(platform) ? "bio" : "biography"], ["bluesky", "douyin", "zhihu", "hackernews", "pixiv"].includes(platform) ? 8000 : 2000), self_declared_location: null };
    if (["facebook", "weibo", "douyin"].includes(platform)) basis.native_profile.bio_status = native.bio_status === "complete" || native.bio_status === "truncated" || native.bio_status === "missing" || native.bio_status === "unavailable" || native.bio_status === "unknown" ? native.bio_status : native.bio_truncated === true ? "truncated" : native.bio_truncated === false ? basis.native_profile.bio ? "complete" : "missing" : "unknown";
    basis.hit.author_id = profile.claim.author_id; basis.hit.author_url = authorUrl; basis.native_content = content;
  }
  if (platform === "bilibili") {
    const stored = database.prepare(`SELECT s.author_id,s.author_url,s.observed_at,p.native_profile_json,p.updated_at
      FROM discovery_search_authors s JOIN discovery_profile_reads p ON p.search_hit_id=s.search_hit_id
      WHERE s.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.expected_revision=?3 AND p.platform='bilibili'
        AND p.profile_ref=?4 AND p.author_id=s.author_id AND p.status='completed' AND p.stop_code IS NULL`).get(hit, campaign, revision, basis.hit.profile_ref);
    if (!stored) throw new Error("review_native_profile_binding_unavailable");
    const profile = object(parse(z.string().parse(stored.native_profile_json))), profileUrl = `https://space.bilibili.com/${z.string().parse(stored.author_id)}`;
    const content = loadDiscoveryContentReceipt(database, campaign, hit), native = object(content.native_content), sources = object(parse(basis.evidence_json)).source_urls;
    if (stored.author_id !== basis.native_author_id || profile.mid !== stored.author_id || profile.profile_url !== profileUrl || stored.author_url !== profileUrl || content.status !== "completed" || content.stop_code !== null || content.claim.expected_revision !== revision || content.claim.platform !== "bilibili" || content.claim.profile_ref !== basis.hit.profile_ref || content.claim.author_id !== basis.native_author_id || content.claim.source_url !== basis.hit.source_url || native.author_id !== stored.author_id || native.source_url !== basis.hit.source_url || !basis.hit.source_url.startsWith("https://www.bilibili.com/video/") || native.bvid !== basis.hit.source_url.slice("https://www.bilibili.com/video/".length) || !Array.isArray(sources) || sources[0] !== basis.hit.source_url) throw new Error("review_native_content_binding_unavailable");
    const followers = profile.follower_count;
    if (!(typeof followers === "number" && Number.isSafeInteger(followers) && followers >= 0 || typeof followers === "bigint" && followers >= 0n && followers <= BigInt(Number.MAX_SAFE_INTEGER))) throw new Error("review_native_followers_invalid");
    basis.native_followers = followers; basis.native_profile = { web_text: null, profile_url: profileUrl, observed_at: z.number().int().safe().parse(stored.updated_at), bio: bounded(profile.sign, 8000), self_declared_location: null, bio_status: profile.bio_status === "complete" || profile.bio_status === "truncated" || profile.bio_status === "missing" || profile.bio_status === "unavailable" ? profile.bio_status : "unknown" };
    basis.hit.author_id = z.string().parse(stored.author_id); basis.hit.author_url = z.string().parse(stored.author_url); basis.hit.author_observed_at = z.number().int().safe().parse(stored.observed_at); basis.native_content = content;
  }
  if (platform === "twitter") {
    const stored = database.prepare(`SELECT p.native_profile_json,p.updated_at FROM discovery_profile_reads p
      JOIN discovery_profile_identity_matches m ON m.search_hit_id=p.search_hit_id AND m.identity_search_hit_id=p.search_hit_id
      WHERE p.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.expected_revision=?3 AND p.platform='twitter'
        AND p.profile_ref=?4 AND p.status='completed' AND p.stop_code IS NULL AND m.account_id=?5
        AND json_extract(p.native_profile_json,'$.account_id')=m.account_id`).get(hit, campaign, revision, basis.hit.profile_ref, basis.native_author_id);
    if (!stored) throw new Error("review_native_profile_binding_unavailable");
    const profile = object(parse(z.string().parse(stored.native_profile_json))), followers = profile.follower_count;
    if (!(typeof followers === "number" && Number.isSafeInteger(followers) && followers >= 0 || typeof followers === "bigint" && followers >= 0n && followers <= BigInt(Number.MAX_SAFE_INTEGER))) throw new Error("review_native_followers_invalid");
    const content = loadDiscoveryContentReceipt(database, campaign, hit), identity = object(object(content.native_content).identity);
    if (content.status !== "completed" || content.stop_code !== null || content.claim.expected_revision !== revision || content.claim.platform !== "twitter" || content.claim.profile_ref !== basis.hit.profile_ref || content.claim.source_url !== basis.hit.source_url || identity.account_id !== basis.native_author_id) throw new Error("review_native_content_binding_unavailable");
    if (typeof identity.profile_url !== "string" || profile.profile_url !== identity.profile_url) throw new Error("review_native_profile_binding_unavailable");
    basis.native_followers = followers;
    basis.native_profile = { web_text: null, profile_url: identity.profile_url, observed_at: z.number().int().safe().parse(stored.updated_at), bio: bounded(profile.bio, 1000), self_declared_location: bounded(profile.self_declared_location, 256) };
    basis.native_content = content;
  }
  if (Number(database.prepare("PRAGMA user_version").get()!.user_version) >= 95) {
    const saved = database.prepare("SELECT material_json FROM discovery_search_materials WHERE search_hit_id=?").get(hit);
    if (saved) {
      const material = creatorSearchMaterial.parse(parse(z.string().parse(saved.material_json)));
      if (material.source_url !== basis.hit.source_url || material.author_url !== (basis.hit.author_url ?? basis.native_profile?.profile_url) || material.native_author_id !== null && material.native_author_id !== basis.native_author_id) throw new Error("review_search_material_binding_changed");
      if (material.native_author_id === basis.native_author_id && material.body_text.trim()) basis.source_description = [basis.source_description, `Search response (${material.body_completeness}): ${material.body_text}`].filter(Boolean).join("\n");
      if (platform === "twitter" && basis.native_content?.status === "completed" && material.author_id === object(object(basis.native_content.native_content).identity).handle && !basis.native_profile?.bio && material.bio && basis.native_profile) {
        basis.native_profile.bio = material.bio; basis.native_profile.bio_status = "unknown";
      }
    }
  }
  basis.evidence_json = stringifyApplicationJson(stripUnauditedDiscoveryProofs(database, parse(basis.evidence_json), basis.candidate_id, campaign, revision, basis.native_author_id));
  basis.language_samples = loadDiscoveryLanguageSamples(database, campaign, revision, basis.candidate_id);
  let bytes = Buffer.byteLength(stringifyApplicationJson(basis.language_samples)) + [basis.criteria_json, basis.scopes_json, basis.evidence_json, basis.source_description ?? "", basis.hit.source_url, basis.hit.title, basis.hit.author_label].reduce((total, value) => total + Buffer.byteLength(value), 0);
  if (basis.native_content) bytes += Buffer.byteLength(stringifyApplicationJson(basis.native_content));
  if (basis.native_profile) {
    const profile = basis.native_profile;
    bytes += Buffer.byteLength(stringifyApplicationJson({ ...(profile.web_text != null ? { web_text: profile.web_text } : {}), profile_url: profile.profile_url, observed_at: profile.observed_at, ...(profile.bio != null ? { bio: profile.bio } : {}), ...(profile.self_declared_location != null ? { self_declared_location: profile.self_declared_location } : {}) }));
  }
  return { ...basis, ...loadDiscoveryReviewReceipts(database, basis, bytes, contactNavigation) };
}
