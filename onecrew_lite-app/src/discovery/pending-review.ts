import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { discoveryJsonValue, discoveryMaterialDigest } from "./library-material.ts";
import { findDiscoveryReviewBasis } from "./review-basis.ts";
import { buildDiscoveryReviewMaterial, type DiscoveryReviewBasis } from "./review-material.ts";
import { DiscoveryLinkedinExperienceRepository } from "./linkedin-experience.ts";

const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
// 评分回执本身会改变候选证据哈希；只比较参与判断的实际观察，避免把所有回执误报为过期。
export function discoveryObservationVersion(raw: unknown): string {
  const value = { ...object(raw) };
  delete value.material_hash; delete value.candidate_evidence_hash;
  return discoveryMaterialDigest(discoveryJsonValue(value));
}

export const pendingReviewLabels = { awaiting_score: "等待最新证据评分", needs_content: "等待补充必需证据", needs_identity: "缺少账号身份依据", read_blocked: "读取受阻，本轮已停止", insufficient_after_read: "证据不足，核实结束", needs_requirement: "等待补充必需联系方式" } as const;
export type PendingReviewState = keyof typeof pendingReviewLabels;
export function pendingReview(db: DatabaseSync, campaign: string, revision: number, candidate: string, verified?: { readonly hit: string; readonly basis: DiscoveryReviewBasis | null }): { state: PendingReviewState; next_action: string; last_processed_at: number; stop_code: string | null; gap: string | null } | null {
  const row = db.prepare(`SELECT c.updated_at,c.evidence_json,h.id AS hit_id,h.platform,h.profile_ref FROM discovery_candidates c
    LEFT JOIN discovery_search_hits h ON h.discovery_campaign_id=c.discovery_campaign_id AND h.profile_ref=c.source_profile_ref
      AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')
    WHERE c.id=?1 AND c.discovery_campaign_id=?2 AND c.criteria_revision=?3 AND c.verdict='pending'`).get(candidate, campaign, revision);
  if (!row) return null;
  const evidence = object(parseApplicationJson(Buffer.from(z.string().parse(row.evidence_json)))), review = object(evidence.reviewed_selection ?? evidence.reviewed_match);
  let last = z.number().int().safe().parse(row.updated_at), state: PendingReviewState, action: string;
  const stopped = db.prepare(`SELECT stop_code,updated_at,kind FROM (
    SELECT stop_code,updated_at,'content' AS kind FROM discovery_content_reads WHERE discovery_campaign_id=?1 AND expected_revision=?2 AND search_hit_id=?3 AND status='failed'
    UNION ALL SELECT stop_code,created_at AS updated_at,'language' AS kind FROM discovery_language_samples WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?4 AND status='failed'
    UNION ALL SELECT stop_code,updated_at,'platform' AS kind FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND platform=?5 AND profile_ref=?6 AND stop_code IN ('opencli_auth_required','opencli_risk_control')
  ) ORDER BY updated_at DESC LIMIT 1`).get(campaign, revision, row.hit_id ?? null, candidate, row.platform ?? null, row.profile_ref ?? null);
  // 仅在同一同步只读事务中复用已验证结果，并保持结果所属的 hit 一致。
  const basis = verified && verified.hit === row.hit_id ? verified.basis : typeof row.hit_id === "string" ? findDiscoveryReviewBasis(db, campaign, revision, row.hit_id) : null;
  const saved = typeof review.review_id === "string" ? db.prepare("SELECT material_json FROM discovery_evidence_reviews WHERE id=?1 AND discovery_campaign_id=?2 AND criteria_revision=?3 AND candidate_id=?4").get(review.review_id, campaign, revision, candidate) : undefined;
  if (basis) last = Math.max(last, Number(basis.native_profile?.observed_at ?? 0), Number(basis.native_content?.captured_at ?? 0));
  const sampleTime = db.prepare("SELECT MAX(created_at) AS time FROM discovery_language_samples WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3").get(campaign, revision, candidate);
  last = Math.max(last, Number(sampleTime?.time ?? 0), Number(stopped?.updated_at ?? 0));
  const needsScore = basis !== null && (!saved || review.policy_version !== "selection-quality-evaluation-v2" || discoveryObservationVersion(parseApplicationJson(Buffer.from(z.string().parse(saved.material_json)))) !== discoveryObservationVersion(buildDiscoveryReviewMaterial(basis)));
  const experiencePending = row.platform === "linkedin" && typeof row.hit_id === "string" && new DiscoveryLinkedinExperienceRepository(db).eligible(campaign, row.hit_id);
  const optionalContent = stopped?.kind === "content" && row.platform === "linkedin" && stopped.stop_code === "linkedin_content_unavailable" && (needsScore || experiencePending);
  if ((object(evidence.assessment).reasons as unknown[] | undefined)?.includes("linkedin_current_employment_evidence_exhausted")) { state = "insufficient_after_read"; action = "当前经历补读已结束，证据仍不足；保留材料，本轮不再读取"; }
  else if (stopped && !optionalContent) { state = "read_blocked"; action = "处理读取错误后再核实，本轮不重复尝试"; }
  else if (!basis) { state = "needs_identity"; action = "完成原生账号与来源归属验证"; }
  else if (needsScore) {
    state = "awaiting_score"; action = "使用当前已保存材料评分，无须重复读取";
  } else if (experiencePending) {
    state = "needs_content"; action = "读取一次 Experience 中的 Present／至今经历；仍缺少必需证据就不推荐，不再读取帖子";
  } else if (object(evidence.assessment).reasons instanceof Array && (object(evidence.assessment).reasons as unknown[]).includes("contact_unavailable")) {
    state = "needs_requirement"; action = "按原任务联系信息要求核实";
  } else if (row.platform === "linkedin" && typeof review.reason === "string") {
    state = "insufficient_after_read"; action = `核实原任务尚缺的条件：${[...review.reason].slice(0, 500).join("")}。当前岗位足够时无需帖子`;
  } else if (basis.native_content === null && !basis.language_samples.length && !basis.source_description) {
    state = "needs_content"; action = "按缺口补读账号发布或转载的相关内容，取得证据后评分";
  } else {
    state = "insufficient_after_read"; action = "保留具体缺口；已用完本轮来源时停止补读";
  }
  return { state, next_action: action, last_processed_at: last, stop_code: stopped && !optionalContent ? z.string().parse(stopped.stop_code) : null, gap: typeof review.reason === "string" ? [...review.reason].slice(0, 500).join("") : null };
}
export function pendingReviewDetails(db: DatabaseSync, campaign: string, revision: number, candidate: string): string[] {
  const value = pendingReview(db, campaign, revision, candidate);
  return value === null ? [] : [`核实状态：${pendingReviewLabels[value.state]}${value.stop_code ? `（${value.stop_code}）` : ""}`, ...(value.gap ? [`证据缺口：${value.gap}`] : []), `下一步：${value.next_action}`, `最后核实时间：${new Date(value.last_processed_at * 1000).toISOString()}`];
}
