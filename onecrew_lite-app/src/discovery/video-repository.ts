import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { assessDiscoveryYoutube, assessedDiscoveryEvidence } from "./qualification.ts";
import { mergeAuditedDiscoveryProofs } from "./review-proof-audit.ts";
import { sourceLocalReadFailure } from "./stops.ts";

const text = z.string().refine(value => value.isWellFormed()), u32 = z.number().int().min(0).max(4294967295), object = z.record(text, z.unknown());
export const discoveryVideoClaim = z.strictObject({ campaign_id: text, expected_revision: u32, profile_ref: text, search_hit_id: text, source_url: text, call_number: u32 });
export type DiscoveryVideoClaim = z.infer<typeof discoveryVideoClaim>;
const candidateSchema = z.strictObject({ campaign_id: text, expected_revision: u32, platform: z.literal("youtube"), source_profile_ref: text.min(1), platform_account_id: text.regex(/^UC[A-Za-z0-9_-]{8,62}$/u), display_name: text.refine(value => !!value.trim() && [...value].length <= 200), profile_url: text, evidence: object });
export type DiscoveryVideoCandidate = z.infer<typeof candidateSchema>;
function validateCandidate(raw: unknown): DiscoveryVideoCandidate {
  const candidate = candidateSchema.parse(raw), url = URL.parse(candidate.profile_url), evidence = candidate.evidence;
  if (!url || url.protocol !== "https:" || !["youtube.com", "www.youtube.com"].includes(url.hostname) || url.pathname !== `/channel/${candidate.platform_account_id}`) throw new Error("discovery_video_candidate_profile_invalid");
    if (["manual_approval", "reviewed_residence", "reviewed_requirements", "reviewed_apparel_content", "reviewed_category_content"].some(key => Object.hasOwn(evidence, key))) throw new Error("discovery_video_review_owner_required");
  const sources = z.array(text.refine(value => Buffer.byteLength(value) <= 2048 && URL.parse(value)?.protocol === "https:")).min(1).max(20).parse(evidence.source_urls);
  if (!sources.length || Buffer.byteLength(stringifyApplicationJson(evidence)) > 16384) throw new Error("discovery_video_evidence_invalid");
  return candidate;
}
function videoTarget(source: string): boolean {
  const url = URL.parse(source); if (!url || url.protocol !== "https:" || !["youtube.com", "www.youtube.com"].includes(url.hostname)) return false;
  if (url.pathname === "/watch") return url.searchParams.getAll("v").some(value => /^[A-Za-z0-9_-]{11}$/u.test(value));
  const parts = url.pathname.split("/"); return ["shorts", "live"].includes(parts[1] ?? "") && /^[A-Za-z0-9_-]{11}$/u.test(parts[2] ?? "");
}

export class DiscoveryVideoRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #transaction<T>(body: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  next(campaign: string): { id: string; profile_ref: string } | null {
    const row = this.#db.prepare(`SELECT h.id,h.profile_ref FROM discovery_search_hits h LEFT JOIN discovery_search_authors p ON p.search_hit_id=h.id
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      WHERE h.discovery_campaign_id=? AND h.platform='youtube' AND (r.status='completed' OR(r.status='technical_stop' AND r.stop_code='interrupted_video_read' AND r.active_query LIKE 'video:'||h.id||':%'))
      AND NOT EXISTS(SELECT 1 FROM discovery_video_completions v WHERE v.search_hit_id=h.id) AND NOT EXISTS(SELECT 1 FROM discovery_video_read_failures f WHERE f.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_candidates known WHERE known.discovery_campaign_id=h.discovery_campaign_id AND known.criteria_revision=(SELECT current_revision FROM discovery_campaigns WHERE id=h.discovery_campaign_id)
        AND known.platform=h.platform AND known.source_profile_ref=h.profile_ref AND r.status='completed' AND known.platform_account_id=p.author_id AND known.verdict='qualified'
        AND json_extract(known.evidence_json,'$.reviewed_match.verified')=1 AND json_extract(known.evidence_json,'$.reviewed_match.native_author_id')=p.author_id AND json_extract(known.evidence_json,'$.reviewed_match.level') IN('medium','good','excellent') AND json_extract(known.evidence_json,'$.reviewed_match.outcome')='supported')
      AND NOT EXISTS(SELECT 1 FROM discovery_candidates c,json_each(c.evidence_json,'$.source_urls') s WHERE c.discovery_campaign_id=h.discovery_campaign_id AND c.platform=h.platform AND s.value=h.source_url)
      ORDER BY CASE WHEN r.status='technical_stop' THEN 0 ELSE 1 END,EXISTS(SELECT 1 FROM discovery_candidates known WHERE known.discovery_campaign_id=h.discovery_campaign_id AND known.platform=h.platform AND known.source_profile_ref=h.profile_ref AND known.display_name=h.author_label),h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1`).get(campaign);
    return row === undefined ? null : z.object({ id: text, profile_ref: text }).parse(row);
  }
  claim(campaign: string, revision: number, profile: string, hit: string): DiscoveryVideoClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      const head = this.#db.prepare("SELECT used_calls,max_calls FROM discovery_campaigns WHERE id=? AND phase='active' AND current_revision=?").get(campaign, revision);
      if (!head) throw new Error("discovery_video_not_approved");
      const budget = z.object({ used_calls: u32, max_calls: u32.nullable() }).parse(head);
      if (budget.max_calls !== null && budget.used_calls >= budget.max_calls) throw new Error("discovery_budget_exhausted");
      const source = this.#db.prepare("SELECT source_url FROM discovery_search_hits WHERE id=? AND discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").get(hit, campaign, profile);
      if (!source || !videoTarget(text.parse(source.source_url))) throw new Error("discovery_video_source_unobserved");
      const url = text.parse(source.source_url), run = z.object({ status: text, stop_code: text.nullable(), active_query: text.nullable() }).parse(this.#db.prepare("SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").get(campaign, profile));
      const prefix = `video:${hit}:`, retry = run.status === "technical_stop" && run.stop_code === "interrupted_video_read" && run.active_query?.startsWith(prefix) === true;
      const explicit = run.status === "technical_stop" && run.active_query === null && ["opencli_timeout", "opencli_command_failed", "opencli_invalid_output", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable"].includes(run.stop_code ?? "");
      if (run.status !== "completed" && !retry && !explicit) throw new Error("discovery_video_platform_stopped");
      if (this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=? AND status='searching') AS busy").get(campaign)!.busy === 1) throw new Error("discovery_read_active");
      if (!retry && this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_video_completions WHERE search_hit_id=?3) OR EXISTS(SELECT 1 FROM discovery_candidates c,json_each(c.evidence_json,'$.source_urls') s WHERE c.discovery_campaign_id=?1 AND c.platform='youtube' AND s.value=?2) AS saved`).get(campaign, url, hit)!.saved === 1) throw new Error("discovery_video_already_saved");
      const claim = discoveryVideoClaim.parse({ campaign_id: campaign, expected_revision: revision, profile_ref: profile, search_hit_id: hit, source_url: url, call_number: budget.used_calls + 1 }), now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(claim.call_number, now, campaign).changes !== 1) throw new Error("discovery_video_campaign_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").run(`${prefix}${claim.call_number}`, now, campaign, profile).changes !== 1) throw new Error("discovery_video_run_changed");
      return claim;
    });
  }
  interrupt(raw: DiscoveryVideoClaim): void {
    const claim = discoveryVideoClaim.parse(raw);
    this.#transaction(() => { if (this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_video_read',updated_at=? WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=? AND status='searching' AND active_query=?").run(Math.floor(Date.now() / 1000), claim.campaign_id, claim.profile_ref, `video:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("discovery_video_claim_stale"); });
  }
  finish(raw: DiscoveryVideoClaim, rawCandidate: unknown, stop: string | null): void {
    const claim = discoveryVideoClaim.parse(raw), candidate = rawCandidate === null ? null : validateCandidate(rawCandidate);
    if ((candidate === null) === (stop === null) || stop !== null && (!stop || Buffer.byteLength(stop) > 128 || /\p{Cc}/u.test(stop))) throw new Error("discovery_video_result_invalid");
    if (candidate !== null && (candidate.campaign_id !== claim.campaign_id || candidate.expected_revision !== claim.expected_revision || candidate.source_profile_ref !== claim.profile_ref || !(candidate.evidence.source_urls as unknown[]).includes(claim.source_url))) throw new Error("discovery_video_candidate_claim_mismatch");
    this.#transaction(() => {
      const now = Math.floor(Date.now() / 1000), local = sourceLocalReadFailure(stop);
      if (this.#db.prepare(`UPDATE discovery_platform_runs SET status=?,active_query=NULL,stop_code=?,updated_at=? WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=? AND status='searching' AND active_query=?
        AND EXISTS(SELECT 1 FROM discovery_search_hits hit WHERE hit.id=? AND hit.discovery_campaign_id=? AND hit.platform='youtube' AND hit.profile_ref=? AND hit.source_url=?)
        AND EXISTS(SELECT 1 FROM discovery_campaigns c WHERE c.id=? AND c.phase='active' AND c.current_revision=? AND c.used_calls>=?)`).run(candidate !== null || local ? "completed" : stop === "opencli_risk_control" ? "risk_control_stop" : "technical_stop", local ? null : stop, now, claim.campaign_id, claim.profile_ref, `video:${claim.search_hit_id}:${claim.call_number}`, claim.search_hit_id, claim.campaign_id, claim.profile_ref, claim.source_url, claim.campaign_id, claim.expected_revision, claim.call_number).changes !== 1) throw new Error("discovery_video_claim_stale");
      if (candidate !== null) {
        this.#db.prepare("DELETE FROM discovery_video_read_failures WHERE search_hit_id=?").run(claim.search_hit_id);
        const criteria = parseApplicationJson(text.parse(this.#db.prepare("SELECT criteria_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=? AND revision=? AND approved_at IS NOT NULL").get(claim.campaign_id, claim.expected_revision)!.criteria_json));
        const previous = this.#db.prepare("SELECT id,verdict,evidence_json FROM discovery_candidates WHERE discovery_campaign_id=? AND platform='youtube' AND platform_account_id=?").get(claim.campaign_id, candidate.platform_account_id);
        const old = previous ? object.parse(parseApplicationJson(text.parse(previous.evidence_json))) : null;
        const merged = old === null ? structuredClone(candidate.evidence) : mergeAuditedDiscoveryProofs(this.#db, candidate.evidence, old, text.parse(previous!.id), claim.campaign_id, claim.expected_revision, candidate.platform_account_id);
        const assessment = assessDiscoveryYoutube(criteria, merged, candidate.platform_account_id), rejected = previous?.verdict === "rejected";
        if (rejected) merged.assessment = old!.assessment ?? null;
        const evidence = rejected ? merged : assessedDiscoveryEvidence(merged, assessment), json = stringifyApplicationJson(evidence); if (Buffer.byteLength(json) > 16384) throw new Error("discovery_video_evidence_too_large");
        this.#db.prepare(`INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
          VALUES(?,?,?,'youtube',?,?,?,?,?,?,?10,?10,'youtube') ON CONFLICT(discovery_campaign_id,platform,platform_account_id) DO UPDATE SET source_platform=excluded.source_platform,display_name=excluded.display_name,profile_url=excluded.profile_url,evidence_json=excluded.evidence_json,verdict=excluded.verdict,updated_at=excluded.updated_at WHERE discovery_candidates.criteria_revision=excluded.criteria_revision`).run(randomUUID(), claim.campaign_id, claim.expected_revision, claim.profile_ref, candidate.platform_account_id, candidate.display_name, candidate.profile_url, rejected ? "rejected" : assessment.verdict, json, now);
        this.#db.prepare("INSERT INTO discovery_video_completions(search_hit_id,completed_at) VALUES(?,?)").run(claim.search_hit_id, now);
        if (this.#db.prepare(`INSERT INTO discovery_video_identity_bindings(search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at) SELECT ?,discovery_campaign_id,criteria_revision,id,platform_account_id,? FROM discovery_candidates WHERE discovery_campaign_id=? AND platform='youtube' AND platform_account_id=? AND criteria_revision=? AND source_profile_ref=?`).run(claim.search_hit_id, now, claim.campaign_id, candidate.platform_account_id, claim.expected_revision, claim.profile_ref).changes !== 1) throw new Error("discovery_video_candidate_identity_changed");
      } else if (local) this.#db.prepare("INSERT INTO discovery_video_read_failures(search_hit_id,call_number,stop_code,updated_at) VALUES(?,?,?,?) ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,stop_code=excluded.stop_code,updated_at=excluded.updated_at").run(claim.search_hit_id, claim.call_number, stop, now);
    });
  }
}
