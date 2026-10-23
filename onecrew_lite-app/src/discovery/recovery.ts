import type { DatabaseSync } from "node:sqlite";

// 原0.7.1 interrupt_discovery_searches的同事务恢复；只记录中断，不执行提供方请求。
export function recoverDiscoveryReads(database: DatabaseSync): number {
  database.exec("BEGIN IMMEDIATE"); let committed = false;
  try {
    const now = Math.floor(Date.now() / 1000); let released = 0;
    if (database.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name='discovery_web_author_pages'").get()) {
      database.prepare("UPDATE discovery_web_author_pages SET status=CASE WHEN material_json IS NULL THEN 'failed' ELSE 'awaiting_resolution' END,stop_code=CASE WHEN material_json IS NULL THEN 'web_author_read_interrupted' ELSE NULL END,updated_at=? WHERE status='reading'").run(now);
      released += Number(database.prepare("UPDATE discovery_platform_runs SET status='completed',active_query=NULL,stop_code=NULL,updated_at=? WHERE status='searching' AND active_query LIKE 'article:%'").run(now).changes);
    }
    database.prepare(`UPDATE discovery_contact_lookups SET receipt_json=json_set(receipt_json,'$.status','failed','$.stop_code','interrupted_contact_lookup','$.links',json('[]')) WHERE json_extract(receipt_json,'$.status')='reading'
      AND EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=discovery_contact_lookups.discovery_campaign_id AND r.status='searching' AND r.profile_ref=json_extract(receipt_json,'$.profile_ref') AND r.active_query='contact:'||discovery_contact_lookups.candidate_id||':'||json_extract(receipt_json,'$.call_number'))`).run();
    released += Number(database.prepare(`UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=? WHERE status='searching'
      AND EXISTS(SELECT 1 FROM discovery_contact_lookups l WHERE l.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND json_extract(l.receipt_json,'$.profile_ref')=discovery_platform_runs.profile_ref AND json_extract(l.receipt_json,'$.stop_code')='interrupted_contact_lookup' AND active_query='contact:'||l.candidate_id||':'||json_extract(l.receipt_json,'$.call_number'))`).run(now).changes);
    database.prepare(`UPDATE discovery_public_page_reads SET status='failed',stop_code='interrupted_public_page_read',updated_at=? WHERE status='reading'
      AND EXISTS(SELECT 1 FROM discovery_candidates c JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.discovery_campaign_id AND r.platform=c.platform AND r.profile_ref=c.source_profile_ref WHERE c.id=discovery_public_page_reads.candidate_id AND c.verdict='qualified' AND r.status='searching' AND r.active_query='page:'||c.id||':'||discovery_public_page_reads.call_number)`).run(now);
    released += Number(database.prepare(`UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=? WHERE status='searching'
      AND EXISTS(SELECT 1 FROM discovery_public_page_reads p JOIN discovery_candidates c ON c.id=p.candidate_id WHERE c.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND c.platform=discovery_platform_runs.platform AND c.source_profile_ref=discovery_platform_runs.profile_ref AND c.verdict='qualified' AND p.status='failed' AND p.stop_code='interrupted_public_page_read' AND active_query='page:'||p.candidate_id||':'||p.call_number)`).run(now).changes);
    const operations = [
      { table: "discovery_facebook_content_targets", platform: "'facebook'", prefix: "targets", code: "interrupted_read", release: false },
      { table: "discovery_identity_reads", platform: "'twitter'", prefix: "identity", code: "interrupted_identity_read", release: false },
      { table: "discovery_facebook_identity_reads", platform: "'facebook'", prefix: "facebook_identity", code: "interrupted_facebook_identity_read", release: true },
      { table: "discovery_linkedin_experience_reads", platform: "'linkedin'", prefix: "experience", code: "interrupted_experience_read", release: false },
      { table: "discovery_douyin_author_reads", platform: "'douyin'", prefix: "source_author", code: "interrupted_source_author_read", release: true },
      { table: "discovery_web_source_reads", platform: "discovery_web_source_reads.origin_platform", prefix: "web_source", code: "interrupted_web_source_read", release: true },
    ] as const;
    // 表名、平台表达式和token前缀全部来自封闭生产清单，不接受请求SQL。
    for (const operation of operations) {
      const { table, platform, prefix, code } = operation;
      database.prepare(`UPDATE ${table} SET status='failed',stop_code=?,updated_at=? WHERE status='reading'
        AND EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=${table}.discovery_campaign_id AND r.platform=${platform} AND r.profile_ref=${table}.profile_ref AND r.status='searching' AND r.active_query='${prefix}:'||${table}.search_hit_id||':'||${table}.call_number)`).run(code, now);
      if (operation.release) {
        const runPlatform = platform.startsWith("'") ? `AND discovery_platform_runs.platform=${platform}` : "AND p.origin_platform=discovery_platform_runs.platform";
        released += Number(database.prepare(`UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=? WHERE status='searching'
          AND EXISTS(SELECT 1 FROM ${table} p WHERE p.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND p.profile_ref=discovery_platform_runs.profile_ref ${runPlatform} AND p.status='failed' AND p.stop_code=? AND active_query='${prefix}:'||p.search_hit_id||':'||p.call_number)`).run(now, code).changes);
      }
    }
    database.prepare(`UPDATE discovery_content_reads SET status='failed',native_content_json=NULL,stop_code='interrupted_content_read',updated_at=? WHERE status='reading' AND platform='facebook'
      AND EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f JOIN discovery_platform_runs r ON r.discovery_campaign_id=f.discovery_campaign_id AND r.platform='facebook' AND r.profile_ref=f.profile_ref WHERE f.search_hit_id=discovery_content_reads.search_hit_id AND f.call_number=discovery_content_reads.call_number AND f.source_url=discovery_content_reads.source_url AND f.status='reading' AND r.status='searching' AND r.active_query='content:'||f.search_hit_id||':'||f.call_number)`).run(now);
    database.prepare(`UPDATE discovery_facebook_original_attempts SET status='failed',stop_code='interrupted_content_read',updated_at=? WHERE status='reading'
      AND EXISTS(SELECT 1 FROM discovery_content_reads c WHERE c.search_hit_id=discovery_facebook_original_attempts.search_hit_id AND c.call_number=discovery_facebook_original_attempts.call_number AND c.source_url=discovery_facebook_original_attempts.source_url AND c.status='failed' AND c.stop_code='interrupted_content_read')`).run(now);
    released += Number(database.prepare(`UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=? WHERE status='searching' AND platform='facebook'
      AND EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND f.profile_ref=discovery_platform_runs.profile_ref AND f.status='failed' AND f.stop_code='interrupted_content_read' AND active_query='content:'||f.search_hit_id||':'||f.call_number)`).run(now).changes);
    released += Number(database.prepare(`UPDATE discovery_platform_runs SET status='technical_stop',stop_code=CASE
      WHEN active_query LIKE 'video:%' THEN 'interrupted_video_read' WHEN active_query LIKE 'transcript:%' THEN 'interrupted_transcript_read'
      WHEN active_query LIKE 'profile:%' THEN 'interrupted_profile_read' WHEN active_query LIKE 'content:%' THEN 'interrupted_content_read'
      WHEN active_query LIKE 'identity:%' THEN 'interrupted_identity_read' WHEN active_query LIKE 'page:%' THEN 'interrupted_public_page_read'
      ELSE 'interrupted_read' END,updated_at=? WHERE status='searching'`).run(now).changes);
    if (!Number.isInteger(released) || released < 0 || released > 4294967295) throw new Error("discovery_recovery_count_overflow");
    database.exec("COMMIT"); committed = true; return released;
  } finally { if (!committed) database.exec("ROLLBACK"); }
}
