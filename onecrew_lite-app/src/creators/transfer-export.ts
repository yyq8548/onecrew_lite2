import type { DatabaseSync } from "node:sqlite";
import { createHash } from "node:crypto";
import { lstat, stat, open, readFile, unlink, link } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { z } from "zod";
import { creatorOutput, platform, sourceKind, text, trim } from "./contracts.ts";
import { transferFollowers } from "./transfer-contracts.ts";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { buildBundle, canonicalCreator, digest, type TransferCreator } from "./transfer-contracts.ts";
import { encodeWorkbook, type WorkbookDisplay } from "./workbook.ts";

export const exportInput = z.object({ format: z.literal("xlsx") }).strict();
let desktopWrites = 0;
const desktopWaiters: (() => void)[] = [];
function acquireDesktopWrite(): Promise<() => void> {
  return new Promise((resolve, reject) => {
    const acquired = () => { let released = false; resolve(() => { if (released) throw new Error("Creator Desktop write permit released twice"); released = true; const next = desktopWaiters.shift(); if (next) next(); else desktopWrites--; }); };
    if (desktopWrites < 2) { desktopWrites++; acquired(); return; }
    const ready = () => { clearTimeout(deadline); acquired(); };
    const deadline = setTimeout(() => { const index = desktopWaiters.indexOf(ready); if (index >= 0) desktopWaiters.splice(index, 1); reject(new Error("Creator Desktop export writer is overloaded")); }, 100);
    desktopWaiters.push(ready);
  });
}
const avatarKeys = { x: ["profile_image_url"], instagram: ["profile_pic_url", "profile_pic_url_hd"], telegram: ["photo_url", "profile_photo_url"], youtube: ["thumbnail_url"], tiktok: ["profile_image"] };
export function exportSnapshot(db: DatabaseSync) {
  const heads = db.prepare("SELECT creator.*,relationship.stage,creator_group.name AS group_name,(SELECT note.value FROM observation_sources source,json_each(source.metadata_json,'$.source_notes') note WHERE source.kind='creator_import' AND note.key=creator.id ORDER BY source.observed_at DESC,source.rowid DESC LIMIT 1) AS source_notes FROM creators creator LEFT JOIN creator_relationships relationship ON relationship.creator_id=creator.id LEFT JOIN creator_groups creator_group ON creator_group.id=creator.group_id ORDER BY creator.id LIMIT 10001").all();
  if (heads.length > 10000) throw new Error("Creator bundle exceeds maximum 10000 Creators");
  const accountOrder = new Map(db.prepare("SELECT DISTINCT account.id,account.created_at,account.rowid FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id=account.id ORDER BY account.created_at,account.rowid").all().map((row, index) => [text.parse(row.id), index]));
  const records: TransferCreator[] = [], displays: WorkbookDisplay[] = [];
  const accountRows = db.prepare("SELECT link.creator_id,account.*,link.state,link.confidence,observation.handle,observation.display_name,observation.profile_url,observation.public_json,source.kind AS source_kind FROM creator_account_links link JOIN platform_accounts account ON account.id=link.platform_account_id LEFT JOIN platform_account_observations observation ON observation.rowid=(SELECT latest.rowid FROM platform_account_observations latest WHERE latest.platform_account_id=account.id ORDER BY latest.observed_at DESC,latest.rowid DESC LIMIT 1) LEFT JOIN observation_sources source ON source.id=observation.source_id ORDER BY link.creator_id,account.created_at,account.rowid,link.id").all();
  const routeRows = db.prepare("SELECT route.creator_id,route.id,route.confidence,locator.channel,locator.kind,locator.normalized_value,locator.display_value FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id ORDER BY route.creator_id,route.created_at,route.rowid").all();
  const sources = db.prepare("SELECT route.id AS route_id,source.kind FROM creator_contact_routes route JOIN locator_observations observation ON observation.locator_id=route.locator_id JOIN observation_sources source ON source.id=observation.source_id WHERE observation.creator_id=route.creator_id OR observation.platform_account_id IN (SELECT platform_account_id FROM creator_account_links WHERE creator_id=route.creator_id) ORDER BY route.creator_id,route.id,observation.observed_at DESC,observation.rowid DESC").all();
  const accountGroups = Map.groupBy(accountRows, row => text.parse(row.creator_id)), routeGroups = Map.groupBy(routeRows, row => text.parse(row.creator_id)), sourceGroups = Map.groupBy(sources, row => text.parse(row.route_id));
  const touches = new Map<string, number>();
  for (const row of db.prepare("SELECT creator_id,max(at) AS at FROM (SELECT creator_id,max(occurred_at) AS at FROM touchpoints GROUP BY creator_id UNION ALL SELECT thread.creator_id,max(email.occurred_at) AS at FROM emails email JOIN email_threads thread ON thread.id=email.thread_id WHERE email.direction='outbound' AND thread.creator_id IS NOT NULL GROUP BY thread.creator_id UNION ALL SELECT route.creator_id,max(message.sent_at) AS at FROM messages message JOIN conversation_contact_routes mapping ON mapping.conversation_id=message.conversation_id JOIN creator_contact_routes route ON route.id=mapping.contact_route_id WHERE message.direction='outbound' AND message.status='sent' GROUP BY route.creator_id) GROUP BY creator_id").all()) if (row.at !== null) touches.set(text.parse(row.creator_id), Number(row.at));
  for (const head of heads) {
    const creator = creatorOutput(head); if (head.stage === null) throw new Error(`Creator ${creator.id} has no relationship row`);
    const accounts = (accountGroups.get(creator.id) ?? []).map(row => {
      const p = platform.parse(row.platform), data = z.object({}).catchall(z.unknown()).safeParse(row.public_json === null ? null : parseStoredJson(text.parse(row.public_json))); let avatar: string | null = null;
      if (data.success) for (const key of ["avatar_url", ...avatarKeys[p]]) { const v = data.data[key]; if (typeof v === "string" && trim(v)) { avatar = trim(v); break; } }
      const profile = { handle: row.handle === null ? null : text.parse(row.handle), display_name: row.display_name === null ? null : text.parse(row.display_name), profile_url: row.profile_url === null ? null : text.parse(row.profile_url), avatar_url: avatar };
      const capture = accountOrder.get(text.parse(row.id)); if (capture === undefined) throw new Error("Creator capture order missing");
      return { capture_order: capture, platform: p, target_kind: text.parse(row.target_kind) as TransferCreator["accounts"][number]["target_kind"], external_id: text.parse(row.external_id), link_state: text.parse(row.state) as "claimed", confidence: text.parse(row.confidence) as "exact", source_kind: row.source_kind === null ? "initial_platform_account" as const : sourceKind.parse(row.source_kind), latest_profile: Object.values(profile).some(v => v !== null) ? profile : null };
    });
    const routes = (routeGroups.get(creator.id) ?? []).map((row, index) => ({ save_order: index, channel: text.parse(row.channel) as TransferCreator["contact_routes"][number]["channel"], locator_kind: text.parse(row.kind) as "handle", normalized_value: text.parse(row.normalized_value), display_value: text.parse(row.display_value), confidence: text.parse(row.confidence) as "asserted", source_kinds: (sourceGroups.get(text.parse(row.id)) ?? []).map(source => sourceKind.parse(source.kind)) }));
    records.push({ semantic_sha256: "", record_key: "", display_name: creator.display_name, tags: creator.tags, notes: head.source_notes === null ? null : text.parse(head.source_notes), score: creator.score, follower_counts: transferFollowers.parse(creator.follower_counts), pinned: creator.pinned_at !== null, relationship: { stage: text.parse(head.stage) as "prospect" }, accounts, contact_routes: routes });
    displays.push({ last_touch_at: touches.get(creator.id) ?? null, updated_at: creator.updated_at, group_name: head.group_name === null ? null : text.parse(head.group_name) });
  }
  const bundle = buildBundle(records), byKey = new Map<string, WorkbookDisplay>();
  const counts = new Map<string, number>();
  for (const [index, record] of records.entries()) { const canonical = canonicalCreator(record, bundle.format_version), semantic = digest(canonical), occurrence = counts.get(semantic) ?? 0; byKey.set(digest(["reacher.creator_record", canonical, occurrence]), displays[index]!); counts.set(semantic, occurrence + 1); }
  return { bundle, display: byKey };
}
export async function writeDesktopExport(snapshot: ReturnType<typeof exportSnapshot>, getDirectory: (() => Promise<string>) | undefined) {
  if (!getDirectory) throw new Error("the operating system Desktop directory is unavailable for Creator export");
  const directory = await getDirectory(), bytes = await encodeWorkbook(snapshot.bundle, snapshot.display), release = await acquireDesktopWrite();
  try { return await persistDesktopExport(directory, bytes, snapshot.bundle); } finally { release(); }
}
async function persistDesktopExport(directory: string, bytes: Buffer, bundle: ReturnType<typeof exportSnapshot>["bundle"]) {
  const metadata = await stat(directory); if (!metadata.isDirectory()) throw new Error("Desktop export location is not a directory");
  const artifact = createHash("sha256").update(bytes).digest("hex"), file_name = `reacher-creators-${artifact.slice(0, 16)}.xlsx`, target = join(directory, file_name);
  const temporary = join(directory, `.onecrew-export-${randomUUID()}.xlsx`), file = await open(temporary, "wx");
  let persisted = false;
  try { await file.writeFile(bytes); await file.sync(); await file.close();
    const existing = await lstat(target).then(value => value, (error: unknown) => { if (error instanceof Error && "code" in error && error.code === "ENOENT") return null; throw error; });
    if (existing) await verifyExistingExport(target, bytes);
    else { persisted = await link(temporary, target).then(() => true, (error: unknown) => { if (error instanceof Error && "code" in error && error.code === "EEXIST") return false; throw error; }); if (!persisted) await verifyExistingExport(target, bytes); }
  } finally { await file.close(); await unlink(temporary); }
  return { destination: "system_desktop" as const, format: "xlsx" as const, file_name, creator_count: bundle.creator_count, content_sha256: bundle.content_sha256, reused_existing: !persisted };
}
async function verifyExistingExport(target: string, expected: Buffer): Promise<void> {
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size !== expected.length || !(await readFile(target)).equals(expected)) throw new Error("Creator export target exists with different content");
}
