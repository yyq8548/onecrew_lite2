import { z } from "zod";
import { candidateAccount, candidateAdministratorSource, candidateCommunity, candidateEvidence, candidateIntent, candidateProfile, candidateQuality } from "./candidate-contracts.ts";
import { tags, text, trim } from "./contracts.ts";

const optionalText = text.nullish().transform(value => value ?? null);
export const creatorSaveCommunityLocator = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("public_username"), username: text }).strict(),
  z.object({ kind: z.literal("stable_community"), external_id: text, community_kind: z.enum(["channel", "supergroup"]) }).strict(),
  z.object({ kind: z.literal("saved_community_route"), creator_id: text, route_id: text }).strict(),
]);
export const creatorSaveTarget = z.discriminatedUnion("platform", [
  z.object({ platform: z.literal("x"), user_id: text, handle: text }).strict(),
  z.object({ platform: z.literal("instagram"), user_id: text, handle: text }).strict(),
  z.object({ platform: z.literal("youtube"), channel_id: text }).strict(),
  z.object({ platform: z.literal("telegram_administrator"), user_id: text, community: creatorSaveCommunityLocator }).strict(),
]);
export const creatorSaveConnection = z.discriminatedUnion("platform", [
  z.object({ platform: z.literal("x"), account_ref_id: z.uuid() }).strict(),
  z.object({ platform: z.literal("instagram"), account_ref_id: z.uuid() }).strict(),
  z.object({ platform: z.literal("telegram"), account_ref_id: z.uuid() }).strict(),
  z.object({ platform: z.literal("youtube"), service_credential_ref_id: z.uuid() }).strict(),
]);
const searchOrigin = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("discovery"), discovery_id: text, candidate_id: text, query: text, search_kind: text, operation_id: text, source_ref: text }).strict(),
  z.object({ kind: z.literal("provider_read"), operation_id: text, source_ref: text }).strict(),
]);
const administratorSelectionSource = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("operator_community") }).strict(),
  z.object({ kind: z.literal("saved_community_route"), creator_id: text, route_id: text }).strict(),
]);
export const creatorSaveProvenanceInput = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("search_selection"), origin: searchOrigin, source_url: optionalText, description: optionalText }).strict(),
  z.object({ kind: z.literal("direct_selection"), source_url: text, description: optionalText }).strict(),
  z.object({ kind: z.literal("administrator_selection"), community_id: text, community_kind: z.enum(["channel", "supergroup"]), source: administratorSelectionSource }).strict(),
]);
export const creatorSaveCandidateInput = z.object({ intent: candidateIntent, target: creatorSaveTarget, connection: creatorSaveConnection, provenance: creatorSaveProvenanceInput, notes: optionalText, tags: z.array(text).nullish().transform(value => value ?? null), quality: candidateQuality }).strict();
export type CreatorSaveCandidateInput = z.infer<typeof creatorSaveCandidateInput>;
export interface ReadCreatorProfile {
  observed_at: number | bigint;
  account: z.infer<typeof candidateAccount>;
  profile: z.infer<typeof candidateProfile>;
  evidence: z.infer<typeof candidateEvidence>[];
  warnings: string[];
  administrator: null | { role: "owner" | "administrator"; community: z.infer<typeof candidateCommunity>; source: z.infer<typeof candidateAdministratorSource> };
}
export interface CreatorSaveReadFailure { kind: "provider_read" | "identity_mismatch" | "source_mismatch"; message: string }
export interface CreatorSaveProfileRead {
  candidate: CreatorSaveCandidateInput;
  result: { ok: true; value: ReadCreatorProfile } | { ok: false; error: CreatorSaveReadFailure };
}
export interface CreatorSaveProfileResolver {
  resolveMany(candidates: readonly CreatorSaveCandidateInput[]): Promise<readonly CreatorSaveProfileRead[]>;
}

export function validateCreatorSaveCandidate(input: CreatorSaveCandidateInput): void {
  if (input.tags !== null) tags.parse(input.tags);
  const { target, connection, provenance } = input;
  const expected = target.platform === "telegram_administrator" ? "telegram" : target.platform;
  const reference = connection.platform === "youtube" ? connection.service_credential_ref_id : connection.account_ref_id;
  if (connection.platform !== expected || reference === "00000000-0000-0000-0000-000000000000") throw new Error("Creator save connection does not match its platform");
  const positive = (value: string) => /^[1-9][0-9]{0,31}$/.test(value);
  const handle = (value: string, max: number, dot = false) => value.length > 0 && value.length <= max && (dot ? /^[A-Za-z0-9_.]+$/ : /^[A-Za-z0-9_]+$/).test(value);
  const values: string[] = [];
  if (target.platform === "x" || target.platform === "instagram") {
    if (!positive(target.user_id) || !handle(target.handle, target.platform === "x" ? 15 : 30, target.platform === "instagram")) throw new Error("Creator save target is invalid");
    values.push(target.user_id, target.handle);
  } else if (target.platform === "youtube") {
    if (!/^UC[A-Za-z0-9_-]{22}$/.test(target.channel_id)) throw new Error("Creator save target is invalid");
    values.push(target.channel_id);
  } else {
    if (!positive(target.user_id)) throw new Error("Creator save target is invalid");
    values.push(target.user_id);
    const community = target.community;
    if (community.kind === "public_username") { if (!handle(community.username, 32)) throw new Error("Creator save target is invalid"); values.push(community.username); }
    else if (community.kind === "stable_community") { if (!positive(community.external_id)) throw new Error("Creator save target is invalid"); values.push(community.external_id); }
    else values.push(community.creator_id, community.route_id);
  }
  const invalidSource = () => { throw new Error("Creator save source is invalid or does not match its target"); };
  const sourceUrl = (value: string) => { const url = URL.parse(value); if (!url || !["https:", "http:"].includes(url.protocol) || !url.hostname || url.username || url.password || url.hash || value.includes("#")) invalidSource(); values.push(value); };
  if (provenance.kind === "administrator_selection") {
    if (target.platform !== "telegram_administrator" || !positive(provenance.community_id)) invalidSource();
    if (target.platform !== "telegram_administrator") throw new Error("Creator save source is invalid");
    values.push(provenance.community_id);
    const community = target.community, source = provenance.source;
    if (community.kind === "public_username" && source.kind === "operator_community") { /* 原公开用户名在实际读取后核对社区身份。 */ }
    else if (community.kind === "stable_community" && source.kind === "operator_community" && community.external_id === provenance.community_id && community.community_kind === provenance.community_kind) { /* 已核对稳定社区标识。 */ }
    else if (community.kind === "saved_community_route" && source.kind === "saved_community_route" && community.creator_id === source.creator_id && community.route_id === source.route_id) values.push(source.creator_id, source.route_id);
    else invalidSource();
  } else {
    if (target.platform === "telegram_administrator") invalidSource();
    if (provenance.kind === "search_selection") { const origin = provenance.origin; values.push(origin.operation_id, origin.source_ref); if (origin.kind === "discovery") values.push(origin.discovery_id, origin.candidate_id, origin.query, origin.search_kind); if (provenance.source_url !== null) sourceUrl(provenance.source_url); }
    else sourceUrl(provenance.source_url);
    if (provenance.description !== null && provenance.description !== "") values.push(provenance.description);
  }
  if (values.some(value => value === "" || trim(value) !== value || /\p{Cc}/u.test(value))) invalidSource();
}

export function parseCreatorSaveCandidates(value: unknown): CreatorSaveCandidateInput[] {
  const candidates = z.array(creatorSaveCandidateInput).min(1).max(50).parse(value);
  if (new Set(candidates.map(candidate => candidate.intent)).size !== 1) throw new Error("Creator save batch cannot mix intents");
  for (const candidate of candidates) validateCreatorSaveCandidate(candidate);
  return candidates;
}
