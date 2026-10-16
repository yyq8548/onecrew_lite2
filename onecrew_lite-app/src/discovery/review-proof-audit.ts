import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { mergePendingDiscoveryEvidence } from "./qualification.ts";
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const text = (value: unknown): string | null => typeof value === "string" ? value : null;

export function stripUnauditedDiscoveryProofs(database: DatabaseSync, raw: unknown, candidateId: string, planId: string, revision: number, author: string): Record<string, unknown> {
  if (!database.isTransaction) throw new Error("review_proof_audit_requires_snapshot");
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) throw new Error("review_evidence_invalid");
  const evidence = { ...object(raw) }, sources = Array.isArray(evidence.source_urls) ? evidence.source_urls : [];
  for (const [key, criterion] of [["reviewed_apparel_content", "apparel_business_content"], ["reviewed_category_content", "category_content"], ["reviewed_residence", "creator_residence_outside_mainland"]]) {
    if (!Object.hasOwn(evidence, key!)) continue;
    const proof = object(evidence[key!]), source = text(proof.source_url) ?? "", code = text(proof.country_code);
    const locallyValid = proof.verified === true && proof.native_author_id === author && (key === "reviewed_category_content" ? proof.policy_version === "category-content-review-v1" && typeof proof.category === "string" && !!proof.category.length && proof.category !== "服装" : ["apparel-residence-review-v1", "apparel-native-review-v2"].includes(String(proof.policy_version))) && sources.includes(source) && (key !== "reviewed_residence" || code !== null && code !== "CN" && /^[A-Z]{2}$/u.test(code));
    const audited = locallyValid && database.prepare(`SELECT EXISTS(
      SELECT 1 FROM discovery_evidence_reviews r
      JOIN discovery_search_hits h ON h.id=r.search_hit_id
      JOIN json_each(r.findings_json) f
      WHERE r.id=?1 AND r.candidate_id=?2 AND r.discovery_campaign_id=?3
        AND r.criteria_revision=?4 AND r.native_author_id=?5
        AND r.material_hash=?6 AND h.source_url=?7
        AND ((r.policy_version='apparel-residence-review-v1'
              AND json_extract(r.material_json,'$.format_version')=1 AND h.platform='youtube')
          OR (r.policy_version='apparel-native-review-v2'
              AND json_extract(r.material_json,'$.format_version')=2 AND h.platform IN ('twitter','tiktok'))
          OR (r.policy_version IN ('category-content-review-v1','language-match-review-v1')
              AND ((json_extract(r.material_json,'$.format_version')=1 AND h.platform='youtube')
                OR (json_extract(r.material_json,'$.format_version')=2 AND h.platform IN ('twitter','tiktok'))
                OR (json_extract(r.material_json,'$.format_version')=2 AND h.platform='bilibili'))))
        AND json_extract(f.value,'$.criterion')=?8
        AND json_extract(f.value,'$.outcome')='supported'
        AND (?8!='creator_residence_outside_mainland' OR json_extract(f.value,'$.country_code')=?9)
        AND (?8!='category_content' OR json_extract(r.material_json,'$.criteria.category')=?10)
    ) AS valid`).get(text(proof.review_id) ?? "", candidateId, planId, revision, author, text(proof.material_hash) ?? "", source, criterion!, code, text(proof.category))?.valid === 1;
    if (!audited) delete evidence[key!];
  }
  for (const [key, criterion] of [["reviewed_language", "content_language"], ["reviewed_match", "match_fit"]]) {
    if (!Object.hasOwn(evidence, key!)) continue;
    const proof = object(evidence[key!]), source = text(proof.source_url) ?? "";
    const locallyValid = proof.verified === true && (proof.policy_version === "language-match-review-v1" || key === "reviewed_match" && proof.policy_version === "initial-match-evaluation-v1") && proof.native_author_id === author && sources.includes(source);
    const audited = locallyValid && database.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_evidence_reviews r
      JOIN discovery_search_hits h ON h.id=r.search_hit_id JOIN json_each(r.findings_json) f
      WHERE r.id=?1 AND r.candidate_id=?2 AND r.discovery_campaign_id=?3
        AND r.criteria_revision=?4 AND r.native_author_id=?5 AND r.material_hash=?6
        AND h.source_url=?7 AND r.policy_version=?13
        AND ((h.platform='youtube' AND json_extract(r.material_json,'$.format_version')=1)
          OR (h.platform IN ('twitter','tiktok','bilibili','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest','pixiv','linkedin','douyin') AND json_extract(r.material_json,'$.format_version')=2)
          OR (h.platform IN ('google','yahoo') AND json_extract(r.material_json,'$.format_version')=2
            AND json_extract(r.material_json,'$.native_platform') IN ('youtube','twitter','douyin') AND EXISTS(
              SELECT 1 FROM discovery_web_source_reads web WHERE web.search_hit_id=h.id
                AND web.discovery_campaign_id=r.discovery_campaign_id AND web.expected_revision=r.criteria_revision
                AND web.origin_platform=h.platform AND web.profile_ref=h.profile_ref AND web.source_url=h.source_url
                AND web.status='completed' AND web.stop_code IS NULL AND web.target_platform=json_extract(r.material_json,'$.native_platform')
                AND web.native_author_id=r.native_author_id
                AND json_extract(web.native_material_json,CASE WHEN web.target_platform IN ('twitter','douyin') THEN '$.content.text' ELSE '$.description' END)=json_extract(r.material_json,'$.video_description'))))
        AND json_extract(f.value,'$.criterion')=?8
        AND json_extract(f.value,'$.outcome')=?9
        AND json_extract(f.value,'$.rationale')=?10
        AND (?8!='content_language' OR json_extract(r.material_json,'$.criteria.language')=?11)
        AND (?8!='match_fit' OR (json_extract(f.value,'$.outcome')='supported' AND json_extract(f.value,'$.match_level')=?12)
          OR (r.policy_version='initial-match-evaluation-v1' AND json_extract(f.value,'$.outcome')='insufficient'
              AND json_extract(f.value,'$.match_level') IS NULL AND ?12 IS NULL))) AS valid`).get(text(proof.review_id) ?? "", candidateId, planId, revision, author, text(proof.material_hash) ?? "", source, criterion!, text(proof.outcome), text(proof.reason), text(proof.language), text(proof.level), text(proof.policy_version))?.valid === 1;
    if (!audited) delete evidence[key!];
  }
  if (Object.hasOwn(evidence, "reviewed_requirements")) {
    const proofs = evidence.reviewed_requirements;
    const valid = Array.isArray(proofs) && proofs.length <= 32 && proofs.every((raw, index) => {
      const proof = object(raw);
      if (proof.index !== index || proof.native_author_id !== author || proof.criteria_revision !== revision || !sources.includes(proof.source_url)) return false;
      const row = database.prepare("SELECT r.material_json FROM discovery_evidence_reviews r JOIN discovery_search_hits h ON h.id=r.search_hit_id JOIN json_each(r.findings_json) f WHERE r.id=? AND r.candidate_id=? AND r.discovery_campaign_id=? AND r.criteria_revision=? AND r.native_author_id=? AND r.material_hash=? AND h.source_url=? AND json_extract(f.value,'$.criterion.requirement')=? AND json_extract(f.value,'$.outcome')=? AND json_extract(f.value,'$.rationale')=?").get(text(proof.review_id) ?? "", candidateId, planId, revision, author, text(proof.material_hash) ?? "", text(proof.source_url) ?? "", index, text(proof.outcome), text(proof.reason));
      if (typeof row?.material_json !== "string") return false;
      const requirements = object(object(parseApplicationJson(row.material_json)).criteria).requirements;
      return Array.isArray(requirements) && isDeepStrictEqual(requirements[index], proof.requirement);
    });
    if (!valid) delete evidence.reviewed_requirements;
  }
  return evidence;
}
export function mergeAuditedDiscoveryProofs(database: DatabaseSync, current: unknown, previous: unknown, candidate: string, campaign: string, revision: number, author: string): Record<string, unknown> {
  const retained = stripUnauditedDiscoveryProofs(database, previous, candidate, campaign, revision, author), merged = mergePendingDiscoveryEvidence(current, retained);
  if (!merged || typeof merged !== "object" || Array.isArray(merged)) throw new Error("discovery_candidate_evidence_invalid");
  const result = merged as Record<string, unknown>;
  for (const key of ["reviewed_apparel_content", "reviewed_category_content", "reviewed_language", "reviewed_match", "reviewed_residence"]) {
    if (!Object.hasOwn(retained, key)) continue;
    const source = object(retained[key]).source_url, sources = result.source_urls;
    if (typeof source !== "string" || !Array.isArray(sources)) throw new Error("discovery_reviewed_source_invalid");
    if (!sources.includes(source)) { sources.splice(Math.min(1, sources.length), 0, source); sources.splice(20); }
    result[key] = structuredClone(retained[key]);
  }
  if (Array.isArray(retained.reviewed_requirements)) {
    const sources = result.source_urls;
    if (!Array.isArray(sources)) throw new Error("discovery_reviewed_source_invalid");
    for (const raw of retained.reviewed_requirements) {
      const source = object(raw).source_url;
      if (typeof source !== "string") throw new Error("discovery_reviewed_source_invalid");
      if (!sources.includes(source)) { sources.splice(Math.min(1, sources.length), 0, source); sources.splice(20); }
    }
    result.reviewed_requirements = structuredClone(retained.reviewed_requirements);
  }
  return result;
}
