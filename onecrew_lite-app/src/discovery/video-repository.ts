import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { discoveryReadBusy } from "./read-capacity.ts";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { assertValidatedCreatorConnection } from "../storage/creator-schema.ts";
import { assessDiscoveryYoutube, assessedDiscoveryEvidence } from "./qualification.ts";
import { mergeAuditedDiscoveryProofs, stripUnauditedDiscoveryProofs } from "./review-proof-audit.ts";
import { sourceLocalReadFailure } from "./stops.ts";
import { youtubeVideoId } from "../platforms/opencli/youtube.ts";
import { loadRetainedYoutubeEvidence, retainedYoutubeFields } from "./youtube-retained.ts";

const text = z.string().refine(value => value.isWellFormed()), u32 = z.number().int().min(0).max(4294967295), object = z.record(text, z.unknown());
// 原生返回体最多64KB，保存时还会保留链接列表与各链接的归属观测。
const maximumVideoEvidenceBytes = 128 * 1024;
export const discoveryVideoClaim = z.strictObject({ campaign_id: text, expected_revision: u32, profile_ref: text, search_hit_id: text, source_url: text, call_number: u32 });
export type DiscoveryVideoClaim = z.infer<typeof discoveryVideoClaim>;
const candidateSchema = z.strictObject({ campaign_id: text, expected_revision: u32, platform: z.literal("youtube"), source_profile_ref: text.min(1), platform_account_id: text.regex(/^UC[A-Za-z0-9_-]{8,62}$/u), display_name: text.refine(value => !!value.trim() && [...value].length <= 200), profile_url: text, evidence: object });
export type DiscoveryVideoCandidate = z.infer<typeof candidateSchema>;
function validateCandidate(raw: unknown, retained = false): DiscoveryVideoCandidate {
  const candidate = candidateSchema.parse(raw), url = URL.parse(candidate.profile_url), evidence = candidate.evidence;
  if (!url || url.protocol !== "https:" || !["youtube.com", "www.youtube.com"].includes(url.hostname) || url.pathname !== `/channel/${candidate.platform_account_id}`) throw new Error("discovery_video_candidate_profile_invalid");
    if (["manual_approval", "reviewed_residence", "reviewed_requirements", "reviewed_apparel_content", "reviewed_category_content", "reviewed_selection", "reviewed_quality"].some(key => Object.hasOwn(evidence, key))) throw new Error("discovery_video_review_owner_required");
  const sources = z.array(text.refine(value => Buffer.byteLength(value) <= 2048 && URL.parse(value)?.protocol === "https:")).min(1).max(20).parse(evidence.source_urls);
  if (!sources.length || Buffer.byteLength(stringifyApplicationJson(evidence)) > maximumVideoEvidenceBytes) throw new Error("discovery_video_evidence_invalid");
  if (!retained && Object.hasOwn(evidence, "retained_youtube")) throw new Error("discovery_video_retained_owner_required");
  return candidate;
}
function videoTarget(source: string): boolean {
  const url = URL.parse(source); if (!url || url.protocol !== "https:" || !["youtube.com", "www.youtube.com"].includes(url.hostname)) return false;
  if (url.pathname === "/watch") return url.searchParams.getAll("v").some(value => /^[A-Za-z0-9_-]{11}$/u.test(value));
  const parts = url.pathname.split("/"); return ["shorts", "live"].includes(parts[1] ?? "") && /^[A-Za-z0-9_-]{11}$/u.test(parts[2] ?? "");
}

export class DiscoveryVideoRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { assertValidatedCreatorConnection(database); this.#db = database; }
  #transaction<T>(body: () => T, readOnly = false): T { this.#db.exec(readOnly ? "BEGIN" : "BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  next(campaign: string, held: readonly string[] = []): { id: string; profile_ref: string } | null {
    return this.#db.isTransaction ? this.#next(campaign, held) : this.#transaction(() => this.#next(campaign, held), true);
  }
  #supplements(campaign: string, hit: string | null = null): { id: string; profile_ref: string }[] {
    const rows = this.#db.prepare(`SELECT h.id,h.profile_ref,c.id AS candidate_id,c.criteria_revision,c.platform_account_id,c.evidence_json FROM discovery_search_hits h
      JOIN discovery_video_completions v ON v.search_hit_id=h.id
      JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=h.discovery_campaign_id
      JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=h.discovery_campaign_id AND c.criteria_revision=b.criteria_revision AND c.platform_account_id=b.native_author_id
      JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id AND d.current_revision=c.criteria_revision
      WHERE h.discovery_campaign_id=? AND(? IS NULL OR h.id=?2) AND c.platform='youtube' AND c.source_profile_ref=h.profile_ref AND c.verdict='pending'
        AND json_extract(c.evidence_json,'$.retained_youtube.fresh_read_attempted')=0
        AND NOT EXISTS(SELECT 1 FROM discovery_video_read_failures f WHERE f.search_hit_id=h.id)
      ORDER BY h.captured_at,h.id`).all(campaign, hit);
    return rows.flatMap(row => {
      const author = text.parse(row.platform_account_id), revision = u32.parse(row.criteria_revision), evidence = object.parse(parseApplicationJson(text.parse(row.evidence_json)));
      loadRetainedYoutubeEvidence(this.#db, evidence, author, text.parse(row.profile_ref));
      const audited = stripUnauditedDiscoveryProofs(this.#db, evidence, text.parse(row.candidate_id), campaign, revision, author);
      const missing = (audited.reviewed_selection as Record<string, unknown> | undefined)?.outcome === "insufficient"
        || Array.isArray(audited.reviewed_requirements) && audited.reviewed_requirements.some(finding => object.parse(finding).outcome === "insufficient");
      return missing ? [z.object({ id: text, profile_ref: text }).parse(row)] : [];
    });
  }
  #next(campaign: string, held: readonly string[]): { id: string; profile_ref: string } | null {
    const skipped = new Set(held);
    for (const row of this.#supplements(campaign)) if (!skipped.has(row.id)) {
      const run = this.#db.prepare("SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").get(campaign, row.profile_ref);
      if (run?.status === "completed" || run?.status === "technical_stop" && run.stop_code === "interrupted_video_read" && String(run.active_query).startsWith(`video:${row.id}:`)) return row;
    }
    const rows = this.#db.prepare(`SELECT h.id,h.profile_ref,p.author_id,rejected.evidence_json,revision.criteria_json FROM discovery_search_hits h LEFT JOIN discovery_search_authors p ON p.search_hit_id=h.id
      JOIN discovery_campaigns campaign ON campaign.id=h.discovery_campaign_id
      JOIN discovery_campaign_revisions revision ON revision.discovery_campaign_id=campaign.id AND revision.revision=campaign.current_revision AND revision.approved_at IS NOT NULL
      LEFT JOIN discovery_candidates rejected ON rejected.discovery_campaign_id=h.discovery_campaign_id AND rejected.criteria_revision=campaign.current_revision
        AND rejected.platform=h.platform AND rejected.source_profile_ref=h.profile_ref AND rejected.platform_account_id=p.author_id AND rejected.verdict='rejected'
        AND EXISTS(SELECT 1 FROM discovery_video_identity_bindings binding WHERE binding.candidate_id=rejected.id AND binding.discovery_campaign_id=rejected.discovery_campaign_id AND binding.criteria_revision=rejected.criteria_revision AND binding.native_author_id=p.author_id)
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      WHERE h.discovery_campaign_id=? AND h.platform='youtube' AND (r.status='completed' OR(r.status='technical_stop' AND r.stop_code='interrupted_video_read' AND r.active_query LIKE 'video:'||h.id||':%'))
      AND NOT EXISTS(SELECT 1 FROM discovery_video_completions v WHERE v.search_hit_id=h.id) AND NOT EXISTS(SELECT 1 FROM discovery_video_read_failures f WHERE f.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_candidates known WHERE p.author_id IS NULL AND known.discovery_campaign_id=h.discovery_campaign_id
        AND known.criteria_revision=(SELECT current_revision FROM discovery_campaigns WHERE id=h.discovery_campaign_id)
        AND known.platform=h.platform AND known.source_profile_ref=h.profile_ref AND known.verdict='qualified' AND known.display_name=h.author_label)
      AND NOT EXISTS(SELECT 1 FROM discovery_candidates known WHERE known.discovery_campaign_id=h.discovery_campaign_id AND known.criteria_revision=(SELECT current_revision FROM discovery_campaigns WHERE id=h.discovery_campaign_id)
        AND known.platform=h.platform AND known.source_profile_ref=h.profile_ref AND r.status='completed' AND known.platform_account_id=p.author_id AND known.verdict='qualified'
        AND ((json_extract(known.evidence_json,'$.reviewed_selection.verified')=1 AND json_extract(known.evidence_json,'$.reviewed_selection.native_author_id')=p.author_id AND json_extract(known.evidence_json,'$.reviewed_selection.outcome')='supported')
          OR(json_extract(known.evidence_json,'$.reviewed_match.verified')=1 AND json_extract(known.evidence_json,'$.reviewed_match.native_author_id')=p.author_id AND json_extract(known.evidence_json,'$.reviewed_match.level') IN('medium','good','excellent') AND json_extract(known.evidence_json,'$.reviewed_match.outcome')='supported')))
      AND NOT EXISTS(SELECT 1 FROM discovery_candidates c,json_each(c.evidence_json,'$.source_urls') s WHERE c.discovery_campaign_id=h.discovery_campaign_id AND c.platform=h.platform AND s.value=h.source_url)
       AND NOT EXISTS(SELECT 1 FROM discovery_candidates c JOIN discovery_video_identity_bindings b ON b.candidate_id=c.id AND b.discovery_campaign_id=c.discovery_campaign_id AND b.criteria_revision=c.criteria_revision AND b.native_author_id=c.platform_account_id
         JOIN discovery_video_completions v ON v.search_hit_id=b.search_hit_id WHERE c.discovery_campaign_id=h.discovery_campaign_id AND c.criteria_revision=campaign.current_revision AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=p.author_id)
      ORDER BY CASE WHEN r.status='technical_stop' THEN 0 ELSE 1 END,EXISTS(SELECT 1 FROM discovery_candidates known WHERE known.discovery_campaign_id=h.discovery_campaign_id AND known.platform=h.platform AND known.source_profile_ref=h.profile_ref AND known.display_name=h.author_label),h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id`).iterate(campaign);
    for (const row of rows) {
      if (skipped.has(text.parse(row.id))) continue;
      if (row.evidence_json !== null && assessDiscoveryYoutube(parseApplicationJson(text.parse(row.criteria_json)), parseApplicationJson(text.parse(row.evidence_json)), text.parse(row.author_id)).reasons.includes("below_min_subscribers")) continue;
      return z.object({ id: text, profile_ref: text }).parse(row);
    }
    return null;
  }
  claim(campaign: string, revision: number, profile: string, hit: string): DiscoveryVideoClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      const head = this.#db.prepare("SELECT used_calls FROM discovery_campaigns WHERE id=? AND phase='active' AND current_revision=?").get(campaign, revision);
      if (!head) throw new Error("discovery_video_not_approved");
      const counter = z.object({ used_calls: u32 }).parse(head);
      const source = this.#db.prepare("SELECT source_url FROM discovery_search_hits WHERE id=? AND discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").get(hit, campaign, profile);
      if (!source || !videoTarget(text.parse(source.source_url))) throw new Error("discovery_video_source_unobserved");
      const url = text.parse(source.source_url), run = z.object({ status: text, stop_code: text.nullable(), active_query: text.nullable() }).parse(this.#db.prepare("SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").get(campaign, profile));
      const prefix = `video:${hit}:`, retry = run.status === "technical_stop" && run.stop_code === "interrupted_video_read" && run.active_query?.startsWith(prefix) === true;
      const explicit = run.status === "technical_stop" && run.active_query === null && ["opencli_timeout", "opencli_command_failed", "opencli_invalid_output", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable"].includes(run.stop_code ?? "");
      if (run.status !== "completed" && !retry && !explicit) throw new Error("discovery_video_platform_stopped");
      if (discoveryReadBusy(this.#db, campaign, ["youtube"])) throw new Error("discovery_read_active");
      const supplement = this.#supplements(campaign, hit).length === 1;
      if (!retry && !supplement && this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_video_completions WHERE search_hit_id=?3) OR EXISTS(SELECT 1 FROM discovery_candidates c,json_each(c.evidence_json,'$.source_urls') s WHERE c.discovery_campaign_id=?1 AND c.platform='youtube' AND s.value=?2) AS saved`).get(campaign, url, hit)!.saved === 1) throw new Error("discovery_video_already_saved");
      if (supplement) this.#db.prepare(`UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.retained_youtube.fresh_read_attempted',json('true')) WHERE id=(SELECT candidate_id FROM discovery_video_identity_bindings WHERE search_hit_id=?)`).run(hit);
      const claim = discoveryVideoClaim.parse({ campaign_id: campaign, expected_revision: revision, profile_ref: profile, search_hit_id: hit, source_url: url, call_number: counter.used_calls + 1 }), now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(claim.call_number, now, campaign).changes !== 1) throw new Error("discovery_video_campaign_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=?").run(`${prefix}${claim.call_number}`, now, campaign, profile).changes !== 1) throw new Error("discovery_video_run_changed");
      return claim;
    });
  }
  interrupt(raw: DiscoveryVideoClaim): void {
    const claim = discoveryVideoClaim.parse(raw);
    this.#transaction(() => { if (this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_video_read',updated_at=? WHERE discovery_campaign_id=? AND platform='youtube' AND profile_ref=? AND status='searching' AND active_query=?").run(Math.floor(Date.now() / 1000), claim.campaign_id, claim.profile_ref, `video:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("discovery_video_claim_stale"); });
  }
  reuse(raw: DiscoveryVideoClaim): boolean {
    const claim = discoveryVideoClaim.parse(raw);
    if (this.#db.prepare("SELECT 1 FROM discovery_video_completions WHERE search_hit_id=?").get(claim.search_hit_id)) return false;
    const rows = this.#db.prepare(`SELECT h.id AS search_hit_id,h.source_url,c.platform_account_id,c.display_name,c.profile_url,c.evidence_json FROM discovery_search_hits h
      JOIN discovery_video_completions v ON v.search_hit_id=h.id
      JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=h.discovery_campaign_id
      JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=h.discovery_campaign_id
        AND c.platform_account_id=b.native_author_id AND c.criteria_revision=b.criteria_revision
      WHERE h.discovery_campaign_id<>?1 AND h.platform='youtube' AND h.profile_ref=?2
        AND c.platform='youtube' AND c.source_profile_ref=h.profile_ref AND json_extract(c.evidence_json,'$.source_urls[0]')=h.source_url
        AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=?3 AND a.author_id<>c.platform_account_id)
      ORDER BY v.completed_at DESC`).all(claim.campaign_id, claim.profile_ref, claim.search_hit_id);
    const videoId = youtubeVideoId(claim.source_url);
    const observed = this.#db.prepare("SELECT author_id FROM discovery_search_authors WHERE search_hit_id=?").get(claim.search_hit_id);
    const row = rows.find(row => youtubeVideoId(text.parse(row.source_url)) === videoId) ?? (observed ? rows.find(row => row.platform_account_id === observed.author_id) : undefined);
    if (!row) return false;
    const saved = object.parse(parseApplicationJson(text.parse(row.evidence_json)));
    const evidence = Object.fromEntries(retainedYoutubeFields.filter(key => Object.hasOwn(saved, key)).map(key => [key, saved[key]]));
    const sameVideo = youtubeVideoId(text.parse(row.source_url)) === videoId;
    evidence.source_urls = [...new Set([...(sameVideo ? [] : [text.parse(row.source_url)]), claim.source_url, ...z.array(text).parse(saved.source_urls)])].slice(0, 20);
    if (!sameVideo) evidence.retained_youtube = { search_hit_id: row.search_hit_id, call_number: claim.call_number, fresh_read_attempted: false };
    this.#finish(claim, { campaign_id: claim.campaign_id, expected_revision: claim.expected_revision, platform: "youtube", source_profile_ref: claim.profile_ref, platform_account_id: row.platform_account_id, display_name: row.display_name, profile_url: row.profile_url, evidence }, null, true);
    return true;
  }
  finish(raw: DiscoveryVideoClaim, rawCandidate: unknown, stop: string | null): void {
    this.#finish(raw, rawCandidate, stop, false);
  }
  #finish(raw: DiscoveryVideoClaim, rawCandidate: unknown, stop: string | null, preserveQualified: boolean): void {
    const claim = discoveryVideoClaim.parse(raw), candidate = rawCandidate === null ? null : validateCandidate(rawCandidate, preserveQualified);
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
        const merged = old === null ? structuredClone(candidate.evidence) : preserveQualified && previous?.verdict === "qualified" ? structuredClone(old) : mergeAuditedDiscoveryProofs(this.#db, candidate.evidence, old, text.parse(previous!.id), claim.campaign_id, claim.expected_revision, candidate.platform_account_id);
        const assessment = assessDiscoveryYoutube(criteria, merged, candidate.platform_account_id), rejected = previous?.verdict === "rejected";
        if (rejected) merged.assessment = old!.assessment ?? null;
        const evidence = rejected ? merged : assessedDiscoveryEvidence(merged, assessment), json = stringifyApplicationJson(evidence); if (Buffer.byteLength(json) > maximumVideoEvidenceBytes) throw new Error("discovery_video_evidence_too_large");
        this.#db.prepare(`INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
          VALUES(?,?,?,'youtube',?,?,?,?,?,?,?10,?10,'youtube') ON CONFLICT(discovery_campaign_id,platform,platform_account_id) DO UPDATE SET source_platform=excluded.source_platform,display_name=excluded.display_name,profile_url=excluded.profile_url,evidence_json=excluded.evidence_json,verdict=excluded.verdict,updated_at=excluded.updated_at WHERE discovery_candidates.criteria_revision=excluded.criteria_revision`).run(randomUUID(), claim.campaign_id, claim.expected_revision, claim.profile_ref, candidate.platform_account_id, candidate.display_name, candidate.profile_url, rejected ? "rejected" : assessment.verdict, json, now);
        this.#db.prepare("INSERT INTO discovery_video_completions(search_hit_id,completed_at) VALUES(?,?) ON CONFLICT(search_hit_id) DO UPDATE SET completed_at=excluded.completed_at").run(claim.search_hit_id, now);
        if (this.#db.prepare(`INSERT INTO discovery_video_identity_bindings(search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at) SELECT ?,discovery_campaign_id,criteria_revision,id,platform_account_id,? FROM discovery_candidates WHERE discovery_campaign_id=? AND platform='youtube' AND platform_account_id=? AND criteria_revision=? AND source_profile_ref=?
          ON CONFLICT(search_hit_id) DO UPDATE SET created_at=excluded.created_at WHERE discovery_video_identity_bindings.discovery_campaign_id=excluded.discovery_campaign_id AND discovery_video_identity_bindings.criteria_revision=excluded.criteria_revision AND discovery_video_identity_bindings.candidate_id=excluded.candidate_id AND discovery_video_identity_bindings.native_author_id=excluded.native_author_id`).run(claim.search_hit_id, now, claim.campaign_id, candidate.platform_account_id, claim.expected_revision, claim.profile_ref).changes !== 1) throw new Error("discovery_video_candidate_identity_changed");
      } else if (local) this.#db.prepare("INSERT INTO discovery_video_read_failures(search_hit_id,call_number,stop_code,updated_at) VALUES(?,?,?,?) ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,stop_code=excluded.stop_code,updated_at=excluded.updated_at").run(claim.search_hit_id, claim.call_number, stop, now);
    });
  }
}
