import { createHash, randomUUID } from "node:crypto";
import { lstatSync } from "node:fs";
import { lstat, open, readFile, rename } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { BrowserAccountDirectoryResolver, VerifiedBrowserAccountDirectory } from "../platforms/browser/binding.ts";
import { verifyPrivateFile } from "../storage/private-backup.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";

const object = z.record(z.string(), z.unknown()), digest = z.string().regex(/^[0-9a-f]{64}$/u);
const pending = z.object({ input: object, archive: z.string(), archive_sha256: digest, content_hash: z.string(), record_count: creatorU64, finished: z.string(), parts: z.array(z.unknown()).max(2048), attempt: z.uuid().nullable() }).strict();
const table = z.object({ campaign_id: z.string(), user_id: z.string(), enabled: z.boolean(), next_check: creatorU64, committed_hash: z.string().nullable(), committed_consent: z.uuid().nullable().optional().default(null), committed_source: z.uuid().nullable().optional().default(null), failures: z.number().int().min(0).max(255).optional().default(0), previous_version: z.uuid().nullable(), sequence: creatorU64, status: z.string(), error: z.string().nullable(), pending: pending.nullable() }).strict();
export const tableSyncStoreSchema = z.object({ local_source_id: z.uuid().nullable().optional().default(null), tables: z.record(digest, table), consent_operation: object.nullable().optional().default(null), consent_user: z.string().nullable().optional().default(null) }).strict();
export type TableSyncState = z.infer<typeof tableSyncStoreSchema>;
export type TableSyncTable = z.infer<typeof table>;
export const tableSyncKey = (user: string, campaign: string) => createHash("sha256").update(`${user}\0${campaign}`).digest("hex");
const hash = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
interface Fingerprint { dev: bigint; ino: bigint; sha256: string }

function validateOwner(raw: unknown, account: AccountContext): TableSyncState {
  const state = tableSyncStoreSchema.parse(raw), user = account.principal.user.id;
  if (state.consent_user !== null && state.consent_user !== user || state.consent_operation !== null && state.consent_user === null) throw new Error("table_sync_account_changed");
  for (const [key, value] of Object.entries(state.tables)) if (value.user_id !== user || key !== tableSyncKey(user, value.campaign_id)) throw new Error("table_sync_account_changed");
  return state;
}

// 每个已验证账号独立存储，实例 owner 排除其他服务，进程内串行器保护网络检查点。
export class TableSyncStore {
  readonly #accounts: AccountContextService;
  readonly #directories: BrowserAccountDirectoryResolver;
  readonly #queues = new Map<string, Promise<void>>();
  #closed = false;
  constructor(accounts: AccountContextService, directories: BrowserAccountDirectoryResolver) { this.#accounts = accounts; this.#directories = directories; }
  #assert(account: AccountContext, directory?: VerifiedBrowserAccountDirectory): void {
    if (this.#closed) throw new Error("table_sync_store_closed");
    this.#accounts.assertCurrent(account); directory?.assertOwned();
  }
  async #read(path: string): Promise<{ state: unknown; fingerprint: Fingerprint | null }> {
    const before = lstatSync(path, { bigint: true, throwIfNoEntry: false });
    if (!before) return { state: { tables: {} }, fingerprint: null };
    await verifyPrivateFile(path);
    const bytes = await readFile(path), after = await lstat(path, { bigint: true });
    if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new Error("table_sync_store_changed");
    return { state: parseApplicationJson(bytes), fingerprint: { dev: after.dev, ino: after.ino, sha256: hash(bytes) } };
  }
  async exclusive<T>(account: AccountContext, action: (state: TableSyncState, checkpoint: () => Promise<void>) => Promise<T>): Promise<T> {
    this.#assert(account);
    const key = stringifyApplicationJson([account.principal.user.id, account.principal.tenantId, account.principal.baseId]), prior = this.#queues.get(key) ?? Promise.resolve();
    let release!: () => void;
    const own = new Promise<void>(resolve => { release = resolve; });
    this.#queues.set(key, own);
    await prior;
    let active = true;
    try {
      this.#assert(account);
      const directory = await this.#directories.accountDirectory(account);
      this.#assert(account, directory);
      if (directory.owner.accountId !== account.principal.user.id || directory.owner.tenantId !== account.principal.tenantId || directory.owner.baseId !== account.principal.baseId) throw new Error("table_sync_account_changed");
      const path = join(directory.directory, "dynamic-table-sync.json"), loaded = await this.#read(path), state = validateOwner(loaded.state, account);
      let fingerprint = loaded.fingerprint;
      const checkpoint = async () => {
        if (!active) throw new Error("table_sync_checkpoint_expired");
        this.#assert(account, directory);
        const validated = validateOwner(state, account), current = await this.#read(path);
        if (current.fingerprint?.dev !== fingerprint?.dev || current.fingerprint?.ino !== fingerprint?.ino || current.fingerprint?.sha256 !== fingerprint?.sha256) throw new Error("table_sync_store_changed");
        const bytes = Buffer.from(stringifyApplicationJson(validated)), temporary = join(directory.directory, `table-sync-${randomUUID()}.pending`);
        const file = await open(temporary, "wx", 0o600);
        try { await verifyPrivateFile(temporary); await file.writeFile(bytes); await file.sync(); }
        finally { await file.close(); }
        this.#assert(account, directory);
        const latest = await this.#read(path);
        if (latest.fingerprint?.dev !== fingerprint?.dev || latest.fingerprint?.ino !== fingerprint?.ino || latest.fingerprint?.sha256 !== fingerprint?.sha256) throw new Error("table_sync_store_changed");
        await rename(temporary, path);
        await verifyPrivateFile(path);
        const written = await lstat(path, { bigint: true }); fingerprint = { dev: written.dev, ino: written.ino, sha256: hash(bytes) };
        this.#assert(account, directory);
      };
      return await action(state, checkpoint);
    } finally { active = false; release(); if (this.#queues.get(key) === own) this.#queues.delete(key); }
  }
  close(): void { this.#closed = true; }
}

export function registerTableSyncResult(state: TableSyncState, user: string, campaign: string): boolean {
  const key = tableSyncKey(user, campaign), value = state.tables[key];
  if (!value) {
    state.tables[key] = table.parse({ campaign_id: campaign, user_id: user, enabled: true, next_check: 0, committed_hash: null, previous_version: null, sequence: 0, status: "waiting", error: null, pending: null });
    return true;
  }
  if (value.status !== "needs_attention") {
    if (!value.enabled) return false;
    const changed = BigInt(value.next_check) !== 0n || value.status !== "waiting";
    value.next_check = 0n; value.status = "waiting"; return changed;
  }
  value.enabled = true; value.status = "waiting"; value.next_check = 0n; value.failures = 0; value.error = null; return true;
}

export function resumeTableSyncConsent(state: TableSyncState, user: string): void {
  for (const value of Object.values(state.tables)) if (value.user_id === user && value.status === "consent_revoked") { value.enabled = true; value.status = "waiting"; value.next_check = 0n; value.failures = 0; value.error = null; }
}
export function tableSyncQueued(value: TableSyncTable, now: bigint): boolean { return value.enabled && !["synced", "up_to_date"].includes(value.status) && value.next_check <= now; }
