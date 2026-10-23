import { randomUUID } from "node:crypto";
import { z } from "zod";
import { creatorSavePreparedFailure } from "../results/creator-save-contracts.ts";
import { canonicalCandidate, type CandidateProvenance, type CanonicalCandidate } from "./candidate-contracts.ts";
import { parseCreatorSaveCandidates, type CreatorSaveCandidateInput, type CreatorSaveProfileResolver, type ReadCreatorProfile } from "./candidate-input.ts";
import { trim } from "./contracts.ts";
export type CreatorSavePreparedFailure = z.infer<typeof creatorSavePreparedFailure>;

export function boundedCandidateText(value: string | null): string | null {
  if (value === null || !trim(value)) return null;
  const text = trim(value); if (Buffer.byteLength(text) <= 8192) return text;
  const marker = "… (truncated)", limit = 8192 - Buffer.byteLength(marker); let result = "";
  for (const character of text) { if (Buffer.byteLength(result + character) > limit) break; result += character; }
  return result + marker;
}
export function candidateLabel(candidate: CreatorSaveCandidateInput): string {
  const target = candidate.target;
  return target.platform === "x" ? `@${target.handle} on X` : target.platform === "instagram" ? `@${target.handle} on Instagram` : target.platform === "youtube" ? `YouTube channel ${target.channel_id}` : `Telegram user ${target.user_id}`;
}
export class CandidateWarnings {
  readonly #groups = new Map<string, string[]>();
  add(message: string, subject: string | null): void { const subjects = this.#groups.get(message) ?? []; if (subject !== null && !subjects.includes(subject)) subjects.push(subject); this.#groups.set(message, subjects); }
  render(): string[] { return [...this.#groups].map(([message, subjects]) => subjects.length ? `${message} Affected: ${subjects.slice(0, 10).join(", ")}${subjects.length > 10 ? ` and ${subjects.length - 10} more` : ""}.` : message); }
}
function provenance(candidate: CreatorSaveCandidateInput, read: ReadCreatorProfile): CandidateProvenance | null {
  const account = candidate.connection.platform === "youtube" ? candidate.connection.service_credential_ref_id : candidate.connection.account_ref_id, source = candidate.provenance;
  if (source.kind === "direct_selection") return { kind: "direct_selection", source_ref: source.source_url, source_url: source.source_url, description: source.description, operator_account_ref_id: account };
  if (source.kind === "administrator_selection") {
    const administrator = read.administrator; if (!administrator) return null;
    return { kind: "administrator_selection", source_ref: `telegram:${source.community_kind}:${source.community_id}`, role: administrator.role, community: administrator.community, source: administrator.source };
  }
  const origin = source.origin;
  return origin.kind === "provider_read" ? { kind: "provider_read", operation_id: origin.operation_id, source_ref: origin.source_ref, source_url: source.source_url, description: source.description, operator_account_ref_id: account } : { kind: "discovery", candidate_id: origin.candidate_id, origin: { kind: "platform_search", discovery_id: origin.discovery_id, account_ref_id: account, query: origin.query, search_kind: origin.search_kind }, selection_context: { operation_id: origin.operation_id, source_ref: origin.source_ref, source_url: source.source_url, description: source.description } };
}
export async function materializeCreatorCandidates(raw: unknown, resolver: CreatorSaveProfileResolver): Promise<{ canonical: CanonicalCandidate[]; failures: CreatorSavePreparedFailure[]; labels: Map<string, string>; warnings: CandidateWarnings; intent: CreatorSaveCandidateInput["intent"] }> {
  const candidates = parseCreatorSaveCandidates(raw), reads = await resolver.resolveMany(candidates);
  if (reads.length !== candidates.length || reads.some((read, index) => read.candidate !== candidates[index])) throw new Error("Creator profile resolver returned a mismatched candidate sequence");
  const canonical: CanonicalCandidate[] = [], failures: CreatorSavePreparedFailure[] = [], labels = new Map<string, string>(), warnings = new CandidateWarnings();
  for (const { candidate, result } of reads) {
    const row_id = `row-${randomUUID().replaceAll("-", "")}`, target = candidate.target, platform = target.platform === "telegram_administrator" ? "telegram" : target.platform;
    const failure = (display: string) => { failures.push(creatorSavePreparedFailure.parse({ row_id, display_name: boundedCandidateText(display), platform, handle: target.platform === "x" || target.platform === "instagram" ? target.handle : null, profile_url: null, reason: "preparation_failed" })); warnings.add("These selections are not in this review because their profile could not be read again just now.", boundedCandidateText(display)); };
    if (!result.ok) { failure(candidateLabel(candidate)); continue; }
    const read = result.value, name = boundedCandidateText(read.account.display_name) ?? candidateLabel(candidate);
    for (const warning of read.warnings) warnings.add(warning, name);
    const source = provenance(candidate, read); if (source === null) { failure(name); continue; }
    const quality = { ...candidate.quality, follower_counts: read.profile.follower_count === null ? {} : { [read.account.platform]: read.profile.follower_count } };
    canonical.push(canonicalCandidate.parse({ row_id, account: read.account, profile: read.profile, profile_observed_at: read.observed_at, provenance: source, contact_evidence: read.evidence, notes: candidate.notes, tags: candidate.tags, quality })); labels.set(row_id, name);
  }
  return { canonical, failures, labels, warnings, intent: candidates[0]!.intent };
}
