import { createHash } from "node:crypto";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { discoveryScale, parseDiscoverySubscribers } from "./qualification.ts";

const text = z.string().refine(value => value.isWellFormed());
const integer = z.union([z.number().int().safe(), z.bigint()]);
export const discoverySearchHit = z.strictObject({ id: text, campaign_id: text, platform: text, profile_ref: text, search_keyword: text, source_url: text, title: text, author_label: text, captured_at: integer, author_id: text.nullable(), author_url: text.nullable(), author_observed_at: integer.nullable() });
export const discoveryContact = z.strictObject({ kind: z.enum(["email", "telegram", "hidden_email", "social_profile", "whatsapp", "wechat", "wechat_official", "website", "messenger", "phone"]), original_value: text, normalized_value: text, source_url: text, source_excerpt: text, captured_at: integer, native_author_id: text, attribution: z.enum(["author_published", "unconfirmed"]) });
export const discoveryLibraryBasis = z.strictObject({ campaign_id: text, revision: z.number().int().positive(), criteria_json: text, scopes_json: text, approved_at: integer, contact_required: z.boolean(), rows: z.array(z.strictObject({ hit: discoverySearchHit, candidate: z.strictObject({ id: text, followers: text.nullish().transform(value => value ?? null), native_platform: text.nullish().transform(value => value ?? null), native_author_id: text, verdict: text, evidence_json: text, identity_receipt: text, manual_approved_at: integer.nullable(), contacts: z.array(discoveryContact).default([]), pending_details: z.array(text).max(4).default([]) }).nullable() })).max(10000) });
export type DiscoveryLibraryBasis = z.infer<typeof discoveryLibraryBasis>;
const matchRating = z.object({ level: z.enum(["excellent", "good", "medium", "low"]), reason: text });
function object(value: unknown): Record<string, unknown> { return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {}; }
// serde_json::Value对象按UTF8键序编码；领域struct继续按声明顺序编码。
export function discoveryJsonValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(discoveryJsonValue);
  if (value !== null && typeof value === "object") {
    const fields: Record<string, unknown> = Object.create(null) as Record<string, unknown>;
    for (const [key, item] of Object.entries(value)) fields[key] = discoveryJsonValue(item);
    return new Proxy(fields, { ownKeys: target => Object.keys(target).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b))) });
  }
  return value;
}
export function discoveryMaterialDigest(value: unknown): string { return createHash("sha256").update(stringifyApplicationJson(value)).digest("hex"); }
export function discoveryScorePasses(verdict: string, evidence: unknown, author: string): boolean {
  const value = object(evidence), assessment = object(value.assessment), reviewed = object(value.reviewed_match), rating = object(assessment.match_rating);
  return verdict === "qualified" && Array.isArray(assessment.reasons) && assessment.reasons.length === 0 && reviewed.verified === true && reviewed.native_author_id === author && reviewed.outcome === "supported" && reviewed.level === rating.level && ["excellent", "good", "medium"].includes(String(rating.level));
}
export function buildDiscoveryLibrarySnapshot(raw: unknown) {
  const basis = discoveryLibraryBasis.parse(raw);
  let bytes = Buffer.byteLength(basis.criteria_json) + Buffer.byteLength(basis.scopes_json);
  for (const source of basis.rows) {
    const hit = source.hit;
    for (const value of [hit.id, hit.campaign_id, hit.platform, hit.profile_ref, hit.source_url, hit.title, hit.author_label, hit.search_keyword, hit.author_id ?? "", hit.author_url ?? ""]) bytes += Buffer.byteLength(value);
    const candidate = source.candidate;
    if (candidate !== null) {
      for (const value of [candidate.id, candidate.native_platform ?? "", candidate.native_author_id, candidate.verdict, candidate.evidence_json, candidate.identity_receipt]) bytes += Buffer.byteLength(value);
      if (candidate.contacts.length) bytes += Buffer.byteLength(stringifyApplicationJson(candidate.contacts));
    }
    if (bytes > 32 * 1024 * 1024) throw new Error("library_source_snapshot_too_large");
  }
  const parse = (value: string) => parseApplicationJson(Buffer.from(value));
  const criteria = discoveryJsonValue(parse(basis.criteria_json));
  const scopes = z.array(z.strictObject({ platform: text, profile_ref: text, keywords: z.array(text) })).parse(parse(basis.scopes_json));
  const rows = basis.rows.map(source => {
    if (!scopes.some(scope => scope.platform === source.hit.platform && scope.profile_ref === source.hit.profile_ref && scope.keywords.includes(source.hit.search_keyword))) throw new Error("library_source_scope_mismatch");
    const candidate = source.candidate;
    let candidateId: string | null = null, authorId: string | null = null, verdict = "unverified", reasons = ["author_candidate_binding_unverified"], evidence: unknown = null, receipt: unknown = null;
    if (candidate !== null) {
      evidence = discoveryJsonValue(parse(candidate.evidence_json)); receipt = discoveryJsonValue(parse(candidate.identity_receipt));
      const assessment = object(object(evidence).assessment); reasons = z.array(text).parse(assessment.reasons);
      if (!["qualified", "pending", "rejected"].includes(candidate.verdict) || reasons.length > 50 || reasons.some(reason => !reason.length || Buffer.byteLength(reason) > 2000) || candidate.verdict === "qualified" && reasons.length) throw new Error("library_source_assessment_invalid");
      candidateId = candidate.id; authorId = candidate.native_author_id;
      if (reasons.some(reason => ["excluded_by_customer", "excluded_account_identity_unverified", "excluded_accounts_invalid", "requirement_unverified", "requirement_contradicted", "requirements_invalid"].includes(reason))) verdict = candidate.verdict;
      else if (basis.contact_required) {
        if (discoveryScorePasses(candidate.verdict, evidence, authorId) && candidate.contacts.length) verdict = "qualified";
        else if (candidate.verdict === "rejected") verdict = "rejected";
        else { verdict = "pending"; if (candidate.verdict === "qualified") reasons.push("contact_unavailable"); }
      } else if (candidate.manual_approved_at !== null) { reasons.unshift("manual_approved"); verdict = "qualified"; }
      else verdict = candidate.verdict;
    }
    const evidenceBasis = [1, basis.campaign_id, basis.revision, criteria, source.hit, candidateId, authorId, verdict, evidence, receipt, candidate?.manual_approved_at ?? null];
    const evidenceVersion = discoveryMaterialDigest(basis.contact_required ? [2, evidenceBasis, candidate?.contacts ?? null] : evidenceBasis);
    if (verdict === "pending" && candidate?.pending_details.length) reasons = [...reasons, ...candidate.pending_details];
    const ratingValue = object(object(evidence).assessment).match_rating;
    const rating = candidate !== null && candidate.manual_approved_at !== null && candidate.verdict !== "qualified" ? null : ratingValue !== null && typeof ratingValue === "object" && !Array.isArray(ratingValue) ? matchRating.parse(ratingValue) : null;
    const pinterest = (candidate?.native_platform ?? source.hit.platform) === "pinterest" && candidate !== null;
    const monthlyRaw = pinterest && typeof object(evidence).pinterest_monthly_views_raw === "string" ? object(evidence).pinterest_monthly_views_raw as string : null;
    const scale = pinterest ? discoveryScale(criteria, candidate.followers ? parseDiscoverySubscribers(candidate.followers) : null, monthlyRaw) : null;
    const bioStatus = object(evidence).profile_bio_status;
    return { hit: source.hit, ...(candidate?.followers !== null && candidate?.followers !== undefined ? { followers: candidate.followers } : {}), ...(candidate?.native_platform !== null && candidate?.native_platform !== undefined ? { native_platform: candidate.native_platform } : {}), candidate_id: candidateId, native_author_id: authorId, verdict, reasons, evidence_version: evidenceVersion, ...(rating !== null ? { match_rating: rating } : {}), ...(typeof bioStatus === "string" && ["complete", "truncated", "missing", "unavailable", "unknown"].includes(bioStatus) ? { bio_status: bioStatus } : {}), ...(scale !== null ? { pinterest_monthly_views_raw: monthlyRaw, pinterest_scale_passed_by: scale.passed_by } : {}), ...(candidate !== null && candidate.contacts.length ? { contacts: candidate.contacts } : {}) };
  });
  if (object(criteria).contact_preference === "public_email_first") rows.sort((left, right) => Number(left.verdict !== "qualified") - Number(right.verdict !== "qualified") || Number(!left.contacts?.some(contact => contact.kind === "email")) - Number(!right.contacts?.some(contact => contact.kind === "email")));
  const snapshotBasis = [1, basis.campaign_id, basis.revision, criteria, scopes, rows];
  return { campaign_id: basis.campaign_id, criteria_revision: basis.revision, source_revision: 0, snapshot_hash: discoveryMaterialDigest(basis.contact_required ? [2, snapshotBasis, "score_and_contact_v1"] : snapshotBasis), rows };
}
