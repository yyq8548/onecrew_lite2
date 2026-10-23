import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { discoveryReadBusy, discoveryReadBusySql } from "./read-capacity.ts";
import { projectTwitterIdentityContent } from "./twitter-observations.ts";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { assertValidatedCreatorConnection } from "../storage/creator-schema.ts";
import { NATIVE_READ_FAILURE_CODES, PLATFORM_READ_FAILURE_CODES } from "../platforms/opencli/errors.ts";
import { facebookSourceValid } from "../platforms/opencli/facebook-profile.ts";
import { validateSavedDiscoveryInstagramProfile } from "./contact-profile-contracts.ts";
import { validateDiscoveryRetainedProfile } from "./native-material.ts";
import { assertDiscoveryReadHealthy, discoveryReadExcludedHits } from "./read-health.ts";
import { assessedDiscoveryEvidence, assessDiscoveryProfile, assessDiscoveryDouyin, assessDiscoveryFacebook, assessDiscoveryInstagram, discoveryCriteriaForPlatform } from "./qualification.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";

const text = z.string().refine(value => value.isWellFormed());
const u32 = z.number().int().min(0).max(4294967295);
const record = z.record(text, z.unknown());
export const discoveryProfileClaim = z.strictObject({ campaign_id: text, expected_revision: u32, search_hit_id: text, platform: text, profile_ref: text, author_id: text, author_url: text, source_url: text, call_number: u32 });
export type DiscoveryProfileClaim = z.infer<typeof discoveryProfileClaim>;
const profileStops = new Set(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required"]);
for (const code of NATIVE_READ_FAILURE_CODES) profileStops.add(code);
for (const code of PLATFORM_READ_FAILURE_CODES) if (code.startsWith("linkedin_profile_") || code.startsWith("facebook_profile_")) profileStops.add(code);
export interface DiscoveryProfileReceipt { claim: DiscoveryProfileClaim; status: string; native_profile: Record<string, unknown> | null; stop_code: string | null; captured_at: number | bigint }
const sourcePlatforms = ["bilibili", "tiktok", "twitter", "facebook", "instagram", "weibo", "bluesky", "zhihu", "hackernews", "pixiv", "linkedin"];

export function validateDiscoveryProfileResult(claim: DiscoveryProfileClaim, native: Record<string, unknown> | null, stop: string | null): void {
  if (native === null) { if (stop !== null && profileStops.has(stop)) return; throw new Error("discovery_profile_result_invalid"); }
  if (stop !== null || Buffer.byteLength(stringifyApplicationJson(native)) > 16000) throw new Error("discovery_profile_result_invalid");
  const key = ({ bilibili: "mid", tiktok: "id", twitter: "handle", facebook: "id", instagram: "id", weibo: "id", bluesky: "did", zhihu: "author_id", hackernews: "author_id", pixiv: "author_id", linkedin: "author_handle" } as Record<string, string>)[claim.platform];
  if (!key || native[key] !== claim.author_id || native[claim.platform === "zhihu" ? "author_url" : "profile_url"] !== claim.author_url) throw new Error("discovery_profile_result_invalid");
  if (claim.platform === "instagram") validateSavedDiscoveryInstagramProfile(claim.author_id, native);
  else if (claim.platform === "twitter") {
    if (native.identity_kind !== "native_id" || typeof native.account_id !== "string" || !/^[1-9][0-9]{0,19}$/u.test(native.account_id) || native.follower_count !== null && !z.number().int().nonnegative().safe().safeParse(native.follower_count).success) throw new Error("discovery_profile_result_invalid");
  } else if (claim.platform === "bilibili" || claim.platform === "tiktok") {
    z.union([z.number().int().nonnegative().safe(), z.bigint().min(0n).max(18446744073709551615n)]).parse(native.follower_count);
  } else validateDiscoveryRetainedProfile(claim.platform, claim.author_id, claim.author_url, claim.source_url, native);
}

export class DiscoveryProfileRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { assertValidatedCreatorConnection(database); this.#db = database; }
  #transaction<T>(body: () => T): T {
    this.#db.exec("BEGIN IMMEDIATE"); let committed = false;
    try { const result = body(); this.#db.exec("COMMIT"); committed = true; return result; }
    finally { if (!committed) this.#db.exec("ROLLBACK"); }
  }
  #load(campaign: string, hit: string): DiscoveryProfileReceipt {
    const statement = this.#db.prepare(`SELECT p.expected_revision,p.platform,p.profile_ref,p.author_id,a.author_url,h.source_url,p.call_number,p.status,p.native_profile_json,p.stop_code,p.updated_at
      FROM discovery_profile_reads p JOIN discovery_search_hits h ON h.id=p.search_hit_id JOIN discovery_search_authors a ON a.search_hit_id=h.id
      WHERE p.discovery_campaign_id=? AND p.search_hit_id=? AND h.discovery_campaign_id=p.discovery_campaign_id AND h.platform=p.platform AND h.profile_ref=p.profile_ref AND a.author_id=p.author_id`);
    statement.setReadBigInts(true);
    const values = statement.get(campaign, hit), raw = values === undefined ? undefined : Object.fromEntries(Object.entries(values).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value]));
    if (!raw) throw new Error("discovery_profile_read_missing");
    const row = z.object({ expected_revision: u32, platform: text, profile_ref: text, author_id: text, author_url: text, source_url: text, call_number: u32, status: text, native_profile_json: text.nullable(), stop_code: text.nullable(), updated_at: z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]) }).parse(raw);
    return { claim: { campaign_id: campaign, search_hit_id: hit, expected_revision: row.expected_revision, platform: row.platform, profile_ref: row.profile_ref, author_id: row.author_id, author_url: row.author_url, source_url: row.source_url, call_number: row.call_number }, status: row.status, native_profile: row.native_profile_json === null ? null : record.parse(parseApplicationJson(row.native_profile_json)), stop_code: row.stop_code, captured_at: row.updated_at };
  }
  get(campaign: string, hit: string): DiscoveryProfileReceipt { return this.#load(campaign, hit); }
  source(campaign: string, hit: string) {
    const source = this.#db.prepare("SELECT h.platform,h.profile_ref,a.author_id,a.author_url,h.source_url FROM discovery_search_hits h JOIN discovery_search_authors a ON a.search_hit_id=h.id WHERE h.id=? AND h.discovery_campaign_id=?").get(hit, campaign);
    if (!source || !sourcePlatforms.includes(text.parse(source.platform))) throw new Error("discovery_profile_source_unobserved");
    return discoveryProfileClaim.omit({ campaign_id: true, expected_revision: true, search_hit_id: true, call_number: true }).parse(source);
  }
  next(campaign: string, excludedPlatforms: readonly string[] = [], heldHits: readonly string[] = [], platform: string | null = null): string | null {
    const held = [...heldHits, ...discoveryReadExcludedHits(this.#db, campaign, "profile")];
    const row = this.#db.prepare(`SELECT h.id FROM discovery_search_hits h JOIN discovery_search_authors a ON a.search_hit_id=h.id
      JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      LEFT JOIN discovery_profile_reads p ON p.discovery_campaign_id=c.id AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=a.author_id
      WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL
        AND h.platform IN ('bilibili','tiktok','twitter','facebook','instagram','weibo','bluesky','zhihu','hackernews','pixiv','linkedin')
        AND h.platform NOT IN(SELECT value FROM json_each(?2)) AND h.id NOT IN(SELECT value FROM json_each(?3))
        AND (?4 IS NULL OR h.platform=?4)
        AND NOT EXISTS(SELECT 1 FROM json_each(v.criteria_json,'$.excluded_accounts') excluded WHERE json_extract(excluded.value,'$.profile_url')=a.author_url)
        AND NOT EXISTS(SELECT 1 FROM discovery_identity_reads i WHERE i.search_hit_id=h.id AND i.status!='completed')
        AND NOT ${discoveryReadBusySql(this.#db, campaign, "h.platform")}
        AND ((r.status='completed' AND p.search_hit_id IS NULL) OR (r.status='technical_stop' AND r.stop_code='interrupted_profile_read' AND p.status='reading' AND p.search_hit_id=h.id AND p.expected_revision=c.current_revision AND r.active_query='profile:'||h.id||':'||p.call_number))
      ORDER BY CASE WHEN p.status='reading' THEN 0 ELSE 1 END,
        CASE WHEN h.platform='linkedin' THEN (SELECT COUNT(*) FROM discovery_profile_reads seen JOIN discovery_search_hits origin ON origin.id=seen.search_hit_id
          WHERE seen.discovery_campaign_id=c.id AND seen.platform=h.platform AND seen.profile_ref=h.profile_ref AND origin.search_keyword=h.search_keyword) ELSE 0 END,
        h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1`).get(campaign, stringifyApplicationJson(excludedPlatforms), stringifyApplicationJson(held), platform);
    return row ? text.parse(row.id) : null;
  }
  recoverInterrupted(): number {
    return this.#transaction(() => Number(this.#db.prepare(`UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_profile_read',updated_at=? WHERE status='searching'
      AND EXISTS(SELECT 1 FROM discovery_profile_reads p JOIN discovery_campaigns c ON c.id=p.discovery_campaign_id
        JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
        WHERE p.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND p.platform=discovery_platform_runs.platform AND p.profile_ref=discovery_platform_runs.profile_ref
          AND p.status='reading' AND p.expected_revision=c.current_revision AND c.phase='active' AND v.approved_at IS NOT NULL AND discovery_platform_runs.active_query='profile:'||p.search_hit_id||':'||p.call_number)`).run(Math.floor(Date.now() / 1000)).changes));
  }
  interrupt(raw: DiscoveryProfileClaim): void {
    const claim = discoveryProfileClaim.parse(raw);
    this.#transaction(() => {
      if (this.#db.prepare(`UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_profile_read',updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND status='searching' AND active_query=?
        AND EXISTS(SELECT 1 FROM discovery_profile_reads p WHERE p.search_hit_id=? AND p.expected_revision=? AND p.call_number=? AND p.status='reading')`).run(Math.floor(Date.now() / 1000), claim.campaign_id, claim.platform, claim.profile_ref, `profile:${claim.search_hit_id}:${claim.call_number}`, claim.search_hit_id, claim.expected_revision, claim.call_number).changes !== 1) throw new Error("discovery_profile_claim_stale");
    });
  }
  #matchingXIdentity(claim: DiscoveryProfileClaim): string {
    const row = this.#db.prepare(`SELECT expected_revision,profile_ref,author_handle,source_url,call_number,status,account_id FROM discovery_identity_reads WHERE discovery_campaign_id=? AND search_hit_id=?`).get(claim.campaign_id, claim.search_hit_id);
    if (!row || row.status !== "completed" || row.expected_revision !== claim.expected_revision || row.profile_ref !== claim.profile_ref || row.author_handle !== claim.author_id || row.source_url !== claim.source_url || typeof row.call_number !== "number" || row.call_number >= claim.call_number || typeof row.account_id !== "string") throw new Error("discovery_profile_identity_required");
    return row.account_id;
  }
  claim(campaign: string, revision: number, hit: string): DiscoveryProfileClaim {
    u32.parse(revision); text.parse(campaign); text.parse(hit);
    return this.#transaction(() => {
      assertDiscoveryReadHealthy(this.#db, campaign, "profile", hit);
      const rawHead = this.#db.prepare(`SELECT c.used_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND v.approved_at IS NOT NULL`).get(campaign, revision);
      if (!rawHead) throw new Error("discovery_profile_not_approved");
      const head = z.object({ used_calls: u32 }).parse(rawHead);
      const source = this.source(campaign, hit);
      if (discoveryReadBusy(this.#db, campaign, [source.platform])) throw new Error("discovery_read_active");
      if (this.#db.prepare("SELECT EXISTS(SELECT 1 FROM json_each(v.criteria_json,'$.excluded_accounts') excluded WHERE json_extract(excluded.value,'$.profile_url')=?) AS excluded FROM discovery_campaign_revisions v WHERE v.discovery_campaign_id=? AND v.revision=? AND v.approved_at IS NOT NULL").get(source.author_url!, campaign, revision)!.excluded === 1) throw new Error("discovery_profile_account_excluded");
      const claim = discoveryProfileClaim.parse({ ...source, campaign_id: campaign, expected_revision: revision, search_hit_id: hit, call_number: head.used_calls + 1 });
      const run = z.object({ status: text, stop_code: text.nullable(), active_query: text.nullable() }).parse(this.#db.prepare("SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").get(campaign, claim.platform, claim.profile_ref));
      const prior = this.#db.prepare("SELECT search_hit_id,call_number,status,expected_revision FROM discovery_profile_reads WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND author_id=?").get(campaign, claim.platform, claim.profile_ref, claim.author_id);
      const interrupted = prior !== undefined && prior.search_hit_id === hit && prior.status === "reading" && prior.expected_revision === revision && run.status === "technical_stop" && run.stop_code === "interrupted_profile_read" && run.active_query === `profile:${hit}:${prior.call_number}`;
      if (!interrupted && (run.status !== "completed" || prior !== undefined)) throw new Error("discovery_profile_already_attempted_or_stopped");
      if (claim.platform === "twitter") this.#matchingXIdentity(claim);
      if (claim.platform === "facebook" && facebookSourceValid(claim.source_url) && this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads WHERE search_hit_id=? AND discovery_campaign_id=? AND profile_ref=? AND expected_revision=? AND source_url=? AND status='completed' AND stop_code IS NULL AND call_number<? AND json_extract(native_identity_json,'$.id') IS ?) AS matched`).get(hit, campaign, claim.profile_ref, revision, claim.source_url, claim.call_number, claim.author_id)!.matched !== 1) throw new Error("discovery_facebook_identity_required");
      const now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(claim.call_number, now, campaign).changes !== 1) throw new Error("discovery_profile_campaign_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").run(`profile:${hit}:${claim.call_number}`, now, campaign, claim.platform, claim.profile_ref).changes !== 1) throw new Error("discovery_profile_run_changed");
      this.#db.prepare(`INSERT INTO discovery_profile_reads VALUES(?,?,?,?,?,?,?,'reading',NULL,NULL,?) ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at`).run(hit, campaign, claim.platform, claim.profile_ref, claim.author_id, revision, claim.call_number, now);
      return claim;
    });
  }
  reuse(rawClaim: DiscoveryProfileClaim): DiscoveryProfileReceipt | null {
    const claim = discoveryProfileClaim.parse(rawClaim);
    const current = this.#load(claim.campaign_id, claim.search_hit_id);
    if (!isDeepStrictEqual(current.claim, claim) || current.status !== "reading") throw new Error("discovery_profile_claim_stale");
    const xId = claim.platform === "twitter" ? this.#matchingXIdentity(claim) : null;
    const sources = this.#db.prepare(`SELECT p.discovery_campaign_id,p.search_hit_id FROM discovery_profile_reads p
      JOIN discovery_search_hits h ON h.id=p.search_hit_id AND h.discovery_campaign_id=p.discovery_campaign_id AND h.platform=p.platform AND h.profile_ref=p.profile_ref
      JOIN discovery_search_authors a ON a.search_hit_id=h.id AND a.author_id=p.author_id
      WHERE p.discovery_campaign_id<>? AND p.platform=? AND p.profile_ref=? AND p.author_id=? AND a.author_url=?
        AND p.status='completed' AND p.stop_code IS NULL AND p.native_profile_json IS NOT NULL
      ORDER BY p.updated_at DESC,p.search_hit_id`).all(claim.campaign_id, claim.platform, claim.profile_ref, claim.author_id, claim.author_url);
    for (const source of sources) {
      const saved = this.#load(text.parse(source.discovery_campaign_id), text.parse(source.search_hit_id));
      const native = saved.native_profile;
      validateDiscoveryProfileResult(saved.claim, native, saved.stop_code);
      if (native === null) throw new Error("discovery_profile_result_invalid");
      if (xId !== null && native.account_id !== xId) continue;
      if (claim.platform === "facebook" && native.adapter_version === "2.0.0" && text.parse(native.observed_profile_url).toLowerCase() !== claim.source_url.replace(/\/+$/u, "").toLowerCase()) continue;
      return this.#finish(claim, native, null, saved.captured_at);
    }
    return null;
  }
  finish(rawClaim: DiscoveryProfileClaim, rawNative: unknown, stop: string | null): DiscoveryProfileReceipt {
    return this.#finish(rawClaim, rawNative, stop, null);
  }
  #finish(rawClaim: DiscoveryProfileClaim, rawNative: unknown, stop: string | null, capturedAt: number | bigint | null): DiscoveryProfileReceipt {
    const claim = discoveryProfileClaim.parse(rawClaim), native = rawNative === null ? null : record.parse(rawNative);
    validateDiscoveryProfileResult(claim, native, stop);
    return this.#transaction(() => {
      const prior = this.#load(claim.campaign_id, claim.search_hit_id);
      if (isDeepStrictEqual(prior.claim, claim) && prior.status !== "reading" && isDeepStrictEqual(prior.native_profile, native) && prior.stop_code === stop) return prior;
      if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("discovery_profile_claim_stale");
      const xId = claim.platform === "twitter" && native !== null ? this.#matchingXIdentity(claim) : null;
      if (xId !== null && native!.account_id !== xId) throw new Error("discovery_profile_native_identity_mismatch");
      const now = Math.floor(Date.now() / 1000), runStop = sourceLocalReadFailure(stop) ? null : stop;
      if (this.#db.prepare(`UPDATE discovery_platform_runs SET status=?,active_query=NULL,stop_code=?,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND status='searching' AND active_query=?
        AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=? AND c.phase='active' AND c.current_revision=? AND v.approved_at IS NOT NULL)`).run(discoveryReadPlatformStatus(stop), runStop, now, claim.campaign_id, claim.platform, claim.profile_ref, `profile:${claim.search_hit_id}:${claim.call_number}`, claim.campaign_id, claim.expected_revision).changes !== 1) throw new Error("discovery_profile_authorization_changed");
      if (this.#db.prepare("UPDATE discovery_profile_reads SET status=?,native_profile_json=?,stop_code=?,updated_at=? WHERE search_hit_id=? AND call_number=? AND status='reading'").run(stop === null ? "completed" : "failed", native === null ? null : stringifyApplicationJson(native), stop, capturedAt ?? now, claim.search_hit_id, claim.call_number).changes !== 1) throw new Error("discovery_profile_receipt_stale");
      const receipt = this.#load(claim.campaign_id, claim.search_hit_id);
      if (xId !== null) this.#db.prepare("INSERT INTO discovery_profile_identity_matches VALUES(?1,?1,?2,?3)").run(claim.search_hit_id, xId, now);
      this.#project(receipt); if (xId !== null) projectTwitterIdentityContent(this.#db, claim); return receipt;
    });
  }
  #project(receipt: DiscoveryProfileReceipt): void {
    const native = receipt.native_profile; if (native === null || receipt.status !== "completed") return;
    const claim = receipt.claim, key = ({ bilibili: "name", weibo: "name", tiktok: "display_name", facebook: "display_name", instagram: "display_name", bluesky: "display_name", twitter: "name", hackernews: "username", zhihu: "author_label", pixiv: "author_label", linkedin: "author_label", douyin: "author_label" } as Record<string, string>)[claim.platform];
    const name = key === undefined ? undefined : native[key];
    if (typeof name !== "string" || !name.trim() || /\p{Cc}/u.test(name)) return;
    const followers = typeof native.follower_count === "bigint" ? native.follower_count : typeof native.follower_count === "number" ? BigInt(native.follower_count) : null;
    if (!["twitter", "facebook", "instagram", "weibo", "bluesky", "zhihu", "hackernews", "pixiv", "linkedin", "douyin"].includes(claim.platform) && followers === null) throw new Error("discovery_profile_follower_count_missing");
    const account = claim.platform === "twitter" ? text.parse(this.#db.prepare("SELECT account_id FROM discovery_profile_identity_matches WHERE search_hit_id=?").get(claim.search_hit_id)!.account_id) : claim.platform === "linkedin" ? text.parse(native.account_id) : claim.author_id;
    const criteria = discoveryCriteriaForPlatform(parseApplicationJson(text.parse(this.#db.prepare("SELECT criteria_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=? AND revision=? AND approved_at IS NOT NULL").get(claim.campaign_id, claim.expected_revision)!.criteria_json)), claim.platform);
    const evidence = { source_urls: [claim.source_url, claim.author_url], subscribers_raw: claim.platform === "douyin" || ["facebook", "instagram"].includes(claim.platform) && followers === null ? typeof native.follower_text === "string" ? native.follower_text : null : followers === null ? null : String(followers), subscribers_source_url: claim.author_url, author_identity_kind: "native_id", author_identity_search_hit_id: claim.platform === "twitter" ? claim.search_hit_id : null, captured_at: receipt.captured_at };
    const assessment = claim.platform === "facebook" && native.follower_text === "" ? { verdict: "rejected" as const, reasons: ["followers_unavailable"], subscribers: null } : claim.platform === "instagram" && native.private_account === true ? { verdict: "rejected" as const, reasons: ["private_account"], subscribers: followers } : claim.platform === "instagram" && followers === null ? assessDiscoveryInstagram(criteria, evidence, account, typeof native.follower_text === "string" ? native.follower_text : null) : claim.platform === "douyin" && followers === null ? assessDiscoveryDouyin(criteria, evidence, account, typeof native.follower_text === "string" ? native.follower_text : null) : claim.platform === "facebook" && followers === null ? assessDiscoveryFacebook(criteria, evidence, account, typeof native.follower_text === "string" ? native.follower_text : null) : assessDiscoveryProfile(criteria, evidence, account, followers);
    const biography = native[["weibo", "linkedin", "douyin"].includes(claim.platform) ? "bio" : claim.platform === "tiktok" ? "signature" : claim.platform === "bilibili" ? "sign" : "biography"];
    const bioStatus = ["complete", "truncated", "missing", "unavailable", "unknown"].includes(String(native.bio_status)) ? native.bio_status : native.bio_truncated === true ? "truncated" : native.bio_truncated === false || claim.platform === "tiktok" ? typeof biography === "string" && biography.trim() ? "complete" : "missing" : "unknown";
    Object.assign(evidence, { profile_bio_status: bioStatus, profile_bio_status_source_url: claim.author_url });
    const saved = stringifyApplicationJson(assessedDiscoveryEvidence(evidence, assessment)); if (Buffer.byteLength(saved) > 16384) throw new Error("discovery_profile_evidence_too_large");
    this.#db.prepare(`INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform) VALUES(?,?,?,?,?,?,?,?,?,?,?11,?11,?4) ON CONFLICT(discovery_campaign_id,platform,platform_account_id) DO NOTHING`).run(randomUUID(), claim.campaign_id, claim.expected_revision, claim.platform, claim.profile_ref, account, [...name].slice(0, 200).join(""), claim.author_url, assessment.verdict, saved, receipt.captured_at);
  }
  projectSaved(campaign: string, hit: string): void {
    if (!this.#db.isTransaction) throw new Error("discovery_profile_projection_requires_transaction");
    this.#project(this.#load(campaign, hit));
  }
}
