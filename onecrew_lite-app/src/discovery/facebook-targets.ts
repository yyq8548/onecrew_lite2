import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { DiscoveryProfileRepository, discoveryProfileClaim, type DiscoveryProfileClaim } from "./profile-repository.ts";
import { validateDiscoveryContentResult } from "./content-result.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";

const text = z.string(), u32 = z.number().int().min(0).max(4294967295);
export interface DiscoveryFacebookTargetsRead { claim: DiscoveryProfileClaim; status: string; source_urls: string[] | null; stop_code: string | null; captured_at: number | bigint }
export const discoveryFacebookReelValid = (source: string) => /^https:\/\/www\.facebook\.com\/reel\/[1-9]\d{0,24}$/u.test(source);
export class DiscoveryFacebookTargetsRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #transaction<T>(body: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  next(campaign: string): string | null {
    const row = this.#db.prepare(`SELECT p.search_hit_id FROM discovery_profile_reads p
      JOIN discovery_campaigns c ON c.id=p.discovery_campaign_id
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_search_hits h ON h.id=p.search_hit_id JOIN discovery_search_authors a ON a.search_hit_id=h.id
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=p.platform AND r.profile_ref=p.profile_ref
      WHERE c.id=? AND c.phase='active' AND v.approved_at IS NOT NULL AND(c.max_calls IS NULL OR c.used_calls<c.max_calls)
        AND p.platform='facebook' AND p.expected_revision=c.current_revision AND p.status='completed' AND p.stop_code IS NULL
        AND h.discovery_campaign_id=c.id AND h.platform=p.platform AND h.profile_ref=p.profile_ref AND a.author_id=p.author_id
        AND a.author_url='https://www.facebook.com/profile.php?id='||p.author_id
        AND(h.source_url=a.author_url OR(json_extract(p.native_profile_json,'$.adapter_version')='2.0.0'
          AND lower(json_extract(p.native_profile_json,'$.observed_profile_url'))=lower(rtrim(h.source_url,'/'))
          AND EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i WHERE i.search_hit_id=h.id AND i.status='completed' AND i.discovery_campaign_id=c.id AND i.profile_ref=p.profile_ref AND i.expected_revision=c.current_revision AND i.source_url=h.source_url AND json_extract(i.native_identity_json,'$.id')=p.author_id)))
        AND json_extract(p.native_profile_json,'$.id')=p.author_id AND r.status='completed'
        AND NOT EXISTS(SELECT 1 FROM discovery_facebook_content_targets t WHERE t.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')
      ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1`).get(campaign);
    return row === undefined ? null : text.parse(row.search_hit_id);
  }
  claim(campaign: string, revision: number, hit: string): DiscoveryProfileClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      if (this.next(campaign) !== hit) throw new Error("Facebook targets require an unattempted, approved completed profile");
      const profile = new DiscoveryProfileRepository(this.#db).get(campaign, hit);
      if (profile.claim.expected_revision !== revision) throw new Error("Facebook target revision changed");
      const timestamp = Math.floor(Date.now() / 1000), updated = this.#db.prepare("UPDATE discovery_campaigns SET used_calls=used_calls+1,updated_at=? WHERE id=? AND current_revision=? AND phase='active' AND(max_calls IS NULL OR used_calls<max_calls) RETURNING used_calls").get(timestamp, campaign, revision);
      const claim = { ...profile.claim, call_number: u32.parse(updated?.used_calls) };
      this.#db.prepare("INSERT INTO discovery_facebook_content_targets VALUES(?,?,'facebook',?,?,?,?,?,'reading',NULL,NULL,?)").run(hit, campaign, claim.profile_ref, claim.author_id, claim.author_url, revision, claim.call_number, timestamp);
      this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='facebook' AND profile_ref=? AND status='completed'").run(`targets:${hit}:${claim.call_number}`, timestamp, campaign, claim.profile_ref);
      return claim;
    });
  }
  get(campaign: string, hit: string): DiscoveryFacebookTargetsRead {
    const statement = this.#db.prepare(`SELECT t.expected_revision,t.profile_ref,t.author_id,t.profile_url,t.call_number,t.status,t.source_urls_json,t.stop_code,t.updated_at,h.source_url
      FROM discovery_facebook_content_targets t JOIN discovery_search_hits h ON h.id=t.search_hit_id
      WHERE t.discovery_campaign_id=? AND t.search_hit_id=? AND h.discovery_campaign_id=t.discovery_campaign_id AND h.platform=t.platform AND h.profile_ref=t.profile_ref`);
    statement.setReadBigInts(true); const raw = statement.get(campaign, hit);
    if (!raw) throw new Error("Facebook target receipt not found");
    const row = z.object({ expected_revision: z.bigint().min(0n).max(4294967295n), profile_ref: text, author_id: text, profile_url: text, call_number: z.bigint().min(0n).max(4294967295n), status: text, source_urls_json: text.nullable(), stop_code: text.nullable(), updated_at: z.bigint().min(-9223372036854775808n).max(9223372036854775807n), source_url: text }).parse(raw);
    return { claim: { campaign_id: campaign, search_hit_id: hit, expected_revision: Number(row.expected_revision), platform: "facebook", profile_ref: row.profile_ref, author_id: row.author_id, author_url: row.profile_url, source_url: row.source_url, call_number: Number(row.call_number) }, status: row.status, source_urls: row.source_urls_json === null ? null : z.array(text).parse(parseApplicationJson(row.source_urls_json)), stop_code: row.stop_code, captured_at: row.updated_at >= BigInt(Number.MIN_SAFE_INTEGER) && row.updated_at <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(row.updated_at) : row.updated_at };
  }
  finish(raw: DiscoveryProfileClaim, urls: readonly string[] | null, stop: string | null): DiscoveryFacebookTargetsRead {
    const claim = discoveryProfileClaim.parse(raw);
    if (urls !== null && stop === null && urls.length >= 1 && urls.length <= 3) {
      if (new Set(urls).size !== urls.length || !urls.every(discoveryFacebookReelValid)) throw new Error("Facebook target locators are invalid or duplicated");
    } else if (urls === null && stop !== null) {
      const { author_url: _url, ...content } = claim;
      validateDiscoveryContentResult(content, null, stop);
    } else throw new Error("Facebook target result must contain locators or a definite stop");
    return this.#transaction(() => {
      const prior = this.get(claim.campaign_id, claim.search_hit_id);
      if (!isDeepStrictEqual(prior.claim, claim)) throw new Error("Facebook target claim changed");
      if (!this.#db.prepare(`SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND v.approved_at IS NOT NULL`).get(claim.campaign_id, claim.expected_revision)) throw new Error("Facebook target authorization changed");
      if (prior.status !== "reading") { if (isDeepStrictEqual(prior.source_urls, urls) && prior.stop_code === stop) return prior; throw new Error("Facebook targets already finished"); }
      const timestamp = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status=?,active_query=NULL,stop_code=?,updated_at=? WHERE discovery_campaign_id=? AND platform='facebook' AND profile_ref=? AND status='searching' AND active_query=?").run(discoveryReadPlatformStatus(stop), sourceLocalReadFailure(stop) ? null : stop, timestamp, claim.campaign_id, claim.profile_ref, `targets:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("Facebook target run changed");
      this.#db.prepare("UPDATE discovery_facebook_content_targets SET status=?,source_urls_json=?,stop_code=?,updated_at=? WHERE search_hit_id=? AND call_number=? AND status='reading'").run(urls === null ? "failed" : "completed", urls === null ? null : stringifyApplicationJson(urls), stop, timestamp, claim.search_hit_id, claim.call_number);
      return this.get(claim.campaign_id, claim.search_hit_id);
    });
  }
  interrupt(claim: DiscoveryProfileClaim): void {
    this.#transaction(() => {
      const prior = this.get(claim.campaign_id, claim.search_hit_id), timestamp = Math.floor(Date.now() / 1000);
      if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("Facebook target claim changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_read',updated_at=? WHERE discovery_campaign_id=? AND platform='facebook' AND profile_ref=? AND status='searching' AND active_query=?").run(timestamp, claim.campaign_id, claim.profile_ref, `targets:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("Facebook target run changed");
      this.#db.prepare("UPDATE discovery_facebook_content_targets SET status='failed',stop_code='interrupted_read',updated_at=? WHERE search_hit_id=? AND call_number=? AND status='reading'").run(timestamp, claim.search_hit_id, claim.call_number);
    });
  }
}
