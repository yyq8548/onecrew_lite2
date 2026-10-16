import { readFileSync } from "node:fs";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { discoveryLibraryBasis, discoverySearchHit, type DiscoveryLibraryBasis } from "./library-material.ts";
import { discoverySubscriberDisplay } from "./qualification.ts";
import { findDiscoveryReviewBasis } from "./review-basis.ts";
import { loadDiscoveryRecommendationContacts } from "./contact-repository.ts";

const SOURCE_ROWS = readFileSync(runtimeAssetPath("discovery/library-source.sql", new URL("./library-source.sql", import.meta.url)), "utf8");
const scopeSchema = z.array(z.strictObject({ platform: z.string(), profile_ref: z.string(), keywords: z.array(z.string()) }));
const parse = (value: string) => parseApplicationJson(Buffer.from(value));
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
function youtubeVideo(source: string): string | null {
  const url = URL.parse(source); if (!url || url.protocol !== "https:" || url.username || url.password || url.port || url.hash || !["youtube.com", "www.youtube.com"].includes(url.hostname)) return null;
  const parts = url.pathname.split("/"); let id: string | null = null;
  if (url.pathname === "/watch" && url.searchParams.getAll("v").length === 1) id = url.searchParams.get("v");
  else if (parts.length === 3 && (parts[1] === "watch" && !url.search || ["shorts", "live"].includes(parts[1]!))) id = parts[2]!;
  return id !== null && /^[A-Za-z0-9_-]{11}$/u.test(id) ? id : null;
}
function youtubeFollowers(database: DatabaseSync, campaign: string, revision: number, candidate: string, author: string, raw: unknown): string | null {
  const evidence = object(raw), source = evidence.subscribers_source_url, captured = evidence.captured_at;
  if (typeof source !== "string" || !(typeof captured === "bigint" && captured > 0n && captured <= 18446744073709551615n || typeof captured === "number" && Number.isSafeInteger(captured) && captured > 0) || !Array.isArray(evidence.source_urls) || !evidence.source_urls.includes(source)) return null;
  const video = youtubeVideo(source); if (video === null) return null;
  const bound = database.prepare(`SELECT h.source_url FROM discovery_video_identity_bindings b
    JOIN discovery_search_hits h ON h.id=b.search_hit_id JOIN discovery_video_completions done ON done.search_hit_id=h.id
    WHERE b.discovery_campaign_id=?1 AND b.criteria_revision=?2 AND b.candidate_id=?3 AND b.native_author_id=?4
      AND h.discovery_campaign_id=b.discovery_campaign_id`).all(campaign, revision, candidate, author).some(row => youtubeVideo(z.string().parse(row.source_url)) === video);
  return bound && typeof evidence.subscribers_raw === "string" ? discoverySubscriberDisplay(evidence.subscribers_raw) : null;
}

export function loadDiscoveryLibraryBasis(database: DatabaseSync, campaign: string, revision: number): DiscoveryLibraryBasis {
  if (!database.isTransaction) throw new Error("library_basis_requires_snapshot");
  if (!campaign || Buffer.byteLength(campaign) > 128 || !Number.isInteger(revision) || revision <= 0 || revision > 2147483647) throw new Error("library_source_input_invalid");
  const head = database.prepare(`SELECT r.criteria_json,r.scope_json,r.approved_at FROM discovery_campaigns c
    JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision
    WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND r.approved_at IS NOT NULL`).get(campaign, revision);
  if (!head) throw new Error("library_source_campaign_not_approved");
  const criteria = z.string().parse(head.criteria_json), scopes = scopeSchema.parse(parse(z.string().parse(head.scope_json)));
  for (const round of database.prepare("SELECT scopes_json FROM discovery_query_rounds WHERE discovery_campaign_id=?1 AND revision=?2 ORDER BY round").all(campaign, revision)) for (const addition of scopeSchema.parse(parse(z.string().parse(round.scopes_json)))) {
    const scope = scopes.find(scope => scope.platform === addition.platform && scope.profile_ref === addition.profile_ref);
    if (!scope) throw new Error("query_round_scope_corrupt"); scope.keywords.push(...addition.keywords);
  }
  const scopesJson = stringifyApplicationJson(scopes), rows: DiscoveryLibraryBasis["rows"] = [], contactCache = new Map<string, ReturnType<typeof loadDiscoveryRecommendationContacts>>();
  let bytes = Buffer.byteLength(criteria) + Buffer.byteLength(scopesJson);
  for (const row of database.prepare(SOURCE_ROWS).all(campaign, revision)) {
    const hit = discoverySearchHit.parse({ id: row.id, campaign_id: campaign, platform: row.platform, profile_ref: row.profile_ref, search_keyword: row.search_keyword, source_url: row.source_url, title: row.title, author_label: row.author_label, captured_at: row.captured_at, author_id: row.author_id, author_url: row.author_url, author_observed_at: row.author_observed_at });
    let candidate: DiscoveryLibraryBasis["rows"][number]["candidate"] = null;
    if (row.candidate_id !== null) {
      const id = z.string().parse(row.candidate_id), author = z.string().parse(row.native_author_id), native = z.string().parse(row.native_platform), evidenceJson = z.string().parse(row.evidence_json);
      let followers: string | null;
      if (native === "youtube") followers = youtubeFollowers(database, campaign, revision, id, author, parse(evidenceJson));
      else {
        const material = findDiscoveryReviewBasis(database, campaign, revision, hit.id, true);
        if (material !== null && (material.candidate_id !== id || material.native_author_id !== author)) throw new Error("library_source_author_mismatch");
        followers = material === null ? null : material.native_followers !== null ? String(material.native_followers) : material.native_follower_estimate !== null ? `~${material.native_follower_estimate.display}` : null;
      }
      const key = `${id}\0${author}`; let contacts = contactCache.get(key);
      if (!contacts) { contacts = loadDiscoveryRecommendationContacts(database, campaign, revision, id, author, hit.id); contactCache.set(key, contacts); }
      candidate = { id, native_platform: native !== hit.platform ? native : null, native_author_id: author, followers, verdict: z.string().parse(row.verdict), evidence_json: evidenceJson, identity_receipt: z.string().parse(row.identity_receipt), manual_approved_at: z.union([z.number().int().safe(), z.bigint()]).nullable().parse(row.manual_approved_at), contacts };
    }
    bytes += [hit.id, hit.campaign_id, hit.platform, hit.profile_ref, hit.source_url, hit.title, hit.author_label, hit.search_keyword, hit.author_id ?? "", hit.author_url ?? ""].reduce((sum, value) => sum + Buffer.byteLength(value), 0);
    if (candidate) { bytes += [candidate.id, candidate.native_platform ?? "", candidate.native_author_id, candidate.verdict, candidate.evidence_json, candidate.identity_receipt].reduce((sum, value) => sum + Buffer.byteLength(value), 0); if (candidate.contacts.length) bytes += Buffer.byteLength(stringifyApplicationJson(candidate.contacts)); }
    if (rows.length === 10000 || bytes > 33554432) throw new Error("library_source_snapshot_too_large"); rows.push({ hit, candidate });
  }
  const policy = database.prepare("SELECT p.policy='score_and_contact_v1' AND COALESCE(json_extract(r.criteria_json,'$.contact_preference'),'')!='public_email_first' AS contact_required FROM discovery_recommendation_policies p JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=p.discovery_campaign_id AND r.revision=p.criteria_revision WHERE p.discovery_campaign_id=?1 AND p.criteria_revision=?2").get(campaign, revision);
  if (!policy) throw new Error("library_source_policy_missing");
  return discoveryLibraryBasis.parse({ campaign_id: campaign, revision, criteria_json: criteria, scopes_json: scopesJson, approved_at: head.approved_at, contact_required: z.union([z.literal(0), z.literal(1)]).parse(policy.contact_required) === 1, rows });
}
