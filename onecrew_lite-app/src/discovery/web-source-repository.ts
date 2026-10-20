import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { assessDiscoveryProfile, assessDiscoveryYoutube, assessedDiscoveryEvidence } from "./qualification.ts";
import { discoveryJsonValue } from "./library-material.ts";
import { discoveryReadPlatformStatus } from "./stops.ts";
import { discoveryWebSourceClaim, discoveryWebSourceRead, discoveryWebSourceTarget, validateDiscoveryWebSourceResult, type DiscoveryWebSourceClaim } from "./web-source-contracts.ts";
const text = z.string().refine(value => value.isWellFormed());
const object = (value: unknown) => z.record(z.string(), z.unknown()).parse(value);
const eligible = `FROM discovery_search_hits h
 JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
 JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
 JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
 WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL
 AND (h.platform IN ('google','yahoo') OR (h.platform='instagram' AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=h.id))) AND r.status='completed'
 AND (c.max_calls IS NULL OR c.used_calls<c.max_calls)
 AND (EXISTS(SELECT 1 FROM json_each(v.scope_json) s WHERE json_extract(s.value,'$.platform')=h.platform
   AND json_extract(s.value,'$.profile_ref')=h.profile_ref AND EXISTS(SELECT 1 FROM json_each(s.value,'$.keywords') k WHERE k.value=h.search_keyword))
 OR EXISTS(SELECT 1 FROM discovery_query_rounds q, json_each(q.scopes_json) s WHERE q.discovery_campaign_id=c.id AND q.revision=c.current_revision
   AND json_extract(s.value,'$.platform')=h.platform AND json_extract(s.value,'$.profile_ref')=h.profile_ref
   AND EXISTS(SELECT 1 FROM json_each(s.value,'$.keywords') k WHERE k.value=h.search_keyword)))
 AND NOT EXISTS(SELECT 1 FROM discovery_web_source_reads p WHERE p.search_hit_id=h.id)
 AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs busy WHERE busy.discovery_campaign_id=c.id AND busy.status='searching')`;

export class DiscoveryWebSourceRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #transaction<T>(work: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const result = work(); this.#db.exec("COMMIT"); committed = true; return result; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  #targetReady(campaign: string, hit: string, target: string, cost: number): boolean {
    return this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_search_hits h JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      WHERE c.id=?1 AND h.id=?2 AND (c.max_calls IS NULL OR c.used_calls+?4<=c.max_calls)
      AND NOT EXISTS(SELECT 1 FROM discovery_web_source_reads p WHERE p.discovery_campaign_id=c.id AND p.profile_ref=h.profile_ref AND p.target_platform=?3 AND p.stop_code IN ('opencli_auth_required','opencli_risk_control'))
      AND NOT EXISTS(SELECT 1 FROM discovery_language_samples stopped JOIN discovery_search_hits sh ON sh.id=stopped.search_hit_id
        JOIN discovery_candidates sc ON sc.id=stopped.candidate_id WHERE stopped.discovery_campaign_id=c.id AND sh.profile_ref=h.profile_ref
        AND sc.platform=?3 AND stopped.stop_code IN ('opencli_auth_required','opencli_risk_control'))
      AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=c.id AND r.profile_ref=h.profile_ref AND r.platform=?3 AND r.status IN ('unauthenticated','risk_control_stop'))) ready`).get(campaign, hit, target, cost)!.ready === 1;
  }
  next(campaign: string, excluded: readonly string[] = [], held: readonly string[] = []): string | null {
    for (const row of this.#db.prepare(`SELECT h.id,h.platform,h.source_url ${eligible} ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id`).all(campaign)) {
      const id = text.parse(row.id), target = discoveryWebSourceTarget(text.parse(row.source_url));
      if (target && !held.includes(id) && !excluded.includes(text.parse(row.platform)) && !excluded.includes(target.platform) && this.#targetReady(campaign, id, target.platform, target.cost)) return id;
    }
    return null;
  }
  source(campaign: string, hit: string) {
    return z.object({ platform: text, profile_ref: text, source_url: text }).parse(this.#db.prepare("SELECT platform,profile_ref,source_url FROM discovery_search_hits WHERE discovery_campaign_id=? AND id=?").get(campaign, hit));
  }
  claim(campaign: string, revision: number, hit: string): DiscoveryWebSourceClaim {
    return this.#transaction(() => {
      const row = this.#db.prepare(`SELECT h.platform,h.profile_ref,h.source_url,c.used_calls ${eligible} AND c.current_revision=?2 AND h.id=?3`).get(campaign, revision, hit);
      if (!row) throw new Error("web_source_outside_approval_attempted_or_stopped");
      const target = discoveryWebSourceTarget(text.parse(row.source_url));
      if (!target) throw new Error("web_source_target_unsupported");
      if (!this.#targetReady(campaign, hit, target.platform, target.cost)) throw new Error("web_source_target_stopped_or_budget_exhausted");
      const claim = discoveryWebSourceClaim.parse({ campaign_id: campaign, expected_revision: revision, search_hit_id: hit, origin_platform: row.platform, profile_ref: row.profile_ref, source_url: row.source_url, target_platform: target.platform, video_id: target.id, call_number: z.number().int().nonnegative().parse(row.used_calls) + target.cost }), now = Math.floor(Date.now() / 1000);
      this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1").run(campaign, claim.call_number, now);
      this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?4,stop_code=NULL,updated_at=?5 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3").run(campaign, claim.origin_platform, claim.profile_ref, `web_source:${hit}:${claim.call_number}`, now);
      this.#db.prepare("INSERT INTO discovery_web_source_reads(search_hit_id,discovery_campaign_id,origin_platform,profile_ref,source_url,expected_revision,call_number,target_platform,video_id,status,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?10,?8,'reading',?9)").run(hit, campaign, claim.origin_platform, claim.profile_ref, claim.source_url, revision, claim.call_number, claim.video_id, now, claim.target_platform);
      return claim;
    });
  }
  get(campaign: string, hit: string) {
    const statement = this.#db.prepare(`SELECT p.expected_revision,p.origin_platform,p.profile_ref,p.source_url,p.target_platform,p.video_id,p.call_number,
      p.status,p.native_author_id,p.native_material_json,p.stop_code,p.updated_at,b.candidate_id,
      COALESCE(c.source_platform=p.origin_platform AND c.source_profile_ref=p.profile_ref AND c.criteria_revision=p.expected_revision AND json_extract(c.evidence_json,'$.source_urls[0]')=p.source_url,0) primary_source
      FROM discovery_web_source_reads p JOIN discovery_search_hits h ON h.id=p.search_hit_id AND h.discovery_campaign_id=p.discovery_campaign_id AND h.platform=p.origin_platform AND h.profile_ref=p.profile_ref AND h.source_url=p.source_url
      LEFT JOIN discovery_video_identity_bindings b ON b.search_hit_id=p.search_hit_id AND b.discovery_campaign_id=p.discovery_campaign_id AND b.criteria_revision=p.expected_revision AND b.native_author_id=p.native_author_id
      LEFT JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=p.discovery_campaign_id AND c.platform=p.target_platform AND c.platform_account_id=p.native_author_id
      WHERE p.discovery_campaign_id=?1 AND p.search_hit_id=?2`);
    statement.setReadBigInts(true);
    const raw = statement.get(campaign, hit);
    if (!raw) throw new Error("web_source_receipt_not_found");
    const row = Object.fromEntries(Object.entries(raw).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value]));
    return discoveryWebSourceRead.parse({ claim: { campaign_id: campaign, search_hit_id: hit, expected_revision: row.expected_revision, origin_platform: row.origin_platform, profile_ref: row.profile_ref, source_url: row.source_url, target_platform: row.target_platform, video_id: row.video_id, call_number: row.call_number }, status: row.status, native_author_id: row.native_author_id, native_material: row.native_material_json === null ? null : parseApplicationJson(Buffer.from(text.parse(row.native_material_json))), stop_code: row.stop_code, captured_at: row.updated_at, candidate_id: row.candidate_id, is_primary_source: row.primary_source === 1 });
  }
  finish(raw: DiscoveryWebSourceClaim, material: unknown, stop: string | null) {
    const claim = discoveryWebSourceClaim.parse(raw), native = validateDiscoveryWebSourceResult(claim, material, stop);
    return this.#transaction(() => {
      const prior = this.get(claim.campaign_id, claim.search_hit_id);
      if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("web_source_claim_stale");
      const now = Math.floor(Date.now() / 1000), nativeStop = stop === "opencli_auth_required" || stop === "opencli_risk_control", status = nativeStop ? "completed" : discoveryReadPlatformStatus(stop);
      if (this.#db.prepare(`UPDATE discovery_platform_runs SET status=?5,active_query=NULL,stop_code=?6,updated_at=?7 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND status='searching' AND active_query=?4
        AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=?1 AND c.current_revision=?8 AND c.phase='active' AND c.used_calls>=?9 AND v.approved_at IS NOT NULL)`).run(claim.campaign_id, claim.origin_platform, claim.profile_ref, `web_source:${claim.search_hit_id}:${claim.call_number}`, status, status === "completed" ? null : stop, now, claim.expected_revision, claim.call_number).changes !== 1) throw new Error("web_source_approval_or_reservation_changed");
      const author = native === null ? null : text.parse(claim.target_platform === "douyin" ? object(native.content).author_id : claim.target_platform === "twitter" ? object(native.identity).account_id : ["instagram", "tiktok"].includes(claim.target_platform) ? object(native.profile).id : native.channelId);
      this.#db.prepare("UPDATE discovery_web_source_reads SET status=?2,native_author_id=?3,native_material_json=?4,stop_code=?5,updated_at=?6 WHERE search_hit_id=?1").run(claim.search_hit_id, native === null ? "failed" : "completed", author, native === null ? null : stringifyApplicationJson(discoveryJsonValue(native)), stop, now);
      if (native !== null) this.#bindCandidate(claim, native, author!, now);
      return this.get(claim.campaign_id, claim.search_hit_id);
    });
  }
  #bindCandidate(claim: DiscoveryWebSourceClaim, native: Record<string, unknown>, author: string, now: number): void {
    const criteria = parseApplicationJson(Buffer.from(text.parse(this.#db.prepare("SELECT criteria_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=? AND revision=?").get(claim.campaign_id, claim.expected_revision)!.criteria_json)));
    const profile = claim.target_platform === "youtube" ? `https://www.youtube.com/channel/${author}` : text.parse(object(native.profile).profile_url);
    const profileTarget = ["instagram", "tiktok"].includes(claim.target_platform);
    const name = claim.target_platform === "douyin" ? object(native.profile).author_label : claim.target_platform === "twitter" ? object(native.profile).name : profileTarget ? object(native.profile).display_name : native.author;
    const evidence = { source_urls: [claim.source_url, profile], captured_at: now, subscribers_raw: native.subscribers ?? null, subscribers_source_url: claim.source_url, author_identity_kind: claim.target_platform === "instagram" ? "profile_url" : "native_id", web_origin: { search_hit_id: claim.search_hit_id, origin_platform: claim.origin_platform, target_platform: claim.target_platform, call_number: claim.call_number } };
    const nativeProfile = profileTarget ? object(native.profile) : {};
    if (profileTarget) Object.assign(evidence, { profile_bio_status: claim.target_platform === "tiktok" ? typeof nativeProfile.signature === "string" && nativeProfile.signature.trim() ? "complete" : "missing" : nativeProfile.bio_status ?? "unknown", profile_bio_status_source_url: profile });
    const assessment = claim.target_platform === "instagram" && nativeProfile.private_account === true ? { verdict: "rejected" as const, reasons: ["private_account"], subscribers: null } : profileTarget ? assessDiscoveryProfile(criteria, evidence, author, typeof nativeProfile.follower_count === "number" ? nativeProfile.follower_count : null) : assessDiscoveryYoutube(criteria, evidence, author), assessed = assessedDiscoveryEvidence(evidence, assessment);
    this.#db.prepare("INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform) VALUES(?1,?2,?3,?12,?4,?5,?6,?7,?8,?9,?10,?10,?11) ON CONFLICT(discovery_campaign_id,platform,platform_account_id) DO NOTHING").run(randomUUID(), claim.campaign_id, claim.expected_revision, claim.profile_ref, author, text.parse(name), profile, assessment.verdict, stringifyApplicationJson(discoveryJsonValue(assessed)), now, claim.origin_platform, claim.target_platform);
    this.#db.prepare("INSERT INTO discovery_video_completions(search_hit_id,completed_at) VALUES(?,?)").run(claim.search_hit_id, now);
    if (this.#db.prepare("INSERT INTO discovery_video_identity_bindings(search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at) SELECT ?1,discovery_campaign_id,criteria_revision,id,platform_account_id,?2 FROM discovery_candidates WHERE discovery_campaign_id=?3 AND criteria_revision=?4 AND platform=?6 AND platform_account_id=?5").run(claim.search_hit_id, now, claim.campaign_id, claim.expected_revision, author, claim.target_platform).changes !== 1) throw new Error("web_source_candidate_outside_revision");
  }
}
