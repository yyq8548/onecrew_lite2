import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { candidateJson, candidateValueJson, type CanonicalCandidate } from "./candidate-contracts.ts";
import { candidateAccountReference, candidateContactSourceRef, candidateSourceMetadata } from "./candidate-provenance.ts";
import { candidateParentUrl, candidateSavedParent } from "./candidate-prepare.ts";
import type { CandidateContactConfirmation } from "./candidate-selection.ts";
import { asciiLower, text, trim } from "./contracts.ts";
import { normalizeContactResult, type ContactLocatorInput } from "./normalization.ts";

export const candidateLocatorKey = (locator: ContactLocatorInput): string => `${locator.channel}:${locator.kind}:${locator.normalized_value}`;
export interface CandidateSourceInput { kind: string; platform: string; source_url: string | null; external_ref: string; account_ref_id: string | null; metadata: unknown }
export function persistCandidateSource(db: DatabaseSync, source: CandidateSourceInput): string {
  const existing = db.prepare("SELECT id FROM observation_sources WHERE external_ref=? LIMIT 1").get(trim(source.external_ref)); if (existing) return text.parse(existing.id);
  const id = randomUUID();
  db.prepare("INSERT INTO observation_sources(id,kind,platform,source_url,external_ref,account_ref_id,metadata_json,observed_at) VALUES(?,?,?,?,?,?,?,?)").run(id, source.kind, source.platform, source.source_url, source.external_ref, source.account_ref_id, candidateValueJson(source.metadata), Math.floor(Date.now() / 1000));
  return id;
}
export function appendCandidateLocatorObservation(db: DatabaseSync, locator: string, source: string, creator: string, account: string, raw: string): void {
  const value = trim(raw); if (!value) throw new Error("locator observation value is required");
  if (db.prepare("SELECT id FROM locator_observations WHERE locator_id=? AND source_id=? AND creator_id IS ? AND platform_account_id IS ? AND observed_value=? LIMIT 1").get(locator, source, creator, account, value)) return;
  db.prepare("INSERT INTO locator_observations(id,locator_id,source_id,creator_id,platform_account_id,observed_value,observed_at) VALUES(?,?,?,?,?,?,?)").run(randomUUID(), locator, source, creator, account, value, Math.floor(Date.now() / 1000));
}
export function linkCandidateDependency(db: DatabaseSync, parent: string, child: string, kind: "link_hub" | "community_administrator"): void {
  db.prepare("INSERT INTO creator_contact_route_dependencies(parent_contact_route_id,child_contact_route_id,kind,created_at) VALUES(?,?,?,?) ON CONFLICT(parent_contact_route_id,child_contact_route_id) DO UPDATE SET kind=excluded.kind").run(parent, child, kind, Math.floor(Date.now() / 1000));
}
export function persistCandidateConfirmations(db: DatabaseSync, creator: string, account: string, input: CanonicalCandidate, confirmations: readonly CandidateContactConfirmation[]): Map<string, string> {
  const routes = new Map<string, string>(), seen = new Set<string>();
  for (const confirmation of confirmations) {
    const preview = confirmation.preview;
    if (!preview.observations.length) throw new Error("contact confirmation requires observations");
    let canonical: ContactLocatorInput | null = null;
    for (const observation of preview.observations) {
      const normalized = normalizeContactResult(observation.channel, observation.value);
      if (!normalized.ok) throw new Error(normalized.message);
      const locator = normalized.value;
      if (locator.channel !== preview.locator.channel || locator.kind !== preview.locator.kind || asciiLower(trim(locator.normalized_value)) !== asciiLower(trim(preview.locator.normalized_value))) throw new Error("contact confirmation does not match its preview");
      canonical ??= locator;
    }
    if (canonical === null) throw new Error("contact confirmation requires observations");
    const key = candidateLocatorKey(canonical); if (seen.has(key)) throw new Error("the same contact locator was confirmed more than once"); seen.add(key);
    const owners = db.prepare("SELECT DISTINCT route.creator_id FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id WHERE locator.channel=? AND locator.kind=? AND locator.normalized_value=? ORDER BY route.creator_id").all(canonical.channel, canonical.kind, canonical.normalized_value).map(row => text.parse(row.creator_id)).filter(id => id !== creator);
    const previewOwners = [...new Set(preview.collision.creator_ids)].filter(id => id !== creator).sort();
    if (candidateJson(owners) !== candidateJson(previewOwners)) throw new Error("contact route collision preview is stale; discover again");
    if (owners.length) throw new Error("contact route already belongs to another Creator");
    const oldLocator = db.prepare("SELECT id FROM contact_locators WHERE channel=? AND kind=? AND normalized_value=?").get(canonical.channel, canonical.kind, canonical.normalized_value), locatorId = oldLocator ? text.parse(oldLocator.id) : randomUUID(), at = Math.floor(Date.now() / 1000);
    db.prepare("INSERT INTO contact_locators(id,channel,kind,normalized_value,display_value,created_at,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(channel,kind,normalized_value) DO UPDATE SET display_value=excluded.display_value,updated_at=excluded.updated_at").run(locatorId, canonical.channel, canonical.kind, trim(canonical.normalized_value), trim(canonical.display_value), at, at);
    const oldRoute = db.prepare("SELECT id FROM creator_contact_routes WHERE creator_id=? AND locator_id=?").get(creator, locatorId), routeId = oldRoute ? text.parse(oldRoute.id) : randomUUID();
    db.prepare("INSERT INTO creator_contact_routes(id,creator_id,locator_id,confidence,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(creator_id,locator_id) DO UPDATE SET confidence=excluded.confidence,updated_at=excluded.updated_at").run(routeId, creator, locatorId, confirmation.confidence, at, at);
    routes.set(key, routeId);
    for (const observation of preview.observations) {
      const source = persistCandidateSource(db, { kind: observation.source, platform: input.account.platform, source_url: observation.source_url, external_ref: candidateContactSourceRef(input.account, input.provenance, observation), account_ref_id: candidateAccountReference(input.provenance), metadata: candidateSourceMetadata(input.provenance) });
      appendCandidateLocatorObservation(db, locatorId, source, creator, account, observation.value);
    }
  }
  const parentRoutes = new Map<string, string>();
  for (const confirmation of confirmations) { const locator = confirmation.preview.locator; if (locator.kind === "url" && ["website", "link_hub"].includes(locator.channel)) parentRoutes.set(locator.normalized_value, routes.get(candidateLocatorKey(locator))!); }
  const edges = new Set<string>();
  for (const confirmation of confirmations) for (const observation of confirmation.preview.observations) {
    if (observation.source !== "link_hub") continue;
    const parentUrl = candidateParentUrl(observation.source_url); if (parentUrl === null) throw new Error("LinkHub-derived contact route is missing its parent URL");
    const parent = parentRoutes.get(parentUrl) ?? candidateSavedParent(db, creator, parentUrl), child = routes.get(candidateLocatorKey(confirmation.preview.locator));
    if (!parent || !child || parent === child) throw new Error("LinkHub-derived contact route requires its distinct parent LinkHub route to be saved");
    const key = `${parent}:${child}`; if (!edges.has(key)) { linkCandidateDependency(db, parent, child, "link_hub"); edges.add(key); }
  }
  return routes;
}
