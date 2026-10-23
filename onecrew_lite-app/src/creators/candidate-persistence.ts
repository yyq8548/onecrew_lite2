import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { candidateJson, candidateValueJson, type CanonicalCandidate } from "./candidate-contracts.ts";
import { candidateAccountReference, candidateAdministrator, candidateSourceMetadata, candidateSourceRef, candidateDiscoverySelectionSource } from "./candidate-provenance.ts";
import { appendCandidateLocatorObservation, candidateLocatorKey, linkCandidateDependency, persistCandidateConfirmations, persistCandidateSource } from "./candidate-graph.ts";
import type { SelectedCandidateRow } from "./candidate-selection.ts";
import { creatorDetail } from "./detail.ts";
import { followers, integer, tags, text, trim } from "./contracts.ts";
import { parseStoredJson } from "../contracts/stored-json.ts";

export function insertCandidateCreator(db: DatabaseSync, input: CanonicalCandidate): string {
  const id = randomUUID(), at = Math.floor(Date.now() / 1000);
  db.prepare("INSERT INTO creators(id,display_name,notes,score,assessment,follower_counts_json,created_at,updated_at,archived_at,tags_json) VALUES(?,?,?,?,?,?,?,?,NULL,?)").run(id, trim(input.account.display_name), input.notes, input.quality.score, input.quality.assessment, candidateJson(input.quality.follower_counts), at, at, candidateJson(tags.parse(input.tags ?? [])));
  db.prepare("INSERT INTO creator_relationships(creator_id,stage,last_touch_at,created_at,updated_at) VALUES(?,'prospect',NULL,?,?)").run(id, at, at);
  return id;
}
function accountAndObservation(db: DatabaseSync, creator: string, input: CanonicalCandidate): { account: string; source: string } {
  const a = input.account, at = Math.floor(Date.now() / 1000);
  if (a.external_id === null || a.target_kind === null) throw new Error("Creator candidate has no stable identity");
  const old = db.prepare("SELECT id FROM platform_accounts WHERE platform=? AND target_kind=? AND external_id=?").get(a.platform, a.target_kind, trim(a.external_id)), account = old ? text.parse(old.id) : randomUUID();
  db.prepare("INSERT INTO platform_accounts(id,platform,target_kind,external_id,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(platform,target_kind,external_id) DO UPDATE SET updated_at=excluded.updated_at").run(account, a.platform, a.target_kind, trim(a.external_id), at, at);
  const oldLink = db.prepare("SELECT id FROM creator_account_links WHERE creator_id=? AND platform_account_id=?").get(creator, account);
  db.prepare("INSERT INTO creator_account_links(id,creator_id,platform_account_id,state,confidence,created_at,updated_at) VALUES(?,?,?,'confirmed','exact',?,?) ON CONFLICT(creator_id,platform_account_id) DO UPDATE SET state=excluded.state,confidence=excluded.confidence,updated_at=excluded.updated_at").run(oldLink ? text.parse(oldLink.id) : randomUUID(), creator, account, at, at);
  const source = persistCandidateSource(db, { kind: candidateAdministrator(input.provenance) ? "community_administrator" : "initial_platform_account", platform: a.platform, source_url: a.profile_url, external_ref: candidateSourceRef(a, input.provenance), account_ref_id: candidateAccountReference(input.provenance), metadata: candidateSourceMetadata(input.provenance) });
  const publicData = candidateValueJson(input.profile);
  const exists = db.prepare("SELECT EXISTS(SELECT 1 FROM platform_account_observations WHERE platform_account_id=?1 AND source_id=?2 AND handle IS ?3 AND display_name IS ?4 AND profile_url IS ?5 AND public_json=?6 AND EXISTS(SELECT 1 FROM platform_account_observations latest WHERE latest.id=(SELECT current.id FROM platform_account_observations current WHERE current.platform_account_id=?1 ORDER BY current.observed_at DESC,current.rowid DESC LIMIT 1) AND (latest.observed_at>?7 OR (latest.handle IS ?3 AND latest.display_name IS ?4 AND latest.profile_url IS ?5 AND latest.public_json=?6)))) AS value").get(account, source, a.handle, trim(a.display_name), a.profile_url, publicData, input.profile_observed_at);
  if (exists?.value !== 1) db.prepare("INSERT INTO platform_account_observations(id,platform_account_id,source_id,handle,display_name,profile_url,public_json,observed_at) VALUES(?,?,?,?,?,?,?,?)").run(randomUUID(), account, source, a.handle, trim(a.display_name), a.profile_url, publicData, input.profile_observed_at);
  return { account, source };
}
function appendOrigin(db: DatabaseSync, creator: string, sourceId: string, input: CanonicalCandidate): void {
  if (input.provenance.kind !== "discovery") return;
  const origin = input.provenance.origin, at = Math.floor(Date.now() / 1000);
  if (origin.kind === "platform_search") {
    db.prepare("INSERT INTO creator_origins(id,kind,creator_id,source_id,discovery_id,source_creator_id,source_contact_route_id,operator_account_ref_id,source_resolved_target_id,discovered_at) VALUES(?,'platform_search',?,?,?,NULL,NULL,?,NULL,?) ON CONFLICT DO NOTHING").run(randomUUID(), creator, sourceId, origin.discovery_id, origin.account_ref_id, at); return;
  }
  const source = origin.source;
  if (source.kind === "creator_contact_route" && source.source_contact_route_id === null) throw new Error("community administrator save requires a live source contact route");
  db.prepare("INSERT INTO creator_origins(id,kind,creator_id,source_id,discovery_id,source_creator_id,source_contact_route_id,operator_account_ref_id,source_resolved_target_id,administrator_role,community_external_id,community_title,community_handle,community_target_kind,discovered_at) VALUES(?,'community_administrator',?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").run(randomUUID(), creator, sourceId, origin.discovery_id, source.kind === "creator_contact_route" ? source.source_creator_id : null, source.kind === "creator_contact_route" ? source.source_contact_route_id : null, source.operator_account_ref_id, source.kind === "creator_contact_route" ? source.source_resolved_target_id : null, origin.role, origin.community.external_id, origin.community.title, origin.community.handle, origin.community.target_kind, at);
}
export function persistSelectedCandidate(db: DatabaseSync, creator: string, row: SelectedCandidateRow, batchTags: Map<string, string[]>): { creator: ReturnType<typeof creatorDetail>; outreach_context: null | { creator_id: string; contact_route_id: string; operator_account_ref_id: string } } {
  const input = row.input, at = Math.floor(Date.now() / 1000);
  if (input.tags !== null) {
    const merged = [...(batchTags.get(creator) ?? [])]; for (const tag of input.tags) if (!merged.includes(tag)) merged.push(tag);
    const parsed = tags.parse(merged); batchTags.set(creator, parsed);
    db.prepare("UPDATE creators SET tags_json=?,updated_at=? WHERE id=?").run(candidateJson(parsed), at, creator);
  }
  const { account, source } = accountAndObservation(db, creator, input);
  const quality = structuredClone(input.quality);
  if (db.prepare("SELECT EXISTS(SELECT 1 FROM platform_account_observations WHERE platform_account_id=? AND observed_at>?) AS value").get(account, input.profile_observed_at)?.value === 1) delete quality.follower_counts[input.account.platform];
  const current = db.prepare("SELECT score,assessment,follower_counts_json FROM creators WHERE id=?").get(creator); if (!current) throw new Error("Creator disappeared while saving candidate");
  if (quality.score !== null || quality.assessment !== null || Object.keys(quality.follower_counts).length) db.prepare("UPDATE creators SET score=?,assessment=?,follower_counts_json=?,updated_at=? WHERE id=?").run(quality.score ?? integer.max(100).nullable().parse(current.score), quality.assessment ?? text.nullable().parse(current.assessment), candidateJson({ ...followers.parse(parseStoredJson(text.parse(current.follower_counts_json))), ...quality.follower_counts }), at, creator);
  appendOrigin(db, creator, source, input);
  const routes = persistCandidateConfirmations(db, creator, account, input, row.confirmations);
  const selectionSource = candidateDiscoverySelectionSource(input.account, input.provenance);
  if (selectionSource) {
    const source = persistCandidateSource(db, selectionSource);
    for (const confirmation of row.confirmations) if (confirmation.preview.observations.some(observation => observation.source === "initial_platform_account")) {
      const route = routes.get(candidateLocatorKey(confirmation.preview.locator)); if (!route) throw new Error("discovery selection route was not persisted");
      const locator = db.prepare("SELECT locator_id FROM creator_contact_routes WHERE id=?").get(route); if (!locator) throw new Error("discovery selection locator is unavailable");
      for (const observation of confirmation.preview.observations) if (observation.source === "initial_platform_account") appendCandidateLocatorObservation(db, text.parse(locator.locator_id), source, creator, account, observation.value);
    }
  }
  const administrator = candidateAdministrator(input.provenance); let outreach_context = null;
  if (administrator) {
    const initial = row.confirmations.find(confirmation => confirmation.preview.observations.some(observation => observation.source === "initial_platform_account")); if (!initial) throw new Error("administrator initial contact route is missing");
    const route = routes.get(candidateLocatorKey(initial.preview.locator)), operator = candidateAccountReference(input.provenance); if (!route || !operator) throw new Error("administrator initial contact route or operator was not persisted");
    if (administrator.source.kind === "creator_contact_route") { if (!administrator.source.source_contact_route_id) throw new Error("administrator source route is unavailable"); linkCandidateDependency(db, administrator.source.source_contact_route_id, route, "community_administrator"); }
    outreach_context = { creator_id: creator, contact_route_id: route, operator_account_ref_id: operator };
  }
  return { creator: creatorDetail(db, creator), outreach_context };
}
