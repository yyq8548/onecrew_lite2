import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { discoveryContentRead, discoveryLanguageSample, discoveryPublicPageRead, discoveryTranscriptRead, type DiscoveryReviewBasis } from "./review-material.ts";

export function loadDiscoveryProfileReceipt(database: DatabaseSync, campaign: string, hit: string) {
  if (!database.isTransaction) throw new Error("review_receipts_require_snapshot");
  const row = database.prepare(`SELECT p.expected_revision,p.platform,p.profile_ref,p.author_id,a.author_url,h.source_url,
    p.call_number,p.status,p.native_profile_json,p.stop_code,p.updated_at
    FROM discovery_profile_reads p JOIN discovery_search_hits h ON h.id=p.search_hit_id
    JOIN discovery_search_authors a ON a.search_hit_id=h.id
    WHERE p.discovery_campaign_id=?1 AND p.search_hit_id=?2 AND h.discovery_campaign_id=?1
      AND h.platform=p.platform AND h.profile_ref=p.profile_ref AND a.author_id=p.author_id`).get(campaign, hit);
  if (!row) throw new Error("discovery_profile_read_not_found");
  return z.object({ claim: z.object({ campaign_id: z.string(), search_hit_id: z.string(), expected_revision: z.number().int().min(0).max(4294967295), platform: z.string(), profile_ref: z.string(), author_id: z.string(), author_url: z.string(), source_url: z.string(), call_number: z.number().int().min(0).max(4294967295) }), status: z.string(), native_profile: z.unknown(), stop_code: z.string().nullable(), captured_at: z.number().int().safe() }).parse({ claim: { campaign_id: campaign, search_hit_id: hit, expected_revision: row.expected_revision, platform: row.platform, profile_ref: row.profile_ref, author_id: row.author_id, author_url: row.author_url, source_url: row.source_url, call_number: row.call_number }, status: row.status, native_profile: row.native_profile_json === null ? null : parseApplicationJson(Buffer.from(z.string().parse(row.native_profile_json))), stop_code: row.stop_code, captured_at: row.updated_at });
}

export function loadDiscoveryContentReceipt(database: DatabaseSync, campaign: string, hit: string) {
  if (!database.isTransaction) throw new Error("review_receipts_require_snapshot");
  const statement = database.prepare(`SELECT content.expected_revision,content.platform,content.profile_ref,content.author_id,
    content.source_url,content.call_number,content.status,content.native_content_json,content.stop_code,content.updated_at
    FROM discovery_content_reads content
    JOIN discovery_search_hits h ON h.id=content.search_hit_id
    JOIN discovery_search_authors a ON a.search_hit_id=h.id
    WHERE content.discovery_campaign_id=?1 AND content.search_hit_id=?2
      AND h.discovery_campaign_id=?1 AND h.platform=content.platform AND h.profile_ref=content.profile_ref
      AND (h.source_url=content.source_url OR (content.platform='facebook' AND EXISTS (
        SELECT 1 FROM discovery_facebook_content_targets t
        WHERE t.discovery_campaign_id=content.discovery_campaign_id AND t.search_hit_id=h.id
          AND t.expected_revision=content.expected_revision AND t.profile_ref=content.profile_ref
          AND t.author_id=content.author_id AND t.profile_url=a.author_url
          AND t.status='completed' AND t.stop_code IS NULL AND t.call_number<content.call_number
          AND EXISTS(SELECT 1 FROM json_each(t.source_urls_json) l WHERE l.value=content.source_url))))
      AND a.author_id=content.author_id`);
  statement.setReadBigInts(true);
  const saved = statement.get(campaign, hit);
  const row = saved === undefined ? undefined : Object.fromEntries(Object.entries(saved).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value]));
  if (!row) throw new Error("discovery_content_read_not_found");
  return discoveryContentRead.parse({ claim: { campaign_id: campaign, search_hit_id: hit, expected_revision: row.expected_revision, platform: row.platform, profile_ref: row.profile_ref, author_id: row.author_id, source_url: row.source_url, call_number: row.call_number }, status: row.status, native_content: row.native_content_json === null ? null : parseApplicationJson(Buffer.from(z.string().parse(row.native_content_json))), stop_code: row.stop_code, captured_at: row.updated_at });
}

export function decodeDiscoveryLanguageSample(json: string, format: number) {
  const value: unknown = parseApplicationJson(Buffer.from(json));
  if (format === 1) {
    const legacy = z.object({ comments: z.array(z.record(z.string(), z.unknown())) }).parse(value);
    for (const comment of legacy.comments) {
      if (typeof comment.author_id !== "string") throw new Error("language_sample_invalid");
      delete comment.author_id;
    }
    return discoveryLanguageSample.parse({ ...z.record(z.string(), z.unknown()).parse(value), comments: legacy.comments });
  }
  if (format !== 2) throw new Error("language_sample_format_unsupported");
  return discoveryLanguageSample.parse(value);
}

export function loadDiscoveryLanguageSamples(database: DatabaseSync, campaign: string, revision: number, candidate: string) {
  if (!database.isTransaction) throw new Error("review_receipts_require_snapshot");
  return database.prepare("SELECT sample_json,sample_format_version FROM discovery_language_samples WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3 AND status='completed' ORDER BY ordinal").all(campaign, revision, candidate).map(row => decodeDiscoveryLanguageSample(z.string().parse(row.sample_json), z.number().int().nonnegative().parse(row.sample_format_version)));
}

// 必须在读取候选、审核记录和原生身份的同一事务快照中调用。
export function loadDiscoveryReviewReceipts(database: DatabaseSync, basis: DiscoveryReviewBasis, bytes: number, contactNavigation: boolean): Pick<DiscoveryReviewBasis, "transcript" | "pages"> {
  if (!database.isTransaction) throw new Error("review_receipts_require_snapshot");
  if (!Number.isSafeInteger(bytes) || bytes < 0 || bytes > 1048576) throw new Error("review_material_too_large");
  const shared = { search_hit_id: basis.hit.id, candidate_id: basis.candidate_id, source_url: basis.hit.source_url, profile_ref: basis.hit.profile_ref, platform_account_id: basis.native_author_id };
  const transcriptRow = database.prepare("SELECT status,segments_json,truncated,stop_code,call_number,updated_at FROM discovery_transcript_reads WHERE search_hit_id=?1 AND candidate_id=?2").get(basis.hit.id, basis.candidate_id);
  let transcript: DiscoveryReviewBasis["transcript"] = null;
  if (transcriptRow) {
    const json = z.string().parse(transcriptRow.segments_json);
    transcript = [discoveryTranscriptRead.parse({ ...shared, status: transcriptRow.status, segments: [], truncated: z.union([z.literal(0), z.literal(1)]).parse(transcriptRow.truncated) === 1, stop_code: transcriptRow.stop_code, call_number: transcriptRow.call_number, captured_at: transcriptRow.updated_at }), json];
    bytes += Buffer.byteLength(json);
  }
  const pages: DiscoveryReviewBasis["pages"] = [];
  const rows = database.prepare(`SELECT requested_url,source_excerpt,call_number,status,document_url,title,markdown_excerpt,truncated,stop_code,updated_at
    FROM discovery_public_page_reads WHERE search_hit_id=?1 AND candidate_id=?2
    ORDER BY CASE WHEN ?3 AND requested_url LIKE 'https://www.youtube.com/channel/%' THEN 0 ELSE 1 END,
      CASE WHEN ?3 THEN -call_number ELSE 0 END,requested_url LIMIT 21`).all(basis.hit.id, basis.candidate_id, Number(contactNavigation));
  for (const row of rows) {
    const page = discoveryPublicPageRead.parse({ ...shared, requested_url: row.requested_url, source_excerpt: row.source_excerpt, call_number: row.call_number, status: row.status, document_url: row.document_url, title: row.title, markdown_excerpt: row.markdown_excerpt, truncated: z.union([z.literal(0), z.literal(1)]).parse(row.truncated) === 1, stop_code: row.stop_code, captured_at: row.updated_at });
    const pageBytes = [page.requested_url, page.source_excerpt, page.title, page.markdown_excerpt, page.document_url ?? ""].reduce((sum, value) => sum + Buffer.byteLength(value), 0);
    if (pages.length === 20 || bytes + pageBytes > 1048576) {
      if (contactNavigation) continue;
      throw new Error("review_material_too_large");
    }
    bytes += pageBytes; pages.push(page);
  }
  return { transcript, pages };
}
