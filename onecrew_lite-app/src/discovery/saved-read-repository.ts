import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { discoveryPublicPageRead, discoveryTranscriptRead } from "./review-material.ts";
import { loadDiscoveryContentReceipt } from "./review-receipts.ts";

function row(database: DatabaseSync, sql: string, parameters: readonly string[]): Record<string, unknown> {
  const statement = database.prepare(sql); statement.setReadBigInts(true);
  const result = statement.get(...parameters); if (!result) throw new Error("discovery_saved_read_not_found");
  return Object.fromEntries(Object.entries(result).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value]));
}
export function getDiscoverySavedTranscript(database: DatabaseSync, campaign: string, hit: string) {
  const saved = row(database, `SELECT t.search_hit_id,t.candidate_id,h.source_url,h.profile_ref,c.platform_account_id,t.status,t.segments_json,t.truncated,t.stop_code,t.call_number,t.updated_at AS captured_at
    FROM discovery_transcript_reads t JOIN discovery_search_hits h ON h.id=t.search_hit_id JOIN discovery_candidates c ON c.id=t.candidate_id
    WHERE h.discovery_campaign_id=?1 AND h.id=?2 AND c.discovery_campaign_id=?1`, [campaign, hit]);
  return discoveryTranscriptRead.parse({ ...saved, segments: parseApplicationJson(Buffer.from(z.string().parse(saved.segments_json))), truncated: z.union([z.literal(0), z.literal(1)]).parse(saved.truncated) === 1 });
}
export function getDiscoverySavedPublicPage(database: DatabaseSync, campaign: string, candidate: string, url: string) {
  const saved = row(database, `SELECT p.candidate_id,p.requested_url,p.search_hit_id,h.source_url,p.source_excerpt,h.profile_ref,c.platform_account_id,
    p.call_number,p.status,p.document_url,p.title,p.markdown_excerpt,p.truncated,p.stop_code,p.updated_at AS captured_at
    FROM discovery_public_page_reads p JOIN discovery_candidates c ON c.id=p.candidate_id JOIN discovery_search_hits h ON h.id=p.search_hit_id
    WHERE p.candidate_id=?1 AND p.requested_url=?2 AND c.discovery_campaign_id=?3 AND h.discovery_campaign_id=?3`, [candidate, url, campaign]);
  return discoveryPublicPageRead.parse({ ...saved, truncated: z.union([z.literal(0), z.literal(1)]).parse(saved.truncated) === 1 });
}
export function getDiscoverySavedContent(database: DatabaseSync, campaign: string, hit: string) {
  database.exec("BEGIN"); let committed = false;
  try { const value = loadDiscoveryContentReceipt(database, campaign, hit); database.exec("COMMIT"); committed = true; return value; }
  finally { if (!committed) database.exec("ROLLBACK"); }
}
export function getDiscoverySavedIdentity(database: DatabaseSync, campaign: string, hit: string) {
  const saved = row(database, `SELECT i.expected_revision,i.profile_ref,i.author_handle,i.source_url,i.post_id,i.call_number,i.status,i.account_id,i.observed_source_url,i.stop_code,i.updated_at AS captured_at
    FROM discovery_identity_reads i JOIN discovery_search_hits h ON h.id=i.search_hit_id JOIN discovery_search_authors a ON a.search_hit_id=h.id
    WHERE i.discovery_campaign_id=?1 AND i.search_hit_id=?2 AND h.discovery_campaign_id=?1 AND h.platform=i.platform AND h.profile_ref=i.profile_ref AND h.source_url=i.source_url AND a.author_id=i.author_handle`, [campaign, hit]);
  const text = z.string(), u32 = z.number().int().min(0).max(4294967295);
  return z.object({ claim: z.object({ campaign_id: text, expected_revision: u32, search_hit_id: text, profile_ref: text, author_handle: text, source_url: text, post_id: text, call_number: u32 }), status: text, account_id: text.nullable(), observed_source_url: text.nullable(), stop_code: text.nullable(), captured_at: z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]) }).parse({ claim: { campaign_id: campaign, search_hit_id: hit, expected_revision: saved.expected_revision, profile_ref: saved.profile_ref, author_handle: saved.author_handle, source_url: saved.source_url, post_id: saved.post_id, call_number: saved.call_number }, status: saved.status, account_id: saved.account_id, observed_source_url: saved.observed_source_url, stop_code: saved.stop_code, captured_at: saved.captured_at });
}
