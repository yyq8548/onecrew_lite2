import { readFileSync } from "node:fs";
import type { DatabaseSync } from "node:sqlite";
import { discoveryReadBusySql } from "./read-capacity.ts";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { assertValidatedCreatorConnection } from "../storage/creator-schema.ts";
import { assertDiscoveryReadHealthy } from "./read-health.ts";
import { loadDiscoveryReviewBasis } from "./review-basis.ts";
import { loadDiscoveryContentReceipt, loadDiscoveryLanguageSamples, loadDiscoveryProfileReceipt } from "./review-receipts.ts";
import { loadDiscoveryWebSourceReceipt } from "./web-receipt.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";
import { discoveryLanguageClaim, validDiscoveryLanguageSample, type DiscoveryLanguageClaim, type DiscoveryLanguageSample } from "./language-contracts.ts";

const eligibleSql = readFileSync(runtimeAssetPath("discovery/language-eligible.sql", new URL("./language-eligible.sql", import.meta.url)), "utf8");
const platforms = new Set(["twitter", "youtube", "bilibili", "tiktok", "facebook", "instagram", "weibo", "bluesky", "hackernews", "linkedin", "douyin", "google", "yahoo", "xiaohongshu"]);
const text = z.string().refine(value => value.isWellFormed());
const object = (value: unknown) => z.record(z.string(), z.unknown()).parse(value);
function observedSource(read: ReturnType<typeof loadDiscoveryContentReceipt>): string | null {
  if (read.status !== "completed" || read.stop_code !== null) return null;
  return ["instagram", "linkedin"].includes(read.claim.platform) ? text.parse(object(read.native_content).source_url) : read.claim.source_url;
}
function twitterHandle(source: string): string {
  const match = /^https:\/\/x\.com\/([A-Za-z0-9_]{1,15})\/status\/[1-9][0-9]{0,19}$/u.exec(source);
  if (!match) throw new Error("language_native_content_binding_invalid");
  return match[1]!.toLowerCase();
}
function utf8Prefix(value: string, maximum: number): string {
  let result = "", size = 0;
  for (const character of value) { size += Buffer.byteLength(character); if (size > maximum) break; result += character; }
  return result;
}

export class DiscoveryLanguageRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { assertValidatedCreatorConnection(database); this.#db = database; }
  #transaction<T>(body: () => T): T {
    this.#db.exec("BEGIN IMMEDIATE"); let committed = false;
    try { const result = body(); this.#db.exec("COMMIT"); committed = true; return result; }
    finally { if (!committed) this.#db.exec("ROLLBACK"); }
  }
  eligible(campaign: string, hit: string): boolean {
    if (this.#db.prepare("SELECT 1 FROM discovery_content_reads WHERE discovery_campaign_id=? AND search_hit_id=? AND platform='linkedin' AND status='failed' AND stop_code='linkedin_content_unavailable'").get(campaign,hit)) return false;
    // 主页简介不能替代作者原帖的语言样本。
    if (this.#db.prepare("SELECT 1 FROM discovery_web_source_reads WHERE discovery_campaign_id=? AND search_hit_id=? AND target_platform IN ('instagram','tiktok')").get(campaign, hit)) return false;
    return this.#db.prepare(eligibleSql.replace("__DISCOVERY_READ_AVAILABLE__", `NOT ${discoveryReadBusySql(this.#db, campaign, "h.platform")} AND NOT ${discoveryReadBusySql(this.#db, campaign, "c.platform")}`)).get(campaign, hit)!.eligible === 1;
  }
  source(campaign: string, hit: string) {
    return z.object({ platform: text, profile_ref: text }).parse(this.#db.prepare("SELECT platform,profile_ref FROM discovery_search_hits WHERE discovery_campaign_id=? AND id=?").get(campaign, hit));
  }
  readContext(raw: DiscoveryLanguageClaim) {
    const claim = discoveryLanguageClaim.parse(raw);
    return this.#transaction(() => {
      const active = this.#db.prepare("SELECT 1 FROM discovery_language_samples s JOIN discovery_platform_runs p ON p.discovery_campaign_id=s.discovery_campaign_id WHERE s.discovery_campaign_id=?1 AND s.criteria_revision=?2 AND s.candidate_id=?3 AND s.search_hit_id=?4 AND s.ordinal=?5 AND s.call_number=?6 AND s.status='reading' AND p.platform=?7 AND p.profile_ref=?8 AND p.status='searching' AND p.active_query=?9").get(claim.campaign_id, claim.revision, claim.candidate_id, claim.search_hit_id, claim.ordinal, claim.call_number, claim.platform, claim.profile_ref, `language:${claim.search_hit_id}:${claim.call_number}`);
      if (!active) throw new Error("language_claim_changed");
      const body = ["douyin", "linkedin", "hackernews", "bluesky", "instagram", "weibo", "xiaohongshu"].includes(claim.platform) ? loadDiscoveryContentReceipt(this.#db, claim.campaign_id, claim.search_hit_id) : null;
      const profile = claim.platform === "linkedin" ? loadDiscoveryProfileReceipt(this.#db, claim.campaign_id, claim.search_hit_id) : null;
      const web = ["google", "yahoo"].includes(claim.platform) ? loadDiscoveryWebSourceReceipt(this.#db, claim.campaign_id, claim.search_hit_id) : null;
      const joint = claim.platform === "douyin" ? this.#db.prepare("SELECT expected_revision,profile_ref,source_url,post_id,call_number,status,stop_code,author_id,duplicate_of FROM discovery_douyin_author_reads WHERE discovery_campaign_id=? AND search_hit_id=?").get(claim.campaign_id, claim.search_hit_id) : null;
      return { body, profile, web, joint };
    });
  }
  next(campaign: string, platform: string, held: readonly string[] = []): string | null {
    if (!platforms.has(platform)) return null;
    const rows = this.#db.prepare("SELECT h.id FROM discovery_search_hits h JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]') WHERE h.discovery_campaign_id=?1 AND h.platform=?2 AND c.verdict='pending' ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id").all(campaign, platform);
    for (const row of rows) { const id = text.parse(row.id); if (!held.includes(id) && this.eligible(campaign, id)) return id; }
    return null;
  }
  claim(campaign: string, revision: number, hit: string): DiscoveryLanguageClaim | null {
    return this.#transaction(() => {
      assertDiscoveryReadHealthy(this.#db, campaign, "content", hit);
      if (!this.eligible(campaign, hit)) return null;
      const basis = loadDiscoveryReviewBasis(this.#db, campaign, revision, hit), previous = loadDiscoveryLanguageSamples(this.#db, campaign, revision, basis.candidate_id);
      const now = Math.floor(Date.now() / 1000);
      const used = this.#db.prepare("UPDATE discovery_campaigns SET used_calls=used_calls+?3,updated_at=?4 WHERE id=?1 AND current_revision=?2 AND phase='active' RETURNING used_calls").get(campaign, revision, 1, now);
      if (!used) return null;
      let savedContentUrls: string[] = [];
      if (basis.hit.platform === "facebook") {
        const saved = this.#db.prepare("SELECT source_urls_json FROM discovery_facebook_content_targets WHERE discovery_campaign_id=? AND search_hit_id=?").get(campaign, hit);
        if (!saved || saved.source_urls_json === null) throw new Error("language_author_unavailable");
        savedContentUrls = z.array(text).parse(parseApplicationJson(Buffer.from(text.parse(saved.source_urls_json))));
        if (!savedContentUrls.some(url => !previous.some(sample => sample.source_url === url))) throw new Error("language_author_unavailable");
      }
      let authorHandle: string, root: string;
      const platform = basis.hit.platform, content = basis.native_content;
      if (platform === "linkedin" || platform === "instagram") {
        const patterns = { linkedin: /^https:\/\/www\.linkedin\.com\/in\/([A-Za-z0-9][A-Za-z0-9_-]{0,99})\/$/u, instagram: /^https:\/\/www\.instagram\.com\/(.*)\/$/u };
        const handle = patterns[platform].exec(basis.hit.author_url ?? "")?.[1];
        if (handle === undefined || platform === "linkedin" && ["me", "self"].includes(handle.toLowerCase())) throw new Error("language_author_unavailable");
        authorHandle = handle;
        root = content ? observedSource(content) ?? "" : "";
        if (!root || platform === "linkedin" && !/^https:\/\/www\.linkedin\.com\/feed\/update\/urn:li:activity:[1-9][0-9]{0,19}\/$/u.test(root)) throw new Error("language_author_unavailable");
      } else if (platform === "facebook") {
        if (!content) throw new Error("language_author_unavailable"); authorHandle = basis.native_author_id; root = content.claim.source_url;
      } else if (platform === "tiktok") { authorHandle = basis.hit.author_label; root = basis.hit.source_url; }
      else if (platform === "google" || platform === "yahoo") {
        const web = loadDiscoveryWebSourceReceipt(this.#db, campaign, hit);
        if (!web.is_primary_source || web.status !== "completed" || web.expected_revision !== revision || web.profile_ref !== basis.hit.profile_ref || web.native_author_id !== basis.native_author_id) throw new Error("language_native_content_binding_invalid");
        authorHandle = web.target_platform === "twitter" ? twitterHandle(web.source_url) : basis.native_author_id;
        root = web.target_platform === "youtube" ? `https://www.youtube.com/watch?v=${web.video_id}` : web.source_url;
      } else if (["youtube", "bilibili", "weibo", "bluesky", "hackernews", "douyin", "xiaohongshu"].includes(platform)) { authorHandle = basis.native_author_id; root = basis.hit.source_url; }
      else { const identity = object(object(content?.native_content).identity); authorHandle = text.parse(identity.handle); root = text.parse(identity.source_url); }
      const claim = discoveryLanguageClaim.parse({ campaign_id: campaign, revision, search_hit_id: hit, candidate_id: basis.candidate_id, platform, profile_ref: basis.hit.profile_ref, author_id: basis.native_author_id, author_handle: authorHandle, root_url: root, ordinal: previous.length + 1, call_number: used.used_calls, previous_urls: previous.map(sample => sample.source_url), saved_content_urls: savedContentUrls });
      this.#db.prepare("INSERT INTO discovery_language_samples(discovery_campaign_id,criteria_revision,candidate_id,search_hit_id,ordinal,call_number,status,sample_json,stop_code,created_at,sample_format_version) VALUES(?1,?2,?3,?4,?5,?6,'reading',NULL,NULL,?7,2)").run(campaign, revision, claim.candidate_id, hit, claim.ordinal, claim.call_number, now);
      this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?4,updated_at=?5 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3").run(campaign, platform, claim.profile_ref, `language:${hit}:${claim.call_number}`, now);
      return claim;
    });
  }
  finish(raw: DiscoveryLanguageClaim, sample: DiscoveryLanguageSample | null, stop: string | null): void {
    const claim = discoveryLanguageClaim.parse(raw);
    if ((sample !== null) === (stop !== null) || sample !== null && !validDiscoveryLanguageSample(sample, claim)) throw new Error("language_sample_invalid");
    this.#transaction(() => {
      this.#assertNativeBinding(claim, sample);
      const changed = this.#db.prepare("UPDATE discovery_language_samples SET status=?7,sample_json=?8,stop_code=?9 WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3 AND search_hit_id=?4 AND ordinal=?5 AND call_number=?6 AND status='reading' AND EXISTS(SELECT 1 FROM discovery_campaigns c WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active')").run(claim.campaign_id, claim.revision, claim.candidate_id, claim.search_hit_id, claim.ordinal, claim.call_number, sample !== null ? "completed" : "failed", sample === null ? null : stringifyApplicationJson(sample), stop);
      if (changed.changes !== 1) throw new Error("language_claim_changed");
      const blocked = ["google", "yahoo"].includes(claim.platform) && (stop === "opencli_auth_required" || stop === "opencli_risk_control"), now = Math.floor(Date.now() / 1000);
      if (blocked) this.#db.prepare("UPDATE discovery_platform_runs SET status=?4,stop_code=?3,active_query=NULL,updated_at=?5 WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2 AND status!='searching'").run(claim.campaign_id, claim.profile_ref, stop, discoveryReadPlatformStatus(stop), now);
      const runStop = !blocked && stop !== "language_sources_exhausted" && !sourceLocalReadFailure(stop) ? stop : null;
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status=?6,stop_code=?7,active_query=NULL,updated_at=?5 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND status='searching' AND active_query=?4").run(claim.campaign_id, claim.platform, claim.profile_ref, `language:${claim.search_hit_id}:${claim.call_number}`, now, blocked ? "completed" : discoveryReadPlatformStatus(stop), runStop).changes !== 1) throw new Error("language_claim_changed");
    });
  }
  #assertNativeBinding(claim: DiscoveryLanguageClaim, sample: DiscoveryLanguageSample | null): void {
    if (["google", "yahoo"].includes(claim.platform)) {
      const web = loadDiscoveryWebSourceReceipt(this.#db, claim.campaign_id, claim.search_hit_id), native = object(web.native), original = text.parse(["twitter", "douyin"].includes(web.target_platform) ? object(native.content).text : native.description);
      if (web.status !== "completed" || !web.is_primary_source || web.expected_revision !== claim.revision || web.origin_platform !== claim.platform || web.profile_ref !== claim.profile_ref || web.native_author_id !== claim.author_id || web.candidate_id !== claim.candidate_id || web.call_number >= claim.call_number || claim.author_handle !== (web.target_platform === "twitter" ? twitterHandle(web.source_url) : claim.author_id) || claim.root_url !== (web.target_platform === "youtube" ? `https://www.youtube.com/watch?v=${web.video_id}` : web.source_url) || sample !== null && sample.text !== utf8Prefix(original, web.target_platform === "douyin" ? 12000 : 8000)) throw new Error("language_native_content_binding_invalid");
    }
    if (sample === null || !["douyin", "linkedin", "hackernews"].includes(claim.platform)) return;
    const body = loadDiscoveryContentReceipt(this.#db, claim.campaign_id, claim.search_hit_id), native = object(body.native_content);
    if (claim.platform === "douyin") {
      const basis = loadDiscoveryReviewBasis(this.#db, claim.campaign_id, claim.revision, claim.search_hit_id);
      if (basis.candidate_id !== claim.candidate_id || basis.native_author_id !== claim.author_id || claim.author_handle !== claim.author_id || basis.hit.profile_ref !== claim.profile_ref || basis.hit.platform !== claim.platform || basis.hit.source_url !== claim.root_url || body.claim.call_number >= claim.call_number || observedSource(body) !== claim.root_url || native.text !== sample.text) throw new Error("language_native_content_binding_invalid");
    } else if (claim.platform === "linkedin") {
      const profile = loadDiscoveryProfileReceipt(this.#db, claim.campaign_id, claim.search_hit_id), identity = object(profile.native_profile), canonical = `https://www.linkedin.com/in/${claim.author_handle}/`;
      if (body.status !== "completed" || body.stop_code !== null || profile.status !== "completed" || profile.stop_code !== null || body.claim.expected_revision !== claim.revision || profile.claim.expected_revision !== claim.revision || body.claim.platform !== claim.platform || profile.claim.platform !== claim.platform || body.claim.profile_ref !== claim.profile_ref || profile.claim.profile_ref !== claim.profile_ref || body.claim.author_id !== claim.author_handle || profile.claim.author_id !== claim.author_handle || body.claim.source_url !== canonical || profile.claim.source_url !== canonical || profile.claim.call_number >= body.claim.call_number || body.claim.call_number >= claim.call_number || observedSource(body) !== claim.root_url || native.account_id !== claim.author_id || identity.account_id !== claim.author_id || native.text !== sample.text) throw new Error("language_native_content_binding_invalid");
    } else {
      const ids = z.array(z.union([z.number().int().safe(), z.bigint()])).parse(native.comment_ids);
      if (body.status !== "completed" || body.stop_code !== null || body.claim.expected_revision !== claim.revision || body.claim.platform !== claim.platform || body.claim.profile_ref !== claim.profile_ref || body.claim.author_id !== claim.author_id || body.claim.source_url !== sample.source_url || body.claim.call_number >= claim.call_number || native.text !== sample.text || ids.length !== sample.comments.length || ids.some((id, index) => sample.comments[index]!.source_url !== `https://news.ycombinator.com/item?id=${id}`)) throw new Error("language_native_content_binding_invalid");
    }
  }
}
