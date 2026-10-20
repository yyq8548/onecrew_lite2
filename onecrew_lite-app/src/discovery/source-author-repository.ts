import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { validateSavedDiscoveryDouyinContent, validateSavedDiscoveryDouyinProfile } from "./douyin-material.ts";
import { DiscoveryProfileRepository } from "./profile-repository.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";

const text = z.string().refine(value => value.isWellFormed()), u32 = z.number().int().min(0).max(4294967295);
export const discoverySourceAuthorClaim = z.strictObject({ campaign_id: text, expected_revision: u32, search_hit_id: text, platform: z.literal("douyin"), profile_ref: text, source_url: text, post_id: text, call_number: u32 });
export type DiscoverySourceAuthorClaim = z.infer<typeof discoverySourceAuthorClaim>;
const stopCode = z.enum(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required"]);
export function discoveryDouyinSourcePost(source: string): string | null { return /^https:\/\/www\.douyin\.com\/video\/([1-9][0-9]{0,19})$/u.exec(source)?.[1] ?? null; }
export class DiscoverySourceAuthorRepository {
  readonly #db: DatabaseSync;
  readonly #profiles: DiscoveryProfileRepository;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; this.#profiles = new DiscoveryProfileRepository(database); }
  #transaction<T>(body: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  next(campaign: string, excluded: readonly string[] = []): string | null {
    if (excluded.includes("douyin")) return null;
    const rows = this.#db.prepare(`SELECT h.id,h.source_url FROM discovery_search_hits h JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id LEFT JOIN discovery_douyin_author_reads p ON p.search_hit_id=h.id
      WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL AND(c.max_calls IS NULL OR c.used_calls<c.max_calls) AND h.platform='douyin' AND r.status='completed' AND a.search_hit_id IS NULL AND p.search_hit_id IS NULL
      AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')
      ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id`).all(campaign);
    for (const row of rows) if (discoveryDouyinSourcePost(text.parse(row.source_url)) !== null) return text.parse(row.id); return null;
  }
  get(campaign: string, hit: string) {
    const statement = this.#db.prepare(`SELECT p.expected_revision,p.platform,p.profile_ref,p.source_url,p.post_id,p.call_number,p.status,p.author_id,p.profile_url,p.duplicate_of,p.stop_code,p.updated_at
      FROM discovery_douyin_author_reads p JOIN discovery_search_hits h ON h.id=p.search_hit_id WHERE p.discovery_campaign_id=?1 AND p.search_hit_id=?2 AND h.discovery_campaign_id=?1 AND h.platform=p.platform AND h.profile_ref=p.profile_ref AND h.source_url=p.source_url`); statement.setReadBigInts(true);
    const raw = statement.get(campaign, hit); if (!raw) throw new Error("discovery_source_author_read_missing");
    const saved = Object.fromEntries(Object.entries(raw).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value]));
    return z.object({ claim: discoverySourceAuthorClaim, status: text, author_id: text.nullable(), profile_url: text.nullable(), duplicate_of: text.nullable(), stop_code: text.nullable(), captured_at: z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]) }).parse({ claim: { campaign_id: campaign, expected_revision: saved.expected_revision, search_hit_id: hit, platform: saved.platform, profile_ref: saved.profile_ref, source_url: saved.source_url, post_id: saved.post_id, call_number: saved.call_number }, status: saved.status, author_id: saved.author_id, profile_url: saved.profile_url, duplicate_of: saved.duplicate_of, stop_code: saved.stop_code, captured_at: saved.updated_at });
  }
  claim(campaign: string, revision: number, hit: string): DiscoverySourceAuthorClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      const head = this.#db.prepare("SELECT c.used_calls,c.max_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND v.approved_at IS NOT NULL").get(campaign, revision);
      if (!head) throw new Error("discovery_source_author_not_approved"); const budget = z.object({ used_calls: u32, max_calls: u32.nullable() }).parse(head); if (budget.max_calls !== null && budget.used_calls >= budget.max_calls) throw new Error("discovery_budget_exhausted");
      const source = this.#db.prepare(`SELECT h.profile_ref,h.source_url FROM discovery_search_hits h JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
        WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform='douyin' AND r.status='completed' AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_douyin_author_reads p WHERE p.search_hit_id=h.id) AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=h.discovery_campaign_id AND b.status='searching')`).get(hit, campaign);
      if (!source) throw new Error("discovery_source_author_already_attempted_or_stopped"); const post = discoveryDouyinSourcePost(text.parse(source.source_url)); if (post === null) throw new Error("discovery_source_author_source_invalid");
      const claim = discoverySourceAuthorClaim.parse({ ...source, campaign_id: campaign, expected_revision: revision, search_hit_id: hit, platform: "douyin", post_id: post, call_number: budget.used_calls + 1 }), now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(claim.call_number, now, campaign).changes !== 1) throw new Error("discovery_source_author_campaign_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='douyin' AND profile_ref=?").run(`source_author:${hit}:${claim.call_number}`, now, campaign, claim.profile_ref).changes !== 1) throw new Error("discovery_source_author_run_changed");
      this.#db.prepare("INSERT INTO discovery_douyin_author_reads VALUES(?1,?2,'douyin',?3,?4,?5,?6,?7,'reading',NULL,NULL,NULL,NULL,?8)").run(hit, campaign, claim.profile_ref, claim.source_url, claim.post_id, revision, claim.call_number, now); return claim;
    });
  }
  finish(rawClaim: DiscoverySourceAuthorClaim, rawProfile: unknown, rawContent: unknown, stop: string | null) {
    const claim = discoverySourceAuthorClaim.parse(rawClaim);
    if (discoveryDouyinSourcePost(claim.source_url) !== claim.post_id) throw new Error("discovery_source_author_source_invalid");
    let profile: Record<string, unknown> | null = null, content: Record<string, unknown> | null = null;
    if (stop === null) {
      const saved = validateSavedDiscoveryDouyinProfile(rawProfile); profile = saved; content = validateSavedDiscoveryDouyinContent(saved.author_id, claim.source_url, rawContent);
      if (Buffer.byteLength(stringifyApplicationJson(profile)) > 16000 || Buffer.byteLength(stringifyApplicationJson(content)) > 16000) throw new Error("discovery_source_author_result_too_large");
    } else { stopCode.parse(stop); if (rawProfile !== null || rawContent !== null) throw new Error("discovery_source_author_result_invalid"); }
    return this.#transaction(() => {
      const prior = this.get(claim.campaign_id, claim.search_hit_id); if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("discovery_source_author_claim_stale"); const now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare(`UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6 WHERE discovery_campaign_id=?1 AND platform='douyin' AND profile_ref=?2 AND status='searching' AND active_query=?3 AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?7 AND v.approved_at IS NOT NULL)`).run(claim.campaign_id, claim.profile_ref, `source_author:${claim.search_hit_id}:${claim.call_number}`, discoveryReadPlatformStatus(stop), sourceLocalReadFailure(stop) ? null : stop, now, claim.expected_revision).changes !== 1) throw new Error("discovery_source_author_claim_stale");
      let author: string | null = null, profileUrl: string | null = null, duplicate: string | null = null;
      if (profile !== null && content !== null) {
        author = text.parse(profile.author_id); profileUrl = text.parse(profile.profile_url);
        const previous = this.#db.prepare("SELECT search_hit_id FROM discovery_profile_reads WHERE discovery_campaign_id=? AND platform='douyin' AND profile_ref=? AND author_id=? AND expected_revision=? AND status='completed' AND stop_code IS NULL").get(claim.campaign_id, claim.profile_ref, author, claim.expected_revision); duplicate = previous ? text.parse(previous.search_hit_id) : null;
        this.#db.prepare("INSERT INTO discovery_search_authors VALUES(?,?,?,?)").run(claim.search_hit_id, author, profileUrl, now);
        if (duplicate === null) {
          this.#db.prepare("INSERT INTO discovery_profile_reads VALUES(?1,?2,'douyin',?3,?4,?5,?6,'completed',?7,NULL,?8)").run(claim.search_hit_id, claim.campaign_id, claim.profile_ref, author, claim.expected_revision, claim.call_number, stringifyApplicationJson(profile), now);
          this.#db.prepare("INSERT INTO discovery_content_reads VALUES(?1,?2,'douyin',?3,?4,?5,?6,?7,'completed',?8,NULL,?9)").run(claim.search_hit_id, claim.campaign_id, claim.profile_ref, author, claim.source_url, claim.expected_revision, claim.call_number, stringifyApplicationJson(content), now);
          this.#profiles.projectSaved(claim.campaign_id, claim.search_hit_id);
        }
      }
      if (this.#db.prepare("UPDATE discovery_douyin_author_reads SET status=?3,author_id=?4,profile_url=?5,duplicate_of=?6,stop_code=?7,updated_at=?8 WHERE search_hit_id=?1 AND call_number=?2 AND status='reading'").run(claim.search_hit_id, claim.call_number, stop === null ? "completed" : "failed", author, profileUrl, duplicate, stop, now).changes !== 1) throw new Error("discovery_source_author_receipt_stale"); return this.get(claim.campaign_id, claim.search_hit_id);
    });
  }
  interrupt(raw: DiscoverySourceAuthorClaim): void {
    const claim = discoverySourceAuthorClaim.parse(raw);
    this.#transaction(() => {
      const prior = this.get(claim.campaign_id, claim.search_hit_id); if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("discovery_source_author_claim_stale"); const now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_douyin_author_reads SET status='failed',stop_code='interrupted_source_author_read',updated_at=? WHERE search_hit_id=? AND status='reading'").run(now, claim.search_hit_id).changes !== 1) throw new Error("discovery_source_author_claim_stale");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='douyin' AND profile_ref=? AND status='searching' AND active_query=?").run(now, claim.campaign_id, claim.profile_ref, `source_author:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("discovery_source_author_claim_stale");
    });
  }
}
