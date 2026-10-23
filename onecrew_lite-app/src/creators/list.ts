import type { DatabaseSync, SQLInputValue } from "node:sqlite";
import { z } from "zod";
import { isLosslessNumber, stringify } from "lossless-json";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { creatorOutput, integer, listCreatorsInput, platform, stage, text, trim } from "./contracts.ts";
import type { ListCreatorsInput } from "./contracts.ts";
import { lastTouch } from "./detail.ts";

const projection = `WITH identity AS (
 SELECT creator.*,relationship.stage,creator_group.name AS group_name,COALESCE(creator.pinned_at,0) AS pin_rank,
 (SELECT account.platform FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id=account.id WHERE link.creator_id=creator.id AND link.state!='rejected' ORDER BY account.created_at ASC,account.rowid ASC LIMIT 1) AS primary_platform,
 ${lastTouch("creator.id")} AS last_touch_at
 FROM creators creator JOIN creator_relationships relationship ON relationship.creator_id=creator.id LEFT JOIN creator_groups creator_group ON creator_group.id=creator.group_id
), projected AS (
 SELECT identity.*,CAST(COALESCE(json_extract(follower_counts_json,'$.' || primary_platform),
 (SELECT json_extract(identity.follower_counts_json,'$.' || account.platform) FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id=account.id WHERE link.creator_id=identity.id AND link.state!='rejected' AND json_extract(identity.follower_counts_json,'$.' || account.platform) IS NOT NULL GROUP BY account.platform ORDER BY MIN(account.created_at) ASC,MIN(account.rowid) ASC LIMIT 1),
 json_extract(follower_counts_json,'$.instagram'),json_extract(follower_counts_json,'$.telegram'),json_extract(follower_counts_json,'$.tiktok'),json_extract(follower_counts_json,'$.x'),json_extract(follower_counts_json,'$.youtube'),-1) AS INTEGER) AS follower_count FROM identity
)`;
function normalize(raw: unknown) {
  const input = listCreatorsInput.parse(raw);
  const query = input.query == null ? null : trim(trim(input.query).replace(/^@+/u, "")) || null;
  const range = (value: ListCreatorsInput["updated_range"]) => value == null || value.from == null && value.to_exclusive == null ? null : { from: value.from ?? null, to_exclusive: value.to_exclusive ?? null };
  return { limit: input.limit ?? null, cursor: input.cursor ?? null, query, stages: [...new Set(input.stages)].sort((a, b) => stage.options.indexOf(a) - stage.options.indexOf(b)), primary_platforms: [...new Set(input.primary_platforms)].sort((a, b) => platform.options.indexOf(a) - platform.options.indexOf(b)), last_touch_range: range(input.last_touch_range), updated_range: range(input.updated_range), sorting: input.sorting.length ? input.sorting : [{ field: "updated_at" as const, direction: "desc" as const }], include_facets: input.include_facets, group: input.group ?? null, include_archived: input.include_archived };
}
type Input = ReturnType<typeof normalize>;
const bind = (values: SQLInputValue[], value: SQLInputValue) => { values.push(value); return `?${values.length}`; };
function filters(input: Input, omitted?: "stage" | "platform") {
  const parts = ["1=1"], values: SQLInputValue[] = [];
  if (!input.include_archived) parts.push("creator.archived_at IS NULL");
  if (input.group?.kind === "uncategorized") parts.push("creator.group_id IS NULL");
  if (input.group?.kind === "group") parts.push(`creator.group_id=${bind(values, input.group.group_id)}`);
  if (omitted !== "stage" && input.stages.length) parts.push(`creator.stage IN (${input.stages.map(value => bind(values, value)).join(",")})`);
  if (omitted !== "platform" && input.primary_platforms.length) parts.push(`creator.primary_platform IN (${input.primary_platforms.map(value => bind(values, value)).join(",")})`);
  if (input.query !== null) {
    const value = bind(values, input.query);
    parts.push(`(instr(lower(creator.display_name),lower(${value}))>0 OR EXISTS(SELECT 1 FROM creator_account_links query_link JOIN platform_account_observations query_observation ON query_observation.id=(SELECT latest.id FROM platform_account_observations latest WHERE latest.platform_account_id=query_link.platform_account_id ORDER BY latest.observed_at DESC,latest.rowid DESC LIMIT 1) WHERE query_link.creator_id=creator.id AND query_link.state!='rejected' AND instr(lower(COALESCE(query_observation.handle,'')),lower(${value}))>0) OR EXISTS(SELECT 1 FROM creator_contact_routes query_route JOIN contact_locators query_locator ON query_locator.id=query_route.locator_id WHERE query_route.creator_id=creator.id AND (instr(lower(query_locator.channel),lower(${value}))>0 OR instr(lower(query_locator.kind),lower(${value}))>0 OR instr(lower(query_locator.normalized_value),lower(${value}))>0 OR instr(lower(query_locator.display_value),lower(${value}))>0)))`);
  }
  for (const [field, range] of [["creator.last_touch_at", input.last_touch_range], ["creator.updated_at", input.updated_range]] as const) {
    if (range?.from != null) parts.push(`${field}>=${bind(values, range.from)}`);
    if (range?.to_exclusive != null) parts.push(`${field}<${bind(values, range.to_exclusive)}`);
  }
  return { predicate: parts.join(" AND "), values };
}
const expressions = { display_name: "lower(creator.display_name)", followers: "creator.follower_count", score: "COALESCE(creator.score,-1)", last_touch_at: "COALESCE(creator.last_touch_at,-1)", updated_at: "creator.updated_at", created_at: "creator.created_at" };
function sortTerms(input: Input) { return [{ expression: "creator.pin_rank", direction: "desc", text: false }, ...input.sorting.map(sort => ({ expression: expressions[sort.field], direction: sort.direction, text: sort.field === "display_name" })), { expression: "creator.id", direction: "desc", text: true }]; }
export function listCreators(db: DatabaseSync, raw: unknown) {
  const input = normalize(raw);
  if (input.group?.kind === "group" && !db.prepare("SELECT 1 FROM creator_groups WHERE id=?").get(input.group.group_id)) throw new Error("creator group not found");
  const context = `creator_list_v2|${JSON.stringify({ ...input, limit: null, cursor: null, include_facets: false })}`;
  const terms = sortTerms(input);
  const { values, predicate } = filters(input);
  let where = predicate;
  if (input.cursor !== null) {
    const encoded = trim(input.cursor);
    if (!/^[A-Za-z0-9_-]+$/u.test(encoded)) throw new Error("page cursor is not valid for this query");
    const bytes = Buffer.from(encoded, "base64url");
    if (bytes.toString("base64url") !== encoded) throw new Error("page cursor is not valid for this query");
    const envelope = z.object({ v: z.literal(1), context: z.literal(context), position: text }).parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes), true));
    const position = z.array(z.unknown()).length(terms.length).parse(parseStoredJson(envelope.position));
    const bindings = position.map((value, index) => {
      if (terms[index]!.text) { if (typeof value !== "string") throw new Error("page cursor is not valid for this query"); return bind(values, value); }
      const raw = isLosslessNumber(value) ? value.toString() : typeof value === "number" && Number.isSafeInteger(value) ? String(value) : "";
      if (!/^-?[0-9]+$/.test(raw)) throw new Error("page cursor is not valid for this query");
      const integer = BigInt(raw); if (integer < -9223372036854775808n || integer > 9223372036854775807n) throw new Error("page cursor is not valid for this query");
      return bind(values, integer);
    });
    where += ` AND (${terms.map((term, index) => `(${[...terms.slice(0, index).map((prior, i) => `${prior.expression}=${bindings[i]}`), `${term.expression}${term.direction === "asc" ? ">" : "<"}${bindings[index]}`].join(" AND ")})`).join(" OR ")})`;
  }
  const limit = Math.min(100, Math.max(1, input.limit ?? 25));
  const facets = input.include_facets ? facetCounts(db, input) : null;
  const order = terms.map(term => `${term.expression} ${term.direction === "asc" ? "ASC" : "DESC"}`).join(",");
  const positions = terms.map((term, i) => `${term.expression} AS cursor_${i}`).join(",");
  const statement = db.prepare(`${projection} SELECT creator.*,(SELECT COUNT(*) FROM creator_account_links link WHERE link.creator_id=creator.id AND link.state!='rejected') AS platform_account_count,(SELECT COUNT(*) FROM creator_contact_routes route WHERE route.creator_id=creator.id) AS contact_route_count,EXISTS(SELECT 1 FROM creator_origins origin WHERE origin.creator_id=creator.id AND origin.kind='manual_entry') AS has_manual_origin,${positions} FROM projected creator WHERE ${where} ORDER BY ${order} LIMIT ${bind(values, limit + 1)}`);
  statement.setReadBigInts(true);
  const rows = statement.all(...values).map(row => Object.fromEntries(Object.entries(row).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value])));
  const kept = rows.slice(0, limit);
  const last = kept.at(-1);
  const next_cursor = rows.length > limit && last ? Buffer.from(JSON.stringify({ v: 1, context, position: stringify(terms.map((_, i) => last[`cursor_${i}`]))! })).toString("base64url") : null;
  const items = kept.map(row => summary(db, row));
  return { items, next_cursor, facets };
}
function facetCounts(db: DatabaseSync, input: Input) {
  const selected = filters(input);
  const total = integer.parse(db.prepare(`${projection} SELECT COUNT(*) AS n FROM projected creator WHERE ${selected.predicate}`).get(...selected.values)!.n);
  const counts = (column: string, omit: "stage" | "platform") => { const selected = filters(input, omit); return db.prepare(`${projection} SELECT ${column} AS value,COUNT(*) AS count FROM projected creator WHERE ${selected.predicate} AND ${column} IS NOT NULL GROUP BY ${column} ORDER BY ${column}`).all(...selected.values); };
  return { total, stages: counts("creator.stage", "stage").map(row => z.object({ value: stage, count: integer }).parse(row)), primary_platforms: counts("creator.primary_platform", "platform").map(row => z.object({ value: platform, count: integer }).parse(row)) };
}
function summary(db: DatabaseSync, row: Record<string, unknown>) {
  const { notes: _notes, ...creator } = creatorOutput(row);
  const identity = db.prepare("SELECT account.platform,observation.handle,observation.public_json FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id=account.id LEFT JOIN platform_account_observations observation ON observation.id=(SELECT latest.id FROM platform_account_observations latest WHERE latest.platform_account_id=account.id ORDER BY latest.observed_at DESC,latest.rowid DESC LIMIT 1) WHERE link.creator_id=? AND link.state!='rejected' ORDER BY account.created_at ASC,account.rowid ASC").all(creator.id).map(value => z.object({ platform, handle: text.nullable(), public_json: text.nullable() }).parse(value));
  let avatar_url: string | null = null;
  const avatarKeys = { x: ["profile_image_url"], instagram: ["profile_pic_url", "profile_pic_url_hd"], telegram: ["photo_url", "profile_photo_url"], youtube: ["thumbnail_url"], tiktok: ["profile_image"] };
  for (const account of identity) {
    if (account.public_json === null) continue;
    const data = parseStoredJson(account.public_json);
    if (data === null || typeof data !== "object" || Array.isArray(data)) continue;
    for (const key of ["avatar_url", ...avatarKeys[account.platform]]) { const value = z.record(z.string(), z.unknown()).parse(data)[key]; if (typeof value === "string" && trim(value)) { avatar_url = trim(value); break; } }
    if (avatar_url !== null) break;
  }
  const platforms = db.prepare("SELECT account.platform FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id=account.id WHERE link.creator_id=? AND link.state!='rejected' GROUP BY account.platform ORDER BY MIN(account.created_at) ASC,MIN(account.rowid) ASC").all(creator.id).map(row => platform.parse(row.platform));
  const channels = db.prepare("SELECT locator.channel FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id WHERE route.creator_id=? GROUP BY locator.channel ORDER BY MIN(route.created_at) ASC,MIN(route.rowid) ASC").all(creator.id).map(row => text.parse(row.channel));
  return { ...creator, stage: stage.parse(row.stage), group: row.group_id === null ? null : { id: text.parse(row.group_id), name: text.parse(row.group_name) }, primary_platform: identity[0]?.platform ?? null, primary_handle: identity[0]?.handle == null ? null : trim(identity[0].handle) || null, avatar_url, platforms, channels, has_manual_origin: row.has_manual_origin === 1, last_touch_at: integer.nullable().parse(row.last_touch_at), platform_account_count: integer.max(4294967295).parse(row.platform_account_count), contact_route_count: integer.max(4294967295).parse(row.contact_route_count) };
}
