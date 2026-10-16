import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema90 } from "../storage/creator-schema.ts";
import { discoveryPublicPageInput, discoveryEligibleContactAuthor, discoveryNativeLinkSource, nextDiscoveryNativeContactLink, observeDiscoveryHiddenEmail, observeDiscoveryPublicPageContacts, type DiscoveryPublicPageInput } from "./contact-native.ts";
import { discoveryCommunityInvite, discoveryObservesLink } from "./contact-navigation.ts";
import { discoveryReadPlatformStatus, sourceLocalReadFailure } from "./stops.ts";
import { getDiscoverySavedPublicPage } from "./saved-read-repository.ts";

const text = z.string(), u32 = z.number().int().min(0).max(4294967295);
export const discoveryPublicPageClaim = z.strictObject({ input: discoveryPublicPageInput, search_hit_id: text, source_url: text, call_number: u32 });
export type DiscoveryPublicPageClaim = z.infer<typeof discoveryPublicPageClaim>;
export const finishDiscoveryPublicPageInput = z.strictObject({ hidden_email_available: z.boolean(), claim: discoveryPublicPageClaim, document_url: text.nullable(), title: text, markdown_excerpt: text, truncated: z.boolean(), stop_code: text.nullable() });
export type FinishDiscoveryPublicPageInput = z.infer<typeof finishDiscoveryPublicPageInput>;
const stopCodes = new Set(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required"]);
export function validateDiscoveryPublicPageResult(raw: unknown): FinishDiscoveryPublicPageInput {
  const input = finishDiscoveryPublicPageInput.parse(raw);
  if (input.hidden_email_available && (input.stop_code !== null || input.document_url !== input.claim.input.requested_url || !input.claim.input.requested_url.startsWith("https://www.youtube.com/channel/UC"))) throw new Error("Hidden email entry requires a successful native channel receipt");
  if (Buffer.byteLength(input.markdown_excerpt) > 20_000 || [...input.title].length > 500 || input.markdown_excerpt.includes("\0")) throw new Error("Public page result exceeds bounded evidence limits");
  if (input.stop_code !== null) {
    if (!stopCodes.has(input.stop_code) || input.document_url !== null || input.title !== "" || input.markdown_excerpt !== "" || input.truncated) throw new Error("Failed public page reads cannot contain successful evidence");
  } else {
    const url = input.document_url === null ? null : URL.parse(input.document_url);
    if (!input.title.trim() || !input.markdown_excerpt.trim() || input.document_url === null || Buffer.byteLength(input.document_url) > 2048 || !url || url.protocol !== "https:" || !url.hostname || url.username || url.password || url.port) throw new Error("Successful public page reads require attributed content");
  }
  return input;
}

export class DiscoveryPublicPageRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema90(database); this.#db = database; }
  #transaction<T>(write: boolean, body: () => T): T {
    this.#db.exec(write ? "BEGIN IMMEDIATE" : "BEGIN DEFERRED");
    let committed = false;
    try { const value = body(); this.#db.exec("COMMIT"); committed = true; return value; }
    finally { if (!committed) this.#db.exec("ROLLBACK"); }
  }
  #scoreValid(input: DiscoveryPublicPageInput): boolean {
    const row = this.#db.prepare("SELECT verdict FROM discovery_candidates WHERE id=? AND discovery_campaign_id=? AND criteria_revision=?").get(input.candidate_id, input.campaign_id, input.expected_revision);
    if (!row) throw new Error("Public page candidate not found");
    return row.verdict === "pending" || discoveryEligibleContactAuthor(this.#db, input.campaign_id, input.expected_revision, input.candidate_id) !== null;
  }
  next(campaign: string, excludedPlatforms: readonly string[] = []): DiscoveryPublicPageInput | null {
    return this.#transaction(false, () => {
      const rows = this.#db.prepare(`SELECT c.id,c.source_profile_ref,c.criteria_revision,l.value,json_extract(c.evidence_json,'$.video_description') AS description,c.verdict
        FROM discovery_candidates c JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
        JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
        JOIN discovery_platform_runs p ON p.discovery_campaign_id=d.id AND p.platform=c.platform AND p.profile_ref=c.source_profile_ref
        JOIN discovery_search_hits h ON h.discovery_campaign_id=d.id AND h.platform=c.platform AND h.profile_ref=c.source_profile_ref AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')
        JOIN discovery_video_completions v ON v.search_hit_id=h.id
        JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.candidate_id=c.id AND b.discovery_campaign_id=d.id AND b.criteria_revision=d.current_revision AND b.native_author_id=c.platform_account_id
        JOIN json_each(c.evidence_json,'$.public_links') l
        WHERE d.id=? AND d.phase='active' AND (d.max_calls IS NULL OR d.used_calls<d.max_calls) AND a.approved_at IS NOT NULL AND c.criteria_revision=d.current_revision
          AND NOT EXISTS(SELECT 1 FROM json_each(?) excluded WHERE excluded.value=c.platform)
          AND ((c.verdict='pending' AND (COALESCE(json_extract(a.criteria_json,'$.requires_public_tool_url'),1)=1 OR json_extract(a.criteria_json,'$.region')='中国大陆以外'
            OR (json_array_length(json_extract(a.criteria_json,'$.requirements'))>0 AND (SELECT COUNT(*) FROM discovery_public_page_reads page WHERE page.candidate_id=c.id)<2)))
            OR(c.verdict='qualified' AND EXISTS(SELECT 1 FROM discovery_recommendation_policies policy WHERE policy.discovery_campaign_id=d.id AND policy.criteria_revision=d.current_revision AND policy.policy='score_and_contact_v1')
              AND NOT EXISTS(SELECT 1 FROM discovery_contact_observations contact WHERE contact.candidate_id=c.id AND contact.criteria_revision=c.criteria_revision AND (COALESCE(json_extract(a.criteria_json,'$.contact_preference'),'')!='public_email_first' OR contact.kind='email'))
              AND (COALESCE(json_extract(a.criteria_json,'$.contact_preference'),'')!='public_email_first' OR NOT EXISTS(SELECT 1 FROM discovery_public_page_reads page WHERE page.candidate_id=c.id))))
          AND json_type(c.evidence_json,'$.video_description')='text' AND l.type='text'
          AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=d.id AND r.status='searching')
          AND ((p.status='completed' AND NOT EXISTS(SELECT 1 FROM discovery_public_page_reads r WHERE r.candidate_id=c.id AND r.requested_url=l.value))
            OR(p.status='technical_stop' AND p.stop_code='interrupted_public_page_read' AND EXISTS(SELECT 1 FROM discovery_public_page_reads r WHERE r.candidate_id=c.id AND r.requested_url=l.value AND r.status='reading' AND r.search_hit_id=h.id AND r.source_excerpt=json_extract(c.evidence_json,'$.video_description') AND p.active_query='page:'||c.id||':'||r.call_number)))
        ORDER BY c.id,l.key`).all(campaign, stringifyApplicationJson(excludedPlatforms));
      for (const raw of rows) {
        const row = z.object({ id: text, source_profile_ref: text, criteria_revision: u32, value: text, description: text, verdict: text }).parse(raw);
        if (row.verdict === "qualified") continue;
        const input = { campaign_id: campaign, expected_revision: row.criteria_revision, profile_ref: row.source_profile_ref, candidate_id: row.id, requested_url: row.value };
        if (Buffer.byteLength(row.value) <= 2048 && row.value.startsWith("https://") && !discoveryCommunityInvite(row.value) && discoveryObservesLink(row.description, row.value) && this.#scoreValid(input)) return input;
      }
      return nextDiscoveryNativeContactLink(this.#db, campaign, excludedPlatforms);
    });
  }
  claim(raw: DiscoveryPublicPageInput): DiscoveryPublicPageClaim {
    const input = discoveryPublicPageInput.parse(raw);
    return this.#transaction(true, () => {
      const head = this.#db.prepare(`SELECT d.used_calls,d.max_calls FROM discovery_campaigns d JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision WHERE d.id=? AND d.current_revision=? AND d.phase='active' AND a.approved_at IS NOT NULL`).get(input.campaign_id, input.expected_revision);
      if (!head) throw new Error("Public page read is not approved at this Campaign revision");
      const budget = z.object({ used_calls: u32, max_calls: u32.nullable() }).parse(head);
      if (budget.max_calls !== null && budget.used_calls >= budget.max_calls) throw new Error("Campaign read budget is exhausted");
      if (this.#db.prepare("SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=? AND status='searching' LIMIT 1").get(input.campaign_id)) throw new Error("A Campaign read is active");
      const saved = this.#db.prepare(`SELECT h.id,h.source_url,json_extract(c.evidence_json,'$.video_description') AS description,c.platform
        FROM discovery_candidates c JOIN discovery_search_hits h ON h.discovery_campaign_id=c.discovery_campaign_id AND h.platform=c.platform AND h.profile_ref=c.source_profile_ref AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')
        JOIN discovery_video_completions v ON v.search_hit_id=h.id
        JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.candidate_id=c.id AND b.discovery_campaign_id=c.discovery_campaign_id AND b.criteria_revision=c.criteria_revision AND b.native_author_id=c.platform_account_id
        WHERE c.id=? AND c.discovery_campaign_id=? AND c.criteria_revision=? AND c.source_profile_ref=?
          AND ((c.verdict='pending' AND EXISTS(SELECT 1 FROM discovery_campaign_revisions a WHERE a.discovery_campaign_id=c.discovery_campaign_id AND a.revision=c.criteria_revision AND (COALESCE(json_extract(a.criteria_json,'$.requires_public_tool_url'),1)=1 OR json_extract(a.criteria_json,'$.region')='中国大陆以外' OR json_array_length(json_extract(a.criteria_json,'$.requirements'))>0)))
            OR(c.verdict='qualified' AND EXISTS(SELECT 1 FROM discovery_recommendation_policies policy WHERE policy.discovery_campaign_id=c.discovery_campaign_id AND policy.criteria_revision=c.criteria_revision AND policy.policy='score_and_contact_v1') AND NOT EXISTS(SELECT 1 FROM discovery_contact_observations contact WHERE contact.candidate_id=c.id AND contact.criteria_revision=c.criteria_revision)))
          AND json_type(c.evidence_json,'$.video_description')='text' AND EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.public_links') l WHERE l.type='text' AND l.value=?)`).get(input.candidate_id, input.campaign_id, input.expected_revision, input.profile_ref, input.requested_url);
      const row = saved === undefined ? null : z.object({ id: text, source_url: text, description: text, platform: text }).parse(saved);
      const source = row ? [row.id, row.source_url, row.description, row.platform] as const : discoveryNativeLinkSource(this.#db, input);
      if (!source) throw new Error("Public link has no pending metadata source in this Campaign scope");
      const [hit, sourceUrl, description, platform] = source;
      if (Buffer.byteLength(input.requested_url) > 2048 || !input.requested_url.startsWith("https://") || discoveryCommunityInvite(input.requested_url) || (!discoveryObservesLink(description, input.requested_url) && stringifyApplicationJson(discoveryNativeLinkSource(this.#db, input)) !== stringifyApplicationJson(source))) throw new Error("Public link is not an eligible exact token in the observed source description");
      if (!this.#scoreValid(input)) throw new Error("contact_lookup_score_not_audited");
      const run = z.object({ status: text, stop_code: text.nullable(), active_query: text.nullable() }).parse(this.#db.prepare("SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").get(input.campaign_id, platform, input.profile_ref));
      const priorRaw = this.#db.prepare("SELECT call_number,status,search_hit_id,source_excerpt FROM discovery_public_page_reads WHERE candidate_id=? AND requested_url=?").get(input.candidate_id, input.requested_url);
      const prior = priorRaw === undefined ? null : z.object({ call_number: u32, status: text, search_hit_id: text, source_excerpt: text }).parse(priorRaw);
      const interrupted = prior !== null && prior.status === "reading" && prior.search_hit_id === hit && prior.source_excerpt === description && run.status === "technical_stop" && run.stop_code === "interrupted_public_page_read" && run.active_query === `page:${input.candidate_id}:${prior.call_number}`;
      if (!interrupted && (run.status !== "completed" || prior !== null)) throw new Error("Public page already attempted or platform stopped");
      const call = u32.parse(budget.used_calls + 1), timestamp = Math.floor(Date.now() / 1000);
      this.#db.prepare("UPDATE discovery_campaigns SET used_calls=?,updated_at=? WHERE id=?").run(call, timestamp, input.campaign_id);
      this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',active_query=?,stop_code=NULL,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").run(`page:${input.candidate_id}:${call}`, timestamp, input.campaign_id, platform, input.profile_ref);
      this.#db.prepare(`INSERT INTO discovery_public_page_reads VALUES(?,?,?,?,'reading',?,NULL,'','',0,NULL,?) ON CONFLICT(candidate_id,requested_url) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at`).run(input.candidate_id, input.requested_url, hit, call, description, timestamp);
      return { input, search_hit_id: hit, source_url: sourceUrl, call_number: call };
    });
  }
  finish(raw: FinishDiscoveryPublicPageInput) {
    const input = validateDiscoveryPublicPageResult(raw), claim = input.claim, scope = claim.input;
    return this.#transaction(true, () => {
      const platform = text.parse(this.#db.prepare("SELECT platform FROM discovery_search_hits WHERE id=? AND discovery_campaign_id=?").get(claim.search_hit_id, scope.campaign_id)?.platform);
      if (input.stop_code === null && (platform !== "youtube" || discoveryEligibleContactAuthor(this.#db, scope.campaign_id, scope.expected_revision, scope.candidate_id) !== null)) {
        const source = discoveryNativeLinkSource(this.#db, scope);
        if (!source) throw new Error("contact_source_no_longer_eligible");
        if (source[0] !== claim.search_hit_id || source[1] !== claim.source_url) throw new Error("contact_source_claim_mismatch");
        if (this.#db.prepare("SELECT source_excerpt FROM discovery_public_page_reads WHERE candidate_id=? AND requested_url=?").get(scope.candidate_id, scope.requested_url)?.source_excerpt !== source[2]) throw new Error("contact_source_changed");
      }
      const lookupPage = this.#db.prepare(`SELECT 1 FROM discovery_candidates c JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.discovery_campaign_id AND p.criteria_revision=c.criteria_revision WHERE c.id=? AND c.discovery_campaign_id=? AND c.criteria_revision=?
        AND (c.verdict='qualified' OR (c.verdict='pending' AND EXISTS(SELECT 1 FROM discovery_campaign_revisions r WHERE r.discovery_campaign_id=c.discovery_campaign_id AND r.revision=c.criteria_revision AND json_array_length(json_extract(r.criteria_json,'$.requirements'))>0))) AND p.policy='score_and_contact_v1'`).get(scope.candidate_id, scope.campaign_id, scope.expected_revision) !== undefined;
      const localFailure = sourceLocalReadFailure(input.stop_code) || lookupPage && input.stop_code !== null && ["opencli_risk_control", "opencli_auth_required", "opencli_command_failed", "opencli_timeout", "opencli_invalid_output", "opencli_invalid_input", "opencli_output_too_large"].includes(input.stop_code);
      const timestamp = Math.floor(Date.now() / 1000);
      const changed = this.#db.prepare(`UPDATE discovery_platform_runs SET status=:status,active_query=NULL,stop_code=:stop,updated_at=:now
        WHERE discovery_campaign_id=:campaign AND profile_ref=:profile AND status='searching' AND active_query=:active
          AND EXISTS(SELECT 1 FROM discovery_campaigns d JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision WHERE d.id=:campaign AND d.phase='active' AND d.current_revision=:revision AND a.approved_at IS NOT NULL)
          AND EXISTS(SELECT 1 FROM discovery_search_hits h JOIN discovery_candidates c ON c.id=:candidate LEFT JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.candidate_id=c.id AND b.discovery_campaign_id=:campaign AND b.criteria_revision=:revision AND b.native_author_id=c.platform_account_id
            WHERE h.id=:hit AND h.discovery_campaign_id=:campaign AND c.discovery_campaign_id=:campaign AND c.criteria_revision=:revision AND(c.platform!='youtube' OR b.search_hit_id IS NOT NULL) AND h.platform=discovery_platform_runs.platform AND c.source_platform=h.platform AND c.source_profile_ref=:profile AND h.profile_ref=:profile AND h.source_url=:source)`).run({ status: localFailure ? "completed" : discoveryReadPlatformStatus(input.stop_code), stop: localFailure ? null : input.stop_code, now: timestamp, campaign: scope.campaign_id, profile: scope.profile_ref, active: `page:${scope.candidate_id}:${claim.call_number}`, revision: scope.expected_revision, candidate: scope.candidate_id, hit: claim.search_hit_id, source: claim.source_url });
      if (changed.changes !== 1) throw new Error("Public page claim is stale or authorization changed");
      const receipt = this.#db.prepare("UPDATE discovery_public_page_reads SET status=?,document_url=?,title=?,markdown_excerpt=?,truncated=?,stop_code=?,updated_at=? WHERE candidate_id=? AND requested_url=? AND search_hit_id=? AND call_number=? AND status='reading'").run(input.stop_code === null ? "completed" : "failed", input.document_url, input.title, input.markdown_excerpt, input.truncated ? 1 : 0, input.stop_code, timestamp, scope.candidate_id, scope.requested_url, claim.search_hit_id, claim.call_number);
      if (receipt.changes !== 1) throw new Error("Public page receipt is stale");
      const result = getDiscoverySavedPublicPage(this.#db, scope.campaign_id, scope.candidate_id, scope.requested_url);
      observeDiscoveryPublicPageContacts(this.#db, scope.campaign_id, scope.expected_revision, result);
      if (input.hidden_email_available) observeDiscoveryHiddenEmail(this.#db, scope.campaign_id, scope.expected_revision, result);
      return result;
    });
  }
  settle(input: FinishDiscoveryPublicPageInput) {
    // 原 owner 将无效成功材料结算为明确失败；第二次失败同时保留两项异常。
    try { return this.finish(input); }
    catch (original) {
      try { return this.finish({ hidden_email_available: false, claim: input.claim, document_url: null, title: "", markdown_excerpt: "", truncated: false, stop_code: "opencli_invalid_output" }); }
      catch (cleanup) { throw new AggregateError([original, cleanup], "discovery_public_page_settlement_failed"); }
    }
  }
  interrupt(raw: DiscoveryPublicPageClaim): void {
    const claim = discoveryPublicPageClaim.parse(raw);
    this.#transaction(true, () => {
      const changed = this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_public_page_read' WHERE discovery_campaign_id=? AND profile_ref=? AND status='searching' AND active_query=?").run(claim.input.campaign_id, claim.input.profile_ref, `page:${claim.input.candidate_id}:${claim.call_number}`);
      if (changed.changes !== 1) throw new Error("Public page claim is stale or authorization changed");
    });
  }
}
