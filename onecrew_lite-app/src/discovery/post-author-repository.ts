import type { DatabaseSync } from "node:sqlite";
import { discoveryReadBusySql } from "./read-capacity.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { twitterPostContentPlan } from "../platforms/opencli/twitter-posts.ts";
import { twitterIdentityPlan } from "../platforms/opencli/profiles.ts";
import { DiscoveryProfileRepository, type DiscoveryProfileClaim } from "./profile-repository.ts";
import { projectTwitterIdentityContent } from "./twitter-observations.ts";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { assertValidatedCreatorConnection } from "../storage/creator-schema.ts";
import { getDiscoverySavedIdentity } from "./saved-read-repository.ts";
import { discoveryPostAuthorClaim, discoveryPostIdForSource, validateDiscoveryPostAuthorResult, type DiscoveryPostAuthorClaim } from "./post-author-contracts.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";

const u32 = z.number().int().min(0).max(4294967295);
export class DiscoveryPostAuthorRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { assertValidatedCreatorConnection(database); this.#db = database; }
  #transaction<T>(body: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  needs(campaign: string, hit: string): boolean { return this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_search_hits h WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform='twitter' AND NOT EXISTS(SELECT 1 FROM discovery_identity_reads i WHERE i.search_hit_id=h.id)) needed").get(hit, campaign)!.needed === 1; }
  claim(campaign: string, revision: number, hit: string): DiscoveryPostAuthorClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      const head = this.#db.prepare(`SELECT c.used_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
        WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND v.approved_at IS NOT NULL
        AND NOT ${discoveryReadBusySql(this.#db, campaign, "'twitter'")}`).get(campaign, revision);
      if (!head) throw new Error("discovery_post_author_not_approved_idle_or_budget");
      const row = this.#db.prepare(`SELECT h.profile_ref,a.author_id AS author_handle,h.source_url FROM discovery_search_hits h JOIN discovery_search_authors a ON a.search_hit_id=h.id
        JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
        WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform='twitter' AND r.status='completed' AND NOT EXISTS(SELECT 1 FROM discovery_identity_reads i WHERE i.search_hit_id=h.id)`).get(hit, campaign);
      if (!row) throw new Error("discovery_post_author_source_unavailable_or_attempted");
      const claim = discoveryPostAuthorClaim.parse({ ...row, campaign_id: campaign, expected_revision: revision, search_hit_id: hit, post_id: "", call_number: u32.parse(u32.parse(head.used_calls) + 1) });
      claim.post_id = discoveryPostIdForSource(claim.author_handle, claim.source_url); const now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(claim.call_number, now, campaign).changes !== 1) throw new Error("discovery_post_author_campaign_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='twitter' AND profile_ref=?").run(`identity:${hit}:${claim.call_number}`, now, campaign, claim.profile_ref).changes !== 1) throw new Error("discovery_post_author_run_changed");
      this.#db.prepare("INSERT INTO discovery_identity_reads VALUES(?1,?2,'twitter',?3,?4,?5,?6,?7,?8,'reading',NULL,NULL,NULL,?9)").run(hit, campaign, claim.profile_ref, claim.author_handle, claim.source_url, claim.post_id, revision, claim.call_number, now);
      return claim;
    });
  }
  finish(raw: DiscoveryPostAuthorClaim, rawResult: unknown) {
    const claim = discoveryPostAuthorClaim.parse(raw), result = validateDiscoveryPostAuthorResult(claim, rawResult);
    return this.#transaction(() => {
      const prior = getDiscoverySavedIdentity(this.#db, claim.campaign_id, claim.search_hit_id); if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("discovery_post_author_claim_stale");
      const now = Math.floor(Date.now() / 1000), runStop = sourceLocalReadFailure(result.stop_code) ? null : result.stop_code;
      if (this.#db.prepare(`UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6 WHERE discovery_campaign_id=?1 AND platform='twitter' AND profile_ref=?2 AND status='searching' AND active_query=?3
        AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?7 AND v.approved_at IS NOT NULL)`).run(claim.campaign_id, claim.profile_ref, `identity:${claim.search_hit_id}:${claim.call_number}`, discoveryReadPlatformStatus(result.stop_code), runStop, now, claim.expected_revision).changes !== 1) throw new Error("discovery_post_author_claim_stale");
      if (this.#db.prepare("UPDATE discovery_identity_reads SET status=?,account_id=?,observed_source_url=?,stop_code=?,updated_at=? WHERE search_hit_id=?").run(result.stop_code === null ? "completed" : "failed", result.account_id, result.observed_source_url, result.stop_code, now, claim.search_hit_id).changes !== 1) throw new Error("discovery_post_author_receipt_missing");
      if (result.native_content !== undefined) {
        const content = twitterPostContentPlan(claim.author_handle, claim.post_id, result.account_id!).decode([result.native_content]);
        const profile = result.native_profile == null ? null : twitterIdentityPlan(claim.author_handle, "x_tweet_detail").decode([result.native_profile]);
        if (profile !== null && (profile.account_id !== result.account_id || profile.follower_count === null)) throw new Error("discovery_post_author_profile_mismatch");
        this.#db.prepare("INSERT INTO discovery_identity_observations VALUES(?,?,?,?,?)").run(claim.search_hit_id, claim.call_number, stringifyApplicationJson(content), profile === null ? null : stringifyApplicationJson(profile), now);
        if (profile !== null) {
          this.#db.prepare("INSERT INTO discovery_profile_reads VALUES(?,?,'twitter',?,?,?,?,'completed',?,NULL,?)").run(claim.search_hit_id, claim.campaign_id, claim.profile_ref, claim.author_handle, claim.expected_revision, claim.call_number, stringifyApplicationJson(profile), now);
          this.#db.prepare("INSERT INTO discovery_profile_identity_matches VALUES(?1,?1,?2,?3)").run(claim.search_hit_id, result.account_id, now);
          new DiscoveryProfileRepository(this.#db).projectSaved(claim.campaign_id, claim.search_hit_id);
          projectTwitterIdentityContent(this.#db, { ...claim, platform: "twitter", author_id: claim.author_handle, author_url: profile.profile_url } satisfies DiscoveryProfileClaim);
        }
      }
      return getDiscoverySavedIdentity(this.#db, claim.campaign_id, claim.search_hit_id);
    });
  }
  interrupt(raw: DiscoveryPostAuthorClaim): void {
    const claim = discoveryPostAuthorClaim.parse(raw);
    this.#transaction(() => {
      const now = Math.floor(Date.now() / 1000), saved = getDiscoverySavedIdentity(this.#db, claim.campaign_id, claim.search_hit_id);
      if (!isDeepStrictEqual(saved.claim, claim) || saved.status !== "reading") throw new Error("discovery_post_author_claim_stale");
      if (this.#db.prepare("UPDATE discovery_identity_reads SET status='failed',stop_code='interrupted_identity_read',updated_at=? WHERE search_hit_id=? AND status='reading'").run(now, claim.search_hit_id).changes !== 1) throw new Error("discovery_post_author_claim_stale");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_identity_read',updated_at=? WHERE discovery_campaign_id=? AND platform='twitter' AND profile_ref=? AND status='searching' AND active_query=?").run(now, claim.campaign_id, claim.profile_ref, `identity:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("discovery_post_author_claim_stale");
    });
  }
}
