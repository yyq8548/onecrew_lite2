import type { DatabaseSync } from "node:sqlite";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { z } from "zod";
import { confidence, creatorOutput, integer, locatorKind, platform, sourceOutput, sourceRow, stage, text, timestamps, channel } from "./contracts.ts";

const nullable = text.nullable();
export const lastTouch = (id: string): string => `(SELECT MAX(value) FROM (
  SELECT MAX(touch.occurred_at) AS value FROM touchpoints touch WHERE touch.creator_id=${id}
  UNION ALL SELECT MAX(email.occurred_at) FROM emails email JOIN email_threads thread ON thread.id=email.thread_id WHERE thread.creator_id=${id} AND email.direction='outbound'
  UNION ALL SELECT MAX(message.sent_at) FROM messages message JOIN conversation_contact_routes link ON link.conversation_id=message.conversation_id JOIN creator_contact_routes route ON route.id=link.contact_route_id WHERE route.creator_id=${id} AND message.direction='outbound' AND message.status='sent'))`;
const linkSchema = z.object({ id: text, creator_id: text, platform_account_id: text, state: z.enum(["claimed", "confirmed", "rejected"]), confidence, ...timestamps });
const accountSchema = z.object({ id: text, platform, target_kind: z.enum(["user", "bot", "channel", "supergroup", "group", "creator", "business"]), external_id: text, ...timestamps });
const observationSchema = z.object({ id: text, platform_account_id: text, source_id: text, handle: nullable, display_name: nullable, profile_url: nullable, public_json: text, observed_at: integer });
const locatorSchema = z.object({ id: text, channel, kind: locatorKind, normalized_value: text, display_value: text, ...timestamps });
const routeSchema = z.object({ id: text, creator_id: text, locator_id: text, confidence, ...timestamps });
const locatorObservation = z.object({ id: text, locator_id: text, source_id: text, creator_id: nullable, platform_account_id: nullable, observed_value: text, observed_at: integer });
export const contactRouteRecordSchema = z.object({ route: routeSchema, locator: locatorSchema, observations: z.array(z.object({ observation: locatorObservation, source: sourceRow.omit({ metadata_json: true }).extend({ metadata: z.json() }) })), allowed_actions: z.array(z.enum(["update", "delete"])) });

export function contactRoutes(db: DatabaseSync, id: string, active: boolean) {
  return db.prepare("SELECT route.* FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id WHERE route.creator_id=? ORDER BY route.created_at ASC,route.rowid ASC").all(id).map(value => {
    const route = routeSchema.parse(value);
    const locator = locatorSchema.parse(db.prepare("SELECT * FROM contact_locators WHERE id=?").get(route.locator_id));
    const observations = db.prepare(`SELECT observation.* FROM locator_observations observation JOIN observation_sources source ON source.id=observation.source_id WHERE observation.locator_id=? AND (observation.creator_id=? OR observation.platform_account_id IN (SELECT platform_account_id FROM creator_account_links WHERE creator_id=?)) ORDER BY observation.observed_at DESC`).all(route.locator_id, id, id).map(value => {
      const observation = locatorObservation.parse(value);
      return { observation, source: sourceOutput(db.prepare("SELECT * FROM observation_sources WHERE id=?").get(observation.source_id)) };
    });
    const allowed_actions: ("update" | "delete")[] = [];
    if (active) {
      const first = observations[0];
      if (observations.length === 1 && first && first.source.kind === "manual_entry" && first.observation.creator_id === id && first.observation.platform_account_id === null && first.source.external_ref === null && first.source.account_ref_id === null && JSON.stringify(first.source.metadata) === "{}" && db.prepare("SELECT COUNT(*) AS n FROM locator_observations WHERE source_id=?").get(first.source.id)!.n === 1) allowed_actions.push("update");
      const root = observations.some(value => {
        const metadata = value.source.metadata;
        const source = metadata !== null && typeof metadata === "object" && !Array.isArray(metadata) && "source" in metadata ? metadata.source : null;
        const kind = source !== null && typeof source === "object" && !Array.isArray(source) && "kind" in source ? source.kind : null;
        return value.source.kind === "initial_platform_account" && kind !== "creator_contact_route";
      });
      if (!root) allowed_actions.push("delete");
    }
    return { route, locator, observations, allowed_actions };
  });
}

const sourcesSql = `WITH source_ids(source_id) AS (
 SELECT observation.source_id FROM creator_account_links link JOIN platform_account_observations observation ON observation.platform_account_id=link.platform_account_id WHERE link.creator_id=?1
 UNION SELECT source_id FROM creator_origins WHERE creator_id=?1
 UNION SELECT source_id FROM locator_observations WHERE creator_id=?1
 UNION SELECT observation.source_id FROM creator_account_links link JOIN locator_observations observation ON observation.platform_account_id=link.platform_account_id WHERE link.creator_id=?1
) SELECT source.* FROM source_ids JOIN observation_sources source ON source.id=source_ids.source_id ORDER BY source.observed_at DESC,source.id ASC`;
export function creatorDetail(db: DatabaseSync, id: string) {
  const row = db.prepare("SELECT * FROM creators WHERE id=?").get(id);
  if (!row) throw new Error("creator not found");
  const creator = creatorOutput(row);
  const relationship = z.object({ creator_id: text, stage, last_touch_at: integer.nullable(), ...timestamps }).parse(db.prepare(`SELECT creator_id,stage,created_at,updated_at,${lastTouch("?1")} AS last_touch_at FROM creator_relationships WHERE creator_id=?1`).get(id));
  const groupRow = db.prepare("SELECT creator_group.id,creator_group.name FROM creators creator JOIN creator_groups creator_group ON creator_group.id=creator.group_id WHERE creator.id=?").get(id);
  const group = groupRow ? z.object({ id: text, name: text }).parse(groupRow) : null;
  const platform_accounts = db.prepare("SELECT pa.* FROM platform_accounts pa JOIN creator_account_links link ON link.platform_account_id=pa.id WHERE link.creator_id=? AND link.state!='rejected' ORDER BY pa.created_at ASC").all(id).map(value => accountSchema.parse(value));
  const account_links = db.prepare("SELECT * FROM creator_account_links WHERE creator_id=? ORDER BY created_at ASC").all(id).map(value => linkSchema.parse(value));
  const account_observations = db.prepare("SELECT observation.* FROM platform_account_observations observation JOIN creator_account_links link ON link.platform_account_id=observation.platform_account_id WHERE link.creator_id=? ORDER BY observation.observed_at DESC,observation.rowid DESC").all(id).map(value => { const { public_json, ...row } = observationSchema.parse(value); return { ...row, public_data: parseStoredJson(public_json) }; });
  const observation_sources = db.prepare(sourcesSql).all(id).map(sourceOutput);
  const origins = db.prepare("SELECT * FROM creator_origins WHERE creator_id=? AND source_id IS NOT NULL ORDER BY discovered_at DESC").all(id).map(value => originOutput(value));
  return { creator, group, relationship, platform_accounts, account_links, account_observations, observation_sources, origins, contact_routes: contactRoutes(db, id, creator.archived_at === null && relationship.stage !== "archived") };
}
function originOutput(value: unknown) {
  const row = z.object({ id: text, creator_id: text, source_id: text, kind: z.enum(["platform_search", "community_administrator", "manual_entry", "creator_import"]), discovered_at: integer, discovery_id: nullable, operator_account_ref_id: nullable, source_creator_id: nullable, source_contact_route_id: nullable, source_resolved_target_id: nullable, administrator_role: nullable, community_external_id: nullable, community_title: nullable, community_handle: nullable, community_target_kind: nullable, import_content_sha256: nullable, imported_creator_id: nullable }).parse(value);
  const base = { id: row.id, creator_id: row.creator_id, source_id: row.source_id, kind: row.kind };
  if (row.kind === "manual_entry") return { ...base, created_at: row.discovered_at };
  if (row.kind === "creator_import") return { ...base, content_sha256: text.parse(row.import_content_sha256), imported_creator_id: text.parse(row.imported_creator_id), imported_at: row.discovered_at };
  const discovery = { ...base, discovery_id: text.parse(row.discovery_id), discovered_at: row.discovered_at };
  if (row.kind === "platform_search") return { ...discovery, account_ref_id: text.parse(row.operator_account_ref_id) };
  const source = row.source_creator_id === null ? { kind: "operator_account" as const, operator_account_ref_id: text.parse(row.operator_account_ref_id) } : { kind: "creator_contact_route" as const, source_creator_id: row.source_creator_id, source_contact_route_id: row.source_contact_route_id, operator_account_ref_id: text.parse(row.operator_account_ref_id), source_resolved_target_id: text.parse(row.source_resolved_target_id) };
  return { ...discovery, role: z.enum(["owner", "administrator"]).parse(row.administrator_role), community: { external_id: text.parse(row.community_external_id), title: text.parse(row.community_title), handle: row.community_handle, target_kind: z.enum(["channel", "supergroup"]).parse(row.community_target_kind) }, source };
}
