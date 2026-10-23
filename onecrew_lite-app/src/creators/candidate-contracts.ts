import { z } from "zod";
import { isLosslessNumber, stringify } from "lossless-json";
import { channel, creatorU64, integer, locatorKind, platform, sourceKind, tags, text, trim } from "./contracts.ts";

export const candidateU64 = creatorU64;
export function candidateJson(value: unknown): string { const encoded = stringify(value); if (encoded === undefined) throw new Error("Creator candidate value cannot be encoded"); return encoded; }
export function candidateValueJson(value: unknown): string { const encoded = stringify(value, (_key, item: unknown) => item !== null && typeof item === "object" && !Array.isArray(item) && !isLosslessNumber(item) ? Object.fromEntries(Object.entries(item).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)) : item); if (encoded === undefined) throw new Error("Creator candidate JSON value cannot be encoded"); return encoded; }
export const candidateIntent = z.enum(["add_new", "save_or_update"]);
export const candidateStatus = z.enum(["eligible_new", "eligible_update", "existing_skipped", "duplicate_skipped", "identity_pending", "contact_conflict", "failed"]);
export const candidateReason = z.enum(["existing_creator", "duplicate_selection", "identity_ambiguous", "conflicting_update", "required_contact_route_conflict", "preparation_failed"]);
export const candidateTargetKind = z.enum(["user", "bot", "channel", "supergroup", "group", "creator", "business"]);
export const candidateAccount = z.object({ platform, external_id: text.nullish().transform(v => v ?? null), target_kind: candidateTargetKind.nullish().transform(v => v ?? null), handle: text.nullish().transform(v => v ?? null), display_name: text, profile_url: text.nullish().transform(v => v ?? null) }).strict();
export const candidateProfile = z.object({ avatar_url: text.nullish().transform(v => v ?? null), description: text.nullish().transform(v => v ?? null), location: text.nullish().transform(v => v ?? null), website_urls: z.array(text).default([]), follower_count: candidateU64.nullish().transform(v => v ?? null) }).strict();
export const candidateQuality = z.object({ score: integer.max(255).nullish().transform(v => v ?? null), assessment: text.nullish().transform(v => v ?? null), follower_counts: z.partialRecord(platform, candidateU64).default({}) });
export const candidateCommunity = z.object({ external_id: text, title: text, handle: text.nullish().transform(v => v ?? null), target_kind: z.enum(["channel", "supergroup"]) }).strict();
export const candidateAdministratorSource = z.discriminatedUnion("kind", [z.object({ kind: z.literal("operator_account"), operator_account_ref_id: text }).strict(), z.object({ kind: z.literal("creator_contact_route"), source_creator_id: text, source_contact_route_id: text.nullish().transform(v => v ?? null), operator_account_ref_id: text, source_resolved_target_id: text }).strict()]);
export const candidateOrigin = z.discriminatedUnion("kind", [z.object({ kind: z.literal("platform_search"), discovery_id: text, account_ref_id: text, query: text, search_kind: text }).strict(), z.object({ kind: z.literal("community_administrator"), discovery_id: text, role: z.enum(["owner", "administrator"]), community: candidateCommunity, source: candidateAdministratorSource }).strict()]);
export const candidateProvenance = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("discovery"), candidate_id: text, origin: candidateOrigin, selection_context: z.object({ operation_id: text, source_ref: text, source_url: text.nullish().transform(v => v ?? null), description: text.nullish().transform(v => v ?? null) }).strict().nullish().transform(v => v ?? null) }).strict(),
  z.object({ kind: z.literal("direct_selection"), source_ref: text, source_url: text.nullish().transform(v => v ?? null), description: text.nullish().transform(v => v ?? null), operator_account_ref_id: text.nullish().transform(v => v ?? null) }).strict(),
  z.object({ kind: z.literal("provider_read"), source_ref: text, operation_id: text, source_url: text.nullish().transform(v => v ?? null), description: text.nullish().transform(v => v ?? null), operator_account_ref_id: text.nullish().transform(v => v ?? null) }).strict(),
  z.object({ kind: z.literal("administrator_selection"), source_ref: text, role: z.enum(["owner", "administrator"]), community: candidateCommunity, source: candidateAdministratorSource }).strict(),
]);
export const candidatePublicLink = z.object({ display_text: text, target_url: text }).strict();
export const candidateEvidence = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("observation"), channel, locator_kind: locatorKind, value: text, source: sourceKind, source_url: text.nullish().transform(v => v ?? null) }).strict(),
  z.object({ kind: z.literal("profile_field"), source: sourceKind, source_url: text.nullish().transform(v => v ?? null), text }).strict(),
  z.object({ kind: z.literal("profile_link"), source: sourceKind, source_url: text.nullish().transform(v => v ?? null), display_text: text, target_url: text }).strict(),
  z.object({ kind: z.literal("held_hub"), source_url: text, text, links: z.array(candidatePublicLink) }).strict(),
]);
export const canonicalCandidate = z.object({ row_id: text, account: candidateAccount, profile: candidateProfile, profile_observed_at: candidateU64, provenance: candidateProvenance, contact_evidence: z.array(candidateEvidence), notes: text.nullish().transform(v => v ?? null), tags: z.array(text).nullish().transform(v => v ?? null), quality: candidateQuality }).strict();
export type CanonicalCandidate = z.infer<typeof canonicalCandidate>;
export type CandidateAccount = z.infer<typeof candidateAccount>;
export type CandidateProvenance = z.infer<typeof candidateProvenance>;
export type CandidateEvidence = z.infer<typeof candidateEvidence>;
export type CandidateIntent = z.infer<typeof candidateIntent>;
export type CandidatePreparedStatus = z.infer<typeof candidateStatus>;
export type CandidatePreparedReason = z.infer<typeof candidateReason>;
export interface CandidateFailure { row_id: string; kind: "invalid_fields" | "invalid_source" | "identity" | "telegram_eligibility" | "contact_normalization" | "tags"; platform: CandidateAccount["platform"]; display_name: string; handle: string | null; profile_url: string | null }
export type CandidateBusinessResult<T> = { ok: true; value: T } | { ok: false; kind: CandidateFailure["kind"]; message: string };
export function normalizeCandidate(input: CanonicalCandidate): CandidateBusinessResult<CanonicalCandidate> {
  const fail = (kind: CandidateFailure["kind"], message: string): CandidateBusinessResult<CanonicalCandidate> => ({ ok: false, kind, message });
  const a = input.account;
  if (a.platform === "telegram" && (a.target_kind !== "user" || !trim(a.handle ?? "") || !trim(a.profile_url ?? ""))) return fail("telegram_eligibility", "Telegram administrator is not eligible for Creator save");
  if (!trim(a.external_id ?? "") || a.target_kind === null || a.target_kind === "bot" || BigInt(input.profile_observed_at) === 0n || BigInt(input.profile_observed_at) > 9223372036854775807n || !trim(a.display_name)) return fail("invalid_fields", "Creator save input fields are invalid");
  if (a.platform === "tiktok" && (!a.external_id?.startsWith("tto:") || a.external_id.length <= 4)) return fail("identity", "Creator save identity is invalid");
  const parsedTags = input.tags === null ? null : tags.safeParse(input.tags); if (parsedTags && !parsedTags.success) return fail("tags", parsedTags.error.issues.map(issue => issue.message).join("; "));
  if (input.notes !== null && !trim(input.notes) || input.quality.assessment !== null && !trim(input.quality.assessment) || input.quality.score !== null && input.quality.score > 100) return fail("invalid_fields", "Creator save input fields are invalid");
  const value = structuredClone(input); value.tags = parsedTags?.success ? parsedTags.data : null; if (value.quality.assessment !== null) value.quality.assessment = trim(value.quality.assessment); if (value.profile.follower_count !== null) value.quality.follower_counts[a.platform] = value.profile.follower_count;
  value.quality.follower_counts = Object.fromEntries(platform.options.filter(key => value.quality.follower_counts[key] !== undefined).map(key => [key, value.quality.follower_counts[key]!]));
  return { ok: true, value };
}
