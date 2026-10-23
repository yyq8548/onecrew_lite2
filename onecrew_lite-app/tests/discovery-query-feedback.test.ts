import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { readDiscoveryQueryFeedback, discoveryLowYieldUnreadHits } from "../src/discovery/query-feedback.ts";
import { discoveryQueryRoundPrompt } from "../src/discovery/query-round-execution.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";

// 通过环境变量回放真实搜索记录，测试只读数据库，不构造平台返回或模型结论。
const path = process.env.ONECREW_QUERY_FEEDBACK_DATABASE;
const campaign = process.env.ONECREW_QUERY_FEEDBACK_CAMPAIGN;
test("真实搜索回执按查询和作者去重，低质量合格结果保留，低命中查询让出未读步骤", { skip: !path || !campaign }, t => {
  const db = new DatabaseSync(path!, { readOnly: true }); t.after(() => db.close());
  const repository = new DiscoveryRepository(db), plan = repository.getPlan(campaign!);
  const before = db.prepare("SELECT total_changes() AS changes").get();
  const feedback = readDiscoveryQueryFeedback(db, campaign!);
  assert.ok(feedback.length > 0);
  for (const item of feedback) {
    const unique = db.prepare(`SELECT COUNT(DISTINCT r.native_author_id) AS n FROM discovery_evidence_reviews r
      JOIN discovery_search_hits h ON h.id=r.search_hit_id
      WHERE r.discovery_campaign_id=? AND r.criteria_revision=? AND r.policy_version='selection-quality-evaluation-v2'
        AND h.platform=? AND h.profile_ref=? AND h.search_keyword=?`).get(campaign!, plan.revision, item.platform, item.profile_ref, item.query);
    assert.equal(item.reviewed_authors, unique!.n);
    assert.equal(item.reviewed_authors, item.matched_authors + item.contradicted_authors + item.insufficient_authors);
    const decided = item.matched_authors + item.contradicted_authors;
    assert.equal(item.defer_unread, decided >= 5 && item.matched_authors * 5 <= decided);
  }
  const low = db.prepare(`SELECT c.platform_account_id,h.search_keyword FROM discovery_candidates c
    JOIN discovery_evidence_reviews r ON r.candidate_id=c.id AND r.id=json_extract(c.evidence_json,'$.reviewed_selection.review_id')
    JOIN discovery_search_hits h ON h.id=r.search_hit_id
    WHERE c.discovery_campaign_id=? AND c.verdict='qualified' AND json_extract(c.evidence_json,'$.reviewed_quality.score')=40`).all(campaign!);
  assert.ok(low.length > 0, "回放记录必须包含真实40分合格账号");
  for (const row of low) assert.ok(feedback.some(item => item.query === row.search_keyword && item.matched_authors > 0));
  const held = discoveryLowYieldUnreadHits(db, campaign!);
  const incomplete = feedback.filter(item => item.insufficient_authors > 0);
  assert.ok(incomplete.length > 0, "回放记录必须包含真实证据缺失");
  for (const item of incomplete) {
    const decided = item.matched_authors + item.contradicted_authors;
    if (decided < 5 || item.matched_authors * 5 > decided) assert.equal(item.defer_unread, false);
  }
  // 已完成的整轮可能没有剩余未读命中；应保持空列表，不重新打开成功或失败的读取。
  const protectedHits = db.prepare(`SELECT search_hit_id FROM discovery_evidence_reviews WHERE discovery_campaign_id=?
    UNION SELECT search_hit_id FROM discovery_profile_reads WHERE discovery_campaign_id=?
    UNION SELECT v.search_hit_id FROM discovery_video_completions v JOIN discovery_search_hits h ON h.id=v.search_hit_id WHERE h.discovery_campaign_id=?`).all(campaign!, campaign!, campaign!);
  for (const row of protectedHits) assert.ok(!held.includes(String(row.search_hit_id)));
  for (const id of held) {
    const hit = db.prepare("SELECT platform,profile_ref,search_keyword FROM discovery_search_hits WHERE id=?").get(id)!;
    assert.ok(feedback.some(item => item.platform === hit.platform && item.profile_ref === hit.profile_ref && item.query === hit.search_keyword && item.defer_unread));
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_profile_reads WHERE search_hit_id=?").get(id)!.n, 0);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_video_completions WHERE search_hit_id=?").get(id)!.n, 0);
  }
  const nextProfile = repository.nextProfileHit(campaign!, [], held);
  if (nextProfile) assert.ok(!held.includes(nextProfile));
  const nextVideo = repository.nextVideoHit(campaign!, held);
  if (nextVideo) assert.ok(!held.includes(nextVideo.id));
  const nextIdentity = repository.nextFacebookIdentity(campaign!, [], held);
  if (nextIdentity) assert.ok(!held.includes(nextIdentity));
  const prompt = discoveryQueryRoundPrompt({ brief: plan.brief, criteria: plan.criteria, target: plan.target_count, counts: repository.countCandidates(campaign!), eligible: plan.scopes, all_previous_queries: plan.scopes, candidate_sample: [], query_feedback: feedback }, "", 1);
  assert.ok(prompt.includes(feedback[0]!.query));
  assert.ok(prompt.includes("including 40"));
  assert.deepEqual(readDiscoveryQueryFeedback(db, campaign!), feedback);
  assert.deepEqual(db.prepare("SELECT total_changes() AS changes").get(), before);
  console.log(JSON.stringify({ queries: feedback.length, deferred_unread_hits: held.length, qualified_low_quality_accounts: low.length }));
});
