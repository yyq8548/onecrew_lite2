import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { discoveryTranscriptSegment } from "./review-material.ts";
import { getDiscoverySavedTranscript } from "./saved-read-repository.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";

const text = z.string().refine(value => value.isWellFormed()), u32 = z.number().int().min(0).max(4294967295);
export const discoveryTranscriptClaim = z.strictObject({ campaign_id: text, expected_revision: u32, profile_ref: text, search_hit_id: text, candidate_id: text, source_url: text, call_number: u32 });
export type DiscoveryTranscriptClaim = z.infer<typeof discoveryTranscriptClaim>;
const stopSchema = z.enum(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required"]);
export function validateDiscoveryTranscriptResult(raw: unknown) {
  const value = z.strictObject({ segments: z.array(discoveryTranscriptSegment).max(200), truncated: z.boolean(), stop_code: stopSchema.nullable() }).parse(raw);
  if (Buffer.byteLength(stringifyApplicationJson(value.segments)) > 24000 || value.segments.some(line => !/^[0-9:]{1,24}$/u.test(line.timestamp) || [...line.speaker].length > 120 || !line.text.trim() || [...line.text].length > 4000) || !value.segments.length && value.truncated || value.stop_code !== null && (value.segments.length > 0 || value.truncated)) throw new Error("discovery_transcript_result_invalid");
  return value;
}
export class DiscoveryTranscriptRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #transaction<T>(body: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  next(campaign: string, held: readonly string[] = []): { id: string; profile_ref: string } | null {
    const result = this.#db.prepare(`SELECT h.id,h.profile_ref FROM discovery_search_hits h JOIN discovery_video_completions v ON v.search_hit_id=h.id
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      JOIN discovery_campaigns d ON d.id=h.discovery_campaign_id JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
      WHERE d.id=?1 AND h.id NOT IN(SELECT value FROM json_each(?2)) AND d.phase='active' AND a.approved_at IS NOT NULL AND h.platform='youtube'
      AND COALESCE(json_extract(a.criteria_json,'$.requires_ai_tool_demo'),1)=1
      AND EXISTS(SELECT 1 FROM discovery_candidates c JOIN discovery_video_identity_bindings b ON b.candidate_id=c.id AND b.search_hit_id=h.id AND b.discovery_campaign_id=d.id AND b.criteria_revision=d.current_revision AND b.native_author_id=c.platform_account_id
        WHERE c.discovery_campaign_id=d.id AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref AND c.criteria_revision=d.current_revision AND c.verdict='pending' AND EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.source_urls') s WHERE s.value=h.source_url))
      AND ((r.status='completed' AND NOT EXISTS(SELECT 1 FROM discovery_transcript_reads t WHERE t.search_hit_id=h.id)) OR(r.status='technical_stop' AND r.stop_code='interrupted_transcript_read' AND EXISTS(SELECT 1 FROM discovery_transcript_reads t WHERE t.search_hit_id=h.id AND t.status='reading' AND r.active_query='transcript:'||h.id||':'||t.call_number)))
      ORDER BY CASE WHEN r.status='technical_stop' THEN 0 ELSE 1 END,h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1`).get(campaign, stringifyApplicationJson(held));
    return result === undefined ? null : z.object({ id: text, profile_ref: text }).parse(result);
  }
  claim(campaign: string, revision: number, profile: string, hit: string): DiscoveryTranscriptClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      const head = this.#db.prepare(`SELECT c.used_calls,c.max_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=c.id AND a.revision=c.current_revision
        WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND a.approved_at IS NOT NULL AND COALESCE(json_extract(a.criteria_json,'$.requires_ai_tool_demo'),1)=1`).get(campaign, revision);
      if (!head) throw new Error("discovery_transcript_not_approved");
      const budget = z.object({ used_calls: u32, max_calls: u32.nullable() }).parse(head);
      if (budget.max_calls !== null && budget.used_calls >= budget.max_calls) throw new Error("discovery_budget_exhausted");
      if (this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=? AND status='searching') busy").get(campaign)!.busy === 1) throw new Error("discovery_read_active");
      const source = this.#db.prepare("SELECT h.source_url FROM discovery_search_hits h JOIN discovery_video_completions v ON v.search_hit_id=h.id WHERE h.id=? AND h.discovery_campaign_id=? AND h.platform='youtube' AND h.profile_ref=?").get(hit, campaign, profile);
      if (!source) throw new Error("discovery_transcript_source_unobserved"); const sourceUrl = text.parse(source.source_url);
      const candidates = this.#db.prepare(`SELECT c.id FROM discovery_candidates c JOIN discovery_video_identity_bindings b ON b.candidate_id=c.id AND b.search_hit_id=?5 AND b.discovery_campaign_id=?1 AND b.criteria_revision=?3 AND b.native_author_id=c.platform_account_id
        WHERE c.discovery_campaign_id=?1 AND c.platform='youtube' AND c.source_profile_ref=?2 AND c.criteria_revision=?3 AND c.verdict='pending' AND EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.source_urls') s WHERE s.value=?4) LIMIT 2`).all(campaign, profile, revision, sourceUrl, hit);
      if (candidates.length !== 1) throw new Error("discovery_transcript_source_owner_not_unique"); const candidate = text.parse(candidates[0]!.id);
      const run = z.object({ status: text, stop_code: text.nullable(), active_query: text.nullable() }).parse(this.#db.prepare("SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").get(campaign, profile));
      const prior = this.#db.prepare("SELECT call_number,status,candidate_id FROM discovery_transcript_reads WHERE search_hit_id=?").get(hit);
      const interrupted = prior !== undefined && prior.status === "reading" && prior.candidate_id === candidate && run.status === "technical_stop" && run.stop_code === "interrupted_transcript_read" && run.active_query === `transcript:${hit}:${u32.parse(prior.call_number)}`;
      if (!interrupted && (run.status !== "completed" || prior !== undefined)) throw new Error("discovery_transcript_already_attempted_or_stopped");
      const call = u32.parse(budget.used_calls + 1), now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(call, now, campaign).changes !== 1) throw new Error("discovery_transcript_campaign_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").run(`transcript:${hit}:${call}`, now, campaign, profile).changes !== 1) throw new Error("discovery_transcript_run_changed");
      this.#db.prepare("INSERT INTO discovery_transcript_reads VALUES(?,?,?,'reading','[]',0,NULL,?) ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at").run(hit, candidate, call, now);
      return discoveryTranscriptClaim.parse({ campaign_id: campaign, expected_revision: revision, profile_ref: profile, search_hit_id: hit, candidate_id: candidate, source_url: sourceUrl, call_number: call });
    });
  }
  finish(rawClaim: DiscoveryTranscriptClaim, rawResult: unknown) {
    const claim = discoveryTranscriptClaim.parse(rawClaim), result = validateDiscoveryTranscriptResult(rawResult);
    return this.#transaction(() => {
      const now = Math.floor(Date.now() / 1000), runStop = sourceLocalReadFailure(result.stop_code) ? null : result.stop_code;
      const changed = this.#db.prepare(`UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
        WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2 AND status='searching' AND active_query=?3
        AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=c.id AND a.revision=c.current_revision WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?7 AND a.approved_at IS NOT NULL)
        AND EXISTS(SELECT 1 FROM discovery_search_hits h WHERE h.id=?8 AND h.discovery_campaign_id=?1 AND h.platform='youtube' AND h.profile_ref=?2 AND h.source_url=?9)
        AND EXISTS(SELECT 1 FROM discovery_video_identity_bindings b JOIN discovery_candidates c ON c.id=b.candidate_id WHERE b.search_hit_id=?8 AND b.discovery_campaign_id=?1 AND b.criteria_revision=?7 AND b.candidate_id=?10 AND b.native_author_id=c.platform_account_id AND c.discovery_campaign_id=?1 AND c.criteria_revision=?7 AND c.platform='youtube' AND c.source_profile_ref=?2)`).run(claim.campaign_id, claim.profile_ref, `transcript:${claim.search_hit_id}:${claim.call_number}`, discoveryReadPlatformStatus(result.stop_code), runStop, now, claim.expected_revision, claim.search_hit_id, claim.source_url, claim.candidate_id);
      if (changed.changes !== 1) throw new Error("discovery_transcript_claim_stale");
      const status = result.stop_code !== null ? "failed" : result.segments.length ? "completed" : "unavailable";
      if (this.#db.prepare("UPDATE discovery_transcript_reads SET status=?4,segments_json=?5,truncated=?6,stop_code=?7,updated_at=?8 WHERE search_hit_id=?1 AND candidate_id=?2 AND call_number=?3 AND status='reading'").run(claim.search_hit_id, claim.candidate_id, claim.call_number, status, stringifyApplicationJson(result.segments), Number(result.truncated), result.stop_code, now).changes !== 1) throw new Error("discovery_transcript_receipt_stale");
      return getDiscoverySavedTranscript(this.#db, claim.campaign_id, claim.search_hit_id);
    });
  }
  interrupt(raw: DiscoveryTranscriptClaim): void {
    const claim = discoveryTranscriptClaim.parse(raw);
    if (this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_transcript_read',updated_at=? WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=? AND status='searching' AND active_query=?").run(Math.floor(Date.now() / 1000), claim.campaign_id, claim.profile_ref, `transcript:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("discovery_transcript_claim_stale");
  }
}
