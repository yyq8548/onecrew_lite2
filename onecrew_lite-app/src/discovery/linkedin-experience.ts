import type { DatabaseSync } from "node:sqlite";
import { discoveryReadBusySql } from "./read-capacity.ts";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { linkedinExperiencePlan } from "../platforms/opencli/linkedin.ts";
import { discoveryContentClaim } from "./review-material.ts";
import type { DiscoveryContentClaim } from "./content-result.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";

const text = z.string(), integer = z.number().int().nonnegative().safe();
export class DiscoveryLinkedinExperienceRepository {
  readonly database: DatabaseSync;
  constructor(database: DatabaseSync) { this.database = database; }
  #transaction<T>(body: () => T): T {
    this.database.exec("BEGIN IMMEDIATE"); let committed = false;
    try { const result = body(); this.database.exec("COMMIT"); committed = true; return result; }
    finally { if (!committed) this.database.exec("ROLLBACK"); }
  }
  next(campaign: string, held: readonly string[] = [], onlyHit: string | null = null): string | null {
    const row = this.database.prepare(`SELECT h.id FROM discovery_search_hits h
      JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      JOIN discovery_search_authors a ON a.search_hit_id=h.id
      JOIN discovery_profile_reads p ON p.search_hit_id=h.id
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform='linkedin' AND r.profile_ref=h.profile_ref
      JOIN discovery_candidates candidate ON candidate.discovery_campaign_id=c.id AND candidate.criteria_revision=c.current_revision
        AND candidate.platform='linkedin' AND candidate.source_profile_ref=h.profile_ref
        AND candidate.platform_account_id=json_extract(p.native_profile_json,'$.account_id')
      WHERE c.id=?1 AND c.phase='active' AND h.platform='linkedin' AND candidate.verdict='pending'
        AND p.discovery_campaign_id=c.id AND p.expected_revision=c.current_revision AND p.status='completed' AND p.stop_code IS NULL
        AND p.profile_ref=h.profile_ref AND p.author_id=a.author_id
        AND r.status='completed'
        AND(COALESCE(json_extract(candidate.evidence_json,'$.reviewed_selection.outcome'),json_extract(candidate.evidence_json,'$.reviewed_match.outcome'))='insufficient'
          OR EXISTS(SELECT 1 FROM json_each(candidate.evidence_json,'$.reviewed_requirements') requirement WHERE json_extract(requirement.value,'$.outcome')='insufficient'))
        AND h.id NOT IN(SELECT value FROM json_each(?2)) AND(?3 IS NULL OR h.id=?3)
        AND NOT ${discoveryReadBusySql(this.database, campaign, "h.platform")}
        AND NOT EXISTS(SELECT 1 FROM discovery_linkedin_experience_reads e WHERE e.discovery_campaign_id=c.id AND(e.search_hit_id=h.id OR e.account_id=candidate.platform_account_id))
      ORDER BY p.call_number,h.id LIMIT 1`).get(campaign, stringifyApplicationJson(held),onlyHit);
    return row ? text.parse(row.id) : null;
  }
  eligible(campaign: string, hit: string): boolean { return this.next(campaign,[],hit) === hit; }
  claim(campaign: string, revision: number, hit: string): DiscoveryContentClaim {
    return this.#transaction(() => {
      if (!this.eligible(campaign, hit)) throw new Error("discovery_linkedin_experience_not_eligible");
      const row = this.database.prepare(`SELECT h.profile_ref,a.author_id,a.author_url,p.native_profile_json,c.used_calls
        FROM discovery_search_hits h JOIN discovery_search_authors a ON a.search_hit_id=h.id
        JOIN discovery_profile_reads p ON p.search_hit_id=h.id JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
        JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
        WHERE h.id=? AND c.id=? AND c.current_revision=? AND v.approved_at IS NOT NULL`).get(hit,campaign,revision);
      if (!row) throw new Error("discovery_linkedin_experience_not_approved");
      const native = z.record(text,z.unknown()).parse(parseApplicationJson(Buffer.from(text.parse(row.native_profile_json))));
      const account = text.parse(native.account_id), call = integer.parse(row.used_calls) + 1;
      const claim = discoveryContentClaim.parse({campaign_id:campaign,expected_revision:revision,search_hit_id:hit,platform:"linkedin",profile_ref:row.profile_ref,author_id:row.author_id,source_url:row.author_url,call_number:call});
      const now = Math.floor(Date.now()/1000);
      this.database.prepare("INSERT INTO discovery_linkedin_experience_reads VALUES(?,?,?,?,?,?,?,?,'reading',NULL,NULL,?)").run(hit,campaign,revision,claim.profile_ref,claim.author_id,account,claim.source_url,call,now);
      if (this.database.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=? AND current_revision=?").run(call,now,campaign,revision).changes !== 1) throw new Error("discovery_linkedin_experience_campaign_changed");
      if (this.database.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='linkedin' AND profile_ref=? AND status='completed'").run(`experience:${hit}:${call}`,now,campaign,claim.profile_ref).changes !== 1) throw new Error("discovery_linkedin_experience_run_changed");
      return claim;
    });
  }
  account(claim: DiscoveryContentClaim): string {
    const row = this.database.prepare("SELECT account_id FROM discovery_linkedin_experience_reads WHERE search_hit_id=? AND discovery_campaign_id=? AND expected_revision=? AND profile_ref=? AND author_handle=? AND profile_url=? AND call_number=?").get(claim.search_hit_id,claim.campaign_id,claim.expected_revision,claim.profile_ref,claim.author_id,claim.source_url,claim.call_number);
    if (!row) throw new Error("discovery_linkedin_experience_claim_missing"); return text.parse(row.account_id);
  }
  reuse(rawClaim: DiscoveryContentClaim) {
    const claim = discoveryContentClaim.parse(rawClaim), account = this.account(claim);
    const saved = this.database.prepare(`SELECT native_experience_json,updated_at FROM discovery_linkedin_experience_reads
      WHERE discovery_campaign_id<>? AND profile_ref=? AND account_id=? AND author_handle=? AND profile_url=?
        AND status='completed' AND stop_code IS NULL AND native_experience_json IS NOT NULL
      ORDER BY updated_at DESC,search_hit_id LIMIT 1`).get(claim.campaign_id,claim.profile_ref,account,claim.author_id,claim.source_url);
    if (!saved) return null;
    const raw = parseApplicationJson(Buffer.from(text.parse(saved.native_experience_json)));
    const native = linkedinExperiencePlan(claim.author_id,account).decode([raw]);
    if (!isDeepStrictEqual(native,raw)) throw new Error("review_linkedin_experience_result_changed");
    return this.#finish(claim,native,null,integer.parse(saved.updated_at));
  }
  finish(claim: DiscoveryContentClaim, raw: unknown, stop: string | null) {
    return this.#finish(claim,raw,stop,null);
  }
  #finish(claim: DiscoveryContentClaim, raw: unknown, stop: string | null, capturedAt: number | null) {
    discoveryContentClaim.parse(claim);
    const account = this.account(claim);
    if (stop !== null && raw !== null) throw new Error("discovery_linkedin_experience_result_invalid");
    const value = stop === null ? linkedinExperiencePlan(claim.author_id,account).decode([raw]) : null;
    return this.#transaction(() => {
      const row = this.database.prepare("SELECT * FROM discovery_linkedin_experience_reads WHERE search_hit_id=?").get(claim.search_hit_id);
      if (!row || row.status !== "reading" || row.call_number !== claim.call_number || row.expected_revision !== claim.expected_revision) throw new Error("discovery_linkedin_experience_claim_stale");
      const now = Math.floor(Date.now()/1000), status = stop === null ? "completed" : "failed";
      this.database.prepare("UPDATE discovery_linkedin_experience_reads SET status=?,native_experience_json=?,stop_code=?,updated_at=? WHERE search_hit_id=?").run(status,value === null ? null : stringifyApplicationJson(value),stop,capturedAt ?? now,claim.search_hit_id);
      if (stop !== null && sourceLocalReadFailure(stop) || value !== null && value.entries.length === 0) {
        const candidates=this.database.prepare("SELECT id,evidence_json FROM discovery_candidates WHERE discovery_campaign_id=? AND criteria_revision=? AND platform='linkedin' AND source_profile_ref=? AND platform_account_id=? AND verdict='pending'").all(claim.campaign_id,claim.expected_revision,claim.profile_ref,account);
        for(const candidate of candidates) {
          const evidence=z.record(text,z.unknown()).parse(parseApplicationJson(Buffer.from(text.parse(candidate.evidence_json))));
          const assessment=z.record(text,z.unknown()).parse(evidence.assessment);
          assessment.reasons=[...new Set([...z.array(text).parse(assessment.reasons),"linkedin_current_employment_evidence_exhausted"])];
          evidence.assessment=assessment;
          this.database.prepare("UPDATE discovery_candidates SET evidence_json=?,updated_at=? WHERE id=? AND verdict='pending'").run(stringifyApplicationJson(evidence),now,text.parse(candidate.id));
        }
      }
      if (this.database.prepare("UPDATE discovery_platform_runs SET status=?,active_query=NULL,stop_code=?,updated_at=? WHERE discovery_campaign_id=? AND platform='linkedin' AND profile_ref=? AND status='searching' AND active_query=?").run(stop === null ? "completed" : discoveryReadPlatformStatus(stop),stop,now,claim.campaign_id,claim.profile_ref,`experience:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("discovery_linkedin_experience_run_changed");
      return {claim,status,native_content:value,stop_code:stop,captured_at:capturedAt ?? now};
    });
  }
}

export function retainedLinkedinExperience(database: DatabaseSync, claim: DiscoveryContentClaim, account: string) {
  const row = database.prepare("SELECT * FROM discovery_linkedin_experience_reads WHERE discovery_campaign_id=? AND search_hit_id=?").get(claim.campaign_id,claim.search_hit_id);
  if (!row || row.status !== "completed") return null;
  if (row.expected_revision !== claim.expected_revision || row.profile_ref !== claim.profile_ref || row.author_handle !== claim.author_id || row.account_id !== account || row.profile_url !== claim.source_url || integer.parse(row.call_number) <= claim.call_number || row.stop_code !== null) throw new Error("review_linkedin_experience_binding_changed");
  const raw = parseApplicationJson(Buffer.from(text.parse(row.native_experience_json)));
  const native = linkedinExperiencePlan(claim.author_id,account).decode([raw]);
  if (!isDeepStrictEqual(native,raw)) throw new Error("review_linkedin_experience_result_changed");
  return {native,captured_at:integer.parse(row.updated_at)};
}
