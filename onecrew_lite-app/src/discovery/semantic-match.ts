import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { discoveryJsonValue } from "./library-material.ts";
import { discoveryReviewFinding, INITIAL_MATCH_REVIEWER } from "./initial-match.ts";
import type { DiscoveryReviewMaterial } from "./review-material.ts";
import type { DiscoveryReviewReceipt, SubmitDiscoveryReviewInput } from "./review-submission.ts";
import type { DiscoveryRepositoryResolver, OwnedDiscoveryRepository } from "./search-execution.ts";
import type { SemanticStructuredClient } from "./semantic-client.ts";
import { structuredSemanticSchema } from "./semantic-planner.ts";
import { DiscoveryScoreGate } from "./score-gate.ts";
import { discoveryExecutionFailureCode, discoveryFindingFailureCodes } from "./execution-failure.ts";
export { DISCOVERY_SCORE_CONCURRENCY } from "./score-gate.ts";

const conciseFinding = discoveryReviewFinding.extend({ rationale: z.string().min(1).max(1000), citations: discoveryReviewFinding.shape.citations.max(8) });
export const discoveryMatchProposal = z.strictObject({ finding: conciseFinding, language_only_pending: z.boolean(), requirement_findings: z.array(conciseFinding).max(32).default([]) });
const RUBRIC_SHA256 = "4490c5a30b0c9fb5859b91ea8ac452237c74c5ad3b0876677e18f4f192751729";
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const u64 = (value: unknown): bigint | null => typeof value === "bigint" && value >= 0n && value <= 18446744073709551615n ? value : typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? BigInt(value) : null;
export async function loadOriginalDiscoveryMatchRubric(path: string): Promise<string> {
  if (!isAbsolute(path)) throw new Error("discovery_match_rubric_path_not_trusted");
  const bytes = await readFile(path); if (bytes.length > 65536) throw new Error("discovery_match_rubric_changed");
  const rubric = new TextDecoder("utf-8", { fatal: true }).decode(bytes).replaceAll("\r\n", "\n");
  if (createHash("sha256").update(rubric).digest("hex") !== RUBRIC_SHA256) throw new Error("discovery_match_rubric_changed"); return rubric;
}
export function loadBundledDiscoveryMatchRubric(): Promise<string> {
  return loadOriginalDiscoveryMatchRubric(runtimeAssetPath("discovery/assets/match-rubric.txt", new URL("./assets/match-rubric.txt", import.meta.url)));
}
function observedSource(material: DiscoveryReviewMaterial): string | null {
  const read = material.native_content; if (!read || read.status !== "completed" || read.stop_code !== null) return null;
  const native = object(read.native_content), source = read.claim.platform === "twitter" ? object(native.identity).source_url : ["instagram", "linkedin", "pinterest"].includes(read.claim.platform) ? native.source_url : read.claim.source_url;
  return typeof source === "string" ? source : null;
}
export function discoveryMatchPromptMaterial(material: DiscoveryReviewMaterial, count: number | bigint | null) {
  const value = object(parseApplicationJson(Buffer.from(stringifyApplicationJson(material))));
  delete value.material_hash; delete value.candidate_evidence_hash;
  value.observed_followers = count;
  // 原serde_json的可变language_samples索引会为旧格式材料插入null；该值参与幂等键。
  if (!Object.hasOwn(value, "language_samples")) value.language_samples = null;
  let matched = observedSource(material);
  if (material.native_content?.claim.platform === "twitter" && material.native_content.status === "completed" && material.native_content.stop_code === null) matched ??= material.native_content.claim.source_url;
  matched ??= material.hit.source_url;
  if (Array.isArray(value.language_samples)) for (const raw of value.language_samples) { const sample = object(raw); if (Array.isArray(sample.comments)) sample.comments = sample.source_url === matched ? sample.comments.slice(0, 5) : []; }
  value.matched_language_source_url = observedSource(material) ?? material.hit.source_url;
  return value;
}
export function discoveryNeedsInstagramContent(material: DiscoveryReviewMaterial, count: number | bigint | null): boolean {
  if (material.hit.platform !== "instagram" || material.native_content || material.video_description || material.transcript) return false;
  if (object(material.native_profile).private_account === true) return false;
  const criteria = object(material.criteria), minimum = u64(criteria.min_subscribers) ?? u64(criteria.min_followers);
  if (minimum !== null && minimum > 0n) {
    const observed = u64(count), estimate = material.native_follower_estimate;
    if (estimate ? BigInt(estimate.lower) < minimum : observed === null || observed < minimum) return false;
  }
  const bio = material.native_profile?.bio?.normalize("NFKC").toLowerCase() ?? "";
  const terms = material.hit.search_keyword.normalize("NFKC").toLowerCase().match(/[\p{L}\p{N}]{3,}/gu) ?? [];
  const topic = terms.filter(term => !["blogger", "creator", "influencer", "account", "official"].includes(term));
  return !topic.some(term => bio.includes(term)) || typeof criteria.language === "string" && !!criteria.language.trim();
}
export function buildDiscoveryMatchPrompt(objective: string, material: DiscoveryReviewMaterial, rawValue: unknown, rubric: string): string {
  if (createHash("sha256").update(rubric).digest("hex") !== RUBRIC_SHA256) throw new Error("discovery_match_rubric_changed");
  const value = object(parseApplicationJson(Buffer.from(stringifyApplicationJson(rawValue)))), criteria = object(value.criteria);
  delete criteria.automatic_query_expansion; delete criteria.table_subject;
  const languageStart = rubric.indexOf("Return language_only_pending=true ONLY"), instagramStart = rubric.indexOf("\nFor Instagram,");
  if (languageStart < 0 || instagramStart <= languageStart) throw new Error("discovery_match_rubric_changed");
  let selected = rubric.slice(0, languageStart);
  if (Array.isArray(criteria.requirements) && criteria.requirements.length > 0) selected += '\nReturn requirement_findings as an array. For EACH frozen criteria.requirements[index], return exactly one finding with criterion={"requirement":index}, outcome supported/contradicted/insufficient, rationale and exact saved citations. Supported means the candidate satisfies the complete condition; contradicted requires positive evidence of violation. Missing evidence or no mention cannot prove absence of a forbidden partnership, an all-time guarantee or an unknown private fact: return insufficient. Preserve exclusions. tool, country_code and match_level must be null. These findings supplement match_fit, which evaluates core topic, followers, language and relevance. A positive match_fit never overrides an unresolved requirement.\n';
  selected += "\nJudge only the approved conditions. Missing engagement counts, publication dates, transcripts, or comments do not block basic category and follower matching when those extra conditions were not requested. A verified author-bound video_title is native content evidence and can be cited with kind=native_content. Distinguish a clear out-of-category sample (low fit) from missing evidence. Request additional evidence only to resolve a specific approved condition. Native observations belong only to their bound source; never treat absent metrics as zero or a complete publishing history.\n";
  if (material.native_follower_estimate) selected += "\nThe native_follower_estimate is a rounded public display, not an exact count. Report its display as approximate (约). Use lower >= required minimum to confirm the follower condition; upper < minimum to confirm failure. If the interval crosses the minimum, the condition is unknown. A null observed_followers does not erase this bound native estimate.\n";
  const language = object(material.criteria).language;
  if (typeof language === "string" && language.trim()) selected += rubric.slice(languageStart, instagramStart);
  else { selected += "No content-language condition is approved. Return language_only_pending=false. Do not request language evidence or comments.\n"; if (Array.isArray(value.language_samples)) for (const sample of value.language_samples) delete object(sample).comments; }
  if ((material.native_platform ?? material.hit.platform) === "instagram") selected += rubric.slice(instagramStart);
  for (const key of ["approved_scopes", "approved_at", "campaign_id", "criteria_revision", "candidate_id", "format_version"]) delete value[key];
  // 评分只传证据和来源，调度信息仍完整保存在原始材料中。
  for (const read of [value.transcript, value.native_content, ...(Array.isArray(value.public_pages) ? value.public_pages : [])]) {
    const entry = object(read);
    for (const key of ["search_hit_id", "candidate_id", "profile_ref", "call_number", "captured_at", "updated_at"]) delete entry[key];
    const claim = object(entry.claim);
    for (const key of ["campaign_id", "expected_revision", "search_hit_id", "profile_ref", "call_number"]) delete claim[key];
    // 已筛选的language_samples保留同一正文及评论，避免重复传输。
    if (entry.language_sample !== undefined && Array.isArray(value.language_samples) && value.language_samples.some(sample => stringifyApplicationJson(sample) === stringifyApplicationJson(entry.language_sample))) delete entry.language_sample;
  }
  selected += "\nEvidence budget: assess the saved profile, matched content and available recent content together. Use the smallest sufficient set of exact citations. If the approved conditions are supported, score immediately; do not request more posts, engagement or comments for corroboration alone. When insufficient, name only the missing approved conditions so collection can target that gap.\n";
  selected += "Keep each rationale under 1000 characters and 4000 UTF-8 bytes. Copy citations exactly from saved fields; never add ellipses, reformat or translate quotations.\n";
  return `${selected}\nUNTRUSTED_SAVED_DATA:\n${stringifyApplicationJson(discoveryJsonValue({ objective, material: value }))}`;
}
export function discoveryMatchEvidenceKey(material: DiscoveryReviewMaterial, count: number | bigint | null, rubric: string, reassess: boolean): string {
  const value = discoveryMatchPromptMaterial(material, count);
  return `initial-v2-${createHash("sha256").update(`${rubric}:${stringifyApplicationJson(discoveryJsonValue(value))}:${reassess ? material.material_hash : "automatic"}`).digest("hex")}`;
}
export interface PreparedSemanticDiscoveryMatch { readonly evidenceKey: string }
export type DiscoveryMatchPreparation = { state: "skipped" } | { state: "cached"; receipt: DiscoveryReviewReceipt } | { state: "model"; prepared: PreparedSemanticDiscoveryMatch };
interface MatchPayload { context: AccountContext; owned: OwnedDiscoveryRepository; input: { campaign_id: string; expected_revision: number; search_hit_id: string }; material: DiscoveryReviewMaterial; prompt: string; observationKey: string }

export class DiscoverySemanticMatcher {
  readonly #accounts: AccountContextService; readonly #repositories: DiscoveryRepositoryResolver; readonly #client: SemanticStructuredClient; readonly #rubric: string;
  readonly #prepared = new WeakMap<PreparedSemanticDiscoveryMatch, MatchPayload>(); readonly #gate = new DiscoveryScoreGate();
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, client: SemanticStructuredClient, verifiedRubric: string) {
    if (createHash("sha256").update(verifiedRubric).digest("hex") !== RUBRIC_SHA256) throw new Error("discovery_match_rubric_changed");
    this.#accounts = accounts; this.#repositories = repositories; this.#client = client; this.#rubric = verifiedRubric;
  }
  #assert(context: AccountContext, owned: OwnedDiscoveryRepository): void {
    this.#accounts.assertCurrent(context); owned.assertOwned();
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_match_repository_owner_mismatch");
  }
  async #commit(context: AccountContext, owned: OwnedDiscoveryRepository, input: SubmitDiscoveryReviewInput): Promise<DiscoveryReviewReceipt> {
    this.#assert(context, owned);
    const committed = await Promise.resolve().then(() => owned.repository.submitReview(context.principal.user.id, input)).then(receipt => ({ ok: true as const, receipt }), error => ({ ok: false as const, error: error as unknown }));
    if (committed.ok) return committed.receipt;
    // 原账号事务可能已提交；只读取同一幂等键回执，不重新评分或再次写入。
    owned.assertOwned();
    const saved = owned.repository.getReview(context.principal.user.id, input.campaign_id, input.request_id);
    if (saved) return saved;
    throw committed.error;
  }
  async prepare(context: AccountContext, raw: unknown, reassess = false): Promise<DiscoveryMatchPreparation> {
    const input = z.strictObject({ campaign_id: z.string().min(1).refine(value => Buffer.byteLength(value) <= 128), expected_revision: z.number().int().positive().max(4294967295), search_hit_id: z.string().min(1).refine(value => Buffer.byteLength(value) <= 128) }).parse(raw);
    this.#accounts.assertCurrent(context); const owned = await this.#repositories.resolve(context); this.#assert(context, owned);
    const saved = await Promise.resolve().then(() => owned.repository.prepareInitialMatchMaterial(input.campaign_id, input.expected_revision, input.search_hit_id)).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    if (!saved.ok) { if (saved.error instanceof Error && ["review_material_author_binding_unavailable", "review_platform_unsupported", "review_source_unavailable"].includes(saved.error.message)) return { state: "skipped" }; throw saved.error; }
    const { material, count, preserve } = saved.value, key = discoveryMatchEvidenceKey(material, count, this.#rubric, reassess);
    const prior = owned.repository.getReview(context.principal.user.id, input.campaign_id, key); this.#assert(context, owned);
    if (prior) return { state: "cached", receipt: prior }; if (preserve && !reassess) return { state: "skipped" };
    const campaign = owned.repository.getPlan(input.campaign_id), criteria = object(material.criteria), minimum = u64(criteria.min_subscribers) ?? u64(criteria.min_followers), observed = u64(count);
    if (minimum !== null && minimum > 0n && observed !== null && observed < minimum && !Array.isArray(criteria.requirements)) {
      const rationale = /[\u4e00-\u9fff]/u.test(campaign.brief) ? `初步评估：已观察到的原生粉丝数为 ${observed}，低于要求的 ${minimum}。此项必要条件已确定不符合；其他样本证据或未知信息不会抵消该门槛失败。` : `Initial assessment: the observed native follower count is ${observed}, below the required minimum of ${minimum}. This confirmed required failure takes precedence over other sample evidence or unknown information.`;
      this.#assert(context, owned);
      return { state: "cached", receipt: await this.#commit(context, owned, { ...input, material_hash: material.material_hash, request_id: key, reviewer: { kind: "agent", label: INITIAL_MATCH_REVIEWER }, findings: [{ criterion: "match_fit", outcome: "supported", match_level: "low", rationale, citations: [], tool: null, country_code: null }] }) };
    }
    if (!reassess && discoveryNeedsInstagramContent(material, count)) return { state: "skipped" };
    const prepared = Object.freeze({ evidenceKey: key });
    this.#prepared.set(prepared, { context, owned, input, material, observationKey: discoveryMatchEvidenceKey(material, count, this.#rubric, false), prompt: buildDiscoveryMatchPrompt(campaign.brief, material, discoveryMatchPromptMaterial(material, count), this.#rubric) });
    return { state: "model", prepared };
  }
  async evaluate(context: AccountContext, prepared: PreparedSemanticDiscoveryMatch): Promise<DiscoveryReviewReceipt | null> {
    const payload = this.#prepared.get(prepared); if (!payload || payload.context !== context) throw new Error("initial_match_preparation_not_owned");
    this.#assert(context, payload.owned);
    this.#prepared.delete(prepared);
    const author = stringifyApplicationJson([context.principal.user.id, payload.input.campaign_id, payload.input.expected_revision, payload.material.native_platform ?? payload.material.hit.platform, payload.material.native_author_id]);
    const permit = this.#gate.acquire(author);
    try {
      let correction = "";
      for (let attempt = 0; attempt < 2; attempt++) {
      const proposal = await this.#client.structured({ prompt: payload.prompt + correction, schema: structuredSemanticSchema(discoveryMatchProposal), decode: discoveryMatchProposal, planning: false, signal: context.signal, assertCurrent: async () => { this.#assert(context, payload.owned); } });
      this.#assert(context, payload.owned);
      const current = payload.owned.repository.prepareInitialMatchMaterial(payload.input.campaign_id, payload.input.expected_revision, payload.input.search_hit_id);
      if (discoveryMatchEvidenceKey(current.material, current.count, this.#rubric, false) !== payload.observationKey) return null;
      const findings = [proposal.finding];
      if (proposal.language_only_pending && proposal.finding.outcome === "insufficient") findings.push({ ...proposal.finding, criterion: "content_language", outcome: "insufficient", match_level: null, tool: null, country_code: null });
      findings.push(...proposal.requirement_findings);
      const result = await this.#commit(context, payload.owned, { ...payload.input, material_hash: current.material.material_hash, request_id: prepared.evidenceKey, reviewer: { kind: "agent", label: INITIAL_MATCH_REVIEWER }, findings }).then(receipt => ({ ok: true as const, receipt }), error => ({ ok: false as const, error: error as unknown }));
      if (result.ok) return result.receipt;
      const code = discoveryExecutionFailureCode(result.error);
      if (attempt !== 0 || !discoveryFindingFailureCodes.some(failure => failure === code)) throw result.error;
      correction = `\nCORRECTION_REQUIRED: The previous proposal failed validation: ${code}. Return one corrected proposal using only the saved evidence. match_fit must use supported with a match_level or insufficient with null match_level. tool and country_code must be null. Use at most 8 exact saved quotations; keep each rationale within 1000 characters and 4000 UTF-8 bytes. Previous proposal: ${stringifyApplicationJson(proposal)}`;
      }
      throw new Error("initial_match_correction_exhausted");
    } finally { permit.release(); }
  }
}
