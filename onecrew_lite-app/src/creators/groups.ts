import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { creatorU64, integer, parseJson, text, timestamps, trim } from "./contracts.ts";

const name = text.transform(trim).refine(value => [...value].length >= 1 && [...value].length <= 64, "creator group input is invalid");
export const groupListInput = z.object({ limit: integer.max(4294967295).nullish(), cursor: text.nullish(), query: text.nullish(), include_archived: z.boolean().default(false) }).strict();
export const groupCreateInput = z.object({ name }).strict();
export const groupUpdateInput = z.object({ group_id: text, expected_revision: creatorU64, name }).strict();
export const groupDeleteInput = z.object({ group_id: text, expected_revision: creatorU64 }).strict();
export const groupDestination = z.discriminatedUnion("kind", [z.object({ kind: z.literal("uncategorized") }).strict(), z.object({ kind: z.literal("group"), group_id: text }).strict()]);
export const groupSetInput = z.object({ creator_id: text.refine(value => trim(value).length > 0), destination: groupDestination }).strict();
export const groupMoveInput = z.object({ creator_ids: z.array(text.refine(value => trim(value).length > 0)).min(1).max(100).refine(values => new Set(values).size === values.length, "creator group input is invalid"), destination: groupDestination }).strict();
const groupSchema = z.object({ id: text, name: text, revision: creatorU64, position: integer, ...timestamps });
function normalizeRow(row: Record<string, unknown>) { return Object.fromEntries(Object.entries(row).map(([key, value]) => [key, typeof value === "bigint" && value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value])); }
function load(db: DatabaseSync, id: string) { const statement = db.prepare("SELECT * FROM creator_groups WHERE id=?"); statement.setReadBigInts(true); const row = statement.get(id); if (!row) throw new Error("creator group not found"); return groupSchema.parse(normalizeRow(row)); }
function requireRevision(db: DatabaseSync, id: string, revision: number | bigint) { const group = load(db, id); if (BigInt(group.revision) !== BigInt(revision)) throw new Error("creator group revision changed"); return group; }
function available(db: DatabaseSync, name: string, excluded: string | null) { if (db.prepare("SELECT 1 FROM creator_groups WHERE name=? COLLATE NOCASE AND (? IS NULL OR id!=?) LIMIT 1").get(name, excluded, excluded)) throw new Error("creator group name already exists"); }
export function listGroups(db: DatabaseSync, raw: unknown) {
  const input = groupListInput.parse(raw), query = input.query == null ? null : trim(input.query) || null;
  const context = `creator_group_list|query=${query ?? ""}|archived=${input.include_archived}`;
  let position: number | null = null, id: string | null = null;
  if (input.cursor != null) {
    const encoded = trim(input.cursor), bytes = Buffer.from(encoded, "base64url");
    if (!/^[A-Za-z0-9_-]+$/u.test(encoded) || bytes.toString("base64url") !== encoded) throw new Error("creator group input is invalid");
    const envelope = z.object({ v: z.literal(1), context: z.literal(context), position: text }).parse(parseJson(new TextDecoder("utf-8", { fatal: true }).decode(bytes)));
    const parts = envelope.position.split("|");
    if (parts.length !== 2 || !/^\+?\d+$/u.test(parts[0]!)) throw new Error("creator group input is invalid");
    position = integer.parse(Number(parts[0])); id = parts[1]!;
  }
  const limit = Math.min(100, Math.max(1, input.limit ?? 50));
  const statement = db.prepare("SELECT creator_group.*,(SELECT COUNT(*) FROM creators creator WHERE creator.group_id=creator_group.id AND (?2 OR creator.archived_at IS NULL)) AS creator_count FROM creator_groups creator_group WHERE (?1 IS NULL OR instr(lower(creator_group.name),lower(?1))>0) AND (?3 IS NULL OR creator_group.position>?3 OR (creator_group.position=?3 AND creator_group.id>?4)) ORDER BY creator_group.position ASC,creator_group.id ASC LIMIT ?5");
  statement.setReadBigInts(true);
  const rows = statement.all(query, Number(input.include_archived), position, id, limit + 1).map(normalizeRow);
  const items = rows.slice(0, limit).map(row => ({ group: groupSchema.parse(row), creator_count: integer.parse(row.creator_count) }));
  const last = items.at(-1), next_cursor = rows.length > limit && last ? Buffer.from(JSON.stringify({ v: 1, context, position: `${last.group.position}|${last.group.id}` })).toString("base64url") : null;
  const uncategorized_count = integer.parse(db.prepare("SELECT COUNT(*) AS n FROM creators WHERE group_id IS NULL AND (? OR archived_at IS NULL)").get(Number(input.include_archived))!.n);
  return { items, uncategorized_count, next_cursor };
}
export function createGroup(db: DatabaseSync, raw: unknown) {
  const input = groupCreateInput.parse(raw);
  available(db, input.name, null);
  const position = integer.parse(db.prepare("SELECT COALESCE(MAX(position),-1)+1 AS position FROM creator_groups").get()!.position);
  const id = randomUUID(), at = Math.floor(Date.now() / 1000);
  db.prepare("INSERT INTO creator_groups (id,name,revision,position,created_at,updated_at) VALUES (?,?,1,?,?,?)").run(id, input.name, position, at, at);
  return load(db, id);
}
export function updateGroup(db: DatabaseSync, raw: unknown) {
  const input = groupUpdateInput.parse(raw), current = requireRevision(db, input.group_id, input.expected_revision);
  if (input.name === current.name) return { group: current, changed: false };
  available(db, input.name, input.group_id);
  db.prepare("UPDATE creator_groups SET name=?,revision=revision+1,updated_at=? WHERE id=? AND revision=?").run(input.name, Math.floor(Date.now() / 1000), input.group_id, input.expected_revision);
  return { group: load(db, input.group_id), changed: true };
}
export function deleteGroup(db: DatabaseSync, raw: unknown) {
  const input = groupDeleteInput.parse(raw);
  requireRevision(db, input.group_id, input.expected_revision);
  const uncategorized_creator_count = integer.parse(db.prepare("SELECT COUNT(*) AS n FROM creators WHERE group_id=?").get(input.group_id)!.n);
  db.prepare("DELETE FROM creator_groups WHERE id=? AND revision=?").run(input.group_id, input.expected_revision);
  return { group_id: input.group_id, uncategorized_creator_count };
}
export function moveGroup(db: DatabaseSync, raw: unknown) {
  const input = groupMoveInput.parse(raw);
  const target = input.destination.kind === "uncategorized" ? null : load(db, input.destination.group_id);
  const group = target === null ? null : { id: target.id, name: target.name };
  let moved_count = 0, unchanged_count = 0;
  const assignments = input.creator_ids.map(creator_id => {
    const current = db.prepare("SELECT group_id FROM creators WHERE id=?").get(creator_id);
    if (!current) throw new Error("creator not found");
    if (current.group_id === (group?.id ?? null)) unchanged_count++;
    else { db.prepare("UPDATE creators SET group_id=? WHERE id=?").run(group?.id ?? null, creator_id); moved_count++; }
    return { creator_id, group };
  });
  return { assignments, moved_count, unchanged_count };
}
