import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { parseApplicationJson } from "../contracts/application-json.ts";
import type { CreatorSearchHit } from "../platforms/opencli/search-results.ts";
import { discoveryPlanInput, discoveryScope, assertRegisteredDiscoveryScopes, appendDiscoveryQueries, type DiscoveryScope } from "./contracts.ts";
import { canAdvanceSearchAfterStop, nextDiscoverySearchKeyword, repairedSearchRecoveryCodes, type DiscoverySearchRecovery } from "./stops.ts";
import { DiscoveryLibraryRepository } from "./library-repository.ts";
import { DiscoveryReviewRepository } from "./review-repository.ts";
import { loadDiscoveryReviewBasis } from "./review-basis.ts";
import { buildDiscoveryReviewMaterial } from "./review-material.ts";
import { assessDiscoveryYoutube } from "./qualification.ts";
import { DiscoveryCandidateRepository, type DiscoveryCandidatePageInput } from "./candidate-repository.ts";
import { DiscoveryIdentityPublicationRepository } from "./identity-library-publication.ts";
import { discoveryProductName } from "./library-name.ts";
import { DiscoveryProfileRepository, type DiscoveryProfileClaim } from "./profile-repository.ts";
import { discoverySearchHit } from "./library-material.ts";
import { recoverDiscoveryReads } from "./recovery.ts";
import { DiscoveryVideoRepository, type DiscoveryVideoClaim } from "./video-repository.ts";
import { getDiscoverySavedContent, getDiscoverySavedPublicPage, getDiscoverySavedTranscript, getDiscoverySavedIdentity } from "./saved-read-repository.ts";
import { DiscoveryTranscriptRepository, type DiscoveryTranscriptClaim } from "./transcript-repository.ts";
import { DiscoveryContentRepository } from "./content-repository.ts";
import type { DiscoveryContentClaim } from "./content-result.ts";
import { DiscoveryPublicPageRepository, type DiscoveryPublicPageClaim, type FinishDiscoveryPublicPageInput } from "./public-page-repository.ts";
import type { DiscoveryPublicPageInput } from "./contact-native.ts";
import { DiscoveryLanguageRepository } from "./language-repository.ts";
import type { DiscoveryLanguageClaim, DiscoveryLanguageSample } from "./language-contracts.ts";
import { DiscoveryContactLookupRepository, type DiscoveryContactLookup } from "./public-page-lookup.ts";
import { DiscoveryPostAuthorRepository } from "./post-author-repository.ts";
import type { DiscoveryPostAuthorClaim } from "./post-author-contracts.ts";
import { DiscoverySourceAuthorRepository, type DiscoverySourceAuthorClaim } from "./source-author-repository.ts";
import { DiscoveryWebSourceRepository } from "./web-source-repository.ts";
import type { DiscoveryWebSourceClaim } from "./web-source-contracts.ts";
import { DiscoveryFacebookIdentityRepository, type DiscoveryFacebookIdentityClaim } from "./facebook-identity.ts";
import { DiscoveryFacebookTargetsRepository } from "./facebook-targets.ts";
import { planDiscoveryStep, type DiscoveryStep } from "./step.ts";
import { loadDiscoveryReadStops } from "./read-health.ts";

export interface DiscoverySearchClaim { planId: string; expectedRevision: number; platform: string; profileRef: string; callNumber: number; query: string }
const runSchema = z.object({ status: z.enum(["unsearched", "searching", "completed", "technical_stop", "risk_control_stop", "unauthorized", "budget_exhausted"]), stop_code: z.string().nullable(), active_query: z.string().nullable(), calls_used: z.number().int().nonnegative(), updated_at: z.number().int().safe() });
const storedScopes = z.array(z.object({ platform: z.string(), profile_ref: z.string(), keywords: z.array(z.string()) }).strict());
const headSchema = z.object({ max_calls: z.number().int().positive().nullable(), used_calls: z.number().int().nonnegative(), scope_json: z.string() });
// 数据库由统一账号仓储持有；调用方必须在每次操作前后校验当前账号与租约。
export class DiscoveryRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { this.#db = database; }
  recoverInterruptedReads() { return recoverDiscoveryReads(this.#db); }
  readStops(campaign: string) { return [...loadDiscoveryReadStops(this.#db, campaign, "profile"), ...loadDiscoveryReadStops(this.#db, campaign, "content")]; }
  remainingSlots(campaign: string, excluded: readonly string[] = []) { return Math.max(0, this.getPlan(campaign).target_count - this.countCandidates(campaign, excluded).qualified); }
  candidateIsQualified(campaign: string, candidate: string): boolean { return this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_candidates c JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id WHERE c.id=?1 AND d.id=?2 AND c.criteria_revision=d.current_revision AND c.verdict='qualified') qualified").get(candidate, campaign)!.qualified === 1; }
  recommendationRevision(campaign: string): string {
    const rows = this.#db.prepare(`SELECT c.id,json_array(c.display_name,c.profile_url,c.platform,c.evidence_json) AS material,o.kind,o.normalized_value,o.source_url
      FROM discovery_candidates c JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id JOIN discovery_contact_observations o ON o.candidate_id=c.id AND o.criteria_revision=c.criteria_revision
      WHERE d.id=?1 AND c.criteria_revision=d.current_revision AND c.verdict='qualified' ORDER BY c.id,o.kind,o.normalized_value,o.source_url`).all(campaign);
    return stringifyApplicationJson(rows.map(row => [z.string().parse(row.id), z.string().parse(row.material), z.string().parse(row.kind), z.string().parse(row.normalized_value), z.string().parse(row.source_url)]));
  }
  nextFacebookIdentity(campaign: string, excluded: readonly string[] = []) { return new DiscoveryFacebookIdentityRepository(this.#db).next(campaign, excluded); }
  claimFacebookIdentity(campaign: string, revision: number, hit: string) { return new DiscoveryFacebookIdentityRepository(this.#db).claim(campaign, revision, hit); }
  finishFacebookIdentity(claim: DiscoveryFacebookIdentityClaim, native: Record<string, unknown> | null, stop: string | null) { return new DiscoveryFacebookIdentityRepository(this.#db).settle(claim, native, stop); }
  getFacebookIdentity(campaign: string, hit: string) { return new DiscoveryFacebookIdentityRepository(this.#db).get(campaign, hit); }
  interruptFacebookIdentity(claim: DiscoveryFacebookIdentityClaim) { return new DiscoveryFacebookIdentityRepository(this.#db).interrupt(claim); }
  nextFacebookTargets(campaign: string) { return new DiscoveryFacebookTargetsRepository(this.#db).next(campaign); }
  claimFacebookTargets(campaign: string, revision: number, hit: string) { return new DiscoveryFacebookTargetsRepository(this.#db).claim(campaign, revision, hit); }
  finishFacebookTargets(claim: DiscoveryProfileClaim, urls: readonly string[] | null, stop: string | null) { return new DiscoveryFacebookTargetsRepository(this.#db).finish(claim, urls, stop); }
  getFacebookTargets(campaign: string, hit: string) { return new DiscoveryFacebookTargetsRepository(this.#db).get(campaign, hit); }
  interruptFacebookTargets(claim: DiscoveryProfileClaim) { return new DiscoveryFacebookTargetsRepository(this.#db).interrupt(claim); }
  planStep(campaign: string, revision: number, excluded: readonly string[] = [], deferred: readonly string[] = []): DiscoveryStep { return planDiscoveryStep(this, campaign, revision, excluded, deferred); }
  planStepScoring(campaign: string, revision: number, excluded: readonly string[], deferred: readonly string[], scoring: readonly string[]): DiscoveryStep {
    const fallback = this.planStep(campaign, revision, excluded, deferred);
    if (fallback.kind === "target_reached" || fallback.kind === "budget_exhausted") return fallback;
    if (this.getPlan(campaign).platform_runs.some(run => run.status === "searching")) return { kind: "busy" };
    for (const hit of scoring) {
      const basis = this.#transaction(() => loadDiscoveryReviewBasis(this.#db, campaign, revision, hit), true);
      if (!excluded.includes(basis.hit.platform)) { const independent = this.nextIndependentRead(campaign, revision, hit, scoring); if (independent) return independent; }
    }
    return fallback;
  }
  nextIndependentRead(campaign: string, revision: number, scoringHit: string, scoring: readonly string[]): DiscoveryStep | null {
    const basis = this.#transaction(() => loadDiscoveryReviewBasis(this.#db, campaign, revision, scoringHit), true), platform = basis.hit.platform;
    if (!["twitter", "bilibili", "facebook", "reddit", "instagram", "weibo", "pinterest", "linkedin", "tiktok", "youtube"].includes(platform)) return null;
    const held = new Set<string>();
    for (const hit of scoring) {
      const source = this.#transaction(() => loadDiscoveryReviewBasis(this.#db, campaign, revision, hit), true);
      if (["google", "yahoo"].includes(source.hit.platform)) return null;
      const rows = this.#db.prepare(`SELECT h.id FROM discovery_search_hits h LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id LEFT JOIN discovery_search_authors held ON held.search_hit_id=?2 LEFT JOIN discovery_identity_reads i ON i.search_hit_id=h.id
        WHERE h.discovery_campaign_id=?1 AND(h.platform!=?3 OR h.id=?2 OR a.author_id=held.author_id OR a.author_id=?4 OR i.account_id=?4
        OR EXISTS(SELECT 1 FROM discovery_video_identity_bindings b WHERE b.search_hit_id=h.id AND b.native_author_id=?4) OR h.source_url IN(SELECT source_url FROM discovery_search_hits WHERE id=?2))`).all(campaign, hit, platform, source.native_author_id);
      for (const row of rows) held.add(z.string().parse(row.id));
    }
    const excludedHits = [...held].sort((left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right)));
    if (platform === "youtube") {
      const transcript = this.nextTranscriptHit(campaign, excludedHits); if (transcript) return { kind: "read_transcript", profile_ref: transcript.profile_ref, search_hit_id: transcript.id };
      const language = this.nextLanguage(campaign, platform, excludedHits); return language ? { kind: "read_content", search_hit_id: language } : null;
    }
    const content = this.nextContentHit(campaign, platform, excludedHits); if (content) return { kind: "read_content", search_hit_id: content };
    const profile = this.nextProfileHit(campaign, [], excludedHits); if (profile) return { kind: this.profileNeedsIdentity(campaign, profile) ? "read_identity" : "read_profile", search_hit_id: profile };
    if (platform === "facebook") { const identity = this.nextFacebookIdentity(campaign); if (identity && !held.has(identity)) return { kind: "read_facebook_identity", search_hit_id: identity }; }
    return null;
  }
  nextWebSource(campaign: string, excluded: readonly string[] = [], held: readonly string[] = []) { return new DiscoveryWebSourceRepository(this.#db).next(campaign, excluded, held); }
  webSourceSource(campaign: string, hit: string) { return new DiscoveryWebSourceRepository(this.#db).source(campaign, hit); }
  claimWebSource(campaign: string, revision: number, hit: string) { return new DiscoveryWebSourceRepository(this.#db).claim(campaign, revision, hit); }
  finishWebSource(claim: DiscoveryWebSourceClaim, native: unknown, stop: string | null) { return new DiscoveryWebSourceRepository(this.#db).finish(claim, native, stop); }
  getWebSource(campaign: string, hit: string) { return new DiscoveryWebSourceRepository(this.#db).get(campaign, hit); }
  nextContentHit(campaign: string, platform: string, held: readonly string[] = []) {
    if (["tiktok", "douyin", "google", "yahoo"].includes(platform)) return this.nextLanguage(campaign, platform, held);
    return this.nextNativeContentHit(campaign, platform, held) ?? this.nextLanguage(campaign, platform, held);
  }
  nextSearchKeyword(campaign: string, scope: DiscoveryScope) { return this.#next(campaign, scope); }
  nextSourceAuthor(campaign: string, excluded: readonly string[] = []) { return new DiscoverySourceAuthorRepository(this.#db).next(campaign, excluded); }
  claimSourceAuthor(campaign: string, revision: number, hit: string) { return new DiscoverySourceAuthorRepository(this.#db).claim(campaign, revision, hit); }
  finishSourceAuthor(claim: DiscoverySourceAuthorClaim, profile: unknown, content: unknown, stop: string | null) { return new DiscoverySourceAuthorRepository(this.#db).finish(claim, profile, content, stop); }
  getSourceAuthor(campaign: string, hit: string) { return new DiscoverySourceAuthorRepository(this.#db).get(campaign, hit); }
  interruptSourceAuthor(claim: DiscoverySourceAuthorClaim) { return new DiscoverySourceAuthorRepository(this.#db).interrupt(claim); }
  profileNeedsIdentity(campaign: string, hit: string) { return new DiscoveryPostAuthorRepository(this.#db).needs(campaign, hit); }
  claimPostAuthor(campaign: string, revision: number, hit: string) { return new DiscoveryPostAuthorRepository(this.#db).claim(campaign, revision, hit); }
  finishPostAuthor(claim: DiscoveryPostAuthorClaim, result: unknown) { return new DiscoveryPostAuthorRepository(this.#db).finish(claim, result); }
  interruptPostAuthor(claim: DiscoveryPostAuthorClaim) { return new DiscoveryPostAuthorRepository(this.#db).interrupt(claim); }
  nextLanguage(campaign: string, platform: string, held: readonly string[] = []) { return new DiscoveryLanguageRepository(this.#db).next(campaign, platform, held); }
  languageSource(campaign: string, hit: string) { return new DiscoveryLanguageRepository(this.#db).source(campaign, hit); }
  languageContext(claim: DiscoveryLanguageClaim) { return new DiscoveryLanguageRepository(this.#db).readContext(claim); }
  claimLanguage(campaign: string, revision: number, hit: string) { return new DiscoveryLanguageRepository(this.#db).claim(campaign, revision, hit); }
  finishLanguage(claim: DiscoveryLanguageClaim, sample: DiscoveryLanguageSample | null, stop: string | null) { return new DiscoveryLanguageRepository(this.#db).finish(claim, sample, stop); }
  nextContactLookup(campaign: string, excluded: readonly string[] = []) { return new DiscoveryContactLookupRepository(this.#db).next(campaign, excluded); }
  claimContactLookup(input: DiscoveryContactLookup) { return new DiscoveryContactLookupRepository(this.#db).claim(input); }
  finishContactLookup(input: DiscoveryContactLookup) { return new DiscoveryContactLookupRepository(this.#db).settle(input); }
  nextPublicPage(campaign: string, excluded: readonly string[] = []) { return new DiscoveryPublicPageRepository(this.#db).next(campaign, excluded); }
  claimPublicPage(input: DiscoveryPublicPageInput) { return new DiscoveryPublicPageRepository(this.#db).claim(input); }
  finishPublicPage(input: FinishDiscoveryPublicPageInput) { return new DiscoveryPublicPageRepository(this.#db).settle(input); }
  interruptPublicPage(claim: DiscoveryPublicPageClaim) { return new DiscoveryPublicPageRepository(this.#db).interrupt(claim); }
  nextNativeContentHit(campaign: string, platform: string, held: readonly string[] = []) { return new DiscoveryContentRepository(this.#db).nextNative(campaign, platform, held); }
  claimContent(campaign: string, revision: number, hit: string) { return new DiscoveryContentRepository(this.#db).claim(campaign, revision, hit); }
  finishContent(claim: DiscoveryContentClaim, native: unknown, stop: string | null) { return new DiscoveryContentRepository(this.#db).finish(claim, native, stop); }
  interruptContent(claim: DiscoveryContentClaim) { return new DiscoveryContentRepository(this.#db).interrupt(claim); }
  contentNativeIdentity(claim: DiscoveryContentClaim) { return new DiscoveryContentRepository(this.#db).nativeIdentity(claim); }
  nextTranscriptHit(campaign: string, held: readonly string[] = []) { return new DiscoveryTranscriptRepository(this.#db).next(campaign, held); }
  claimTranscript(campaign: string, revision: number, profile: string, hit: string) { return new DiscoveryTranscriptRepository(this.#db).claim(campaign, revision, profile, hit); }
  finishTranscript(claim: DiscoveryTranscriptClaim, result: unknown) { return new DiscoveryTranscriptRepository(this.#db).finish(claim, result); }
  interruptTranscript(claim: DiscoveryTranscriptClaim) { return new DiscoveryTranscriptRepository(this.#db).interrupt(claim); }
  getContent(campaign: string, hit: string) { return getDiscoverySavedContent(this.#db, campaign, hit); }
  getIdentity(campaign: string, hit: string) { return getDiscoverySavedIdentity(this.#db, campaign, hit); }
  getTranscript(campaign: string, hit: string) { return getDiscoverySavedTranscript(this.#db, campaign, hit); }
  getPublicPage(campaign: string, candidate: string, url: string) { return getDiscoverySavedPublicPage(this.#db, campaign, candidate, url); }
  nextVideoHit(campaign: string) { return new DiscoveryVideoRepository(this.#db).next(campaign); }
  claimVideo(campaign: string, revision: number, profile: string, hit: string) { return new DiscoveryVideoRepository(this.#db).claim(campaign, revision, profile, hit); }
  finishVideo(claim: DiscoveryVideoClaim, candidate: unknown, stop: string | null) { new DiscoveryVideoRepository(this.#db).finish(claim, candidate, stop); return this.getPlan(claim.campaign_id); }
  interruptVideo(claim: DiscoveryVideoClaim) { return new DiscoveryVideoRepository(this.#db).interrupt(claim); }
  claimProfile(campaign: string, revision: number, hit: string) { return new DiscoveryProfileRepository(this.#db).claim(campaign, revision, hit); }
  nextProfileHit(campaign: string, excluded: readonly string[] = [], held: readonly string[] = []) { return new DiscoveryProfileRepository(this.#db).next(campaign, excluded, held); }
  recoverInterruptedProfiles() { return new DiscoveryProfileRepository(this.#db).recoverInterrupted(); }
  finishProfile(claim: DiscoveryProfileClaim, native: unknown, stop: string | null) { return new DiscoveryProfileRepository(this.#db).finish(claim, native, stop); }
  interruptProfile(claim: DiscoveryProfileClaim) { return new DiscoveryProfileRepository(this.#db).interrupt(claim); }
  getProfile(campaign: string, hit: string) { return new DiscoveryProfileRepository(this.#db).get(campaign, hit); }
  listSearchHits(input: DiscoveryCandidatePageInput) {
    return this.#transaction(() => {
      const statement = this.#db.prepare(`SELECT h.id,h.discovery_campaign_id AS campaign_id,h.platform,h.profile_ref,h.search_keyword,h.source_url,h.title,h.author_label,h.captured_at,p.author_id,p.author_url,p.observed_at AS author_observed_at FROM discovery_search_hits h LEFT JOIN discovery_search_authors p ON p.search_hit_id=h.id WHERE h.discovery_campaign_id=? AND h.id>? ORDER BY h.id LIMIT ?`);
      statement.setReadBigInts(true);
      return statement.all(input.campaign_id, input.after_id ?? "", input.limit).map(row => discoverySearchHit.parse(Object.fromEntries(Object.entries(row).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value]))));
    }, true);
  }
  recentPlans() {
    return this.#transaction(() => {
      const statement = this.#db.prepare(`SELECT c.id,r.brief_json,c.phase,c.target_count,c.used_calls,c.max_calls,c.updated_at FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision ORDER BY c.updated_at DESC,c.created_at DESC,c.id DESC LIMIT 5`);
      statement.setReadBigInts(true);
      return statement.all().map(raw => {
        const value = Object.fromEntries(Object.entries(raw).map(([key, item]) => [key, typeof item === "bigint" && item >= BigInt(Number.MIN_SAFE_INTEGER) && item <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(item) : item]));
        const { brief_json, ...row } = z.object({ id: z.string(), brief_json: z.string(), phase: z.string(), target_count: z.number().int(), used_calls: z.number().int(), max_calls: z.number().int().nullable(), updated_at: z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]) }).parse(value);
        return { ...row, brief: z.string().parse(parseApplicationJson(brief_json)) };
      });
    }, true);
  }
  listCandidates(input: DiscoveryCandidatePageInput) { return new DiscoveryCandidateRepository(this.#db).list(input); }
  countCandidates(campaign: string, excludedPlatforms: readonly string[] = []) { return new DiscoveryCandidateRepository(this.#db).count(campaign, excludedPlatforms); }
  prepareLibraryPublication(campaign: string, revision: number) { return new DiscoveryIdentityPublicationRepository(this.#db).preparePublication(campaign, revision); }
  readLibraryMaterial(campaign: string, revision: number) { return new DiscoveryIdentityPublicationRepository(this.#db).readMaterial(campaign, revision); }
  getLibraryTableName(campaign: string): { name: string; previous_auto_name: string } {
    return this.#transaction(() => {
      const initial = z.object({ brief_json: z.string(), date: z.string(), ordinal: z.number().int() }).parse(this.#db.prepare(`SELECT r.brief_json,strftime('%m%d',c.created_at,'unixepoch','localtime') AS date,
        (SELECT COUNT(*) FROM discovery_campaigns p WHERE p.rowid<=c.rowid AND date(p.created_at,'unixepoch','localtime')=date(c.created_at,'unixepoch','localtime')) AS ordinal
        FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=1 WHERE c.id=?`).get(campaign));
      const brief = z.string().parse(parseApplicationJson(initial.brief_json)), prefixes = ["Creator location:", "Content language:", "Content category:", "Preferred search keywords:", "Search only these entries:", "Search all supported entries;"];
      const lines: string[] = []; for (const line of brief.split(/\r?\n/u)) { if (prefixes.some(prefix => line.startsWith(prefix))) break; lines.push(line); }
      const target = [...lines.join(" ").trim()].filter(character => !/\p{Cc}/u.test(character)).slice(0, 120).join(""), product = discoveryProductName(brief);
      if (!target || !product) throw new Error("discovery_target_name_missing");
      const current = z.object({ criteria_json: z.string(), target_count: z.number().int().nonnegative().max(4294967295) }).parse(this.#db.prepare("SELECT r.criteria_json,c.target_count FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.id=?").get(campaign));
      const criteria = z.record(z.string(), z.unknown()).parse(parseApplicationJson(current.criteria_json)), selected = typeof criteria.table_subject === "string" && criteria.table_subject.trim() ? criteria.table_subject : product;
      const subject = [...selected.trim()].filter(character => !/\p{Cc}/u.test(character)).slice(0, 40).join("");
      return { name: `${subject}${current.target_count}个`, previous_auto_name: `${product}_${initial.date}_${initial.ordinal}` };
    }, true);
  }
  prepareLibrarySnapshot(campaign: string, revision: number) { return new DiscoveryLibraryRepository(this.#db).prepareSnapshot(campaign, revision); }
  prepareReviewMaterial(campaign: string, revision: number, hit: string) { return buildDiscoveryReviewMaterial(this.#transaction(() => loadDiscoveryReviewBasis(this.#db, campaign, revision, hit), true)); }
  prepareInitialMatchMaterial(campaign: string, revision: number, hit: string) {
    const basis = this.#transaction(() => loadDiscoveryReviewBasis(this.#db, campaign, revision, hit), true), evidence = z.record(z.string(), z.unknown()).parse(parseApplicationJson(Buffer.from(basis.evidence_json)));
    const reviewed = evidence.reviewed_match !== null && typeof evidence.reviewed_match === "object" && !Array.isArray(evidence.reviewed_match) ? evidence.reviewed_match as Record<string, unknown> : {};
    return { material: buildDiscoveryReviewMaterial(basis), count: basis.native_followers ?? assessDiscoveryYoutube(parseApplicationJson(Buffer.from(basis.criteria_json)), evidence, basis.native_author_id).subscribers, preserve: reviewed.policy_version === "initial-match-evaluation-v1" && reviewed.outcome === "supported" };
  }
  submitReview(account: string, input: unknown) { return new DiscoveryReviewRepository(this.#db).submit(account, input); }
  getReview(account: string, campaign: string, request: string) { return new DiscoveryReviewRepository(this.#db).get(account, campaign, request); }
  #transaction<T>(body: () => T, readOnly = false): T {
    this.#db.exec(readOnly ? "BEGIN DEFERRED" : "BEGIN IMMEDIATE"); let committed = false;
    try { const result = body(); this.#db.exec("COMMIT"); committed = true; return result; }
    finally { if (!committed) this.#db.exec("ROLLBACK"); }
  }
  #scopes(id: string, revision: number, json: string): DiscoveryScope[] {
    let scopes = storedScopes.parse(parseStoredJson(json, true));
    for (const row of this.#db.prepare("SELECT scopes_json FROM discovery_query_rounds WHERE discovery_campaign_id=? AND revision=? ORDER BY round").all(id, revision)) {
      if (typeof row.scopes_json !== "string") throw new Error("query_round_scope_corrupt");
      const additions = storedScopes.parse(parseStoredJson(row.scopes_json, true));
      scopes = scopes.map(scope => ({ ...scope, keywords: [...scope.keywords] }));
      for (const addition of additions) { const scope = scopes.find(value => value.platform === addition.platform && value.profile_ref === addition.profile_ref); if (!scope) throw new Error("query_round_scope_corrupt"); scope.keywords.push(...addition.keywords); }
    }
    return scopes;
  }
  #head(id: string, revision: number) {
    const row = this.#db.prepare("SELECT c.max_calls,c.used_calls,r.scope_json FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.id=? AND c.phase='active' AND c.current_revision=? AND r.approved_at IS NOT NULL").get(id, revision);
    if (!row) throw new Error("discovery_plan_revision_changed"); return headSchema.parse(row);
  }
  #run(id: string, platform: string, profile: string) { return runSchema.parse(this.#db.prepare("SELECT status,stop_code,active_query,calls_used,updated_at FROM discovery_platform_runs WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").get(id, platform, profile)); }
  #next(id: string, scope: DiscoveryScope) {
    const completed = new Set(this.#db.prepare("SELECT search_keyword FROM discovery_search_completions WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").all(id, scope.platform, scope.profile_ref).map(row => z.string().parse(row.search_keyword)));
    return nextDiscoverySearchKeyword(scope.keywords, this.#run(id, scope.platform, scope.profile_ref), completed);
  }
  getPlan(id: string) {
    return this.#transaction(() => {
      const row = this.#db.prepare("SELECT c.id,c.phase,c.current_revision,c.target_count,c.max_calls,c.used_calls,c.created_at,c.updated_at,r.brief_json,r.opencli_version,r.criteria_json,r.scope_json,r.approved_at FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.id=?").get(id);
      if (!row) throw new Error("discovery_plan_missing");
      const value = z.object({ id: z.string(), phase: z.enum(["draft", "active", "paused", "completed", "archived"]), current_revision: z.number().int().positive(), target_count: z.number().int().positive(), max_calls: z.number().int().positive().nullable(), used_calls: z.number().int().nonnegative(), created_at: z.number().int().safe(), updated_at: z.number().int().safe(), brief_json: z.string(), opencli_version: z.string(), criteria_json: z.string(), scope_json: z.string(), approved_at: z.number().int().safe().nullable() }).parse(row);
      const runs = this.#db.prepare("SELECT platform,profile_ref,status,cursor,calls_used,stop_code,updated_at,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=? ORDER BY platform,profile_ref").all(id).map(item => z.object({ platform: z.string(), profile_ref: z.string(), status: z.string(), cursor: z.string().nullable(), calls_used: z.number().int().nonnegative(), stop_code: z.string().nullable(), updated_at: z.number().int().safe(), active_query: z.string().nullable() }).parse(item));
      return { id: value.id, phase: value.phase, revision: value.current_revision, brief: z.string().parse(parseApplicationJson(Buffer.from(value.brief_json))), opencli_version: value.opencli_version, criteria: z.record(z.string(), z.unknown()).parse(parseApplicationJson(Buffer.from(value.criteria_json))), scopes: this.#scopes(id, value.current_revision, value.scope_json), platform_runs: runs, target_count: value.target_count, max_calls: value.max_calls, used_calls: value.used_calls, approved_at: value.approved_at, created_at: value.created_at, updated_at: value.updated_at };
    }, true);
  }
  findPlan(id: string) { return this.#db.prepare("SELECT id FROM discovery_campaigns WHERE id=?").get(id) === undefined ? null : this.getPlan(id); }
  create(raw: unknown): string {
    const input = discoveryPlanInput.parse(raw); assertRegisteredDiscoveryScopes(input.scopes);
    return this.#transaction(() => {
      this.#refinementSource(input.criteria);
      const now = Math.floor(Date.now() / 1000);
      this.#db.prepare("INSERT INTO discovery_campaigns(id,phase,current_revision,target_count,max_calls,used_calls,created_at,updated_at) VALUES (?,'draft',1,?,?,0,?,?)").run(input.id, input.target_count, input.max_calls, now, now);
      this.#db.prepare("INSERT INTO discovery_campaign_revisions(discovery_campaign_id,revision,brief_json,opencli_version,criteria_json,scope_json,approved_at) VALUES (?,1,?,?,?,?,NULL)").run(input.id, stringifyApplicationJson(input.brief), input.opencli_version, stringifyApplicationJson(input.criteria), stringifyApplicationJson(input.scopes));
      this.#db.prepare("INSERT INTO discovery_recommendation_policies VALUES (?,1,'score_and_contact_v1')").run(input.id); return input.id;
    });
  }
  #refinementSource(criteria: Record<string, unknown>): string | null {
    if (criteria.refinement === undefined) return null;
    const source = z.strictObject({ campaign_id: z.string().min(1).max(128), expected_revision: z.number().int().positive() }).parse(criteria.refinement);
    const current = this.#db.prepare("SELECT EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.id=? AND c.current_revision=? AND r.approved_at IS NOT NULL) AS valid").get(source.campaign_id, source.expected_revision);
    if (current?.valid !== 1) throw new Error("discovery_refinement_source_changed");
    return source.campaign_id;
  }
  #seedRefinement(campaign: string, source: string, scope: DiscoveryScope): void {
    const rows = this.#db.prepare("SELECT h.search_keyword,h.source_url,h.title,h.author_label,h.captured_at,a.author_id,a.author_url,a.observed_at FROM discovery_search_hits h LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id WHERE h.discovery_campaign_id=? AND h.platform=? AND h.profile_ref=? AND EXISTS(SELECT 1 FROM json_each(?) keyword WHERE keyword.value=h.search_keyword) ORDER BY h.captured_at,h.id").all(source, scope.platform, scope.profile_ref, stringifyApplicationJson(scope.keywords));
    for (const row of rows) {
      const id = randomUUID();
      this.#db.prepare("INSERT INTO discovery_search_hits(id,discovery_campaign_id,platform,profile_ref,search_keyword,source_url,title,author_label,captured_at) VALUES(?,?,?,?,?,?,?,?,?)").run(id, campaign, scope.platform, scope.profile_ref, row.search_keyword!, row.source_url!, row.title!, row.author_label!, row.captured_at!);
      this.#db.prepare("INSERT INTO discovery_search_positions(search_hit_id) VALUES (?)").run(id);
      if (row.author_id !== null && row.author_url !== null && row.observed_at !== null) this.#db.prepare("INSERT INTO discovery_search_authors(search_hit_id,author_id,author_url,observed_at) VALUES(?,?,?,?)").run(id, row.author_id!, row.author_url!, row.observed_at!);
    }
  }
  approve(id: string, revision: number): void {
    z.number().int().positive().parse(revision);
    this.#transaction(() => {
      const now = Math.floor(Date.now() / 1000);
      const savedCriteria = this.#db.prepare("SELECT criteria_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=? AND revision=?").get(id, revision);
      if (typeof savedCriteria?.criteria_json !== "string") throw new Error("discovery_plan_revision_changed");
      const refinement = this.#refinementSource(z.record(z.string(), z.unknown()).parse(parseApplicationJson(savedCriteria.criteria_json)));
      if (this.#db.prepare("UPDATE discovery_campaigns SET phase='active',updated_at=? WHERE id=? AND phase='draft' AND current_revision=?").run(now, id, revision).changes !== 1) throw new Error("discovery_plan_revision_changed");
      const row = this.#db.prepare("SELECT scope_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=? AND revision=? AND approved_at IS NULL").get(id, revision);
      if (typeof row?.scope_json !== "string") throw new Error("discovery_plan_revision_changed");
      const scopes = z.array(discoveryScope).parse(parseStoredJson(row.scope_json, true)); assertRegisteredDiscoveryScopes(scopes);
      if (this.#db.prepare("UPDATE discovery_campaign_revisions SET approved_at=? WHERE discovery_campaign_id=? AND revision=? AND approved_at IS NULL").run(now, id, revision).changes !== 1) throw new Error("discovery_plan_revision_changed");
      for (const scope of scopes) {
        this.#db.prepare("INSERT INTO discovery_platform_runs(discovery_campaign_id,platform,profile_ref,status,cursor,calls_used,stop_code,updated_at) VALUES (?,?,?,'unsearched',NULL,0,NULL,?)").run(id, scope.platform, scope.profile_ref, now);
        if (refinement !== null) this.#seedRefinement(id, refinement, scope);
      }
    });
  }
  claimSearch(id: string, revision: number, platform: string, profile: string, explicitRecovery = false, recovery: DiscoverySearchRecovery | null = null): DiscoverySearchClaim {
    return this.#transaction(() => {
      const head = this.#head(id, revision); if (head.max_calls !== null && head.used_calls >= head.max_calls) throw new Error("discovery_budget_exhausted");
      const scope = this.#scopes(id, revision, head.scope_json).find(value => value.platform === platform && value.profile_ref === profile); if (!scope) throw new Error("discovery_search_outside_scope"); assertRegisteredDiscoveryScopes([scope]);
      const run = this.#run(id, platform, profile), interrupted = run.status === "technical_stop" && run.stop_code === "interrupted_read";
      if (recovery !== null && (run.status !== "technical_stop" || !repairedSearchRecoveryCodes.has(recovery.expected_stop_code) || run.stop_code !== recovery.expected_stop_code || run.updated_at !== recovery.expected_stopped_at)) throw new Error("discovery_repair_checkpoint_changed");
      if (interrupted && !explicitRecovery) throw new Error("discovery_explicit_recovery_required");
      if (!["unsearched", "completed"].includes(run.status) && !interrupted && recovery === null && !(run.status === "technical_stop" && canAdvanceSearchAfterStop(run.stop_code))) throw new Error("discovery_platform_stopped_or_busy");
      let query = interrupted ? run.active_query : this.#next(id, scope);
      if (recovery !== null && run.active_query !== null) {
        const index = scope.keywords.indexOf(run.active_query);
        if (index < 0) throw new Error("discovery_repair_checkpoint_changed");
        const completed = new Set(this.#db.prepare("SELECT search_keyword FROM discovery_search_completions WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").all(id, platform, profile).map(row => z.string().parse(row.search_keyword)));
        query = scope.keywords.slice(index + 1).find(keyword => !completed.has(keyword)) ?? null;
      }
      if (query === null || !scope.keywords.includes(query)) throw new Error("discovery_no_remaining_keyword");
      if (run.calls_used >= 4294967295 || head.used_calls >= 4294967295) throw new Error("discovery_call_counter_overflow");
      const callNumber = run.calls_used + 1, now = Math.floor(Date.now() / 1000);
      if (this.#db.prepare("UPDATE discovery_platform_runs SET status='searching',calls_used=?,stop_code=NULL,active_query=?,updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND calls_used=?").run(callNumber, query, now, id, platform, profile, run.calls_used).changes !== 1) throw new Error("discovery_claim_changed");
      if (this.#db.prepare("UPDATE discovery_campaigns SET used_calls=used_calls+1,updated_at=? WHERE id=?").run(now, id).changes !== 1) throw new Error("discovery_plan_missing");
      return { planId: id, expectedRevision: revision, platform, profileRef: profile, callNumber, query };
    });
  }
  finishSearch(claim: DiscoverySearchClaim, status: "completed" | "technical_stop" | "risk_control_stop" | "unauthorized", stopCode: string | null, hits: readonly CreatorSearchHit[]): void {
    if (status === "completed" ? stopCode !== null : stopCode === null || !stopCode.length || Buffer.byteLength(stopCode) > 128) throw new Error("discovery_completion_invalid");
    if (hits.length > 50 || status !== "completed" && hits.length) throw new Error("discovery_completion_invalid");
    const seen = new Set<string>();
    for (const hit of hits) {
      const url = URL.parse(hit.source_url);
      if (!url || url.protocol !== "https:" || Buffer.byteLength(hit.source_url) > 2048 || !hit.title.trim() || [...hit.title].length > 500 || [...hit.author_label].length > 200 || /\p{Cc}/u.test(hit.title + hit.author_label) || seen.has(hit.source_url) || (hit.author_id === null) !== (hit.author_url === null)) throw new Error("discovery_search_hit_invalid");
      seen.add(hit.source_url);
    }
    this.#transaction(() => {
      const now = Math.floor(Date.now() / 1000), preserve = status === "technical_stop" && (canAdvanceSearchAfterStop(stopCode) || stopCode !== null && repairedSearchRecoveryCodes.has(stopCode));
      const changed = this.#db.prepare("UPDATE discovery_platform_runs SET status=?5,stop_code=?6,active_query=CASE WHEN ?10 THEN ?9 ELSE NULL END,updated_at=?7 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND calls_used=?4 AND status='searching' AND active_query=?9 AND EXISTS(SELECT 1 FROM discovery_campaigns c WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?8)").run(claim.planId, claim.platform, claim.profileRef, claim.callNumber, status, stopCode, now, claim.expectedRevision, claim.query, Number(preserve));
      if (changed.changes !== 1) throw new Error("discovery_search_claim_stale");
      if (status === "completed") this.#db.prepare("INSERT INTO discovery_search_completions VALUES (?,?,?,?,?)").run(claim.planId, claim.platform, claim.profileRef, claim.query, now);
      for (const hit of hits) {
        const id = randomUUID();
        const inserted = this.#db.prepare("INSERT INTO discovery_search_hits(id,discovery_campaign_id,platform,profile_ref,search_keyword,source_url,title,author_label,captured_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(discovery_campaign_id,platform,profile_ref,source_url) DO NOTHING").run(id, claim.planId, claim.platform, claim.profileRef, claim.query, hit.source_url, hit.title, hit.author_label, now);
        if (inserted.changes === 1) this.#db.prepare("INSERT INTO discovery_search_positions(search_hit_id) VALUES (?)").run(id);
        if (hit.author_id !== null && hit.author_url !== null) {
          const saved = this.#db.prepare("SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND source_url=?").get(claim.planId, claim.platform, claim.profileRef, hit.source_url); if (typeof saved?.id !== "string") throw new Error("discovery_search_hit_missing");
          const prior = this.#db.prepare("SELECT author_id,author_url FROM discovery_search_authors WHERE search_hit_id=?").get(saved.id);
          if (prior && (prior.author_id !== hit.author_id || prior.author_url !== hit.author_url)) throw new Error("discovery_search_author_conflict");
          if (!prior) this.#db.prepare("INSERT INTO discovery_search_authors VALUES (?,?,?,?)").run(saved.id, hit.author_id, hit.author_url, now);
        }
      }
      this.#db.prepare("UPDATE discovery_campaigns SET updated_at=? WHERE id=?").run(now, claim.planId);
    });
  }
  appendQueries(accountId: string, id: string, revision: number, expected: readonly DiscoveryScope[], additions: readonly DiscoveryScope[]): void {
    if (!accountId.trim()) throw new Error("query_round_invalid"); appendDiscoveryQueries(expected, additions);
    this.#transaction(() => {
      const row = this.#db.prepare("SELECT r.scope_json,c.used_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.id=? AND c.current_revision=? AND c.phase='active' AND r.approved_at IS NOT NULL AND json_extract(r.criteria_json,'$.automatic_query_expansion')=1 AND (c.max_calls IS NULL OR c.used_calls<c.max_calls) AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs p WHERE p.discovery_campaign_id=c.id AND p.status='searching')").get(id, revision);
      if (!row || typeof row.scope_json !== "string" || typeof row.used_calls !== "number") throw new Error("query_round_plan_changed");
      if (stringifyApplicationJson(this.#scopes(id, revision, row.scope_json)) !== stringifyApplicationJson(expected)) throw new Error("query_round_plan_changed");
      for (const addition of additions) {
        const prior = expected.find(scope => scope.platform === addition.platform && scope.profile_ref === addition.profile_ref); if (!prior) throw new Error("query_round_outside_scope");
        if (this.#next(id, prior) !== null) throw new Error("query_round_not_exhausted");
        if (this.#run(id, addition.platform, addition.profile_ref).status !== "completed") throw new Error("query_round_platform_stopped");
      }
      this.#db.prepare("INSERT INTO discovery_query_rounds(discovery_campaign_id,revision,round,operator_account_id,scopes_json,used_calls,created_at) SELECT ?1,?2,COALESCE(MAX(round),0)+1,?3,?4,?5,?6 FROM discovery_query_rounds WHERE discovery_campaign_id=?1 AND revision=?2").run(id, revision, accountId, stringifyApplicationJson(additions), row.used_calls, Math.floor(Date.now() / 1000));
    });
  }
  increaseBudget(accountId: string, input: Readonly<{ decisionId: string; planId: string; expectedRevision: number; expectedMaxCalls: number; newMaxCalls: number }>): void {
    if (!accountId.trim() || Buffer.byteLength(accountId) > 256 || !input.planId.length || Buffer.byteLength(input.planId) > 128 || !input.decisionId.trim() || Buffer.byteLength(input.decisionId) > 128 || !Number.isInteger(input.expectedRevision) || input.expectedRevision < 1 || !Number.isInteger(input.expectedMaxCalls) || input.expectedMaxCalls < 1 || !Number.isInteger(input.newMaxCalls) || input.newMaxCalls <= input.expectedMaxCalls || input.newMaxCalls > 10000) throw new Error("budget_increase_input_invalid");
    this.#transaction(() => {
      const prior = this.#db.prepare("SELECT operator_account_id,discovery_campaign_id,criteria_revision,previous_max_calls,new_max_calls FROM discovery_budget_increases WHERE id=?").get(input.decisionId);
      if (prior) {
        if (prior.operator_account_id !== accountId || prior.discovery_campaign_id !== input.planId || prior.criteria_revision !== input.expectedRevision || prior.previous_max_calls !== input.expectedMaxCalls || prior.new_max_calls !== input.newMaxCalls) throw new Error("budget_increase_idempotency_conflict");
        return;
      }
      const head = this.#head(input.planId, input.expectedRevision);
      if (head.max_calls === null) throw new Error("budget_increase_requires_finite_approval");
      const active = this.#db.prepare("SELECT COUNT(*) AS n FROM discovery_platform_runs WHERE discovery_campaign_id=? AND status='searching'").get(input.planId);
      if (head.max_calls !== input.expectedMaxCalls || head.used_calls > head.max_calls || active?.n !== 0) throw new Error("budget_increase_plan_changed");
      const now = Math.floor(Date.now() / 1000);
      this.#db.prepare("INSERT INTO discovery_budget_increases(id,operator_account_id,discovery_campaign_id,criteria_revision,previous_max_calls,new_max_calls,used_calls_at_decision,created_at) VALUES (?,?,?,?,?,?,?,?)").run(input.decisionId, accountId, input.planId, input.expectedRevision, head.max_calls, input.newMaxCalls, head.used_calls, now);
      if (this.#db.prepare("UPDATE discovery_campaigns SET max_calls=?,updated_at=? WHERE id=?").run(input.newMaxCalls, now, input.planId).changes !== 1) throw new Error("discovery_plan_missing");
    });
  }
  adjustTarget(accountId: string, input: Readonly<{ decisionId: string; planId: string; expectedRevision: number; expectedTargetCount: number; newTargetCount: number }>): void {
    if (!accountId.trim() || Buffer.byteLength(accountId) > 256 || !input.planId.length || Buffer.byteLength(input.planId) > 128 || !input.decisionId.trim() || Buffer.byteLength(input.decisionId) > 128 || !Number.isInteger(input.expectedRevision) || input.expectedRevision < 1 || !Number.isInteger(input.expectedTargetCount) || input.expectedTargetCount < 1 || !Number.isInteger(input.newTargetCount) || input.newTargetCount < 1 || input.newTargetCount === input.expectedTargetCount || input.newTargetCount > 10000) throw new Error("target_increase_input_invalid");
    this.#transaction(() => {
      const prior = this.#db.prepare("SELECT operator_account_id,discovery_campaign_id,criteria_revision,previous_target_count,new_target_count FROM discovery_target_increases WHERE id=?").get(input.decisionId);
      if (prior) {
        if (prior.operator_account_id !== accountId || prior.discovery_campaign_id !== input.planId || prior.criteria_revision !== input.expectedRevision || prior.previous_target_count !== input.expectedTargetCount || prior.new_target_count !== input.newTargetCount) throw new Error("target_increase_idempotency_conflict"); return;
      }
      const row = this.#db.prepare("SELECT c.phase,c.current_revision,c.target_count,c.used_calls,(SELECT COUNT(*) FROM discovery_platform_runs r WHERE r.discovery_campaign_id=c.id AND r.status='searching') AS active_reads FROM discovery_campaigns c WHERE c.id=?").get(input.planId);
      if (!row) throw new Error("target_increase_plan_unavailable");
      if (row.phase !== "active" || row.current_revision !== input.expectedRevision || row.target_count !== input.expectedTargetCount || row.active_reads !== 0 || typeof row.used_calls !== "number") throw new Error("target_increase_plan_changed");
      const now = Math.floor(Date.now() / 1000);
      this.#db.prepare("INSERT INTO discovery_target_increases(id,operator_account_id,discovery_campaign_id,criteria_revision,previous_target_count,new_target_count,used_calls_at_decision,created_at) VALUES (?,?,?,?,?,?,?,?)").run(input.decisionId, accountId, input.planId, input.expectedRevision, input.expectedTargetCount, input.newTargetCount, row.used_calls, now);
      if (this.#db.prepare("UPDATE discovery_campaigns SET target_count=?,updated_at=? WHERE id=?").run(input.newTargetCount, now, input.planId).changes !== 1) throw new Error("discovery_plan_missing");
    });
  }
  recoverInterruptedSearches(): number {
    return this.#transaction(() => {
      let recovered = 0;
      const rows = this.#db.prepare("SELECT c.id,c.current_revision,r.scope_json FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.phase='active' AND r.approved_at IS NOT NULL").all();
      for (const row of rows) {
        if (typeof row.id !== "string" || typeof row.current_revision !== "number" || typeof row.scope_json !== "string") throw new Error("discovery_plan_corrupt");
        for (const scope of this.#scopes(row.id, row.current_revision, row.scope_json)) {
          const run = this.#run(row.id, scope.platform, scope.profile_ref);
          if (run.status === "searching" && run.active_query !== null && scope.keywords.includes(run.active_query)) recovered += Number(this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_read',updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND status='searching' AND active_query=?").run(Math.floor(Date.now() / 1000), row.id, scope.platform, scope.profile_ref, run.active_query).changes);
        }
      }
      return recovered;
    });
  }
  interruptSearch(claim: DiscoverySearchClaim): void {
    this.#transaction(() => {
      const changed = this.#db.prepare("UPDATE discovery_platform_runs SET status='technical_stop',stop_code='interrupted_read',updated_at=? WHERE discovery_campaign_id=? AND platform=? AND profile_ref=? AND calls_used=? AND status='searching' AND active_query=?").run(Math.floor(Date.now() / 1000), claim.planId, claim.platform, claim.profileRef, claim.callNumber, claim.query);
      if (changed.changes !== 1) throw new Error("discovery_search_claim_stale");
    });
  }
}
