import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { facebookSourceValid } from "../platforms/opencli/facebook-profile.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";

const text = z.string(), u32 = z.number().int().min(0).max(4294967295), record = z.record(text, z.unknown());
export const discoveryFacebookIdentityClaim = z.strictObject({ campaign_id: text, search_hit_id: text, expected_revision: u32, profile_ref: text, source_url: text, call_number: u32 });
export type DiscoveryFacebookIdentityClaim = z.infer<typeof discoveryFacebookIdentityClaim>;
export interface DiscoveryFacebookIdentityRead { claim: DiscoveryFacebookIdentityClaim; status: string; native_identity: Record<string, unknown> | null; stop_code: string | null; captured_at: number | bigint }
const stops = new Set(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required", "interrupted_facebook_identity_read"]);
export function validateDiscoveryFacebookIdentity(claim: DiscoveryFacebookIdentityClaim, native: Record<string, unknown> | null, stop: string | null): void {
  if (!facebookSourceValid(claim.source_url)) throw new Error("Facebook identity requires its saved profile source");
  if (native === null) { if (stop !== null && stops.has(stop)) return; throw new Error("Facebook identity result does not bind its native ID to the saved profile"); }
  const parsed = z.strictObject({ adapter_version: z.literal("1.0.0"), source_url: z.literal(claim.source_url), id: text.regex(/^[1-9]\d{0,19}$/u), display_name: text.refine(name => !!name.trim() && [...name].length <= 200 && !/\p{Cc}/u.test(name)), profile_url: text.refine(source => facebookSourceValid(source) && source.replace(/\/+$/u, "").toLowerCase() === claim.source_url.replace(/\/+$/u, "").toLowerCase()), evidence_source: z.literal("native_profile_header_response") }).safeParse(native);
  if (stop !== null || !parsed.success || Buffer.byteLength(stringifyApplicationJson(native)) > 4000) throw new Error("Facebook identity result does not bind its native ID to the saved profile");
}
export class DiscoveryFacebookIdentityRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #transaction<T>(body: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  next(campaign: string, excluded: readonly string[] = []): string | null {
    if (excluded.includes("facebook")) return null;
    const rows = this.#db.prepare(`SELECT h.id,h.source_url FROM discovery_search_hits h
      JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      WHERE c.id=? AND c.phase='active' AND v.approved_at IS NOT NULL AND(c.max_calls IS NULL OR c.used_calls<c.max_calls)
        AND h.platform='facebook' AND r.status='completed'
        AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i WHERE i.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')
      ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id`).all(campaign);
    for (const raw of rows) { const row = z.object({ id: text, source_url: text }).parse(raw); if (facebookSourceValid(row.source_url)) return row.id; }
    return null;
  }
  claim(campaign: string, revision: number, hit: string): DiscoveryFacebookIdentityClaim {
    u32.parse(revision);
    return this.#transaction(() => {
      const raw = this.#db.prepare(`SELECT h.profile_ref,h.source_url FROM discovery_search_hits h
        JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
        JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
        JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
        WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND v.approved_at IS NOT NULL AND(c.max_calls IS NULL OR c.used_calls<c.max_calls)
          AND h.id=? AND h.platform='facebook' AND r.status='completed'
          AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=h.id)
          AND NOT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i WHERE i.search_hit_id=h.id)
          AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')`).get(campaign, revision, hit);
      if (!raw) throw new Error("Facebook identity source is unapproved, attempted, bound or stopped");
      const source = z.object({ profile_ref: text, source_url: text }).parse(raw);
      if (!facebookSourceValid(source.source_url)) throw new Error("Facebook identity requires a saved vanity profile");
      const timestamp = Math.floor(Date.now() / 1000), count = this.#db.prepare("UPDATE discovery_campaigns SET used_calls=used_calls+1,updated_at=? WHERE id=? AND used_calls<4294967295 RETURNING used_calls").get(timestamp, campaign);
      const call_number = u32.parse(count?.used_calls), claim = { campaign_id: campaign, expected_revision: revision, search_hit_id: hit, ...source, call_number };
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='facebook' AND profile_ref=? AND status='completed'").run(`facebook_identity:${hit}:${call_number}`, timestamp, campaign, claim.profile_ref).changes !== 1) throw new Error("Facebook identity platform changed");
      this.#db.prepare("INSERT INTO discovery_facebook_identity_reads VALUES(?,?,'facebook',?,?,?,?,'reading',NULL,NULL,?)").run(hit, campaign, claim.profile_ref, claim.source_url, revision, call_number, timestamp);
      return claim;
    });
  }
  get(campaign: string, hit: string): DiscoveryFacebookIdentityRead {
    const statement = this.#db.prepare(`SELECT i.expected_revision,i.profile_ref,i.source_url,i.call_number,i.status,i.native_identity_json,i.stop_code,i.updated_at
      FROM discovery_facebook_identity_reads i JOIN discovery_search_hits h ON h.id=i.search_hit_id
      WHERE i.discovery_campaign_id=? AND i.search_hit_id=? AND h.discovery_campaign_id=i.discovery_campaign_id AND h.platform=i.platform AND h.profile_ref=i.profile_ref AND h.source_url=i.source_url`);
    statement.setReadBigInts(true); const raw = statement.get(campaign, hit);
    if (!raw) throw new Error("Facebook identity receipt not found at its saved source");
    const row = z.object({ expected_revision: z.bigint().min(0n).max(4294967295n), profile_ref: text, source_url: text, call_number: z.bigint().min(0n).max(4294967295n), status: text, native_identity_json: text.nullable(), stop_code: text.nullable(), updated_at: z.bigint().min(-9223372036854775808n).max(9223372036854775807n) }).parse(raw);
    return { claim: { campaign_id: campaign, search_hit_id: hit, expected_revision: Number(row.expected_revision), profile_ref: row.profile_ref, source_url: row.source_url, call_number: Number(row.call_number) }, status: row.status, native_identity: row.native_identity_json === null ? null : record.parse(parseApplicationJson(row.native_identity_json)), stop_code: row.stop_code, captured_at: row.updated_at >= BigInt(Number.MIN_SAFE_INTEGER) && row.updated_at <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(row.updated_at) : row.updated_at };
  }
  finish(raw: DiscoveryFacebookIdentityClaim, native: Record<string, unknown> | null, stop: string | null): DiscoveryFacebookIdentityRead {
    const claim = discoveryFacebookIdentityClaim.parse(raw); validateDiscoveryFacebookIdentity(claim, native, stop);
    return this.#transaction(() => {
      const prior = this.get(claim.campaign_id, claim.search_hit_id);
      if (!isDeepStrictEqual(prior.claim, claim)) throw new Error("Facebook identity claim changed");
      if (prior.status !== "reading") { if (isDeepStrictEqual(prior.native_identity, native) && prior.stop_code === stop) return prior; throw new Error("Facebook identity already finished differently"); }
      const timestamp = Math.floor(Date.now() / 1000);
      const changed = this.#db.prepare(`UPDATE discovery_platform_runs SET status=:status,active_query=NULL,stop_code=:stop,updated_at=:now
        WHERE discovery_campaign_id=:campaign AND platform='facebook' AND profile_ref=:profile AND status='searching' AND active_query=:active
          AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=:campaign AND c.current_revision=:revision AND c.phase='active' AND v.approved_at IS NOT NULL)`).run({ status: discoveryReadPlatformStatus(stop), stop: sourceLocalReadFailure(stop) ? null : stop, now: timestamp, campaign: claim.campaign_id, profile: claim.profile_ref, active: `facebook_identity:${claim.search_hit_id}:${claim.call_number}`, revision: claim.expected_revision });
      if (changed.changes !== 1) throw new Error("Facebook identity authorization changed");
      if (native !== null) { const id = text.parse(native.id); this.#db.prepare("INSERT INTO discovery_search_authors VALUES(?,?,?,?)").run(claim.search_hit_id, id, `https://www.facebook.com/profile.php?id=${id}`, timestamp); }
      this.#db.prepare("UPDATE discovery_facebook_identity_reads SET status=?,native_identity_json=?,stop_code=?,updated_at=? WHERE search_hit_id=? AND call_number=? AND status='reading'").run(native === null ? "failed" : "completed", native === null ? null : stringifyApplicationJson(native), stop, timestamp, claim.search_hit_id, claim.call_number);
      return this.get(claim.campaign_id, claim.search_hit_id);
    });
  }
  settle(claim: DiscoveryFacebookIdentityClaim, native: Record<string, unknown> | null, stop: string | null): DiscoveryFacebookIdentityRead {
    try { return this.finish(claim, native, stop); }
    catch (original) { try { return this.finish(claim, null, "opencli_invalid_output"); } catch (cleanup) { throw new AggregateError([original, cleanup], "discovery_facebook_identity_settlement_failed"); } }
  }
  interrupt(raw: DiscoveryFacebookIdentityClaim): DiscoveryFacebookIdentityRead {
    const claim = discoveryFacebookIdentityClaim.parse(raw);
    return this.#transaction(() => {
      const prior = this.get(claim.campaign_id, claim.search_hit_id);
      if (!isDeepStrictEqual(prior.claim, claim) || prior.status !== "reading") throw new Error("Facebook identity claim changed");
      const timestamp = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform='facebook' AND profile_ref=? AND status='searching' AND active_query=?").run(timestamp, claim.campaign_id, claim.profile_ref, `facebook_identity:${claim.search_hit_id}:${claim.call_number}`).changes !== 1) throw new Error("Facebook identity platform changed");
      this.#db.prepare("UPDATE discovery_facebook_identity_reads SET status='failed',stop_code='interrupted_facebook_identity_read',updated_at=? WHERE search_hit_id=? AND call_number=? AND status='reading'").run(timestamp, claim.search_hit_id, claim.call_number);
      return this.get(claim.campaign_id, claim.search_hit_id);
    });
  }
}
