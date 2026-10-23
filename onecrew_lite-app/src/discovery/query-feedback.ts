import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";

const feedbackRow = z.object({ platform: z.string(), profile_ref: z.string(), query: z.string(), author_id: z.string(), outcome: z.enum(["supported", "contradicted", "insufficient"]), verdict: z.string(), reason: z.string(), title: z.string(), reviewed_at: z.number() });
export interface DiscoveryQueryFeedback {
  platform: string; profile_ref: string; query: string; reviewed_authors: number;
  matched_authors: number; contradicted_authors: number; insufficient_authors: number;
  latest_review_at: number; defer_unread: boolean;
  examples: { outcome: string; verdict: string; reason: string; title: string }[];
}
const queryKey = (platform: string, profile: string, query: string) => JSON.stringify([platform, profile, query]);

// 只统计当前条件下每个查询、作者的最后一次有效评估；质量档位不参与查询命中率。
export function readDiscoveryQueryFeedback(db: DatabaseSync, campaign: string): DiscoveryQueryFeedback[] {
  const rows = db.prepare(`WITH reviews AS (
    SELECT h.platform,h.profile_ref,h.search_keyword AS query,r.native_author_id AS author_id,r.verdict,r.findings_json,h.title,r.created_at AS reviewed_at,
      ROW_NUMBER() OVER(PARTITION BY h.platform,h.profile_ref,h.search_keyword,r.native_author_id ORDER BY r.created_at DESC,r.rowid DESC) AS position
    FROM discovery_evidence_reviews r JOIN discovery_campaigns c ON c.id=r.discovery_campaign_id AND c.current_revision=r.criteria_revision
    JOIN discovery_search_hits h ON h.id=r.search_hit_id AND h.discovery_campaign_id=c.id
    WHERE c.id=? AND r.policy_version='selection-quality-evaluation-v2'
  ) SELECT platform,profile_ref,query,author_id,verdict,title,reviewed_at,
      json_extract(f.value,'$.outcome') AS outcome,json_extract(f.value,'$.rationale') AS reason
    FROM reviews,json_each(reviews.findings_json) f WHERE position=1 AND json_extract(f.value,'$.criterion')='selection_fit'
    ORDER BY reviewed_at DESC,platform,query,author_id`).all(campaign);
  const groups = new Map<string, DiscoveryQueryFeedback>();
  for (const raw of rows) {
    const row = feedbackRow.parse(raw), key = queryKey(row.platform, row.profile_ref, row.query);
    let group = groups.get(key);
    if (!group) {
      group = { platform: row.platform, profile_ref: row.profile_ref, query: row.query, reviewed_authors: 0, matched_authors: 0, contradicted_authors: 0, insufficient_authors: 0, latest_review_at: row.reviewed_at, defer_unread: false, examples: [] };
      groups.set(key, group);
    }
    group.reviewed_authors++;
    if (row.outcome === "supported") group.matched_authors++;
    else if (row.outcome === "contradicted") group.contradicted_authors++;
    else group.insufficient_authors++;
    if (group.examples.length < 5) group.examples.push({ outcome: row.outcome, verdict: row.verdict, reason: [...row.reason].slice(0, 500).join(""), title: [...row.title].slice(0, 250).join("") });
    // 缺少证据不代表检索不匹配；只用已有明确结论的作者判断查询收益。
    const decided = group.matched_authors + group.contradicted_authors;
    group.defer_unread = decided >= 5 && group.matched_authors * 5 <= decided;
  }
  return [...groups.values()];
}

export function discoveryLowYieldUnreadHits(db: DatabaseSync, campaign: string): string[] {
  const lowYield = new Set(readDiscoveryQueryFeedback(db, campaign).filter(item => item.defer_unread).map(item => queryKey(item.platform, item.profile_ref, item.query)));
  if (lowYield.size === 0) return [];
  // 已经开始的核实、已有候选和联系补充继续处理，只有未开始的命中调整优先级。
  const rows = db.prepare(`SELECT h.id,h.platform,h.profile_ref,h.search_keyword FROM discovery_search_hits h
    LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id
    WHERE h.discovery_campaign_id=?
      AND NOT EXISTS(SELECT 1 FROM discovery_candidates c WHERE c.discovery_campaign_id=h.discovery_campaign_id
        AND c.criteria_revision=(SELECT current_revision FROM discovery_campaigns WHERE id=h.discovery_campaign_id)
        AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref
        AND (c.platform_account_id=a.author_id OR EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.source_urls') s WHERE s.value=h.source_url)))
      AND NOT EXISTS(SELECT 1 FROM discovery_profile_reads p WHERE p.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_identity_reads p WHERE p.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads p WHERE p.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_douyin_author_reads p WHERE p.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_video_completions p WHERE p.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_video_read_failures p WHERE p.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_content_reads p WHERE p.search_hit_id=h.id)
    ORDER BY h.id`).all(campaign);
  const hit = z.object({ id: z.string(), platform: z.string(), profile_ref: z.string(), search_keyword: z.string() });
  return rows.map(row => hit.parse(row)).filter(row => lowYield.has(queryKey(row.platform, row.profile_ref, row.search_keyword))).map(row => row.id);
}
