import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { discoveryScorePasses } from "./library-material.ts";
import { discoveryNonContact, extractDiscoveryContacts } from "./contacts.ts";
import { discoveryCommunityInvite, discoveryContactPageLinks, discoveryContactPriority, discoveryContactReadKey, discoveryObservesLink, discoveryPromotional, discoveryThirdPartyLink, trimDiscoveryLink } from "./contact-navigation.ts";
import { discoveryProfileContactFields } from "./contact-profile.ts";
import { discoveryLookupPageMatchesMaterial, discoveryYoutubeProfileUrl, loadDiscoveryContactLookupLinks, storeDiscoveryContacts } from "./contact-repository.ts";
import { buildDiscoveryReviewMaterial, discoveryPublicPageRead, type DiscoveryReviewMaterial } from "./review-material.ts";
import { loadDiscoveryReviewBasis } from "./review-basis.ts";
import { stripUnauditedDiscoveryProofs } from "./review-proof-audit.ts";

const text = z.string();
export const discoveryPublicPageInput = z.strictObject({ campaign_id: text, expected_revision: z.number().int().min(0).max(4294967295), candidate_id: text, profile_ref: text, requested_url: text });
export type DiscoveryPublicPageInput = z.infer<typeof discoveryPublicPageInput>;
const object = z.record(text, z.unknown());
const snapshot = (database: DatabaseSync) => { if (!database.isTransaction) throw new Error("contact_projection_requires_snapshot"); };
export function discoveryEligibleContactAuthor(database: DatabaseSync, campaign: string, revision: number, candidate: string): string | null {
  snapshot(database);
  const row = database.prepare(`SELECT c.platform_account_id,c.verdict,c.evidence_json FROM discovery_candidates c
    JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.discovery_campaign_id AND p.criteria_revision=c.criteria_revision
    WHERE c.id=? AND c.discovery_campaign_id=? AND c.criteria_revision=? AND p.policy='score_and_contact_v1'
      AND (c.verdict='qualified' OR (c.verdict='pending'
        AND EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.assessment.reasons') reason WHERE reason.value='requirement_unverified')
        AND NOT EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.assessment.reasons') reason WHERE reason.value!='requirement_unverified')))`).get(candidate, campaign, revision);
  if (!row) return null;
  const author = text.parse(row.platform_account_id), evidence = stripUnauditedDiscoveryProofs(database, parseApplicationJson(text.parse(row.evidence_json)), candidate, campaign, revision, author);
  const reasons = object.parse(evidence.assessment ?? {}).reasons;
  const requiresEvidence = row.verdict === "pending" && Array.isArray(reasons) && reasons.length > 0 && reasons.every(reason => reason === "requirement_unverified");
  return discoveryScorePasses(requiresEvidence ? "qualified" : text.parse(row.verdict), evidence, author) ? author : null;
}
export function discoveryHasHiddenEmail(database: DatabaseSync, campaign: string, revision: number, candidate: string, author: string): boolean {
  snapshot(database);
  return database.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_contact_observations o JOIN discovery_campaigns d ON d.id=o.discovery_campaign_id AND d.current_revision=o.criteria_revision WHERE o.candidate_id=? AND o.discovery_campaign_id=? AND o.criteria_revision=? AND o.native_author_id=? AND o.kind='hidden_email' AND o.attribution='author_published') AS found`).get(candidate, campaign, revision, author)!.found === 1;
}
export function loadDiscoveryNativeContactMaterial(database: DatabaseSync, raw: DiscoveryPublicPageInput): DiscoveryReviewMaterial | null {
  snapshot(database); const input = discoveryPublicPageInput.parse(raw), author = discoveryEligibleContactAuthor(database, input.campaign_id, input.expected_revision, input.candidate_id);
  if (author === null) return null;
  const row = database.prepare(`SELECT h.id FROM discovery_search_hits h JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id
    AND c.source_platform=h.platform AND c.source_profile_ref=h.profile_ref
    WHERE c.id=? AND c.discovery_campaign_id=? AND c.criteria_revision=? AND c.source_profile_ref=? AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')`).get(input.candidate_id, input.campaign_id, input.expected_revision, input.profile_ref);
  if (!row) return null;
  const material = buildDiscoveryReviewMaterial(loadDiscoveryReviewBasis(database, input.campaign_id, input.expected_revision, text.parse(row.id), true));
  if (material.candidate_id !== input.candidate_id || material.native_author_id !== author) throw new Error("contact_source_author_mismatch");
  return material;
}
export function discoveryContactHostBlocked(database: DatabaseSync, campaign: string, source: string): boolean {
  snapshot(database); const url = URL.parse(source); if (!url) return true;
  return database.prepare(`SELECT r.requested_url FROM discovery_public_page_reads r JOIN discovery_candidates c ON c.id=r.candidate_id
    WHERE c.discovery_campaign_id=? AND r.status='failed' AND r.stop_code IN ('opencli_risk_control','opencli_auth_required')`).all(campaign).some(row => URL.parse(text.parse(row.requested_url))?.hostname === url.hostname);
}
function nativeTexts(material: DiscoveryReviewMaterial): (string | null | undefined)[] {
  const content = material.native_content?.native_content == null ? null : object.parse(material.native_content.native_content), value = content?.[material.hit.platform === "bilibili" ? "description" : "text"];
  return [material.native_profile?.bio, typeof value === "string" ? value : null, material.video_description];
}
export function discoveryAuthorObservesLink(database: DatabaseSync, material: DiscoveryReviewMaterial, url: string): boolean {
  const fields = discoveryProfileContactFields(database, material);
  return discoveryObservesLink(fields, url) || nativeTexts(material).some(text => text != null && discoveryObservesLink(text, url) && !discoveryPromotional(url, text) && !discoveryThirdPartyLink(url, text)) || material.public_pages.some(page => page.status === "completed" && page.stop_code === null && discoveryYoutubeProfileUrl(material) === page.requested_url && discoveryObservesLink(page.markdown_excerpt, url) && !discoveryPromotional(url, page.markdown_excerpt) && !discoveryThirdPartyLink(url, page.markdown_excerpt));
}
export function discoveryNativeLinkSource(database: DatabaseSync, input: DiscoveryPublicPageInput): readonly [string, string, string, string] | null {
  const material = loadDiscoveryNativeContactMaterial(database, input);
  if (material === null || discoveryContactHostBlocked(database, input.campaign_id, input.requested_url) || discoveryNonContact(input.requested_url)) return null;
  const source = (excerpt: string): readonly [string, string, string, string] => [material.hit.id, material.hit.source_url, excerpt, material.hit.platform];
  const youtube = discoveryYoutubeProfileUrl(material);
  if (youtube === input.requested_url) return source(input.requested_url);
  const fields = discoveryProfileContactFields(database, material);
  for (const text of [...nativeTexts(material), fields]) if (text != null && discoveryObservesLink(text, input.requested_url) && !discoveryPromotional(input.requested_url, text) && !discoveryThirdPartyLink(input.requested_url, text)) return source(text);
  for (const page of material.public_pages) {
    if (page.status !== "completed" || page.stop_code !== null) continue;
    if (youtube === page.requested_url && discoveryObservesLink(page.markdown_excerpt, input.requested_url) && !discoveryPromotional(input.requested_url, page.markdown_excerpt) && !discoveryThirdPartyLink(input.requested_url, page.markdown_excerpt)) return source(page.markdown_excerpt);
    if (discoveryAuthorObservesLink(database, material, page.requested_url) && page.document_url !== null && discoveryContactPageLinks(page.document_url, page.markdown_excerpt).includes(input.requested_url)) return source(page.markdown_excerpt);
  }
  for (const [link, query] of loadDiscoveryContactLookupLinks(database, input.candidate_id, input.expected_revision)) if (link === input.requested_url) return source(query);
  return null;
}
export function nextDiscoveryNativeContactLink(database: DatabaseSync, campaign: string, excludedPlatforms: readonly string[]): DiscoveryPublicPageInput | null {
  snapshot(database);
  const rows = database.prepare(`SELECT c.id,c.criteria_revision,c.source_profile_ref,p.status,p.active_query FROM discovery_candidates c
    JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
    JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
    JOIN discovery_platform_runs p ON p.discovery_campaign_id=d.id AND p.platform=c.source_platform AND p.profile_ref=c.source_profile_ref
    JOIN discovery_recommendation_policies policy ON policy.discovery_campaign_id=d.id AND policy.criteria_revision=d.current_revision
    WHERE d.id=? AND d.phase='active' AND c.criteria_revision=d.current_revision AND a.approved_at IS NOT NULL
      AND NOT EXISTS(SELECT 1 FROM json_each(?) excluded WHERE excluded.value IN (c.platform,c.source_platform))
      AND policy.policy='score_and_contact_v1' AND (c.verdict='qualified' OR (c.verdict='pending' AND json_array_length(json_extract(a.criteria_json,'$.requirements'))>0)) AND (d.max_calls IS NULL OR d.used_calls<d.max_calls)
      AND (c.verdict='qualified' OR (SELECT COUNT(*) FROM discovery_public_page_reads page WHERE page.candidate_id=c.id)<2)
      AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=d.id AND r.status='searching')
      AND (p.status='completed' OR (p.status='technical_stop' AND p.stop_code='interrupted_public_page_read')) ORDER BY c.id`).all(campaign, stringifyApplicationJson(excludedPlatforms));
  for (const raw of rows) {
    const row = z.object({ id: text, criteria_revision: z.number().int().min(0).max(4294967295), source_profile_ref: text, status: text, active_query: text.nullable() }).parse(raw);
    const input = { campaign_id: campaign, candidate_id: row.id, expected_revision: row.criteria_revision, profile_ref: row.source_profile_ref, requested_url: "" }, material = loadDiscoveryNativeContactMaterial(database, input);
    if (material === null) continue;
    const texts = nativeTexts(material), links: string[] = [], youtube = discoveryYoutubeProfileUrl(material), fields = discoveryProfileContactFields(database, material);
    if (youtube !== null) links.push(youtube);
    const observed = (text: string) => {
      for (const token of text.split(/\p{White_Space}+/u)) {
        const url = trimDiscoveryLink(token);
        if (url.startsWith("https://") && Buffer.byteLength(url) <= 2048 && !url.includes("...") && !url.includes("…") && !discoveryCommunityInvite(url) && !discoveryThirdPartyLink(url, text)) links.push(url);
      }
    };
    for (const text of [...texts, fields]) if (text != null) observed(text);
    for (const page of material.public_pages) if (page.status === "completed" && page.stop_code === null) {
      if (youtube === page.requested_url) observed(page.markdown_excerpt);
      if (discoveryAuthorObservesLink(database, material, page.requested_url) && page.document_url !== null) links.push(...discoveryContactPageLinks(page.document_url, page.markdown_excerpt));
    }
    links.push(...loadDiscoveryContactLookupLinks(database, row.id, row.criteria_revision).map(([link]) => link));
    links.sort((a, b) => discoveryContactPriority(a, youtube) - discoveryContactPriority(b, youtube));
    const seen = new Set<string>();
    for (const url of links) {
      const key = discoveryContactReadKey(url);
      if (seen.has(key)) continue; seen.add(key);
      if (discoveryNonContact(url) || material.public_pages.some(page => page.status !== "reading" && (discoveryContactReadKey(page.requested_url) === key || page.document_url !== null && discoveryContactReadKey(page.document_url) === key)) || discoveryContactPriority(url, youtube) >= 4 && texts.some(text => text != null && discoveryPromotional(url, text))) continue;
      const prior = database.prepare("SELECT status,call_number FROM discovery_public_page_reads WHERE candidate_id=? AND requested_url=?").get(row.id, url);
      const eligible = prior === undefined ? row.status === "completed" : prior.status === "reading" && row.status === "technical_stop" && row.active_query === `page:${row.id}:${z.number().int().min(0).max(4294967295).parse(prior.call_number)}`;
      if (!eligible) continue;
      input.requested_url = url;
      if (discoveryNativeLinkSource(database, input) !== null) return { ...input };
    }
  }
  return null;
}
export function observeDiscoveryPublicPageContacts(database: DatabaseSync, campaign: string, revision: number, raw: z.infer<typeof discoveryPublicPageRead>): void {
  snapshot(database); const page = discoveryPublicPageRead.parse(raw);
  if (page.status !== "completed" || page.stop_code !== null) return;
  const author = discoveryEligibleContactAuthor(database, campaign, revision, page.candidate_id); if (author === null) return;
  if (author !== page.platform_account_id) throw new Error("contact_page_author_mismatch");
  const source = page.document_url;
  if (source === null || discoveryNonContact(page.requested_url) || discoveryPromotional(page.requested_url, page.source_excerpt) || discoveryNonContact(source)) return;
  if (loadDiscoveryContactLookupLinks(database, page.candidate_id, revision).some(([link]) => link === page.requested_url)) {
    const material = loadDiscoveryNativeContactMaterial(database, { campaign_id: campaign, expected_revision: revision, candidate_id: page.candidate_id, profile_ref: page.profile_ref, requested_url: page.requested_url });
    if (material === null || !discoveryLookupPageMatchesMaterial(database, material, page)) return;
  }
  const nativeChannel = `https://www.youtube.com/channel/${author}`;
  const attribution = /^UC[A-Za-z0-9_-]{22}$/u.test(author) && page.requested_url === nativeChannel && source === nativeChannel ? "author_published" : "unconfirmed";
  storeDiscoveryContacts(database, campaign, revision, page.candidate_id, extractDiscoveryContacts(page.markdown_excerpt, source, author, page.captured_at, attribution));
}
export function observeDiscoveryHiddenEmail(database: DatabaseSync, campaign: string, revision: number, raw: z.infer<typeof discoveryPublicPageRead>): void {
  snapshot(database); const page = discoveryPublicPageRead.parse(raw);
  if (page.status !== "completed" || page.stop_code !== null) throw new Error("hidden_email_invalid_receipt");
  const author = discoveryEligibleContactAuthor(database, campaign, revision, page.candidate_id); if (author === null) return;
  const expected = `https://www.youtube.com/channel/${author}`;
  if (!author.startsWith("UC") || Buffer.byteLength(author) !== 24 || page.requested_url !== expected || page.document_url !== expected || author !== page.platform_account_id) throw new Error("hidden_email_author_mismatch");
  const status = page.markdown_excerpt.split("\n").findLast(line => line.startsWith("YouTube hidden email status: "));
  const excerpt = status ?? "YouTube native businessEmailLabel/businessEmailButton entry observed; address not disclosed";
  if (status) database.prepare("UPDATE discovery_contact_observations SET source_excerpt=?,captured_at=? WHERE candidate_id=? AND discovery_campaign_id=? AND criteria_revision=? AND native_author_id=? AND kind='hidden_email' AND source_url=? AND captured_at<=?").run(excerpt, page.captured_at, page.candidate_id, campaign, revision, author, expected, page.captured_at);
  storeDiscoveryContacts(database, campaign, revision, page.candidate_id, [{ kind: "hidden_email", original_value: "有隐藏邮箱，需手动查看", normalized_value: expected, source_url: expected, source_excerpt: excerpt, captured_at: page.captured_at, native_author_id: author, attribution: "author_published" }]);
}
export function discoveryManualWechatLabels(bio: string): { kind: "wechat" | "wechat_official"; value: string }[] {
  const patterns = [
    ["wechat", /(?:个人微信|商务微信|合作微信|我的微信|微信号)\p{White_Space}*[:：]\p{White_Space}*([A-Za-z][A-Za-z0-9_-]{5,19})(?:[^A-Za-z0-9_-]|$)/gu],
    ["wechat_official", /(?:我的公众号|微信公众号|公众号)(?:叫|是|名称)?\p{White_Space}*[:：]?\p{White_Space}*[「“【]([^」”】\r\n<>]{1,40})[」”】]/gu],
  ] as const;
  const result: { kind: "wechat" | "wechat_official"; value: string }[] = [];
  for (const [kind, expression] of patterns) for (const match of bio.matchAll(expression)) {
    const value = match[1]!.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
    if (!result.some(row => row.kind === kind && row.value === value)) result.push({ kind, value });
  }
  return result.slice(0, 10);
}
export interface DiscoveryManualContactCandidate {
  campaign_id: string; criteria_revision: number; id: string; platform: string; platform_account_id: string; source_profile_ref: string; evidence: unknown;
}
export function loadDiscoveryManualWechat(database: DatabaseSync, candidate: DiscoveryManualContactCandidate): { kind: "wechat" | "wechat_official"; value: string; source_url: string; native_author_id: string }[] {
  snapshot(database);
  const current = database.prepare("SELECT EXISTS(SELECT 1 FROM discovery_campaigns WHERE id=? AND current_revision=?) AS present").get(candidate.campaign_id, candidate.criteria_revision)!.present === 1;
  if (!current || discoveryEligibleContactAuthor(database, candidate.campaign_id, candidate.criteria_revision, candidate.id) !== candidate.platform_account_id) return [];
  const urls = object.parse(candidate.evidence).source_urls, source = Array.isArray(urls) && typeof urls[0] === "string" ? urls[0] : null;
  const row = database.prepare("SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND source_url=?").get(candidate.campaign_id, candidate.platform, candidate.source_profile_ref, source);
  if (!row) return [];
  const material = buildDiscoveryReviewMaterial(loadDiscoveryReviewBasis(database, candidate.campaign_id, candidate.criteria_revision, text.parse(row.id), true));
  if (material.native_author_id !== candidate.platform_account_id || material.candidate_id !== candidate.id || material.native_profile?.bio == null) return [];
  return discoveryManualWechatLabels(material.native_profile.bio).map(row => ({ ...row, source_url: material.native_profile!.profile_url, native_author_id: material.native_author_id }));
}
