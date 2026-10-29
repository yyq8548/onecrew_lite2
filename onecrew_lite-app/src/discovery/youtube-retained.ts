import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";

const reference = z.strictObject({ search_hit_id: z.string().min(1), call_number: z.number().int().positive(), fresh_read_attempted: z.boolean() });
export const retainedYoutubeFields = ["source_urls", "subscribers_raw", "subscribers_source_url", "captured_at", "video_title", "video_description", "video_observations", "public_links", "public_link_observations"] as const;

// 跨视频复用只接受同一浏览器、原生作者和已完成身份回执，原视频正文及时间保持原样。
export function loadRetainedYoutubeEvidence(database: DatabaseSync, evidence: Record<string, unknown>, author: string, profile: string) {
  if (evidence.retained_youtube === undefined) return null;
  const retained = reference.parse(evidence.retained_youtube);
  const row = database.prepare(`SELECT h.source_url,c.evidence_json FROM discovery_search_hits h
    JOIN discovery_video_completions v ON v.search_hit_id=h.id
    JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=h.discovery_campaign_id
    JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=h.discovery_campaign_id AND c.criteria_revision=b.criteria_revision
      AND c.platform='youtube' AND c.platform_account_id=b.native_author_id AND c.source_profile_ref=h.profile_ref
    WHERE h.id=? AND h.platform='youtube' AND h.profile_ref=? AND b.native_author_id=?
      AND json_extract(c.evidence_json,'$.source_urls[0]')=h.source_url`).get(retained.search_hit_id, profile, author);
  if (!row) throw new Error("discovery_retained_youtube_identity_unavailable");
  const saved = z.record(z.string(), z.unknown()).parse(parseApplicationJson(z.string().parse(row.evidence_json)));
  for (const key of retainedYoutubeFields.filter(key => key !== "source_urls")) if (!isDeepStrictEqual(evidence[key], saved[key])) throw new Error("discovery_retained_youtube_material_changed");
  if (!Array.isArray(evidence.source_urls) || evidence.source_urls[0] !== row.source_url) throw new Error("discovery_retained_youtube_source_changed");
  return { ...retained, source_url: z.string().parse(row.source_url) };
}
