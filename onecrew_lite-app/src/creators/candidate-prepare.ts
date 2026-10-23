import type { DatabaseSync } from "node:sqlite";
import { type CandidateBusinessResult, type CandidateFailure, type CandidateIntent, type CandidatePreparedReason, type CandidatePreparedStatus, type CanonicalCandidate, candidateJson, canonicalCandidate, normalizeCandidate } from "./candidate-contracts.ts";
import { candidateHubHosts, candidateObservations, type CandidateContactObservation } from "./candidate-extraction.ts";
import { candidateAdministrator, validateCandidateSource } from "./candidate-provenance.ts";
import { creatorActive, resolveIdentity } from "./contacts.ts";
import { text, trim } from "./contracts.ts";
import { normalizeContactResult, type ContactLocatorInput } from "./normalization.ts";

export interface CandidateRouteCollision { status: "none" | "possible" | "conflict"; creator_ids: string[] }
export interface CandidateRoutePreview { locator: ContactLocatorInput; observations: CandidateContactObservation[]; collision: CandidateRouteCollision }
export interface CandidateRouteOption { route_id: string; channel: ContactLocatorInput["channel"]; kind: ContactLocatorInput["kind"]; normalized_value: string; display_value: string; sources: CandidateContactObservation["source"][]; collision: CandidateRouteCollision; required: boolean }
export interface CandidatePreparedRow {
  input: CanonicalCandidate;
  preview: Omit<CanonicalCandidate, "contact_evidence"> & { intent: CandidateIntent; status: CandidatePreparedStatus; reason: CandidatePreparedReason | null; identity_resolution: ReturnType<typeof resolveIdentity>; route_options: CandidateRouteOption[] };
  routes: [string, CandidateRoutePreview][];
  required_parent_routes: Record<string, string[]>;
}
export function candidateSavedParent(db: DatabaseSync, creator: string, url: string): string | null {
  const row = db.prepare("SELECT route.id FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id WHERE route.creator_id=? AND locator.kind='url' AND locator.channel IN ('website','link_hub') AND locator.normalized_value=? ORDER BY route.id LIMIT 1").get(creator, url);
  return row ? text.parse(row.id) : null;
}
export function candidateParentUrl(value: string | null): string | null {
  if (value === null) return null;
  const url = URL.parse(value.startsWith("http://") || value.startsWith("https://") ? value : `https://${value}`);
  if (!url || !candidateHubHosts.has(url.hostname.replace(/^(?:www\.)+/u, ""))) return null;
  const normalized = normalizeContactResult("link_hub", value); return normalized.ok ? normalized.value.normalized_value : null;
}
function prepareRow(db: DatabaseSync, raw: CanonicalCandidate, intent: CandidateIntent): CandidateBusinessResult<CandidatePreparedRow> {
  const fail = (kind: CandidateFailure["kind"], message: string): CandidateBusinessResult<CandidatePreparedRow> => ({ ok: false, kind, message });
  const normalized = normalizeCandidate(raw); if (!normalized.ok) return normalized;
  const input = normalized.value, source = validateCandidateSource(db, input); if (!source.ok) return source;
  const identity = resolveIdentity(db, { candidate: input.account }), administrator = candidateAdministrator(input.provenance);
  if (identity.status === "exact_existing_creator") {
    if (administrator?.source.kind === "creator_contact_route" && administrator.source.source_creator_id === identity.creator_id || !creatorActive(db, identity.creator_id)) return fail("identity", "Creator save identity is invalid");
  }
  const observations = candidateObservations(input); if (!observations.ok) return observations;
  const grouped = new Map<string, CandidateRoutePreview>();
  for (const observation of observations.value) {
    const result = normalizeContactResult(observation.channel, observation.value); if (!result.ok) return fail("contact_normalization", result.message);
    const locator = result.value, key = `${locator.channel}:${locator.kind}:${locator.normalized_value}`;
    let group = grouped.get(key); if (!group) { group = { locator, observations: [], collision: { status: "none", creator_ids: [] } }; grouped.set(key, group); }
    if (!group.observations.some(current => current.source === observation.source && current.source_url === observation.source_url && current.value === observation.value)) group.observations.push(observation);
  }
  const routes: CandidatePreparedRow["routes"] = [], options: CandidateRouteOption[] = [];
  for (const key of [...grouped.keys()].sort()) {
    const route = grouped.get(key)!, locator = route.locator;
    const owners = db.prepare("SELECT DISTINCT route.creator_id FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id WHERE locator.channel=? AND locator.kind=? AND locator.normalized_value=? ORDER BY route.creator_id").all(locator.channel, locator.kind, locator.normalized_value).map(row => text.parse(row.creator_id)).filter(id => identity.status !== "exact_existing_creator" || id !== identity.creator_id);
    route.collision = { status: owners.length === 0 ? "none" : owners.length === 1 ? "possible" : "conflict", creator_ids: owners };
    const id = `route-${routes.length}`;
    options.push({ route_id: id, channel: locator.channel, kind: locator.kind, normalized_value: locator.normalized_value, display_value: locator.display_value, sources: route.observations.map(observation => observation.source), collision: route.collision, required: route.observations.some(observation => observation.source === "initial_platform_account") });
    routes.push([id, route]);
  }
  let status: CandidatePreparedStatus, reason: CandidatePreparedReason | null = null;
  if (options.some(route => route.required && route.collision.status !== "none")) { status = "contact_conflict"; reason = "required_contact_route_conflict"; }
  else if (identity.status === "new") status = "eligible_new";
  else if (identity.status === "exact_existing_creator") { status = intent === "add_new" ? "existing_skipped" : "eligible_update"; if (intent === "add_new") reason = "existing_creator"; }
  else { status = "identity_pending"; reason = "identity_ambiguous"; }
  const parents: Record<string, string[]> = {};
  for (const [id, route] of routes) for (const observation of route.observations) {
    if (observation.source !== "link_hub") continue;
    const parentUrl = candidateParentUrl(observation.source_url); if (parentUrl === null) return fail("contact_normalization", "Link-hub observation has no valid parent");
    if (identity.status === "exact_existing_creator" && candidateSavedParent(db, identity.creator_id, parentUrl) !== null) continue;
    const parent = routes.find(([, current]) => current.locator.kind === "url" && ["website", "link_hub"].includes(current.locator.channel) && current.locator.normalized_value === parentUrl);
    if (!parent || parent[0] === id) return fail("contact_normalization", "Link-hub observation has no distinct parent");
    (parents[id] ??= []).push(parent[0]);
  }
  const { contact_evidence: _evidence, ...preview } = input;
  return { ok: true, value: { input, preview: { ...preview, intent, status, reason, identity_resolution: identity, route_options: options }, routes, required_parent_routes: parents } };
}
export function prepareCandidateRows(db: DatabaseSync, raw: readonly unknown[], intent: CandidateIntent): { rows: CandidatePreparedRow[]; failures: CandidateFailure[]; intent: CandidateIntent } {
  if (raw.length < 1 || raw.length > 50 || Buffer.byteLength(candidateJson(raw)) > 16 * 1024 * 1024) throw new Error("Creator save batch fields are invalid");
  const ids = new Set<string>(), rows: CandidatePreparedRow[] = [], failures: CandidateFailure[] = [];
  for (const value of raw) {
    const input = canonicalCandidate.parse(value);
    if (!trim(input.row_id) || Buffer.byteLength(input.row_id) > 256 || ids.has(input.row_id)) throw new Error("Creator save row identity is invalid");
    ids.add(input.row_id);
    const result = prepareRow(db, input, intent);
    if (result.ok) rows.push(result.value);
    else failures.push({ row_id: input.row_id, kind: result.kind, platform: input.account.platform, display_name: input.account.display_name, handle: input.account.handle, profile_url: input.account.profile_url });
  }
  if (!rows.length && failures.length) throw new Error(`Creator save preparation failed: ${failures[0]!.kind}`);
  const groups = new Map<string, CandidatePreparedRow[]>();
  for (const row of rows) if (row.preview.status === "eligible_new" || row.preview.status === "eligible_update") { const a = row.input.account, key = `${a.platform}:${a.target_kind}:${a.external_id}`; const group = groups.get(key) ?? []; group.push(row); groups.set(key, group); }
  for (const group of groups.values()) {
    if (group.length < 2) continue;
    const first = group[0]!;
    if (intent === "save_or_update" && group.some(row => candidateJson([row.input.notes, row.input.quality]) !== candidateJson([first.input.notes, first.input.quality]))) for (const row of group) { row.preview.status = "identity_pending"; row.preview.reason = "conflicting_update"; }
    else if (intent === "add_new" || first.preview.status === "eligible_new") for (const row of group.slice(1)) { row.preview.status = "duplicate_skipped"; row.preview.reason = "duplicate_selection"; }
  }
  return { rows, failures, intent };
}
