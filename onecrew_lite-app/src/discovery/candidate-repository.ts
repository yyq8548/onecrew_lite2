import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateCreatorSchema90 } from "../storage/creator-schema.ts";
import { discoveryEligibleContactAuthor, discoveryHasHiddenEmail, loadDiscoveryManualWechat } from "./contact-native.ts";
import { loadDiscoveryContactObservations } from "./contact-repository.ts";
import { loadIdentityPublicationBasis } from "./identity-library-publication.ts";
import { buildIdentityGroups } from "./identity-groups.ts";

const integer = z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]);
const text = z.string().refine(value => value.isWellFormed());
const rawCandidate = z.object({ id: text, campaign_id: text, criteria_revision: z.number().int().min(0).max(4294967295), platform: text, source_profile_ref: text, platform_account_id: text, display_name: text, profile_url: text, verdict: text, evidence_json: text, created_at: integer, updated_at: integer });
export const discoveryCandidatePageInput = z.strictObject({ campaign_id: text.min(1), after_id: text.nullable(), limit: z.number().int().min(1).max(100) });
export type DiscoveryCandidatePageInput = z.infer<typeof discoveryCandidatePageInput>;

export class DiscoveryCandidateRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { validateCreatorSchema90(database); this.#db = database; }
  #read<T>(body: () => T): T {
    this.#db.exec("BEGIN DEFERRED"); let committed = false;
    try { const result = body(); this.#db.exec("COMMIT"); committed = true; return result; }
    finally { if (!committed) this.#db.exec("ROLLBACK"); }
  }
  list(raw: DiscoveryCandidatePageInput) {
    const input = discoveryCandidatePageInput.parse(raw);
    return this.#read(() => {
      const statement = this.#db.prepare(`WITH ranked AS (SELECT *,CASE WHEN verdict='pending' THEN 4 ELSE CASE json_extract(evidence_json,'$.assessment.match_rating.level') WHEN 'excellent' THEN 0 WHEN 'good' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END END AS match_rank
        FROM discovery_candidates WHERE discovery_campaign_id=?1)
        SELECT id,discovery_campaign_id AS campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,
        CASE WHEN EXISTS(SELECT 1 FROM discovery_manual_approvals m WHERE m.discovery_campaign_id=ranked.discovery_campaign_id AND m.criteria_revision=ranked.criteria_revision AND m.candidate_id=ranked.id)
          THEN json_set(evidence_json,'$.manual_approval',json_object('approved_at',(SELECT approved_at FROM discovery_manual_approvals m WHERE m.discovery_campaign_id=ranked.discovery_campaign_id AND m.criteria_revision=ranked.criteria_revision AND m.candidate_id=ranked.id)))
          ELSE json_remove(evidence_json,'$.manual_approval') END AS evidence_json,created_at,updated_at
        FROM ranked WHERE ?2='' OR (match_rank,id)>(SELECT match_rank,id FROM ranked WHERE id=?2) ORDER BY match_rank,id LIMIT ?3`);
      statement.setReadBigInts(true);
      const rows = statement.all(input.campaign_id, input.after_id ?? "", input.limit).map(row => Object.fromEntries(Object.entries(row).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value])));
      return rows.map(raw => {
        const { evidence_json, ...candidate } = rawCandidate.parse(raw), evidence = z.record(text, z.unknown()).parse(parseApplicationJson(evidence_json));
        for (const key of ["contact_status", "associated_social_accounts", "manual_profile_contact", "manual_wechat_accounts"]) delete evidence[key];
        const author = discoveryEligibleContactAuthor(this.#db, candidate.campaign_id, candidate.criteria_revision, candidate.id);
        if (["linkedin", "reddit"].includes(candidate.platform) && author === candidate.platform_account_id && this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_campaigns WHERE id=? AND current_revision=?) AS current").get(candidate.campaign_id, candidate.criteria_revision)!.current === 1) evidence.manual_profile_contact = true;
        const wechat = loadDiscoveryManualWechat(this.#db, { ...candidate, evidence }); if (wechat.length) evidence.manual_wechat_accounts = wechat;
        const social = loadDiscoveryContactObservations(this.#db, candidate.campaign_id, candidate.criteria_revision, candidate.id, candidate.platform_account_id, true); if (social.length) evidence.associated_social_accounts = social;
        if (author !== null && discoveryHasHiddenEmail(this.#db, candidate.campaign_id, candidate.criteria_revision, candidate.id, candidate.platform_account_id) && loadDiscoveryContactObservations(this.#db, candidate.campaign_id, candidate.criteria_revision, candidate.id, candidate.platform_account_id).length === 0) evidence.contact_status = { has_contact: false, manual_email_required: true };
        return { ...candidate, evidence };
      });
    });
  }
  count(campaign: string, excludedPlatforms: readonly string[] = []) {
    text.min(1).parse(campaign); const excluded = z.array(text).parse(excludedPlatforms);
    return this.#read(() => {
      const row = this.#db.prepare(`SELECT c.current_revision,c.phase,p.policy FROM discovery_campaigns c JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.id AND p.criteria_revision=c.current_revision WHERE c.id=?`).get(campaign);
      if (!row) throw new Error("discovery_plan_missing");
      const head = z.object({ current_revision: z.number().int(), phase: text, policy: text }).parse(row), exclusions = stringifyApplicationJson(excluded);
      const public_page_errors = z.number().int().nonnegative().parse(this.#db.prepare(`SELECT COUNT(*) AS count FROM discovery_public_page_reads r JOIN discovery_candidates c ON c.id=r.candidate_id WHERE c.discovery_campaign_id=? AND c.criteria_revision=? AND r.status='failed' AND NOT EXISTS(SELECT 1 FROM json_each(?) e WHERE e.value=c.platform)`).get(campaign, head.current_revision, exclusions)!.count);
      if (head.policy === "score_and_contact_v1") {
        if (head.phase !== "active") return { public_page_errors, qualified: 0, pending: 0, rejected: 0 };
        const groups = buildIdentityGroups(loadIdentityPublicationBasis(this.#db, campaign, head.current_revision, false)).groups;
        const members = groups.flatMap(group => group.members).filter(member => !excluded.includes(member.platform));
        return { public_page_errors, qualified: groups.filter(group => group.members.some(member => member.verdict === "qualified" && !excluded.includes(member.platform))).length, pending: members.filter(member => member.verdict === "pending").length, rejected: members.filter(member => member.verdict === "rejected").length };
      }
      const counts = this.#db.prepare(`WITH candidates AS (SELECT d.id,d.verdict,d.verdict='qualified' OR m.candidate_id IS NOT NULL AS recommended
        FROM discovery_campaigns c LEFT JOIN discovery_candidates d ON d.discovery_campaign_id=c.id JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.id AND p.criteria_revision=c.current_revision
        LEFT JOIN discovery_manual_approvals m ON m.discovery_campaign_id=c.id AND m.criteria_revision=c.current_revision AND m.criteria_revision=d.criteria_revision AND m.candidate_id=d.id
        WHERE c.id=?1 AND (d.id IS NULL OR d.criteria_revision=c.current_revision) AND NOT EXISTS(SELECT 1 FROM json_each(?2) e WHERE e.value=d.platform))
        SELECT COUNT(*) FILTER(WHERE recommended) AS qualified,COUNT(*) FILTER(WHERE id IS NOT NULL AND verdict!='rejected' AND NOT recommended) AS pending,COUNT(*) FILTER(WHERE verdict='rejected' AND NOT recommended) AS rejected FROM candidates`).get(campaign, exclusions);
      return { public_page_errors, ...z.object({ qualified: z.number().int().nonnegative(), pending: z.number().int().nonnegative(), rejected: z.number().int().nonnegative() }).parse(counts) };
    });
  }
}
