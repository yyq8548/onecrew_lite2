import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { loadDiscoveryNativeContactMaterial } from "./contact-native.ts";
import { discoveryContactSearchQuery, discoveryExactUsername, discoveryLinkedUsername } from "./contact-navigation.ts";
import { DiscoveryProfileRepository } from "./profile-repository.ts";

const text = z.string(), u32 = z.number().int().min(0).max(4294967295);
export const discoveryContactLookup = z.object({ campaign_id: text, expected_revision: u32, candidate_id: text, profile_ref: text, method: text, native_author_id: text, author_label: text, username: text.nullable(), query: text, call_number: u32, status: text, links: z.array(text), stop_code: text.nullable() });
export type DiscoveryContactLookup = z.infer<typeof discoveryContactLookup>;
export class DiscoveryContactLookupRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema(database); this.#db = database; }
  #transaction<T>(write: boolean, body: () => T): T {
    this.#db.exec(write ? "BEGIN IMMEDIATE" : "BEGIN DEFERRED"); let committed = false;
    try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; }
    finally { if (!committed) this.#db.exec("ROLLBACK"); }
  }
  #page(input: DiscoveryContactLookup) { return { campaign_id: input.campaign_id, expected_revision: input.expected_revision, candidate_id: input.candidate_id, profile_ref: input.profile_ref, requested_url: "" }; }
  #next(campaign: string, excluded: readonly string[], candidate: string | null): DiscoveryContactLookup | null {
    const rows = this.#db.prepare(`SELECT c.id,c.criteria_revision,c.source_profile_ref FROM discovery_candidates c
      JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
      JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
      JOIN discovery_platform_runs p ON p.discovery_campaign_id=d.id AND p.platform=c.source_platform AND p.profile_ref=c.source_profile_ref
      WHERE d.id=? AND d.phase='active' AND a.approved_at IS NOT NULL AND c.criteria_revision=d.current_revision
        AND NOT EXISTS(SELECT 1 FROM json_each(?) excluded WHERE excluded.value IN(c.platform,c.source_platform))
        AND (? IS NULL OR c.id=?) AND c.verdict='qualified' AND p.status='completed' AND(d.max_calls IS NULL OR d.used_calls<d.max_calls)
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=d.id AND r.status='searching') ORDER BY c.id`).all(campaign, stringifyApplicationJson(excluded), candidate, candidate);
    for (const raw of rows) {
      const row = z.object({ id: text, criteria_revision: u32, source_profile_ref: text }).parse(raw);
      const input: DiscoveryContactLookup = { campaign_id: campaign, expected_revision: row.criteria_revision, candidate_id: row.id, profile_ref: row.source_profile_ref, method: "", native_author_id: "", author_label: "", username: null, query: "", call_number: 0, status: "planned", links: [], stop_code: null };
      const material = loadDiscoveryNativeContactMaterial(this.#db, this.#page(input));
      if (material === null) continue;
      let bluesky: string | null = null;
      if (material.hit.platform === "bluesky") {
        const native = new DiscoveryProfileRepository(this.#db).get(campaign, material.hit.id).native_profile;
        const handle = native?.handle;
        if (typeof handle === "string" && Buffer.byteLength(handle) <= 64 && handle.includes(".") && handle.split(".").every(part => part.length >= 1 && part.length <= 63 && !part.startsWith("-") && !part.endsWith("-") && /^[a-z0-9-]+$/u.test(part))) bluesky = handle;
      }
      const content = material.native_content?.native_content == null ? null : z.record(text, z.unknown()).parse(material.native_content.native_content);
      const contentText = content?.[material.hit.platform === "bilibili" ? "description" : "text"];
      const linked = [material.native_profile?.bio, material.video_description, typeof contentText === "string" ? contentText : null].flatMap(value => value == null ? [] : [value]).map(discoveryLinkedUsername).find(value => value !== null) ?? null;
      input.native_author_id = material.native_author_id; input.author_label = material.hit.author_label;
      const profile = material.native_profile?.profile_url ?? material.hit.author_url;
      input.username = bluesky ?? (profile == null ? null : discoveryExactUsername(profile)) ?? linked;
      const name = (input.username ?? input.author_label).trim();
      if (!name || Buffer.byteLength(name) > 200 || /[\n\r\0"]/u.test(name)) continue;
      const criteria = z.record(text, z.unknown()).parse(parseApplicationJson(text.parse(this.#db.prepare("SELECT criteria_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=? AND revision=?").get(campaign, row.criteria_revision)?.criteria_json)));
      const terms = Array.isArray(criteria.contact_search_terms) ? criteria.contact_search_terms.filter((value): value is string => typeof value === "string") : [];
      input.query = discoveryContactSearchQuery(name, typeof criteria.language === "string" ? criteria.language : "", terms);
      for (const method of ["maigret", "search"]) {
        if (method === "maigret" && input.username === null) continue;
        if (this.#db.prepare("SELECT 1 FROM discovery_contact_lookups WHERE candidate_id=? AND criteria_revision=? AND method=?").get(row.id, row.criteria_revision, method)) continue;
        if (method === "maigret" && this.#db.prepare(`SELECT 1 FROM discovery_contact_lookups WHERE discovery_campaign_id=? AND method='maigret' AND json_extract(receipt_json,'$.status')='failed' AND json_extract(receipt_json,'$.stop_code') IN('opencli_unavailable','opencli_version_changed','opencli_daemon_unavailable','opencli_extension_unavailable','opencli_command_unavailable','opencli_risk_control','opencli_auth_required','opencli_profile_unavailable') LIMIT 1`).get(campaign)) continue;
        if (method === "search") {
          if (this.#db.prepare("SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform='google' AND profile_ref=? AND status IN('unauthorized','risk_control_stop','technical_stop') LIMIT 1").get(campaign, input.profile_ref)) continue;
          if (this.#db.prepare("SELECT 1 FROM discovery_contact_lookups WHERE discovery_campaign_id=? AND method='search' AND json_extract(receipt_json,'$.stop_code') IN('opencli_risk_control','opencli_auth_required') LIMIT 1").get(campaign)) continue;
        }
        input.method = method; return input;
      }
    }
    return null;
  }
  next(campaign: string, excluded: readonly string[] = []): DiscoveryContactLookup | null { return this.#transaction(false, () => this.#next(campaign, excluded, null)); }
  claim(raw: DiscoveryContactLookup): DiscoveryContactLookup {
    const input = discoveryContactLookup.parse(raw);
    return this.#transaction(true, () => {
      const planned = this.#next(input.campaign_id, [], input.candidate_id);
      if (planned === null) throw new Error("contact_lookup_not_eligible");
      if (!isDeepStrictEqual(planned, input)) throw new Error("contact_lookup_plan_changed");
      const used = u32.parse(this.#db.prepare("SELECT used_calls FROM discovery_campaigns WHERE id=?").get(input.campaign_id)?.used_calls);
      planned.call_number = u32.parse(used + 1); planned.status = "reading";
      this.#db.prepare("UPDATE discovery_campaigns SET used_calls=used_calls+1 WHERE id=?").run(input.campaign_id);
      this.#db.prepare("INSERT INTO discovery_contact_lookups VALUES(?,?,?,?,?)").run(input.candidate_id, input.campaign_id, input.expected_revision, input.method, stringifyApplicationJson(planned));
      const changed = this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=? WHERE discovery_campaign_id=? AND profile_ref=? AND platform=(SELECT source_platform FROM discovery_candidates WHERE id=?)").run(`contact:${input.candidate_id}:${planned.call_number}`, input.campaign_id, input.profile_ref, input.candidate_id);
      if (changed.changes !== 1) throw new Error("contact_lookup_source_run_changed");
      return planned;
    });
  }
  finish(raw: DiscoveryContactLookup): DiscoveryContactLookup {
    const input = discoveryContactLookup.parse(raw);
    if (input.links.length > 5 || input.links.some(value => { const url = URL.parse(value); return Buffer.byteLength(value) > 2048 || !url || url.protocol !== "https:" || !url.hostname || !!url.username || !!url.password || !!url.port; }) || !["completed", "failed"].includes(input.status) || input.status === "failed" && (input.links.length !== 0 || input.stop_code === null) || input.status === "completed" && input.stop_code !== null) throw new Error("contact_lookup_invalid_result");
    return this.#transaction(true, () => {
      const row = this.#db.prepare("SELECT receipt_json FROM discovery_contact_lookups WHERE candidate_id=? AND criteria_revision=? AND method=?").get(input.candidate_id, input.expected_revision, input.method);
      const saved = discoveryContactLookup.parse(parseApplicationJson(text.parse(row?.receipt_json)));
      if (saved.status !== "reading") { if (isDeepStrictEqual(saved, input)) return saved; throw new Error("contact_lookup_already_finished"); }
      saved.status = input.status; saved.links = input.links; saved.stop_code = input.stop_code;
      if (!isDeepStrictEqual(saved, input)) throw new Error("contact_lookup_claim_changed");
      const approved = this.#db.prepare("SELECT 1 FROM discovery_campaigns WHERE id=? AND current_revision=? AND phase='active'").get(input.campaign_id, input.expected_revision) !== undefined;
      if (!approved || loadDiscoveryNativeContactMaterial(this.#db, this.#page(input)) === null) { input.status = "failed"; input.links = []; input.stop_code = "contact_lookup_stale"; }
      this.#db.prepare("UPDATE discovery_contact_lookups SET receipt_json=? WHERE candidate_id=? AND criteria_revision=? AND method=?").run(stringifyApplicationJson(input), input.candidate_id, input.expected_revision, input.method);
      this.#db.prepare("UPDATE discovery_platform_runs SET status='completed',active_query=NULL WHERE discovery_campaign_id=? AND profile_ref=? AND active_query=?").run(input.campaign_id, input.profile_ref, `contact:${input.candidate_id}:${input.call_number}`);
      if (input.method === "search" && ["opencli_risk_control", "opencli_auth_required"].includes(input.stop_code ?? "")) this.#db.prepare("UPDATE discovery_platform_runs SET status=?,stop_code=? WHERE discovery_campaign_id=? AND platform='google' AND profile_ref=?").run(input.stop_code === "opencli_risk_control" ? "risk_control_stop" : "unauthorized", input.stop_code, input.campaign_id, input.profile_ref);
      return input;
    });
  }
  settle(input: DiscoveryContactLookup): DiscoveryContactLookup {
    try { return this.finish(input); }
    catch (original) {
      try { return this.finish({ ...input, status: "failed", links: [], stop_code: "opencli_invalid_output" }); }
      catch (cleanup) { throw new AggregateError([original, cleanup], "discovery_contact_lookup_settlement_failed"); }
    }
  }
}
