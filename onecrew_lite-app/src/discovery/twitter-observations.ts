import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { twitterPostContentPlan } from "../platforms/opencli/twitter-posts.ts";
import type { DiscoveryProfileClaim } from "./profile-repository.ts";
import { discoveryPostIdForSource } from "./post-author-contracts.ts";

// 复用原身份读取的调用编号和采集时间，不增加读取次数。
export function projectTwitterIdentityContent(db: DatabaseSync, claim: DiscoveryProfileClaim): void {
  if (!db.isTransaction) throw new Error("discovery_twitter_observation_requires_transaction");
  if (claim.platform !== "twitter" || Number(db.prepare("PRAGMA user_version").get()!.user_version) < 95) return;
  const saved = db.prepare(`SELECT observation.native_content_json,observation.captured_at,identity.account_id,identity.post_id,identity.call_number
    FROM discovery_identity_observations observation JOIN discovery_identity_reads identity ON identity.search_hit_id=observation.search_hit_id
    JOIN discovery_profile_reads profile ON profile.search_hit_id=identity.search_hit_id
    JOIN discovery_profile_identity_matches binding ON binding.search_hit_id=profile.search_hit_id AND binding.identity_search_hit_id=identity.search_hit_id AND binding.account_id=identity.account_id
    WHERE identity.search_hit_id=?1 AND identity.discovery_campaign_id=?2 AND identity.expected_revision=?3 AND identity.profile_ref=?4
      AND identity.author_handle=?5 AND identity.source_url=?6 AND identity.status='completed' AND identity.stop_code IS NULL
      AND observation.call_number=identity.call_number AND profile.expected_revision=identity.expected_revision AND profile.discovery_campaign_id=identity.discovery_campaign_id
      AND profile.profile_ref=identity.profile_ref AND profile.platform='twitter' AND profile.author_id=identity.author_handle
      AND profile.status='completed' AND profile.stop_code IS NULL AND profile.call_number>=identity.call_number
      AND json_extract(profile.native_profile_json,'$.account_id')=identity.account_id`).get(claim.search_hit_id, claim.campaign_id, claim.expected_revision, claim.profile_ref, claim.author_id, claim.source_url);
  if (!saved) return;
  const account = z.string().parse(saved.account_id), post = z.string().parse(saved.post_id);
  const native = twitterPostContentPlan(claim.author_id, post, account).decode([parseApplicationJson(Buffer.from(z.string().parse(saved.native_content_json)))]);
  if (discoveryPostIdForSource(claim.author_id, claim.source_url) !== native.identity.post_id) throw new Error("discovery_twitter_observation_source_changed");
  const prior = db.prepare("SELECT 1 FROM discovery_content_reads WHERE search_hit_id=?").get(claim.search_hit_id);
  if (prior) return;
  db.prepare("INSERT INTO discovery_content_reads VALUES(?,?,'twitter',?,?,?,?,?,'completed',?,NULL,?)")
    .run(claim.search_hit_id, claim.campaign_id, claim.profile_ref, claim.author_id, claim.source_url, claim.expected_revision, Number(saved.call_number), stringifyApplicationJson(native), Number(saved.captured_at));
  db.prepare("INSERT INTO discovery_content_identity_bindings VALUES(?1,?1,?1,?2,?3)").run(claim.search_hit_id, account, Number(saved.captured_at));
}
