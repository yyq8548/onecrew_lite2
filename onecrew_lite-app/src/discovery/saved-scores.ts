import { createHash, randomUUID } from "node:crypto";
import { lstat, open, readFile, rename, unlink } from "node:fs/promises";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import type { AccountDatabases } from "../identity/account-databases.ts";
import type { AccountContext } from "../identity/account-context.ts";
import { discoveryScoreCapacity } from "./projection-lane.ts";

const activeCapacity = discoveryScoreCapacity(Number.MAX_SAFE_INTEGER);
const text = z.string().refine(value => value.isWellFormed());
export const savedDiscoveryScore = z.object({ hit: text, deferred_platforms: z.array(text) });
export type SavedDiscoveryScore = z.infer<typeof savedDiscoveryScore>;
const deferredScore = savedDiscoveryScore.extend({ reason: text.min(1) });
const scoreState = z.strictObject({ version: z.literal(2), active: z.array(savedDiscoveryScore).max(activeCapacity), deferred: z.array(deferredScore) });
type ScoreState = z.infer<typeof scoreState>;
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

// 保存尚未取得评分回执的来源及延后平台；恢复时依据证据键复用已提交回执。
export class SavedDiscoveryScores {
  readonly #owned: OwnedDiscoveryQueueDirectory; readonly #directory: string; readonly #path: string;
  constructor(owned: OwnedDiscoveryQueueDirectory, campaign: string, revision: number) {
    owned.assertOwned(); z.string().min(1).parse(owned.owner.accountId); text.parse(campaign); z.number().int().min(0).max(4294967295).parse(revision);
    this.#owned = owned; this.#directory = join(owned.directory, "discovery-score-queue");
    this.#path = join(this.#directory, `${createHash("sha256").update(stringifyApplicationJson([owned.owner.accountId, campaign, revision])).digest("hex")}.json`);
  }
  get path(): string { this.#owned.assertOwned(); return this.#path; }
  async #read(): Promise<ScoreState> {
    this.#owned.assertOwned(); await verifyPrivateDirectory(this.#owned.directory); this.#owned.assertOwned();
    const metadata = await lstat(this.#path).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (metadata === null) return { version: 2, active: [], deferred: [] };
    await verifyPrivateDirectory(this.#directory); await verifyPrivateFile(this.#path); this.#owned.assertOwned();
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.nlink !== 1) throw new Error("score_queue_file_invalid");
    const bytes = await readFile(this.#path); this.#owned.assertOwned();
    const value = parseApplicationJson(bytes);
    const state = Array.isArray(value) ? { version: 2 as const, active: z.array(savedDiscoveryScore).parse(value), deferred: [] } : scoreState.parse(value);
    if (state.active.length > activeCapacity) throw new Error("score_queue_exceeds_capacity");
    const hits = [...state.active, ...state.deferred].map(row => row.hit);
    if (new Set(hits).size !== hits.length) throw new Error("score_queue_file_invalid");
    return state;
  }
  loadState(): Promise<ScoreState> { return serialized(() => this.#read()); }
  async load(): Promise<SavedDiscoveryScore[]> { return (await this.loadState()).active; }
  async loadDeferred(): Promise<z.infer<typeof deferredScore>[]> { return (await this.loadState()).deferred; }
  async #update(edit: (state: ScoreState) => ScoreState): Promise<void> {
    const before = await this.#read(), state = edit(before);
    if (isDeepStrictEqual(before, state)) return;
    if (state.active.length > activeCapacity) throw new Error("score_queue_exceeds_capacity");
    const metadata = await lstat(this.#directory).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (metadata === null) await createPrivateDirectory(this.#directory); else await verifyPrivateDirectory(this.#directory);
    this.#owned.assertOwned();
    const temporary = join(this.#directory, `.score-${randomUUID()}.json`); let created = false, installed = false;
    const outcome = await (async () => {
      const file = await open(temporary, "wx", 0o600); created = true;
      try { await verifyPrivateFile(temporary); await file.writeFile(stringifyApplicationJson(state), "utf8"); await file.sync(); }
      finally { await file.close(); }
      this.#owned.assertOwned(); await rename(temporary, this.#path); installed = true;
      await verifyPrivateFile(this.#path); this.#owned.assertOwned();
    })().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    const cleanup = created && !installed ? await unlink(temporary).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown })) : { ok: true as const };
    if (!outcome.ok && !cleanup.ok) throw new AggregateError([outcome.error, cleanup.error], "score_queue_write_and_cleanup_failed");
    if (!outcome.ok) throw outcome.error; if (!cleanup.ok) throw cleanup.error;
  }
  add(raw: unknown): Promise<void> { const row = savedDiscoveryScore.parse(raw); return serialized(() => this.#update(state => ({ ...state, active: [...state.active.filter(previous => previous.hit !== row.hit), row], deferred: state.deferred.filter(previous => previous.hit !== row.hit) }))); }
  defer(raw: unknown, reason: string): Promise<void> { const row = deferredScore.parse({ ...savedDiscoveryScore.parse(raw), reason }); return serialized(() => this.#update(state => ({ ...state, active: state.active.filter(previous => previous.hit !== row.hit), deferred: [...state.deferred.filter(previous => previous.hit !== row.hit), row] }))); }
  remove(hit: string): Promise<void> { text.parse(hit); return serialized(() => this.#update(state => ({ ...state, active: state.active.filter(row => row.hit !== hit), deferred: state.deferred.filter(row => row.hit !== hit) }))); }
}
