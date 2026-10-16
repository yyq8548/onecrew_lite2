import { createHash, randomUUID } from "node:crypto";
import { lstat, open, readFile, rename, unlink } from "node:fs/promises";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import type { AccountDatabases } from "../identity/account-databases.ts";
import type { AccountContext } from "../identity/account-context.ts";

const text = z.string().refine(value => value.isWellFormed());
export const savedDiscoveryScore = z.object({ hit: text, deferred_platforms: z.array(text) });
export type SavedDiscoveryScore = z.infer<typeof savedDiscoveryScore>;
export interface OwnedDiscoveryQueueDirectory { readonly directory: string; readonly owner: Readonly<{ accountId: string; tenantId: string; baseId: string }>; assertOwned(): void }
export async function resolveDiscoveryScoreDirectory(databases: AccountDatabases, context: AccountContext): Promise<OwnedDiscoveryQueueDirectory> {
  const directory = await databases.accountDirectory(context), original = await databases.discoveryResolver().resolve(context);
  original.assertOwned();
  if (directory.owner.accountId !== original.owner.accountId || directory.owner.tenantId !== original.owner.tenantId || directory.owner.baseId !== original.owner.baseId) throw new Error("score_queue_directory_owner_mismatch");
  return Object.freeze({ directory: directory.directory, owner: original.owner, assertOwned: () => original.assertOwned() });
}
let writing: Promise<void> = Promise.resolve();
function serialized<T>(body: () => Promise<T>): Promise<T> {
  const operation = writing.then(body);
  // 锁链继续处理下一次操作；失败仍通过本次operation完整返回调用方。
  writing = operation.then(() => undefined, () => undefined); return operation;
}

// 只保存未启动评分的hit和延后来源，不保存材料、凭据或已开始模型请求。
export class SavedDiscoveryScores {
  readonly #owned: OwnedDiscoveryQueueDirectory; readonly #directory: string; readonly #path: string;
  constructor(owned: OwnedDiscoveryQueueDirectory, campaign: string, revision: number) {
    owned.assertOwned(); z.string().min(1).parse(owned.owner.accountId); text.parse(campaign); z.number().int().min(0).max(4294967295).parse(revision);
    this.#owned = owned; this.#directory = join(owned.directory, "discovery-score-queue");
    this.#path = join(this.#directory, `${createHash("sha256").update(stringifyApplicationJson([owned.owner.accountId, campaign, revision])).digest("hex")}.json`);
  }
  get path(): string { this.#owned.assertOwned(); return this.#path; }
  async #read(): Promise<SavedDiscoveryScore[]> {
    this.#owned.assertOwned(); await verifyPrivateDirectory(this.#owned.directory); this.#owned.assertOwned();
    const metadata = await lstat(this.#path).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (metadata === null) return [];
    await verifyPrivateDirectory(this.#directory); await verifyPrivateFile(this.#path); this.#owned.assertOwned();
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.nlink !== 1) throw new Error("score_queue_file_invalid");
    const bytes = await readFile(this.#path); this.#owned.assertOwned();
    const rows = z.array(savedDiscoveryScore).parse(parseApplicationJson(bytes)); if (rows.length > 5) throw new Error("score_queue_exceeds_capacity"); return rows;
  }
  load(): Promise<SavedDiscoveryScore[]> { return serialized(() => this.#read()); }
  async #update(edit: (rows: SavedDiscoveryScore[]) => SavedDiscoveryScore[]): Promise<void> {
    const before = await this.#read(), rows = edit(before);
    if (isDeepStrictEqual(before, rows)) return;
    if (rows.length > 5) throw new Error("score_queue_exceeds_capacity");
    const metadata = await lstat(this.#directory).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (metadata === null) await createPrivateDirectory(this.#directory); else await verifyPrivateDirectory(this.#directory);
    this.#owned.assertOwned();
    const temporary = join(this.#directory, `.score-${randomUUID()}.json`); let created = false, installed = false;
    const outcome = await (async () => {
      const file = await open(temporary, "wx", 0o600); created = true;
      try { await verifyPrivateFile(temporary); await file.writeFile(stringifyApplicationJson(rows), "utf8"); await file.sync(); }
      finally { await file.close(); }
      this.#owned.assertOwned(); await rename(temporary, this.#path); installed = true;
      await verifyPrivateFile(this.#path); this.#owned.assertOwned();
    })().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    const cleanup = created && !installed ? await unlink(temporary).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown })) : { ok: true as const };
    if (!outcome.ok && !cleanup.ok) throw new AggregateError([outcome.error, cleanup.error], "score_queue_write_and_cleanup_failed");
    if (!outcome.ok) throw outcome.error; if (!cleanup.ok) throw cleanup.error;
  }
  add(raw: unknown): Promise<void> { const row = savedDiscoveryScore.parse(raw); return serialized(() => this.#update(rows => [...rows.filter(previous => previous.hit !== row.hit), row])); }
  remove(hit: string): Promise<void> { text.parse(hit); return serialized(() => this.#update(rows => rows.filter(row => row.hit !== hit))); }
}
