import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { discoveryReadBusy } from "./read-capacity.ts";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import type { PublicOpenCliWebRead } from "../platforms/opencli/web.ts";
import { validatePublicWebUrl } from "../platforms/opencli/public-url.ts";
import { TEMPORARILY_DISABLED_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { discoveryWebSourceTarget } from "./web-source-contracts.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";
import { discoveryWebAuthorEvidenceFailure, discoveryWebAuthorResolution, validateDiscoveryWebAuthorResolution } from "./web-author-material.ts";
import { discoveryMaterialDigest, discoveryJsonValue } from "./library-material.ts";

const text = z.string().min(1), integer = z.number().int().nonnegative().safe();
export const discoveryWebAuthorClaim = z.strictObject({ campaign_id: text, expected_revision: integer.positive(), search_hit_id: text, platform: z.enum(["google", "yahoo"]), profile_ref: text, requested_url: text, page_kind: z.enum(["article", "author"]), expected_author: z.string().nullable(), call_number: integer.positive() });
export type DiscoveryWebAuthorClaim = z.infer<typeof discoveryWebAuthorClaim>;
const sourceShape = z.object({ platform: z.enum(["google", "yahoo"]), profile_ref: text, source_url: text, search_keyword: z.string(), title: z.string() });

export class DiscoveryWebAuthorRepository {
  readonly #db: DatabaseSync;
  constructor(db: DatabaseSync) { this.#db = db; }
  #transaction<T>(work: () => T): T { this.#db.exec("BEGIN IMMEDIATE"); let committed = false; try { const result = work(); this.#db.exec("COMMIT"); committed = true; return result; } finally { if (!committed) this.#db.exec("ROLLBACK"); } }
  #available(): boolean { return !!this.#db.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name='discovery_web_author_pages'").get(); }
  #source(campaign: string, hit: string) { return sourceShape.parse(this.#db.prepare("SELECT platform,profile_ref,source_url,search_keyword,title FROM discovery_search_hits WHERE discovery_campaign_id=? AND id=?").get(campaign, hit)); }
  #target(campaign: string, hit: string) {
    const source = this.#source(campaign, hit);
    if (discoveryWebSourceTarget(source.source_url)) return null;
    validatePublicWebUrl(source.source_url);
    const article = this.#db.prepare("SELECT status,resolution_json FROM discovery_web_author_pages WHERE discovery_campaign_id=? AND search_hit_id=? AND page_kind='article'").get(campaign, hit);
    if (!article) return { ...source, requested_url: source.source_url, page_kind: "article" as const, expected_author: null };
    if (article.status !== "completed" || typeof article.resolution_json !== "string") return null;
    const resolution = discoveryWebAuthorResolution.parse(parseApplicationJson(Buffer.from(article.resolution_json)));
    if (!resolution.author_page_url || resolution.native_source_url || this.#db.prepare("SELECT 1 FROM discovery_web_author_pages WHERE search_hit_id=? AND page_kind='author'").get(hit)) return null;
    return { ...source, requested_url: resolution.author_page_url, page_kind: "author" as const, expected_author: resolution.author_name };
  }
  next(campaign: string, excluded: readonly string[] = []) {
    if (!this.#available()) return null;
    const rows = this.#db.prepare(`SELECT h.id,h.platform FROM discovery_search_hits h JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref WHERE c.id=? AND c.phase='active' AND v.approved_at IS NOT NULL AND h.platform IN ('google','yahoo') AND r.status='completed' ORDER BY h.captured_at,h.id`).all(campaign);
    for (const row of rows) if (!excluded.includes(String(row.platform)) && this.#target(campaign, String(row.id))) return String(row.id);
    return null;
  }
  ready(campaign: string, revision: number, excluded: readonly string[] = []) {
    if (!this.#available()) return [];
    return this.#db.prepare(`SELECT p.*,h.platform FROM discovery_web_author_pages p JOIN discovery_search_hits h ON h.id=p.search_hit_id WHERE p.discovery_campaign_id=? AND p.criteria_revision=? AND p.status='awaiting_resolution' ORDER BY p.call_number`).all(campaign, revision).filter(row => !excluded.includes(String(row.platform))).map(row => {
      const page = parseApplicationJson(Buffer.from(text.parse(row.material_json))) as unknown as PublicOpenCliWebRead;
      const article = row.page_kind === "author" ? this.#db.prepare("SELECT resolution_json FROM discovery_web_author_pages WHERE search_hit_id=? AND page_kind='article' AND status='completed'").get(row.search_hit_id!) : null;
      const expected = article ? discoveryWebAuthorResolution.parse(parseApplicationJson(Buffer.from(text.parse(article.resolution_json)))).author_name : null;
      return { claim: discoveryWebAuthorClaim.parse({ campaign_id: campaign, expected_revision: revision, search_hit_id: row.search_hit_id, platform: row.platform, profile_ref: row.profile_ref, requested_url: row.requested_url, page_kind: row.page_kind, expected_author: expected, call_number: row.call_number }), page, material_hash: discoveryMaterialDigest(discoveryJsonValue(page)) };
    }).slice(0, 20);
  }
  finishRead(claim: DiscoveryWebAuthorClaim, page: PublicOpenCliWebRead) {
    return this.#transaction(() => {
      this.assertClaim(claim);
      if (page.requested_url !== claim.requested_url || Buffer.byteLength(stringifyApplicationJson(page)) > 65536) throw new Error("web_author_page_binding_invalid");
      const now = Math.floor(Date.now() / 1000);
      this.#db.prepare("UPDATE discovery_web_author_pages SET status='awaiting_resolution',material_json=?,updated_at=? WHERE search_hit_id=? AND requested_url=? AND status='reading'").run(stringifyApplicationJson(page), now, claim.search_hit_id, claim.requested_url);
      this.#db.prepare("UPDATE discovery_platform_runs SET status='completed',active_query=NULL,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").run(now, claim.campaign_id, claim.platform, claim.profile_ref);
      return { claim, status: "awaiting_resolution", stop_code: null, native_search_hit_id: null, resolution: null };
    });
  }
  beginResolution(campaign: string, revision: number, hit: string, hash: string) {
    const saved = this.ready(campaign, revision).find(row => row.claim.search_hit_id === hit);
    if (!saved || saved.material_hash !== hash) throw new Error("web_author_material_changed");
    this.#transaction(() => {
      const claim = saved.claim, now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("SELECT 1 FROM discovery_campaigns WHERE id=? AND current_revision=? AND phase='active'").get(campaign, revision) === undefined || discoveryReadBusy(this.#db, campaign, [claim.platform])) throw new Error("web_author_approval_or_active_read_changed");
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND status='completed'").run(`article:${hit}:${claim.call_number}`, now, campaign, claim.platform, claim.profile_ref).changes !== 1) throw new Error("web_author_platform_changed");
      this.#db.prepare("UPDATE discovery_web_author_pages SET status='reading',updated_at=? WHERE search_hit_id=? AND requested_url=? AND status='awaiting_resolution'").run(now, hit, claim.requested_url);
    });
    return saved;
  }
  rejectResolution(campaign: string, revision: number, hit: string, hash: string, code: string) {
    const failure = discoveryWebAuthorEvidenceFailure.parse(code);
    const saved = this.beginResolution(campaign, revision, hit, hash);
    return this.finish(saved.claim, saved.page, null, failure);
  }
  claim(campaign: string, revision: number, hit: string): DiscoveryWebAuthorClaim {
    return this.#transaction(() => {
      const target = this.#target(campaign, hit);
      if (!target) throw new Error("web_author_already_attempted_or_unsupported");
      if (TEMPORARILY_DISABLED_CREATOR_SOURCES.includes(target.platform)) throw new OpenCliError("opencli_read_command_unavailable");
      const head = this.#db.prepare(`SELECT c.used_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND v.approved_at IS NOT NULL`).get(campaign, revision);
      if (discoveryReadBusy(this.#db, campaign, [target.platform])) throw new Error("discovery_read_active");
      if (!head) throw new Error("web_author_approval_budget_or_active_read_changed");
      const claim = discoveryWebAuthorClaim.parse({ campaign_id: campaign, expected_revision: revision, search_hit_id: hit, platform: target.platform, profile_ref: target.profile_ref, requested_url: target.requested_url, page_kind: target.page_kind, expected_author: target.expected_author, call_number: integer.parse(head.used_calls) + 1 }), now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND status='completed'").run(`article:${hit}:${claim.call_number}`, now, campaign, claim.platform, claim.profile_ref).changes !== 1) throw new Error("web_author_platform_changed");
      this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(claim.call_number, now, campaign);
      this.#db.prepare("INSERT INTO discovery_web_author_pages(search_hit_id,discovery_campaign_id,criteria_revision,requested_url,page_kind,profile_ref,call_number,status,updated_at) VALUES(?,?,?,?,?,?,?,'reading',?)").run(hit, campaign, revision, claim.requested_url, claim.page_kind, claim.profile_ref, claim.call_number, now);
      return claim;
    });
  }
  assertClaim(claim: DiscoveryWebAuthorClaim): void {
    const active = this.#db.prepare(`SELECT 1 FROM discovery_web_author_pages p JOIN discovery_campaigns c ON c.id=p.discovery_campaign_id AND c.current_revision=p.criteria_revision JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.profile_ref=p.profile_ref
      WHERE p.search_hit_id=? AND p.requested_url=? AND p.call_number=? AND p.criteria_revision=? AND p.status='reading' AND c.phase='active' AND r.platform=? AND r.status='searching' AND r.active_query=?`).get(claim.search_hit_id, claim.requested_url, claim.call_number, claim.expected_revision, claim.platform, `article:${claim.search_hit_id}:${claim.call_number}`);
    if (!active) throw new Error("web_author_claim_stale");
  }
  finish(raw: DiscoveryWebAuthorClaim, page: PublicOpenCliWebRead | null, resolution: unknown, stop: string | null) {
    const claim = discoveryWebAuthorClaim.parse(raw);
    const checked = stop === null && page ? validateDiscoveryWebAuthorResolution(page, resolution, claim.expected_author ?? undefined) : null;
    if (stop === null && !checked) throw new Error("web_author_resolution_missing");
    return this.#transaction(() => {
      this.assertClaim(claim); const now = Math.floor(Date.now() / 1000), source = this.#source(claim.campaign_id, claim.search_hit_id);
      if (page && (page.requested_url !== claim.requested_url || Buffer.byteLength(stringifyApplicationJson(page)) > 65536)) throw new Error("web_author_page_binding_invalid");
      let nativeHit: string | null = null;
      if (checked?.native_source_url) {
        const native = discoveryWebSourceTarget(checked.native_source_url);
        if (!native) throw new Error("web_author_native_source_unsupported");
        const existing = this.#db.prepare("SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND source_url=?").get(claim.campaign_id, source.platform, source.profile_ref, checked.native_source_url);
        nativeHit = existing ? text.parse(existing.id) : randomUUID();
        if (!existing) {
          this.#db.prepare("INSERT INTO discovery_search_hits VALUES(?,?,?,?,?,?,?,?,?)").run(nativeHit, claim.campaign_id, source.platform, source.profile_ref, source.search_keyword, checked.native_source_url, source.title, checked.author_name, now);
          this.#db.prepare("INSERT INTO discovery_search_positions(search_hit_id) VALUES(?)").run(nativeHit);
        }
        this.#db.prepare("INSERT INTO discovery_web_author_edges VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(native_search_hit_id) DO NOTHING").run(nativeHit, claim.search_hit_id, source.source_url, claim.page_kind === "author" ? claim.requested_url : null, checked.author_name, checked.attribution_quote, checked.native_source_url, now);
      }
      this.#db.prepare("UPDATE discovery_web_author_pages SET status=?,material_json=?,resolution_json=?,stop_code=?,updated_at=? WHERE search_hit_id=? AND requested_url=? AND status='reading'").run(stop === null ? "completed" : "failed", page ? stringifyApplicationJson(page) : null, checked ? stringifyApplicationJson(checked) : null, stop, now, claim.search_hit_id, claim.requested_url);
      const status = sourceLocalReadFailure(stop) || stop?.startsWith("web_author_") || stop?.startsWith("discovery_") ? "completed" : discoveryReadPlatformStatus(stop);
      this.#db.prepare("UPDATE discovery_platform_runs SET status=?,active_query=NULL,stop_code=?,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").run(status, status === "completed" ? null : stop, now, claim.campaign_id, claim.platform, claim.profile_ref);
      return { claim, status: stop === null ? "completed" : "failed", stop_code: stop, native_search_hit_id: nativeHit, resolution: checked };
    });
  }
}
