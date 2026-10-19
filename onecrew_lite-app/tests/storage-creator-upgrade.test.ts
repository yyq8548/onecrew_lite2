import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test, { after, before } from "node:test";
import { extractCreatorHistory } from "./creator-history.ts";
import { fileURLToPath } from "node:url";
import { backupLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { upgradeCreatorStagingCopy } from "../src/storage/creator-upgrade.ts";
import { validateCreatorSchema90, validateCreatorSchemaVersion } from "../src/storage/creator-schema.ts";
import { legacyResendAuthConnectionId } from "../src/storage/creator-mailbox-edge.ts";
import { migrateCreatorTaskOriginEdge } from "../src/storage/creator-task-origin-edge.ts";

let oracleDirectory: string;
before(async () => {
  oracleDirectory = await extractCreatorHistory();
});
after(async () => { if (oracleDirectory) await rm(oracleDirectory, { recursive: true }); });
test("Legacy mailbox UUID accepts original Rust representations and arbitrary bits", () => {
  const hyphenated = "01020304-1112-2122-3132-414243444546";
  const expected = legacyResendAuthConnectionId(hyphenated);
  assert.equal(expected, "53475041-5f56-0c63-6466-096f000b0b08");
  for (const value of [hyphenated.replaceAll("-", ""), hyphenated.toUpperCase(), `{${hyphenated}}`, `urn:uuid:${hyphenated}`]) assert.equal(legacyResendAuthConnectionId(value), expected);
  for (const value of ["invalid", `{${hyphenated.replaceAll("-", "")}}`, `URN:UUID:${hyphenated}`, ` ${hyphenated}`]) assert.equal(legacyResendAuthConnectionId(value), undefined);
});
for (const legacyVersion of Array.from({ length: 11 }, (_, index) => 27 + index)) test(`Creator v${legacyVersion} isolated copy preserves Creator data while initializing discovery`, async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-upgrade-", import.meta.url)));
  try {
    const sourcePath = join(oracleDirectory, `creator-schema${legacyVersion}-oracle.sqlite`);
    const originalBytes = await readFile(sourcePath);
    const stagingPath = join(directory, "private staging", "creator.sqlite");
    await backupLegacyDatabase(sourcePath, stagingPath, "creator");
    const staged = new DatabaseSync(stagingPath);
    try {
      staged.exec(`INSERT INTO creators(id,display_name,notes,created_at,updated_at${legacyVersion >= 35 ? ",tags_json" : ""}) VALUES('historical','历史作者','preserved',1,2${legacyVersion >= 35 ? ",'[]'" : ""})`);
      if (legacyVersion <= 31) {
        const mailbox = legacyVersion === 31 ? "urn:uuid:00000000-0000-0000-0000-000000000000" : "historical-non-uuid";
        staged.prepare("INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES(?,'resend','history','synthetic@example.com','[]',1,1,2)").run(mailbox);
      }
      if (legacyVersion === 33) {
        staged.exec("INSERT INTO tasks(id,instruction,schedule_json,status,created_by,created_at,updated_at,kind) VALUES('retired-task','history','{}','completed','ui',1,2,'mailbox_triage')");
        staged.exec("INSERT INTO task_runs(id,task_id,scheduled_for,state,idempotency_key,claimed_at,lease_expires_at,finished_at,origin) VALUES('run','retired-task',1,'expired','task-key',1,2,2,'external')");
        staged.exec("INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES('mailbox','imap','history','synthetic@example.com','[]',1,1,2)");
        staged.exec("INSERT INTO email_threads(id,mailbox_ref_id,provider_thread_key,subject,active_folder,created_at,updated_at) VALUES('thread','mailbox','history','history','inbox',1,2)");
        staged.exec("INSERT INTO emails(id,thread_id,provider_message_id,direction,subject,body_text,delivery_status,occurred_at,origin) VALUES('email','thread','history','inbound','history','保留邮件','received',1,'provider_import')");
        staged.exec("INSERT INTO reply_triage(id,channel,email_id,intent,summary,task_run_id,created_at) VALUES('triage','email','email','other','保留业务事实','run',1)");
      }
      if (legacyVersion === 34) {
        const replay = { outcomes: [{ kind: "accepted", creator: { creator_id: "historical", display_name: "历史作者", stage: "prospect", created_at: 1, contact_routes: [] } }, { kind: "rejected", message: "historical rejected input" }], replayed: false };
        staged.prepare("INSERT INTO idempotency_records(key,action,state,result_json,created_at,updated_at) VALUES('batch-replay','creator_batch_create','completed',?,1,2)").run(JSON.stringify(replay));
        const impact = { creator_inputs: 1, creators_created: 1, creators_merged_by_public_identity: 0, creator_scores_filled: 0, follower_counts_filled: 0, platform_accounts_created: 0, claimed_links_created: 0, confirmed_links_created: 0, rejected_links_created: 0, links_promoted_to_confirmed: 0, profile_observations_created: 0, contact_routes_created: 0, creators_unchanged: 0 };
        staged.prepare("INSERT INTO creator_import_receipts(content_sha256,format,format_version,profile,snapshot_kind,creator_count,impact_json,imported_at) VALUES('digest','reacher.creator_bundle',1,'portable_public','full',1,?,1)").run(JSON.stringify(impact));
      }
      if (legacyVersion === 35 || legacyVersion === 32) {
        staged.exec("INSERT INTO campaigns(id,name,status,goal,created_at,updated_at) VALUES('old-campaign','history','archived','history',1,2)");
        staged.exec("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES('operator','x','profile','history',1,2)");
        staged.exec("INSERT INTO platform_accounts(id,platform,target_kind,external_id,created_at,updated_at) VALUES('account','x','user','123',1,2)");
        staged.exec("INSERT INTO creator_account_links(id,creator_id,platform_account_id,state,confidence,created_at,updated_at) VALUES('link','historical','account','confirmed','exact',1,2)");
        staged.exec("INSERT INTO observation_sources(id,kind,platform,account_ref_id,observed_at) VALUES('source','initial_platform_account','x','operator',1)");
        const origin = { kind: "campaign_discovery", discovery_id: "discovery", campaign_id: "old-campaign", account_ref_id: "operator", query: "history", search_kind: "users" };
        const payload = { candidate_id: "candidate", origin, account: { platform: "x", target_kind: "user", external_id: "123" } };
        staged.prepare("INSERT INTO creator_candidate_snapshots(discovery_id,candidate_id,origin_kind,origin_json,candidate_json,account_platform,account_target_kind,account_external_id,account_display_name,source_kind,campaign_id,account_ref_id,created_at,updated_at) VALUES('discovery','candidate','campaign_discovery',?,?,'x','user','123','author','campaign_discovery','old-campaign','operator',1,2)").run(JSON.stringify(origin), JSON.stringify(payload));
        staged.exec("INSERT INTO creator_origins(id,kind,creator_id,source_id,discovery_id,campaign_id,discovered_at) VALUES('origin','campaign_discovery','historical','source','discovery','old-campaign',1)");
        staged.exec("INSERT INTO audit_events(id,kind,creator_id,campaign_id,payload_json,created_at) VALUES('audit','historical','historical','old-campaign','{\"preserved\":true}',1)");
        const replay = { platform_accounts: [payload.account], origins: [{ kind: "campaign_discovery", id: "origin", discovery_id: "discovery", campaign_id: "old-campaign", creator_id: "historical", source_id: "source", discovered_at: 1 }] };
        staged.prepare("INSERT INTO idempotency_records(key,action,state,result_json,created_at,updated_at) VALUES('history-replay','read','completed',?,1,2)").run(JSON.stringify(replay));
      }
      if (legacyVersion === 36) {
        staged.exec("INSERT INTO idempotency_records(key,action,state,result_json,created_at,updated_at) VALUES('retired','direct_message_send','remote_started',' raw historical recovery ',1,2)");
        const impact = '{"contact_routes":18446744073709551614,"dependencies":0,"email_threads":0,"email_messages":0,"email_recipients":0,"email_drafts":0,"direct_message_conversations":0,"direct_messages":0,"outreach_touchpoints":0,"audit_events":0,"idempotency_records":0,"target_resolutions":0,"identity_conflicts":0,"observations":0,"contact_locators":0,"observation_sources":0,"resolved_targets":0,"candidate_snapshots_detached":1,"creator_origins_detached":0,"local_records_total":18446744073709551615}';
        staged.prepare("INSERT INTO creator_contact_route_delete_operations(id,creator_id,contact_route_id,plan_fingerprint,impact_json,status,expires_at,created_at,updated_at) VALUES('operation','historical','route','fingerprint',?,'prepared',99,1,2)").run(impact);
      }
    }
    finally { staged.close(); }
    assert.deepEqual(await upgradeCreatorStagingCopy({ sourcePath, stagingPath }), { fromVersion: legacyVersion, toVersion: 90 });
    const current = new DatabaseSync(stagingPath, { readOnly: true });
    try {
      validateCreatorSchema90(current);
      assert.equal(current.prepare("SELECT notes FROM creators WHERE id='historical'").get()!.notes, "preserved");
      assert.equal(current.prepare("SELECT count(*) AS count FROM discovery_campaigns").get()!.count, 0);
      if (legacyVersion <= 31) {
        const auth = current.prepare("SELECT id,secret_record_id,available FROM email_auth_connections").get()!;
        assert.equal(auth.available, legacyVersion === 31 ? 1 : 0);
        assert.equal(auth.secret_record_id, legacyVersion === 31 ? "urn:uuid:00000000-0000-0000-0000-000000000000" : "historical-non-uuid");
        if (legacyVersion === 31) assert.equal(auth.id, "52455345-4e44-2d41-5554-482d434f4e4e");
      }
      if (legacyVersion === 33) assert.equal(current.prepare("SELECT summary FROM reply_triage WHERE id='triage'").get()!.summary, "保留业务事实");
      if (legacyVersion === 34) {
        const replay = JSON.parse(String(current.prepare("SELECT result_json FROM idempotency_records WHERE key='batch-replay'").get()!.result_json));
        assert.deepEqual(replay.outcomes.map((outcome: { input_index: number }) => outcome.input_index), [0, 1]);
        assert.deepEqual(replay.outcomes[0].creator.tags, []);
        assert.equal(JSON.parse(String(current.prepare("SELECT impact_json FROM creator_import_receipts WHERE content_sha256='digest'").get()!.impact_json)).creator_tags_filled, 0);
      }
      if (legacyVersion === 35 || legacyVersion === 32) {
        const origin = current.prepare("SELECT kind,operator_account_ref_id FROM creator_origins WHERE id='origin'").get()!;
        assert.equal(origin.kind, "platform_search");
        assert.equal(origin.operator_account_ref_id, "operator");
        const replay = JSON.parse(String(current.prepare("SELECT result_json FROM idempotency_records WHERE key='history-replay'").get()!.result_json));
        assert.equal(replay.origins[0].kind, "platform_search");
        assert.equal(replay.origins[0].account_ref_id, "operator");
        assert.equal(replay.origins[0].campaign_id, undefined);
        assert.equal(current.prepare("SELECT payload_json FROM audit_events WHERE id='audit'").get()!.payload_json, '{"preserved":true}');
      }
      if (legacyVersion === 36) {
        const claim = current.prepare("SELECT state,result_json FROM idempotency_records WHERE key='retired'").get()!;
        assert.equal(claim.state, "failed");
        assert.equal(JSON.parse(String(claim.result_json)).recovery, " raw historical recovery ");
        const impact = String(current.prepare("SELECT impact_json FROM creator_contact_route_delete_operations WHERE id='operation'").get()!.impact_json);
        assert.ok(impact.includes('"contact_routes":18446744073709551614'));
        assert.ok(impact.includes('"local_records_total":18446744073709551614'));
        assert.ok(!impact.includes("candidate_snapshots_detached"));
      }
    } finally { current.close(); }
    assert.deepEqual(await readFile(sourcePath), originalBytes);
  } finally { await rm(directory, { recursive: true }); }
});
for (const version of Array.from({ length: 52 }, (_, index) => 38 + index)) {
  test(`Creator v${version} isolated copy reaches v90 while preserving rows`, async () => {
    const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-upgrade-", import.meta.url)));
    try {
      const sourcePath = join(oracleDirectory, `creator-schema${version}-oracle.sqlite`);
      const sourceBytes = await readFile(sourcePath);
      const stagingPath = join(directory, "private staging", "creator.sqlite");
      await backupLegacyDatabase(sourcePath, stagingPath, "creator");
      const staged = new DatabaseSync(stagingPath);
      let stagedClosed = false;
      try {
      const provider = version >= 86 ? "gmail" : "imap";
      staged.exec("PRAGMA foreign_keys=ON");
      staged.prepare("INSERT INTO creators(id,display_name,notes,created_at,updated_at,tags_json) VALUES('creator','中文作者','keep historical notes',1,2,?)").run(JSON.stringify(["标签"]));
      staged.prepare("INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES('mailbox',?,'history','synthetic@example.com','[]',1,1,2)").run(provider);
      staged.exec("INSERT INTO email_threads(id,mailbox_ref_id,provider_thread_key,subject,active_folder,created_at,updated_at) VALUES('thread','mailbox','history-thread','history','inbox',1,2)");
      staged.exec("INSERT INTO emails(id,thread_id,provider_message_id,direction,subject,body_text,delivery_status,occurred_at,origin) VALUES('email','thread','history-message','inbound','history','历史邮件','received',1,'provider_import')");
      staged.exec(`INSERT INTO discovery_campaigns(rowid,id,phase,current_revision,target_count,max_calls,used_calls,created_at,updated_at) VALUES(123,'campaign','active',1,1,${version < 65 ? "40" : "NULL"},0,1,2)`);
      staged.exec("INSERT INTO discovery_campaign_revisions(discovery_campaign_id,revision,brief_json,opencli_version,criteria_json,scope_json) VALUES('campaign',1,'{}','1','{}','{}')");
      staged.exec("INSERT INTO discovery_platform_runs(discovery_campaign_id,platform,profile_ref,status,updated_at) VALUES('campaign','youtube','profile','completed',1)");
      const candidateFields = "id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at";
      const candidateValues = "'candidate','campaign',1,'youtube','profile','public-id','author','https://example.com/author','pending','{}',1,2";
      staged.exec(`INSERT INTO discovery_candidates(${candidateFields}${version >= 77 ? ",source_platform" : ""}) VALUES(${candidateValues}${version >= 77 ? ",'youtube'" : ""})`);
      if (version >= 65) staged.exec("INSERT INTO discovery_contact_observations(candidate_id,discovery_campaign_id,criteria_revision,native_author_id,kind,original_value,normalized_value,source_url,source_excerpt,captured_at,attribution) VALUES('candidate','campaign',1,'public-id','email','synthetic@example.com','synthetic@example.com','https://example.com/author','public contact',1,'author_published')");
      if (version >= 55) staged.exec("INSERT INTO discovery_target_increases(id,operator_account_id,discovery_campaign_id,criteria_revision,previous_target_count,new_target_count,used_calls_at_decision,created_at) VALUES('decision','operator','campaign',1,1,2,0,1)");
      staged.exec("INSERT INTO discovery_platform_runs(discovery_campaign_id,platform,profile_ref,status,updated_at) VALUES('campaign','google','engine','completed',1)");
      const targetPlatform = version >= 78 ? "twitter" : "youtube";
      const videoId = version >= 78 ? "456" : "abcdefghijk";
      const sourceUrl = version >= 78 ? "https://x.com/creator/status/456" : `https://www.youtube.com/watch?v=${videoId}`;
      const authorId = version >= 78 ? "123" : `UC${"a".repeat(22)}`;
      staged.prepare("INSERT INTO discovery_search_hits(id,discovery_campaign_id,platform,profile_ref,search_keyword,source_url,title,author_label,captured_at) VALUES('web-hit','campaign','google','engine','query',?,'post','author',1)").run(sourceUrl);
      const identity = { account_id: authorId, post_id: videoId, source_url: sourceUrl };
      const material = targetPlatform === "twitter" ? { identity, profile: { account_id: authorId }, content: { identity } } : { channelId: authorId, videoId };
      if (version >= 76) staged.prepare("INSERT INTO discovery_web_source_reads(search_hit_id,discovery_campaign_id,origin_platform,profile_ref,source_url,expected_revision,call_number,target_platform,video_id,status,native_author_id,native_material_json,updated_at) VALUES('web-hit','campaign','google','engine',?,1,1,?,?,'completed',?,?,1)")
        .run(sourceUrl, targetPlatform, videoId, authorId, JSON.stringify(material));
      const preservedTables = ["email_threads", "emails", "discovery_campaigns", "discovery_campaign_revisions", "discovery_platform_runs", "discovery_candidates", "discovery_search_hits"];
      if (version >= 50) {
        staged.exec("INSERT INTO discovery_video_completions(search_hit_id,completed_at) VALUES('web-hit',1)");
        staged.exec("INSERT INTO discovery_video_identity_bindings(search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at) VALUES('web-hit','campaign',1,'candidate','public-id',1)");
        preservedTables.push("discovery_video_completions", "discovery_video_identity_bindings");
      }
      if (version >= 51) {
        staged.exec(`INSERT INTO discovery_evidence_reviews(id,operator_account_id,discovery_campaign_id,criteria_revision,search_hit_id,candidate_id,native_author_id,profile_ref,request_id,request_hash,material_hash,material_json,policy_version,reviewer_json,findings_json,verdict,assessment_json,created_at) VALUES('review','operator','campaign',1,'web-hit','candidate','public-id','profile','request','${"a".repeat(64)}','${"b".repeat(64)}','{"format_version":1,"text":"历史审核"}','v1','{}','[]','pending','{}',1)`);
        preservedTables.push("discovery_evidence_reviews");
      }
      if (version >= 55) preservedTables.push("discovery_target_increases");
      if (version >= 65) preservedTables.push("discovery_contact_observations");
      staged.exec("INSERT INTO discovery_platform_runs(discovery_campaign_id,platform,profile_ref,status,updated_at) VALUES('campaign','twitter','receipt','completed',1),('campaign','facebook','receipt','completed',1)");
      if (version < 46) staged.exec("INSERT INTO discovery_platform_runs(discovery_campaign_id,platform,profile_ref,status,updated_at) VALUES('campaign','bilibili','receipt','completed',1)");
      staged.exec("INSERT INTO discovery_search_hits(id,discovery_campaign_id,platform,profile_ref,search_keyword,source_url,title,author_label,captured_at) VALUES('receipt-hit','campaign','twitter','receipt','query','https://x.com/creator/status/789','post','author',1),('facebook-hit','campaign','facebook','receipt','query','https://www.facebook.com/profile.php?id=123','profile','author',1)");
      if (version >= 59) {
        staged.exec("INSERT INTO discovery_language_samples(discovery_campaign_id,criteria_revision,candidate_id,search_hit_id,ordinal,call_number,status,sample_json,created_at) VALUES('campaign',1,'candidate','receipt-hit',1,1,'completed','{\"comments\":[\"历史文本\"]}',1)");
        preservedTables.push("discovery_language_samples");
      }
      if (version >= 42) staged.exec("INSERT INTO discovery_search_authors(search_hit_id,author_id,author_url,observed_at) VALUES('receipt-hit','123','https://x.com/creator',1),('facebook-hit','123','https://www.facebook.com/profile.php?id=123',1)");
      if (version >= 43) staged.prepare("INSERT INTO discovery_profile_reads(search_hit_id,discovery_campaign_id,platform,profile_ref,author_id,expected_revision,call_number,status,native_profile_json,updated_at) VALUES('receipt-hit','campaign',?,'receipt','123',1,1,'completed','{}',1)").run(version < 45 ? "bilibili" : "twitter");
      if (version >= 63) staged.exec("INSERT INTO discovery_profile_reads(search_hit_id,discovery_campaign_id,platform,profile_ref,author_id,expected_revision,call_number,status,native_profile_json,updated_at) VALUES('facebook-hit','campaign','facebook','receipt','123',1,1,'reading',NULL,1)");
      if (version >= 44) staged.prepare("INSERT INTO discovery_content_reads(search_hit_id,discovery_campaign_id,platform,profile_ref,author_id,source_url,expected_revision,call_number,status,native_content_json,updated_at) VALUES('receipt-hit','campaign',?,'receipt','123','https://x.com/creator/status/789',1,2,'completed','{}',1)").run(version < 46 ? "bilibili" : "twitter");
      if (version >= 47) staged.exec("INSERT INTO discovery_identity_reads(search_hit_id,discovery_campaign_id,platform,profile_ref,author_handle,source_url,post_id,expected_revision,call_number,status,account_id,observed_source_url,updated_at) VALUES('receipt-hit','campaign','twitter','receipt','creator','https://x.com/creator/status/789','789',1,3,'completed','123','https://x.com/creator/status/789',1)");
      if (version >= 48) staged.exec("INSERT INTO discovery_profile_identity_matches(search_hit_id,identity_search_hit_id,account_id,updated_at) VALUES('receipt-hit','receipt-hit','123',1)");
      if (version >= 49) staged.exec("INSERT INTO discovery_content_identity_bindings(search_hit_id,identity_search_hit_id,profile_search_hit_id,account_id,created_at) VALUES('receipt-hit','receipt-hit','receipt-hit','123',1)");
      if (version >= 64) staged.exec("INSERT INTO discovery_facebook_content_targets(search_hit_id,discovery_campaign_id,platform,profile_ref,author_id,profile_url,expected_revision,call_number,status,updated_at) VALUES('facebook-hit','campaign','facebook','receipt','123','https://www.facebook.com/profile.php?id=123',1,2,'reading',1)");
      if (version >= 42) preservedTables.push("discovery_search_authors");
      if (version >= 43) preservedTables.push("discovery_profile_reads");
      if (version >= 44) preservedTables.push("discovery_content_reads");
      if (version >= 47) preservedTables.push("discovery_identity_reads");
      if (version >= 48) preservedTables.push("discovery_profile_identity_matches");
      if (version >= 49) preservedTables.push("discovery_content_identity_bindings");
      if (version >= 64) preservedTables.push("discovery_facebook_content_targets");
      if (version === 38) {
        staged.exec("INSERT INTO discovery_search_hits(id,discovery_campaign_id,platform,profile_ref,search_keyword,source_url,title,author_label,captured_at) VALUES('legacy-video','campaign','youtube','profile','legacy query','https://www.youtube.com/watch?v=abcdefghijk','legacy','author',3)");
        staged.prepare("UPDATE discovery_candidates SET evidence_json=? WHERE id='candidate'").run(JSON.stringify({ source_urls: ["https://www.youtube.com/watch?v=abcdefghijk"] }));
      }
      if (version >= 76) preservedTables.push("discovery_web_source_reads");
      if (version >= 87) {
        staged.exec("INSERT INTO gmail_bulk_batches(id,mailbox_ref_id,subject,body,state,created_at) VALUES('batch','mailbox','history','content','paused',1)");
        staged.exec("INSERT INTO gmail_bulk_items(id,batch_id,recipient,source,fingerprint,state,detail) VALUES('item','batch','synthetic@example.com','creator','fingerprint','unknown','remote outcome unknown')");
        preservedTables.push("gmail_bulk_batches", "gmail_bulk_items");
      }
      const preservedRows = preservedTables.map((table) => staged.prepare(`SELECT * FROM ${table}`).all().map((row) =>
        table === "discovery_candidates" && version < 77 ? { ...row, source_platform: row.platform }
          : table === "discovery_language_samples" && version < 61 ? { ...row, sample_format_version: 1 } : { ...row }));
      staged.exec("INSERT INTO idempotency_records(key,action,mailbox_ref_id,state,result_json,created_at,updated_at) VALUES('attempt-key','email_send','mailbox','remote_started','{}',1,2)");
      if (version === 86) {
        const insertClaim = staged.prepare("INSERT INTO idempotency_records(key,action,mailbox_ref_id,state,created_at,updated_at) VALUES(?,'email_send','mailbox',?,1,2)");
        for (const state of ["remote_accepted", "remote_verified", "completed", "in_progress", "failed"]) insertClaim.run(`claim-${state}`, state);
        staged.exec("INSERT INTO idempotency_records(key,action,mailbox_ref_id,state,created_at,updated_at) VALUES('other-action','read','mailbox','remote_started',1,2)");
      }
      const creator = staged.prepare("SELECT * FROM creators").get();
      const mailbox = staged.prepare("SELECT * FROM mailbox_refs").get();
      const claims = staged.prepare("SELECT * FROM idempotency_records ORDER BY key").all();
      validateCreatorSchemaVersion(staged, version);
      staged.close();
      stagedClosed = true;
      assert.deepEqual(await upgradeCreatorStagingCopy({ sourcePath, stagingPath }), { fromVersion: version, toVersion: 90 });
      const current = new DatabaseSync(stagingPath, { readOnly: true });
      try {
        validateCreatorSchema90(current);
        assert.equal(current.prepare("SELECT rowid FROM discovery_campaigns WHERE id='campaign'").get()!.rowid, 123);
        if (version < 65) {
          assert.deepEqual(current.prepare("SELECT policy FROM discovery_recommendation_policies").all().map((row) => row.policy), ["legacy"]);
          assert.equal(current.prepare("SELECT count(*) AS count FROM discovery_contact_observations").get()!.count, 0);
        }
        assert.deepEqual(current.prepare("SELECT * FROM creators").get(), creator);
        assert.deepEqual(current.prepare("SELECT * FROM mailbox_refs").get(), mailbox);
        assert.deepEqual(current.prepare("SELECT * FROM idempotency_records ORDER BY key").all(), claims);
        for (const [index, table] of preservedTables.entries()) assert.deepEqual(current.prepare(`SELECT * FROM ${table}`).all().map((row) => ({ ...row })), preservedRows[index]);
        if (version < 76) assert.equal(current.prepare("SELECT count(*) AS count FROM discovery_web_source_reads").get()!.count, 0);
        if (version === 38) {
          assert.deepEqual(current.prepare("SELECT search_hit_id,completed_at FROM discovery_video_completions").all().map((row) => ({ ...row })), [{ search_hit_id: "legacy-video", completed_at: 2 }]);
          assert.equal(current.prepare("SELECT completed_at FROM discovery_search_completions WHERE platform='youtube' AND search_keyword='legacy query'").get()!.completed_at, 3);
        }
        if (version === 86) assert.deepEqual(current.prepare("SELECT idempotency_key FROM gmail_send_attempts ORDER BY idempotency_key").all().map((row) => row.idempotency_key),
          ["attempt-key", "claim-completed", "claim-remote_accepted", "claim-remote_verified"]);
      } finally { current.close(); }
      assert.deepEqual(await readFile(sourcePath), sourceBytes);
      } finally { if (!stagedClosed) staged.close(); }
    } finally { await rm(directory, { recursive: true }); }
  });
}

test("Creator staging validation failure rolls back and preserves both files", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-failure-", import.meta.url)));
  try {
    const sourcePath = join(oracleDirectory, "creator-schema89-oracle.sqlite");
    const sourceBytes = await readFile(sourcePath);
    const stagingPath = join(directory, "private staging", "creator.sqlite");
    await backupLegacyDatabase(sourcePath, stagingPath, "creator");
    const staged = new DatabaseSync(stagingPath);
    staged.prepare("INSERT INTO creators(id,display_name,created_at,updated_at,tags_json) VALUES('bad','test',1,1,?)").run(JSON.stringify([" duplicate "]));
    staged.close();
    const before = await readFile(stagingPath);
    await assert.rejects(upgradeCreatorStagingCopy({ sourcePath, stagingPath }), /tags/);
    assert.deepEqual(await readFile(stagingPath), before);
    assert.deepEqual(await readFile(sourcePath), sourceBytes);
    const unchanged = new DatabaseSync(stagingPath, { readOnly: true });
    try { assert.equal(unchanged.prepare("PRAGMA user_version").get()!.user_version, 89); }
    finally { unchanged.close(); }
  } finally { await rm(directory, { recursive: true }); }
});

test("Creator v27 late mailbox constraint failure rolls back all previous migration edges", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-rollback-", import.meta.url)));
  try {
    const sourcePath = join(oracleDirectory, "creator-schema27-oracle.sqlite");
    const sourceBytes = await readFile(sourcePath);
    const stagingPath = join(directory, "private staging", "creator.sqlite");
    await backupLegacyDatabase(sourcePath, stagingPath, "creator");
    const staged = new DatabaseSync(stagingPath);
    try {
      staged.exec("INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES('historical','resend','','synthetic@example.com','[]',1,1,2)");
      validateCreatorSchemaVersion(staged, 27);
    } finally { staged.close(); }
    const before = await readFile(stagingPath);
    await assert.rejects(upgradeCreatorStagingCopy({ sourcePath, stagingPath }), /CHECK constraint failed/);
    assert.deepEqual(await readFile(stagingPath), before);
    assert.deepEqual(await readFile(sourcePath), sourceBytes);
    const unchanged = new DatabaseSync(stagingPath, { readOnly: true });
    try {
      validateCreatorSchemaVersion(unchanged, 27);
      assert.equal(unchanged.prepare("SELECT count(*) AS count FROM sqlite_schema WHERE name IN ('creator_groups','email_auth_connections')").get()!.count, 0);
      assert.equal(unchanged.prepare("SELECT label FROM mailbox_refs WHERE id='historical'").get()!.label, "");
    } finally { unchanged.close(); }
  } finally { await rm(directory, { recursive: true }); }
});

test("Creator v35 ambiguous historical operator attribution rejects without modifying either file", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-ambiguous-", import.meta.url)));
  try {
    const sourcePath = join(oracleDirectory, "creator-schema35-oracle.sqlite");
    const sourceBytes = await readFile(sourcePath);
    const stagingPath = join(directory, "private staging", "creator.sqlite");
    await backupLegacyDatabase(sourcePath, stagingPath, "creator");
    const staged = new DatabaseSync(stagingPath);
    try {
      staged.exec("INSERT INTO creators(id,display_name,created_at,updated_at,tags_json) VALUES('creator','history',1,2,'[]')");
      staged.exec("INSERT INTO campaigns(id,name,status,goal,created_at,updated_at) VALUES('campaign','history','archived','history',1,2)");
      staged.exec("INSERT INTO platform_accounts(id,platform,target_kind,external_id,created_at,updated_at) VALUES('account','x','user','123',1,2)");
      staged.exec("INSERT INTO creator_account_links(id,creator_id,platform_account_id,state,confidence,created_at,updated_at) VALUES('link','creator','account','confirmed','exact',1,2)");
      for (const operator of ["operator-a", "operator-b"]) {
        staged.prepare("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES(?,'x','profile','history',1,2)").run(operator);
        const origin = { kind: "campaign_discovery", discovery_id: "discovery", campaign_id: "campaign", account_ref_id: operator, query: "history", search_kind: "users" };
        const payload = { candidate_id: operator, origin, account: { platform: "x", target_kind: "user", external_id: "123" } };
        staged.prepare("INSERT INTO creator_candidate_snapshots(discovery_id,candidate_id,origin_kind,origin_json,candidate_json,account_platform,account_target_kind,account_external_id,account_display_name,source_kind,campaign_id,account_ref_id,created_at,updated_at) VALUES('discovery',?,'campaign_discovery',?,?,'x','user','123','author','campaign_discovery','campaign',?,1,2)").run(operator, JSON.stringify(origin), JSON.stringify(payload), operator);
      }
      staged.exec("INSERT INTO observation_sources(id,kind,observed_at) VALUES('source','initial_platform_account',1)");
      staged.exec("INSERT INTO creator_origins(id,kind,creator_id,source_id,discovery_id,campaign_id,discovered_at) VALUES('origin','campaign_discovery','creator','source','discovery','campaign',1)");
      validateCreatorSchemaVersion(staged, 35);
    } finally { staged.close(); }
    const before = await readFile(stagingPath);
    await assert.rejects(upgradeCreatorStagingCopy({ sourcePath, stagingPath }), /ambiguous operator accounts/);
    assert.deepEqual(await readFile(stagingPath), before);
    assert.deepEqual(await readFile(sourcePath), sourceBytes);
  } finally { await rm(directory, { recursive: true }); }
});

test("Creator v30 task origin retirement preserves external rows and clears retired reply attribution", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-origin-", import.meta.url)));
  try {
    const sourcePath = join(oracleDirectory, "creator-schema30-oracle.sqlite");
    const sourceBytes = await readFile(sourcePath);
    const stagingPath = join(directory, "private staging", "creator.sqlite");
    await backupLegacyDatabase(sourcePath, stagingPath, "creator");
    const db = new DatabaseSync(stagingPath);
    try {
      db.exec("PRAGMA foreign_keys=ON");
      db.exec("INSERT INTO tasks(id,instruction,schedule_json,status,created_by,created_at,updated_at,kind) VALUES('external-task','history','{}','completed','ui',1,2,'custom'),('internal-task','history','{}','completed','ui',1,2,'custom')");
      db.exec("INSERT INTO task_runs(id,task_id,scheduled_for,state,idempotency_key,claimed_at,lease_expires_at,finished_at,origin,chat_session_id) VALUES('external','external-task',1,'expired','external-key',1,2,2,'external',NULL),('internal','internal-task',1,'expired','internal-key',1,2,2,'envoy','historical-session')");
      db.exec("INSERT INTO task_attention(id,task_id,kind,task_run_id,occurrence_key,occurred_at) VALUES('attention','internal-task','run_failed','internal','internal-occurrence',2)");
      db.exec("INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES('mailbox','resend','history','synthetic@example.com','[]',1,1,2)");
      db.exec("INSERT INTO email_threads(id,mailbox_ref_id,provider_thread_key,subject,active_folder,created_at,updated_at) VALUES('thread','mailbox','history','history','inbox',1,2)");
      db.exec("INSERT INTO emails(id,thread_id,provider_message_id,direction,subject,body_text,delivery_status,occurred_at,origin) VALUES('email','thread','history','inbound','history','保留邮件','received',1,'provider_import')");
      db.exec("INSERT INTO reply_triage(id,channel,email_id,intent,summary,task_run_id,created_at) VALUES('triage','email','email','other','保留回复','internal',1)");
      const external = db.prepare("SELECT * FROM task_runs WHERE id='external'").get();
      validateCreatorSchemaVersion(db, 30);
      db.exec("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE");
      migrateCreatorTaskOriginEdge(db);
      db.exec("PRAGMA user_version=31");
      validateCreatorSchemaVersion(db, 31);
      db.exec("COMMIT; PRAGMA foreign_keys=ON");
      assert.deepEqual(db.prepare("SELECT * FROM task_runs WHERE id='external'").get(), external);
      assert.equal(db.prepare("SELECT count(*) AS count FROM task_runs WHERE id='internal'").get()!.count, 0);
      assert.equal(db.prepare("SELECT count(*) AS count FROM task_attention").get()!.count, 0);
      assert.equal(db.prepare("SELECT task_run_id FROM reply_triage WHERE id='triage'").get()!.task_run_id, null);
      assert.equal(db.prepare("SELECT summary FROM reply_triage WHERE id='triage'").get()!.summary, "保留回复");
    } finally { if (db.isTransaction) db.exec("ROLLBACK"); db.close(); }
    assert.deepEqual(await readFile(sourcePath), sourceBytes);
  } finally { await rm(directory, { recursive: true }); }
});

test("Legacy mailbox SQLite storage types fail without coercing historical values", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-types-", import.meta.url)));
  try {
    const sourcePath = join(oracleDirectory, "creator-schema31-oracle.sqlite");
    const sourceBytes = await readFile(sourcePath);
    const stagingPath = join(directory, "private staging", "creator.sqlite");
    await backupLegacyDatabase(sourcePath, stagingPath, "creator");
    const db = new DatabaseSync(stagingPath);
    try {
      db.exec("INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES('mailbox','resend','history','synthetic@example.com','[]','invalid',1,2)");
      assert.equal(db.prepare("SELECT typeof(available) AS storage_type FROM mailbox_refs").get()!.storage_type, "text");
      validateCreatorSchemaVersion(db, 31);
    } finally { db.close(); }
    const before = await readFile(stagingPath);
    await assert.rejects(upgradeCreatorStagingCopy({ sourcePath, stagingPath }), /invalid SQLite type/);
    assert.deepEqual(await readFile(stagingPath), before);
    assert.deepEqual(await readFile(sourcePath), sourceBytes);
  } finally { await rm(directory, { recursive: true }); }
});
