import { createHash } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { candidateJson, type CandidateAccount, type CandidateBusinessResult, type CandidateProvenance, type CanonicalCandidate } from "./candidate-contracts.ts";
import { trim } from "./contracts.ts";

export function candidateAccountReference(provenance: CandidateProvenance): string | null {
  if (provenance.kind === "direct_selection" || provenance.kind === "provider_read") return provenance.operator_account_ref_id;
  if (provenance.kind === "administrator_selection") return provenance.source.operator_account_ref_id;
  return provenance.origin.kind === "platform_search" ? provenance.origin.account_ref_id : provenance.origin.source.operator_account_ref_id;
}
export function candidateAdministrator(provenance: CandidateProvenance) {
  if (provenance.kind === "administrator_selection") return { community: provenance.community, source: provenance.source, role: provenance.role };
  if (provenance.kind === "discovery" && provenance.origin.kind === "community_administrator") return { community: provenance.origin.community, source: provenance.origin.source, role: provenance.origin.role };
  return null;
}
export function candidateSourceUrl(provenance: CandidateProvenance): string | null {
  if (provenance.kind === "direct_selection" || provenance.kind === "provider_read") return provenance.source_url;
  if (provenance.kind === "discovery") return provenance.selection_context?.source_url ?? null;
  return null;
}
function fingerprint(account: CandidateAccount, provenance: CandidateProvenance): string {
  return createHash("sha256").update(candidateJson([account.platform, account.external_id, account.target_kind, provenance])).digest("hex");
}
export function candidateSourceRef(account: CandidateAccount, provenance: CandidateProvenance): string {
  return provenance.kind === "discovery" ? `candidate:${provenance.origin.discovery_id}:${provenance.candidate_id}` : `creator-save:v1:${fingerprint(account, provenance)}`;
}
export function candidateSourceMetadata(provenance: CandidateProvenance): unknown {
  if (provenance.kind !== "discovery") return { creator_save_v1: provenance };
  const origin = provenance.origin;
  return origin.kind === "platform_search" ? { account_ref_id: origin.account_ref_id, candidate_id: provenance.candidate_id, discovery_id: origin.discovery_id, query: origin.query, search_kind: origin.search_kind } : { candidate_id: provenance.candidate_id, community: origin.community, discovery_id: origin.discovery_id, role: origin.role, source: origin.source };
}
export function candidateContactSourceRef(account: CandidateAccount, provenance: CandidateProvenance, observation: { source: string; source_url: string | null }): string {
  return provenance.kind === "discovery" ? `contact:${provenance.origin.discovery_id}:${observation.source}:${observation.source_url ?? "inline"}:${provenance.candidate_id}` : `contact:${candidateSourceRef(account, provenance)}:${observation.source}:${observation.source_url ?? "inline"}`;
}
export function candidateDiscoverySelectionSource(account: CandidateAccount, provenance: CandidateProvenance) {
  if (provenance.kind !== "discovery" || provenance.selection_context === null) return null;
  return { kind: "initial_platform_account" as const, platform: account.platform, source_url: provenance.selection_context.source_url, external_ref: `creator-discovery-selection:v1:${fingerprint(account, provenance)}`, account_ref_id: candidateAccountReference(provenance), metadata: { creator_discovery_selection_v1: provenance } };
}
export function validateCandidateSource(db: DatabaseSync, input: CanonicalCandidate): CandidateBusinessResult<null> {
  const invalid = (): CandidateBusinessResult<null> => ({ ok: false, kind: "invalid_source", message: "Creator save source is invalid" });
  const provenance = input.provenance, account = candidateAccountReference(provenance);
  if (account !== null) { const row = db.prepare("SELECT platform FROM account_refs WHERE id = ?").get(account); if (!row || row.platform !== input.account.platform) return invalid(); }
  else if (input.account.platform !== "youtube") return invalid();
  if (provenance.kind === "discovery") {
    if (!trim(provenance.candidate_id) || !trim(provenance.origin.discovery_id) || provenance.selection_context !== null && (!trim(provenance.selection_context.operation_id) || !trim(provenance.selection_context.source_ref))) return invalid();
  } else if (!trim(provenance.source_ref) || provenance.kind === "provider_read" && !trim(provenance.operation_id)) return invalid();
  const administrator = candidateAdministrator(provenance);
  if (administrator) {
    const { community, source } = administrator;
    if (input.account.platform !== "telegram" || input.account.target_kind !== "user" || !trim(community.external_id) || !trim(community.title)) return invalid();
    if (source.kind === "creator_contact_route") {
      if (source.source_contact_route_id === null) return invalid();
      const row = db.prepare("SELECT EXISTS(SELECT 1 FROM creator_contact_routes route JOIN creators creator ON creator.id = route.creator_id JOIN locator_resolutions resolution ON resolution.locator_id = route.locator_id AND resolution.account_ref_id = ?3 AND resolution.state = 'active' JOIN resolved_targets target ON target.id = resolution.resolved_target_id AND target.account_ref_id = ?3 WHERE route.id = ?1 AND route.creator_id = ?2 AND creator.archived_at IS NULL AND target.id = ?4 AND target.platform = 'telegram' AND target.target_kind = ?5 AND target.external_target_id = ?6) AS valid").get(source.source_contact_route_id, source.source_creator_id, source.operator_account_ref_id, source.source_resolved_target_id, community.target_kind, community.external_id);
      if (row?.valid !== 1) return invalid();
    }
  }
  return { ok: true, value: null };
}
