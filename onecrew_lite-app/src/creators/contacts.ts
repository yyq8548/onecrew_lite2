import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { channel, platform, text, trim } from "./contracts.ts";
import { contactRoutes } from "./detail.ts";
import { normalizeContact } from "./normalization.ts";

const value = text.transform(trim).refine(value => value.length > 0, "contact route value is required");
export const contactAddInput = z.object({ creator_id: text, channel, value, source_url: value.nullish() }).strict();
export const contactUpdateInput = contactAddInput.extend({ contact_route_id: text });
export const contactDeleteInput = z.object({ creator_id: text, contact_route_id: text }).strict();
export const contactConfirmInput = z.object({ operation_id: text }).strict();
const targetKind = z.enum(["user", "bot", "channel", "supergroup", "group", "creator", "business"]);
export const identityResolveInput = z.object({ candidate: z.object({ platform, external_id: text.nullish(), target_kind: targetKind.nullish(), handle: text.nullish(), display_name: text, profile_url: text.nullish() }) }).strict();
export function creatorActive(db: DatabaseSync, id: string) { const current = db.prepare("SELECT creator.archived_at IS NULL AND relationship.stage!='archived' AS active FROM creators creator JOIN creator_relationships relationship ON relationship.creator_id=creator.id WHERE creator.id=?").get(id); if (!current) throw new Error("creator not found"); return current.active === 1; }
export function activeCreator(db: DatabaseSync, id: string) { if (!creatorActive(db, id)) throw new Error("archived Creator cannot be used for outreach; delete it or use an active Creator"); }
export function routeById(db: DatabaseSync, creatorId: string, routeId: string) { const routes = contactRoutes(db, creatorId, true); const route = routes.find(route => route.route.id === routeId); if (!route) throw new Error("contact route not found for Creator"); return route; }
export type ContactRoute = ReturnType<typeof routeById>;
export function hasRootInitial(route: ContactRoute): boolean {
  return route.observations.some(value => { const metadata = value.source.metadata; const source = metadata !== null && typeof metadata === "object" && !Array.isArray(metadata) && "source" in metadata ? metadata.source : null; const kind = source !== null && typeof source === "object" && !Array.isArray(source) && "kind" in source ? source.kind : null; return value.source.kind === "initial_platform_account" && kind !== "creator_contact_route"; });
}
const contactPlatform = (channel: string) => ["x", "instagram", "telegram", "youtube", "tiktok"].includes(channel) ? channel : null;
function audit(db: DatabaseSync, kind: string, creator: string, route: string, locator: string, at: number) { db.prepare("INSERT INTO audit_events (id,kind,creator_id,contact_locator_id,contact_route_id,payload_json,created_at) VALUES (?,?,?,?,?,?,?)").run(randomUUID(), kind, creator, locator, route, JSON.stringify({ route_id: route, details: { route_id: route } }), at); }
function addResult(status: "added" | "already_exists", record: ContactRoute) { const { route, locator } = record; return { status, creator_id: route.creator_id, route_id: route.id, channel: locator.channel, locator_kind: locator.kind, normalized_value: locator.normalized_value, display_value: locator.display_value, confidence: route.confidence, created_at: route.created_at }; }
export function addContact(db: DatabaseSync, raw: unknown) {
  const input = contactAddInput.parse(raw), canonical = normalizeContact(input.channel, input.value);
  activeCreator(db, input.creator_id);
  const owners = db.prepare("SELECT route.id,route.creator_id FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id WHERE locator.channel=? AND locator.kind=? AND locator.normalized_value=? ORDER BY route.created_at ASC,route.rowid ASC").all(canonical.channel, canonical.kind, canonical.normalized_value);
  if (owners.some(owner => owner.creator_id !== input.creator_id)) throw new Error("contact route already belongs to another Creator");
  const existingOwner = owners[0];
  if (existingOwner) return addResult("already_exists", routeById(db, input.creator_id, text.parse(existingOwner.id)));
  const old = db.prepare("SELECT id FROM contact_locators WHERE channel=? AND kind=? AND normalized_value=?").get(canonical.channel, canonical.kind, canonical.normalized_value);
  const locator = old ? text.parse(old.id) : randomUUID(), source = randomUUID(), route = randomUUID(), at = Math.floor(Date.now() / 1000);
  db.prepare("INSERT INTO contact_locators (id,channel,kind,normalized_value,display_value,created_at,updated_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(channel,kind,normalized_value) DO UPDATE SET display_value=excluded.display_value,updated_at=excluded.updated_at").run(locator, canonical.channel, canonical.kind, canonical.normalized_value, canonical.display_value, at, at);
  db.prepare("INSERT INTO observation_sources (id,kind,platform,source_url,external_ref,account_ref_id,metadata_json,observed_at) VALUES (?,'manual_entry',?,?,NULL,NULL,'{}',?)").run(source, contactPlatform(input.channel), input.source_url ?? null, at);
  db.prepare("INSERT INTO creator_contact_routes (id,creator_id,locator_id,confidence,created_at,updated_at) VALUES (?,?,?,'asserted',?,?)").run(route, input.creator_id, locator, at, at);
  db.prepare("INSERT INTO locator_observations (id,locator_id,source_id,creator_id,platform_account_id,observed_value,observed_at) VALUES (?,?,?,?,NULL,?,?)").run(randomUUID(), locator, source, input.creator_id, input.value, at);
  audit(db, "creator_contact_route_added", input.creator_id, route, locator, at);
  return addResult("added", routeById(db, input.creator_id, route));
}
export function updateContact(db: DatabaseSync, raw: unknown) {
  const input = contactUpdateInput.parse(raw), canonical = normalizeContact(input.channel, input.value);
  activeCreator(db, input.creator_id);
  const existing = routeById(db, input.creator_id, input.contact_route_id);
  if (!existing.allowed_actions.includes("update")) throw new Error("only an active manually added contact route can be updated");
  const result = (status: "updated" | "unchanged" | "replacement_required", route = existing) => ({ status, creator_id: input.creator_id, route_id: existing.route.id, route });
  if (existing.locator.channel !== canonical.channel || existing.locator.kind !== canonical.kind || existing.locator.normalized_value !== canonical.normalized_value) return result("replacement_required");
  const observation = existing.observations[0]!;
  if (existing.locator.display_value === canonical.display_value && existing.route.confidence === "asserted" && observation.observation.observed_value === input.value && observation.source.source_url === (input.source_url ?? null)) return result("unchanged");
  const at = Math.floor(Date.now() / 1000);
  db.prepare("UPDATE contact_locators SET display_value=?,updated_at=? WHERE id=?").run(canonical.display_value, at, existing.locator.id);
  db.prepare("UPDATE observation_sources SET platform=?,source_url=?,observed_at=? WHERE id=?").run(contactPlatform(input.channel), input.source_url ?? null, at, observation.source.id);
  db.prepare("UPDATE locator_observations SET observed_value=?,observed_at=? WHERE id=?").run(input.value, at, observation.observation.id);
  db.prepare("UPDATE creator_contact_routes SET confidence='asserted',updated_at=? WHERE id=?").run(at, existing.route.id);
  audit(db, "creator_contact_route_updated", input.creator_id, existing.route.id, existing.locator.id, at);
  return result("updated", routeById(db, input.creator_id, existing.route.id));
}
export function resolveIdentity(db: DatabaseSync, raw: unknown) {
  const { candidate } = identityResolveInput.parse(raw);
  const ids = (sql: string, ...values: string[]) => db.prepare(sql).all(...values).map(row => text.parse(row.creator_id));
  const external = candidate.external_id == null ? null : trim(candidate.external_id);
  if (external && candidate.target_kind) {
    const exact = ids("SELECT DISTINCT link.creator_id FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id=account.id WHERE account.platform=? AND account.target_kind=? AND account.external_id=? AND link.state='confirmed' ORDER BY link.creator_id", candidate.platform, candidate.target_kind, external);
    if (exact.length === 1) return { status: "exact_existing_creator" as const, creator_id: exact[0]! };
    if (exact.length > 1) return { status: "conflicting_matches" as const, creator_ids: exact };
  }
  const handle = candidate.handle == null ? null : trim(candidate.handle).replace(/^@+/u, "").toLowerCase();
  if (!handle || !candidate.target_kind) return { status: "new" as const };
  const possible = ids("SELECT DISTINCT link.creator_id FROM platform_account_observations observation JOIN platform_accounts account ON account.id=observation.platform_account_id JOIN creator_account_links link ON link.platform_account_id=account.id WHERE account.platform=? AND account.target_kind=? AND lower(ltrim(observation.handle,'@'))=? AND link.state!='rejected' ORDER BY link.creator_id", candidate.platform, candidate.target_kind, handle);
  return possible.length === 0 ? { status: "new" as const } : { status: possible.length === 1 ? "possible_match" as const : "conflicting_matches" as const, creator_ids: possible };
}
