import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { discoveryContentClaim } from "./review-material.ts";
import { validateDiscoveryContentResult, type DiscoveryContentClaim } from "./content-result.ts";
import { loadDiscoveryContentReceipt, loadDiscoveryProfileReceipt } from "./review-receipts.ts";
import { assertDiscoveryReadHealthy, discoveryReadExcludedHits } from "./read-health.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";
import { validateDiscoveryRetainedProfile } from "./native-material.ts";

const text = z.string(), u32 = z.number().int().min(0).max(4294967295), object = z.record(text, z.unknown());
export class DiscoveryContentRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #transaction<T>(body: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  #twitterIdentity(claim: DiscoveryContentClaim): string {
    const saved = this.#db.prepare("SELECT status,expected_revision,profile_ref,author_handle,source_url,call_number,account_id FROM discovery_identity_reads WHERE discovery_campaign_id=? AND search_hit_id=?").get(claim.campaign_id, claim.search_hit_id);
    if (!saved || saved.status !== "completed" || saved.expected_revision !== claim.expected_revision || saved.profile_ref !== claim.profile_ref || saved.author_handle !== claim.author_id || saved.source_url !== claim.source_url || u32.parse(saved.call_number) >= claim.call_number || typeof saved.account_id !== "string") throw new Error("discovery_content_original_identity_missing");
    const matched = this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_profile_reads p JOIN discovery_profile_identity_matches m ON m.search_hit_id=p.search_hit_id
      WHERE p.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.platform='twitter' AND p.profile_ref=?3 AND p.author_id=?4 AND p.expected_revision=?5 AND p.status='completed' AND m.identity_search_hit_id=?1 AND m.account_id=?6 AND p.call_number>?7 AND p.call_number<?8) matched`).get(claim.search_hit_id, claim.campaign_id, claim.profile_ref, claim.author_id, claim.expected_revision, saved.account_id, u32.parse(saved.call_number), claim.call_number);
    if (matched?.matched !== 1) throw new Error("discovery_content_native_profile_match_missing"); return saved.account_id;
  }
  #binding(claim: DiscoveryContentClaim): string {
    const row = this.#db.prepare("SELECT account_id FROM discovery_content_identity_bindings WHERE search_hit_id=?1 AND identity_search_hit_id=?1 AND profile_search_hit_id=?1").get(claim.search_hit_id);
    if (!row) throw new Error("discovery_content_reserved_identity_missing"); return text.parse(row.account_id);
  }
  nativeIdentity(raw: DiscoveryContentClaim): string {
    const claim = discoveryContentClaim.parse(raw);
    return this.#transaction(() => { const identity = this.#twitterIdentity(claim); if (this.#binding(claim) !== identity) throw new Error("discovery_content_native_binding_changed"); return identity; });
  }
  nextNative(campaign: string, platform: string, held: readonly string[] = []): string | null {
    const excluded = [...held, ...discoveryReadExcludedHits(this.#db, campaign, "content")];
    const row = this.#db.prepare(`SELECT h.id FROM discovery_search_hits h JOIN discovery_search_authors a ON a.search_hit_id=h.id
      JOIN discovery_profile_reads p ON p.discovery_campaign_id=h.discovery_campaign_id AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=a.author_id
      JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      LEFT JOIN discovery_content_reads content ON content.search_hit_id=h.id
      WHERE c.id=?1 AND c.phase='active' AND(c.max_calls IS NULL OR c.used_calls<c.max_calls) AND h.platform=?2 AND h.id NOT IN(SELECT value FROM json_each(?3)) AND p.status='completed' AND p.expected_revision=c.current_revision
      AND(h.platform!='instagram' OR COALESCE(json_extract(p.native_profile_json,'$.private_account'),0)!=1)
      AND NOT EXISTS(SELECT 1 FROM discovery_candidates settled WHERE settled.discovery_campaign_id=c.id AND settled.criteria_revision=c.current_revision
        AND settled.source_profile_ref=h.profile_ref AND settled.platform=h.platform AND settled.verdict IN ('qualified','rejected')
        AND (settled.platform_account_id=a.author_id OR settled.platform_account_id=(SELECT account_id FROM discovery_profile_identity_matches WHERE search_hit_id=h.id)
          OR h.platform='linkedin' AND settled.platform_account_id=json_extract(p.native_profile_json,'$.account_id')))
      AND(h.platform!='linkedin' OR(p.search_hit_id=h.id AND p.stop_code IS NULL AND (
        content.status='reading'
        OR EXISTS(SELECT 1 FROM discovery_campaign_revisions v WHERE v.discovery_campaign_id=c.id AND v.revision=c.current_revision AND length(trim(json_extract(v.criteria_json,'$.language')))>0)
        OR EXISTS(SELECT 1 FROM discovery_candidates pending JOIN discovery_evidence_reviews reviewed
          ON reviewed.id=COALESCE(json_extract(pending.evidence_json,'$.reviewed_selection.review_id'),json_extract(pending.evidence_json,'$.reviewed_match.review_id')) AND reviewed.discovery_campaign_id=c.id
          AND reviewed.criteria_revision=c.current_revision AND reviewed.search_hit_id=h.id AND reviewed.candidate_id=pending.id
          AND reviewed.native_author_id=pending.platform_account_id AND reviewed.profile_ref=h.profile_ref
          AND json_extract(reviewed.material_json,'$.native_profile.observed_at')=p.updated_at
          WHERE pending.discovery_campaign_id=c.id AND pending.criteria_revision=c.current_revision AND pending.platform='linkedin'
            AND pending.source_profile_ref=h.profile_ref AND pending.platform_account_id=json_extract(p.native_profile_json,'$.account_id') AND pending.verdict='pending'
            AND(COALESCE(json_extract(pending.evidence_json,'$.reviewed_selection.outcome'),json_extract(pending.evidence_json,'$.reviewed_match.outcome'))='insufficient'
              OR EXISTS(SELECT 1 FROM json_each(pending.evidence_json,'$.reviewed_requirements') requirement WHERE json_extract(requirement.value,'$.outcome')='insufficient'))))))
      AND(h.platform!='twitter' OR(p.search_hit_id=h.id AND EXISTS(SELECT 1 FROM discovery_profile_identity_matches m JOIN discovery_identity_reads i ON i.search_hit_id=m.identity_search_hit_id WHERE m.search_hit_id=h.id AND i.status='completed' AND i.account_id=m.account_id AND i.expected_revision=c.current_revision AND i.profile_ref=h.profile_ref AND i.discovery_campaign_id=c.id AND i.author_handle=a.author_id AND i.source_url=h.source_url AND i.call_number<p.call_number)))
      AND h.id=(SELECT h2.id FROM discovery_search_hits h2 JOIN discovery_search_authors a2 ON a2.search_hit_id=h2.id WHERE h2.discovery_campaign_id=h.discovery_campaign_id AND h2.platform=h.platform AND h2.profile_ref=h.profile_ref AND a2.author_id=a.author_id ORDER BY h2.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h2.id),9223372036854775807),h2.id LIMIT 1)
      AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs busy WHERE busy.discovery_campaign_id=c.id AND busy.status='searching')
      AND(h.platform!='facebook' OR EXISTS(SELECT 1 FROM discovery_facebook_content_targets t WHERE t.search_hit_id=h.id AND t.discovery_campaign_id=c.id AND t.expected_revision=c.current_revision AND t.profile_ref=h.profile_ref AND t.author_id=a.author_id AND t.profile_url=a.author_url AND t.status='completed' AND(json_extract(p.native_profile_json,'$.adapter_version')!='2.0.0' OR EXISTS(SELECT 1 FROM json_each(t.source_urls_json) l WHERE NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.source_url=l.value)))))
      AND NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.expected_revision=c.current_revision AND f.status='failed' AND f.stop_code IN('opencli_invalid_output','opencli_output_too_large'))
      AND((r.status='completed' AND(content.search_hit_id IS NULL OR(h.platform='facebook' AND content.status='failed' AND json_extract(p.native_profile_json,'$.adapter_version')='2.0.0' AND EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.call_number=content.call_number AND f.status='failed' AND f.stop_code IN('opencli_command_failed','opencli_timeout','interrupted_content_read')))))
        OR(r.status='technical_stop' AND r.stop_code='interrupted_content_read' AND NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.status='reading') AND content.status='reading' AND content.expected_revision=c.current_revision AND(h.platform!='twitter' OR EXISTS(SELECT 1 FROM discovery_content_identity_bindings b WHERE b.search_hit_id=h.id)) AND r.active_query='content:'||h.id||':'||content.call_number))
      ORDER BY CASE WHEN content.status='reading' THEN 0 ELSE 1 END,h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1`).get(campaign, platform, stringifyApplicationJson(excluded));
    return row === undefined ? null : text.parse(row.id);
  }
  claim(campaign: string, revision: number, hit: string): DiscoveryContentClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      assertDiscoveryReadHealthy(this.#db, campaign, "content", hit);
      const head = this.#db.prepare("SELECT c.used_calls,c.max_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND v.approved_at IS NOT NULL").get(campaign, revision);
      if (!head) throw new Error("discovery_content_not_approved"); const budget = z.object({ used_calls: u32, max_calls: u32.nullable() }).parse(head);
      if (budget.max_calls !== null && budget.used_calls >= budget.max_calls) throw new Error("discovery_budget_exhausted");
      if (this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=? AND status='searching') busy").get(campaign)!.busy === 1) throw new Error("discovery_read_active");
      const row = this.#db.prepare(`SELECT h.platform,h.profile_ref,a.author_id,h.source_url FROM discovery_search_hits h JOIN discovery_search_authors a ON a.search_hit_id=h.id
        JOIN discovery_profile_reads p ON p.discovery_campaign_id=h.discovery_campaign_id AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=a.author_id AND p.status='completed' AND p.expected_revision=?3
        WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform IN('bilibili','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pixiv','pinterest','linkedin')
        AND(h.platform!='instagram' OR COALESCE(json_extract(p.native_profile_json,'$.private_account'),0)!=1)
        AND h.id=(SELECT h2.id FROM discovery_search_hits h2 JOIN discovery_search_authors a2 ON a2.search_hit_id=h2.id WHERE h2.discovery_campaign_id=h.discovery_campaign_id AND h2.platform=h.platform AND h2.profile_ref=h.profile_ref AND a2.author_id=a.author_id ORDER BY h2.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h2.id),9223372036854775807),h2.id LIMIT 1)`).get(hit, campaign, revision);
      if (!row) throw new Error("discovery_content_profile_observation_missing");
      const claim = discoveryContentClaim.parse({ ...row, campaign_id: campaign, expected_revision: revision, search_hit_id: hit, call_number: 0 }); let nativeFacebook = false;
      if (claim.platform === "facebook") {
        const target = this.#db.prepare("SELECT t.*,h.source_url FROM discovery_facebook_content_targets t JOIN discovery_search_hits h ON h.id=t.search_hit_id WHERE t.discovery_campaign_id=? AND t.search_hit_id=? AND h.discovery_campaign_id=t.discovery_campaign_id AND h.platform=t.platform AND h.profile_ref=t.profile_ref").get(campaign, hit);
        if (!target || target.status !== "completed" || target.stop_code !== null || target.expected_revision !== revision || target.profile_ref !== claim.profile_ref || target.author_id !== claim.author_id || target.source_url !== claim.source_url) throw new Error("discovery_content_facebook_targets_missing");
        const savedProfile = loadDiscoveryProfileReceipt(this.#db, campaign, hit); nativeFacebook = object.parse(savedProfile.native_profile).adapter_version === "2.0.0";
        if (nativeFacebook && this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts WHERE search_hit_id=? AND expected_revision=? AND status='failed' AND stop_code IN('opencli_invalid_output','opencli_output_too_large')) failed").get(hit, revision)!.failed === 1) throw new Error("discovery_content_facebook_structural_stop");
        const urls = z.array(text).nullable().parse(target.source_urls_json === null ? null : parseApplicationJson(Buffer.from(text.parse(target.source_urls_json))));
        const source = nativeFacebook ? this.#db.prepare("SELECT l.value FROM discovery_facebook_content_targets t JOIN json_each(t.source_urls_json) l WHERE t.search_hit_id=?1 AND NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=t.search_hit_id AND f.source_url=l.value) ORDER BY l.key LIMIT 1").get(hit)?.value : urls?.[0];
        if (typeof source !== "string") throw new Error("discovery_content_facebook_targets_exhausted"); claim.source_url = source;
      }
      const run = z.object({ status: text, stop_code: text.nullable(), active_query: text.nullable() }).parse(this.#db.prepare("SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").get(campaign, claim.platform, claim.profile_ref));
      const prior = this.#db.prepare("SELECT call_number,status,expected_revision FROM discovery_content_reads WHERE search_hit_id=? AND discovery_campaign_id=?").get(hit, campaign);
      const interrupted = prior !== undefined && prior.status === "reading" && prior.expected_revision === revision && run.status === "technical_stop" && run.stop_code === "interrupted_content_read" && run.active_query === `content:${hit}:${u32.parse(prior.call_number)}`;
      const nativeRetry = nativeFacebook && run.status === "completed" && this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_content_reads c JOIN discovery_facebook_original_attempts f ON f.search_hit_id=c.search_hit_id AND f.call_number=c.call_number AND f.source_url=c.source_url WHERE c.search_hit_id=? AND c.status='failed' AND f.status='failed' AND f.stop_code IN('opencli_command_failed','opencli_timeout','interrupted_content_read')) retry`).get(hit)!.retry === 1;
      if (nativeFacebook && interrupted) throw new Error("discovery_content_facebook_interrupted_no_replay");
      if (!interrupted && (run.status !== "completed" || prior !== undefined && !nativeRetry)) throw new Error("discovery_content_already_attempted_or_stopped");
      claim.call_number = u32.parse(budget.used_calls + 1);
      const nativeAccount = claim.platform === "twitter" ? this.#twitterIdentity(claim) : null;
      if (nativeAccount !== null && interrupted && this.#binding(claim) !== nativeAccount) throw new Error("discovery_content_native_binding_changed");
      const now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(claim.call_number, now, campaign).changes !== 1) throw new Error("discovery_content_campaign_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").run(`content:${hit}:${claim.call_number}`, now, campaign, claim.platform, claim.profile_ref).changes !== 1) throw new Error("discovery_content_run_changed");
      this.#db.prepare("INSERT INTO discovery_content_reads VALUES(?,?,?,?,?,?,?,?,'reading',NULL,NULL,?) ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at,source_url=excluded.source_url,status='reading',native_content_json=NULL,stop_code=NULL").run(hit, campaign, claim.platform, claim.profile_ref, claim.author_id, claim.source_url, revision, claim.call_number, now);
      if (nativeFacebook) this.#db.prepare("INSERT INTO discovery_facebook_original_attempts VALUES(?,?,?,?,?,?,?,'reading',NULL,NULL,?)").run(hit, campaign, claim.profile_ref, claim.author_id, claim.source_url, revision, claim.call_number, now);
      if (nativeAccount !== null && !interrupted) this.#db.prepare("INSERT INTO discovery_content_identity_bindings VALUES(?1,?1,?1,?2,?3)").run(hit, nativeAccount, now);
      return claim;
    });
  }
  finish(rawClaim: DiscoveryContentClaim, raw: unknown, stop: string | null) {
    const claim = discoveryContentClaim.parse(rawClaim), native = validateDiscoveryContentResult(claim, raw, stop);
    return this.#transaction(() => {
      if (claim.platform === "facebook") {
        const saved = this.#db.prepare("SELECT status,native_content_json,stop_code,updated_at FROM discovery_facebook_original_attempts WHERE search_hit_id=? AND discovery_campaign_id=? AND expected_revision=? AND profile_ref=? AND author_id=? AND source_url=? AND call_number=? AND status!='reading'").get(claim.search_hit_id, claim.campaign_id, claim.expected_revision, claim.profile_ref, claim.author_id, claim.source_url, claim.call_number);
        if (saved) { const previous: unknown = saved.native_content_json === null ? null : parseApplicationJson(Buffer.from(text.parse(saved.native_content_json))); if (!isDeepStrictEqual(previous, native) || saved.stop_code !== stop) throw new Error("discovery_content_facebook_outcome_changed"); return { claim, status: text.parse(saved.status), native_content: previous, stop_code: stop, captured_at: z.number().int().safe().parse(saved.updated_at) }; }
      }
      const prior = loadDiscoveryContentReceipt(this.#db, claim.campaign_id, claim.search_hit_id);
      if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("discovery_content_claim_stale");
      if (native !== null && ["pinterest", "linkedin"].includes(claim.platform)) {
        const profile = loadDiscoveryProfileReceipt(this.#db, claim.campaign_id, claim.search_hit_id);
        if (profile.status !== "completed" || profile.stop_code !== null || profile.claim.expected_revision !== claim.expected_revision || profile.claim.platform !== claim.platform || profile.claim.profile_ref !== claim.profile_ref || profile.claim.author_id !== claim.author_id || profile.claim.call_number >= claim.call_number) throw new Error("discovery_content_profile_identity_changed");
        if (claim.platform === "pinterest" && (profile.claim.source_url !== claim.source_url || native.profile_url !== profile.claim.author_url)) throw new Error("discovery_content_profile_identity_changed");
        if (claim.platform === "linkedin") { const original = validateDiscoveryRetainedProfile("linkedin", profile.claim.author_id, profile.claim.author_url, profile.claim.source_url, profile.native_profile); if (profile.claim.author_url !== claim.source_url || !isDeepStrictEqual(original.account_id, native.account_id)) throw new Error("discovery_content_profile_identity_changed"); }
      }
      if (claim.platform === "twitter") { const expected = this.#twitterIdentity(claim); if (this.#binding(claim) !== expected || native !== null && object.parse(native.identity).account_id !== expected) throw new Error("discovery_content_native_binding_changed"); }
      const now = Math.floor(Date.now() / 1000), runStop = sourceLocalReadFailure(stop) ? null : stop;
      if (this.#db.prepare(`UPDATE discovery_platform_runs SET status=?5,active_query=NULL,stop_code=?6,updated_at=?7 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND status='searching' AND active_query=?4 AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?8 AND v.approved_at IS NOT NULL)`).run(claim.campaign_id, claim.platform, claim.profile_ref, `content:${claim.search_hit_id}:${claim.call_number}`, discoveryReadPlatformStatus(stop), runStop, now, claim.expected_revision).changes !== 1) throw new Error("discovery_content_claim_stale");
      const json = native === null ? null : stringifyApplicationJson(native), status = stop === null ? "completed" : "failed";
      if (this.#db.prepare("UPDATE discovery_content_reads SET status=?3,native_content_json=?4,stop_code=?5,updated_at=?6 WHERE search_hit_id=?1 AND call_number=?2 AND status='reading'").run(claim.search_hit_id, claim.call_number, status, json, stop, now).changes !== 1) throw new Error("discovery_content_receipt_stale");
      if (claim.platform === "facebook") this.#db.prepare("UPDATE discovery_facebook_original_attempts SET status=?3,native_content_json=?4,stop_code=?5,updated_at=?6 WHERE search_hit_id=?1 AND call_number=?2 AND source_url=?7 AND status='reading'").run(claim.search_hit_id, claim.call_number, status, json, stop, now, claim.source_url);
      return loadDiscoveryContentReceipt(this.#db, claim.campaign_id, claim.search_hit_id);
    });
  }
  interrupt(raw: DiscoveryContentClaim): void {
    const claim = discoveryContentClaim.parse(raw);
    this.#transaction(() => {
      const now = Math.floor(Date.now() / 1000), token = `content:${claim.search_hit_id}:${claim.call_number}`;
      const nativeFacebook = claim.platform === "facebook" && this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts WHERE search_hit_id=? AND call_number=? AND source_url=? AND status='reading') active").get(claim.search_hit_id, claim.call_number, claim.source_url)!.active === 1;
      if (nativeFacebook) {
        if (this.#db.prepare("UPDATE discovery_content_reads SET status='failed',native_content_json=NULL,stop_code='interrupted_content_read',updated_at=? WHERE search_hit_id=? AND discovery_campaign_id=? AND call_number=? AND source_url=? AND status='reading'").run(now, claim.search_hit_id, claim.campaign_id, claim.call_number, claim.source_url).changes !== 1) throw new Error("discovery_content_claim_stale");
        if (this.#db.prepare("UPDATE discovery_facebook_original_attempts SET status='failed',stop_code='interrupted_content_read',updated_at=? WHERE search_hit_id=? AND call_number=? AND source_url=? AND status='reading'").run(now, claim.search_hit_id, claim.call_number, claim.source_url).changes !== 1) throw new Error("discovery_content_claim_stale");
      }
      const status = nativeFacebook ? "completed" : "technical_stop", stop = nativeFacebook ? null : "interrupted_content_read";
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status=?,stop_code=?,active_query=?,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND status='searching' AND active_query=?").run(status, stop, nativeFacebook ? null : token, now, claim.campaign_id, claim.platform, claim.profile_ref, token).changes !== 1) throw new Error("discovery_content_claim_stale");
    });
  }
}
