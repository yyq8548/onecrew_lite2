import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { discoveryContact, discoveryScorePasses } from "./library-material.ts";
import { discoveryContactPlatform, discoveryNonContact, discoverySocialProfile, extractDiscoveryContacts, type DiscoveryContact } from "./contacts.ts";
import { loadDiscoveryReviewBasis } from "./review-basis.ts";
import { buildDiscoveryReviewMaterial } from "./review-material.ts";
import { stripUnauditedDiscoveryProofs } from "./review-proof-audit.ts";
import { discoveryProfileContactFields } from "./contact-profile.ts";
import { discoveryObservesLink, discoveryPromotional, discoveryThirdPartyLink } from "./contact-navigation.ts";
import type { PreparedDiscoveryReview } from "./review-submission.ts";
import { discoveryPublicPageRead, type DiscoveryReviewMaterial } from "./review-material.ts";

const contactObject = z.record(z.string(), z.unknown());
type PublicPage = z.infer<typeof discoveryPublicPageRead>;
const nativeSocialPlatforms = ["twitter", "instagram", "tiktok", "douyin", "linkedin", "facebook", "youtube", "xiaohongshu"];
export function loadDiscoveryContactLookupLinks(database: DatabaseSync, candidate: string, revision: number): [string, string][] {
  if (!database.isTransaction) throw new Error("contact_projection_requires_snapshot");
  const links: [string, string][] = [];
  for (const row of database.prepare("SELECT receipt_json FROM discovery_contact_lookups WHERE candidate_id=? AND criteria_revision=? ORDER BY method").all(candidate, revision)) {
    const receipt = z.object({ campaign_id: z.string(), expected_revision: z.number().int().min(0).max(4294967295), candidate_id: z.string(), profile_ref: z.string(), method: z.string(), native_author_id: z.string(), author_label: z.string(), username: z.string().nullable(), query: z.string(), call_number: z.number().int().min(0).max(4294967295), status: z.string(), links: z.array(z.string()), stop_code: z.string().nullable() }).parse(parseApplicationJson(z.string().parse(row.receipt_json)));
    if (receipt.status === "completed" && receipt.stop_code === null) for (const link of receipt.links) links.push([link, receipt.query]);
  }
  return links;
}
export function discoveryYoutubeProfileUrl(material: DiscoveryReviewMaterial): string | null {
  return (material.native_platform ?? material.hit.platform) === "youtube" && /^UC[A-Za-z0-9_-]{8,62}$/u.test(material.native_author_id) ? `https://www.youtube.com/channel/${material.native_author_id}` : null;
}
export function discoveryLookupPageMatchesMaterial(database: DatabaseSync, material: DiscoveryReviewMaterial, page: PublicPage): boolean {
  if (discoveryNonContact(page.requested_url) || discoveryPromotional(page.requested_url, page.source_excerpt) || page.document_url !== null && discoveryNonContact(page.document_url)) return false;
  if (!loadDiscoveryContactLookupLinks(database, material.candidate_id, material.criteria_revision).some(([link]) => link === page.requested_url)) return true;
  const bound = [material.hit.author_url, material.native_profile?.profile_url, discoveryYoutubeProfileUrl(material)];
  return bound.some(url => url != null && discoveryObservesLink(page.markdown_excerpt, url));
}
export function storeDiscoveryContacts(database: DatabaseSync, campaign: string, revision: number, candidate: string, contacts: readonly DiscoveryContact[]): void {
  if (!database.isTransaction) throw new Error("contact_store_requires_transaction");
  const row = database.prepare("SELECT platform FROM discovery_candidates WHERE id=? AND discovery_campaign_id=? AND criteria_revision=?").get(candidate, campaign, revision);
  if (!row) throw new Error("contact_candidate_unavailable");
  const platform = z.string().parse(row.platform), insert = database.prepare(`INSERT INTO discovery_contact_observations
    (candidate_id,discovery_campaign_id,criteria_revision,native_author_id,kind,original_value,normalized_value,source_url,source_excerpt,captured_at,attribution)
    VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(candidate_id,criteria_revision,normalized_value,source_url) DO NOTHING`);
  for (const raw of contacts.filter(contact => contact.kind !== "social_profile" || nativeSocialPlatforms.includes(platform) || discoveryContactPlatform(contact.normalized_value) !== platform).slice(0, 100)) {
    const contact = discoveryContact.parse(raw);
    insert.run(candidate, campaign, revision, contact.native_author_id, contact.kind, contact.original_value, contact.normalized_value, contact.source_url, contact.source_excerpt, contact.captured_at, contact.attribution);
  }
}
export function observeDiscoveryReviewContacts(database: DatabaseSync, prepared: PreparedDiscoveryReview): void {
  if (!database.isTransaction) throw new Error("contact_store_requires_transaction");
  const evidence = parseApplicationJson(prepared.evidence_json);
  if (!discoveryScorePasses(prepared.receipt.verdict, evidence, prepared.receipt.native_author_id)) return;
  const material = buildDiscoveryReviewMaterial(prepared.basis), contacts: DiscoveryContact[] = [];
  const add = (text: string, source: string, captured: number | bigint, attribution: DiscoveryContact["attribution"]) => contacts.push(...extractDiscoveryContacts(text, source, material.native_author_id, captured, attribution, material.hit.author_label ?? ""));
  const fields = discoveryProfileContactFields(database, material), profile = material.native_profile;
  if (profile) add(fields, profile.profile_url, profile.observed_at, "author_published");
  else if (material.native_platform === "youtube") {
    if (material.hit.author_observed_at === null) throw new Error("contact_native_source_time_unavailable");
    add(fields, material.hit.source_url, material.hit.author_observed_at, "author_published");
  }
  if (material.video_description !== null) {
    const captured = contactObject.parse(parseApplicationJson(prepared.basis.evidence_json)).captured_at;
    if (typeof captured === "number" && Number.isSafeInteger(captured) || typeof captured === "bigint" && captured >= -9223372036854775808n && captured <= 9223372036854775807n) add(material.video_description, material.hit.source_url, captured, "author_published");
  }
  if (profile) for (const text of [profile.bio, profile.self_declared_location]) if (text != null) add(text, profile.profile_url, profile.observed_at, "author_published");
  const read = material.native_content;
  if (read?.native_content != null) {
    const content = contactObject.parse(read.native_content), text = content[read.claim.platform === "bilibili" ? "description" : "text"];
    if (typeof text === "string") {
      const source = read.status !== "completed" || read.stop_code !== null ? null : ["instagram", "linkedin"].includes(read.claim.platform) ? content.source_url : read.claim.source_url;
      if (typeof source !== "string") throw new Error("contact_native_source_unavailable");
      add(text, source, read.captured_at, "author_published");
    }
  }
  for (const page of material.public_pages) if (page.status === "completed" && page.stop_code === null && discoveryLookupPageMatchesMaterial(database, material, page) && page.document_url !== null) add(page.markdown_excerpt, page.document_url, page.captured_at, "unconfirmed");
  const platform = material.native_platform ?? material.hit.platform;
  if (nativeSocialPlatforms.includes(platform)) {
    const url = profile?.profile_url ?? material.hit.author_url ?? discoveryYoutubeProfileUrl(material), captured = profile?.observed_at ?? material.hit.author_observed_at ?? (platform === "youtube" ? material.hit.captured_at : null);
    if (url !== null && captured !== null && discoverySocialProfile(url) && discoveryContactPlatform(url) === platform) contacts.push({ kind: "social_profile", original_value: url, normalized_value: url, source_url: url, source_excerpt: "Verified native author account", captured_at: captured, native_author_id: material.native_author_id, attribution: "author_published" });
  }
  storeDiscoveryContacts(database, material.campaign_id, material.criteria_revision, material.candidate_id, contacts);
}

export function loadDiscoveryContactObservations(database: DatabaseSync, campaign: string, revision: number, candidate: string, author: string, socialOnly = false): DiscoveryContact[] {
  if (!database.isTransaction) throw new Error("contact_projection_requires_snapshot");
  const statement = database.prepare(`SELECT kind,original_value,normalized_value,source_url,source_excerpt,captured_at,attribution
    FROM discovery_contact_observations WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3 AND native_author_id=?4
      AND ((?5=1 AND kind='social_profile' AND attribution='author_published') OR (?5=0 AND (kind IN ('email','telegram') OR (kind IN ('hidden_email','social_profile','whatsapp','wechat','wechat_official','website','messenger','phone') AND attribution='author_published'))))
    ORDER BY normalized_value,source_url LIMIT 100`);
  statement.setReadBigInts(true);
  const contacts = statement.all(campaign, revision, candidate, author, Number(socialOnly)).map(row => discoveryContact.parse({ ...row, captured_at: typeof row.captured_at === "bigint" && row.captured_at >= BigInt(Number.MIN_SAFE_INTEGER) && row.captured_at <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(row.captured_at) : row.captured_at, native_author_id: author, attribution: row.attribution === "author_published" ? "author_published" : "unconfirmed" })).filter(contact => (contact.attribution === "author_published" || !discoveryNonContact(contact.source_url))
    && (contact.kind !== "email" || extractDiscoveryContacts(contact.source_excerpt, contact.source_url, author, contact.captured_at, contact.attribution).some(current => current.kind === "email" && current.normalized_value === contact.normalized_value)));
  if (!contacts.length) return contacts;
  const row = database.prepare("SELECT display_name,profile_url,evidence_json FROM discovery_candidates WHERE id=? AND discovery_campaign_id=? AND criteria_revision=? AND platform_account_id=?").get(candidate, campaign, revision, author)!;
  const owner = z.string().parse(row.display_name), evidence = contactObject.parse(parseApplicationJson(z.string().parse(row.evidence_json))), texts = new Map<string, string[]>();
  const add = (source: string, value: unknown) => { if (typeof value === "string") texts.set(source, [...texts.get(source) ?? [], value]); };
  if (Array.isArray(evidence.source_urls) && typeof evidence.source_urls[0] === "string") add(evidence.source_urls[0], evidence.video_description);
  for (const profile of database.prepare("SELECT native_profile_json FROM discovery_profile_reads WHERE discovery_campaign_id=? AND expected_revision=? AND author_id=? AND status='completed' AND stop_code IS NULL").all(campaign, revision, author)) {
    const native = contactObject.parse(parseApplicationJson(z.string().parse(profile.native_profile_json)));
    for (const key of ["bio", "biography", "description", "sign"]) add(z.string().parse(row.profile_url), native[key]);
  }
  for (const page of database.prepare("SELECT requested_url,document_url,markdown_excerpt FROM discovery_public_page_reads WHERE candidate_id=? AND status='completed' AND stop_code IS NULL").all(candidate)) {
    add(z.string().parse(page.requested_url), page.markdown_excerpt); if (typeof page.document_url === "string") add(page.document_url, page.markdown_excerpt);
  }
  return contacts.filter(contact => !(texts.get(contact.source_url) ?? [contact.source_excerpt]).some(text => discoveryThirdPartyLink(contact.original_value, text, owner) || discoveryPromotional(contact.original_value, text)));
}

export function loadDiscoveryRecommendationContacts(database: DatabaseSync, campaign: string, revision: number, candidate: string, author: string, hit: string): DiscoveryContact[] {
  const routes = loadDiscoveryContactObservations(database, campaign, revision, candidate, author);
  const eligible = database.prepare(`SELECT c.platform_account_id,c.verdict,c.evidence_json FROM discovery_candidates c
    JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.discovery_campaign_id AND p.criteria_revision=c.criteria_revision
    WHERE c.id=?1 AND c.discovery_campaign_id=?2 AND c.criteria_revision=?3 AND p.policy='score_and_contact_v1' AND c.verdict='qualified'`).get(candidate, campaign, revision);
  if (!eligible) return routes;
  const savedAuthor = z.string().parse(eligible.platform_account_id), evidence = stripUnauditedDiscoveryProofs(database, parseApplicationJson(Buffer.from(z.string().parse(eligible.evidence_json))), candidate, campaign, revision, savedAuthor);
  if (savedAuthor !== author || !discoveryScorePasses(z.string().parse(eligible.verdict), evidence, author)) return routes;
  const material = buildDiscoveryReviewMaterial(loadDiscoveryReviewBasis(database, campaign, revision, hit, true));
  if (material.candidate_id !== candidate || material.native_author_id !== author) throw new Error("contact_reprojection_author_mismatch");
  const profile = material.native_profile;
  if (profile) for (const text of [profile.bio, profile.self_declared_location]) if (text != null) routes.push(...extractDiscoveryContacts(text, profile.profile_url, author, profile.observed_at, "author_published"));
  const platform = material.native_platform ?? material.hit.platform;
  if (nativeSocialPlatforms.includes(platform)) {
    const url = profile?.profile_url ?? material.hit.author_url ?? (platform === "youtube" ? `https://www.youtube.com/channel/${author}` : null);
    const captured = profile?.observed_at ?? material.hit.author_observed_at ?? (platform === "youtube" ? material.hit.captured_at : null);
    if (url !== null && captured !== null && discoverySocialProfile(url) && discoveryContactPlatform(url) === platform) routes.push({ kind: "social_profile", original_value: url, normalized_value: url, source_url: url, source_excerpt: "Verified native author account", captured_at: captured, native_author_id: author, attribution: "author_published" });
  }
  const seen = new Set<string>();
  return routes.filter(route => { const key = `${route.kind}\0${route.normalized_value}`; if (seen.has(key)) return false; seen.add(key); return true; });
}
